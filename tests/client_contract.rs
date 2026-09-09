use prost::Message;
use sigil_temporal::client::{self, Exchange, api, host};
use sigil_temporal::proto::temporal::api::workflowservice::v1 as service;
use sigil_temporal::proto::temporal::api::{common::v1 as common, enums::v1 as enums};
use sigil_temporal::proto::temporal::api::{failure::v1 as failure, history::v1 as history};

const START_SUCCESS: &[u8] = include_bytes!("../conformance/responses/start-success.pb");
const DESCRIBE_SUCCESS: &[u8] = include_bytes!("../conformance/responses/describe-completed.pb");
const EMPTY_PAGE: &[u8] = include_bytes!("../conformance/responses/history-empty-page.pb");

struct FakeHost {
    calls: Vec<host::Call>,
    response: Option<Result<host::Response, host::Failure>>,
}

impl FakeHost {
    fn bytes(bytes: &[u8]) -> Self {
        Self {
            calls: Vec::new(),
            response: Some(Ok(host::Response {
                status: 0,
                sent: host::SendState::MessageSent,
                message: Some(bytes.to_vec()),
                grpc_message: None,
                status_details_bin: None,
                initial_metadata: Vec::new(),
                trailing_metadata: Vec::new(),
            })),
        }
    }
}

impl Exchange for FakeHost {
    fn exchange(&mut self, request: host::Call) -> Result<host::Response, host::Failure> {
        self.calls.push(request);
        self.response.take().expect("one exchange only")
    }
}

fn payload(bytes: &[u8]) -> api::Payload {
    api::Payload {
        metadata: vec![api::MetadataEntry {
            key: "encoding".into(),
            value: b"json/plain".to_vec(),
        }],
        data: bytes.to_vec(),
    }
}

fn start_request() -> api::StartRequest {
    api::StartRequest {
        profile: "workflow".into(),
        namespace: "local-data-execution-gcp.hgzph".into(),
        workflow_id: "capi-m5-00000000-x1".into(),
        workflow_type: "WorkflowForChildWorkflow".into(),
        task_queue: "task-queue-for-child-workflow".into(),
        payloads: vec![
            payload(br#"{"eventFeedViewAssetID":"synthetic"}"#),
            payload(br#"{"actionID":"synthetic","resourceID":"synthetic"}"#),
        ],
        request_id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into(),
        timeout_millis: 10_000,
    }
}

fn describe_request() -> api::DescribeRequest {
    api::DescribeRequest {
        profile: "workflow".into(),
        namespace: "local-data-execution-gcp.hgzph".into(),
        workflow_id: "capi-m5-00000000-x1".into(),
        timeout_millis: 10_000,
    }
}

fn history_request(close: bool) -> api::HistoryRequest {
    api::HistoryRequest {
        profile: "workflow".into(),
        namespace: "local-data-execution-gcp.hgzph".into(),
        workflow_id: "capi-m5-00000000-r1".into(),
        wait_new_event: close,
        filter: if close {
            api::HistoryFilter::CloseEvent
        } else {
            api::HistoryFilter::AllEvents
        },
        skip_archival: close,
        next_page_token: Vec::new(),
        timeout_millis: if close { 65_000 } else { 10_000 },
    }
}

#[test]
fn request_bytes_match_independent_protoc_oracles_and_exchange_once() {
    let mut host = FakeHost::bytes(START_SUCCESS);
    let response = client::start_workflow_execution(&mut host, start_request()).expect("start");
    assert_eq!(host.calls.len(), 1);
    assert_eq!(
        host.calls[0].message,
        include_bytes!("../conformance/requests/start-workflow-execution.pb")
    );
    assert_eq!(host.calls[0].profile, "workflow");
    assert_eq!(host.calls[0].rpc, "start");
    assert_eq!(host.calls[0].timeout_millis, 10_000);
    assert_eq!(host.calls[0].max_response_bytes, 4_194_304);
    assert_eq!(response.run_id, "11111111-2222-3333-4444-555555555555");
    assert!(response.started);
    assert_eq!(response.status.number, 1);
    assert_eq!(
        response.status.label.as_deref(),
        Some("WORKFLOW_EXECUTION_STATUS_RUNNING")
    );
    assert_eq!(response.effect, api::MutationEffect::Applied);

    let mut host = FakeHost::bytes(DESCRIBE_SUCCESS);
    let response =
        client::describe_workflow_execution(&mut host, describe_request()).expect("describe");
    assert_eq!(host.calls.len(), 1);
    assert_eq!(
        host.calls[0].message,
        include_bytes!("../conformance/requests/describe-workflow-execution.pb")
    );
    assert_eq!(host.calls[0].rpc, "describe");
    assert_eq!(response.status.number, 2);

    for close in [false, true] {
        let mut host = FakeHost::bytes(EMPTY_PAGE);
        let mut request = history_request(close);
        let expected = if close {
            include_bytes!("../conformance/requests/history-close-event.pb").as_slice()
        } else {
            request.next_page_token = b"synthetic-page-token".to_vec();
            include_bytes!("../conformance/requests/history-all-events-page-token.pb").as_slice()
        };
        client::get_workflow_execution_history(&mut host, request).expect("history");
        assert_eq!(host.calls.len(), 1);
        assert_eq!(host.calls[0].message, expected);
        assert_eq!(host.calls[0].rpc, "history");
        assert_eq!(
            host.calls[0].timeout_millis,
            if close { 65_000 } else { 10_000 }
        );
    }
}

#[test]
fn start_existing_and_future_status_are_not_reinterpreted() {
    for (bytes, started, number, known) in [
        (
            include_bytes!("../conformance/responses/start-existing.pb").as_slice(),
            false,
            2,
            true,
        ),
        (
            include_bytes!("../conformance/responses/start-future-status.pb").as_slice(),
            true,
            31_415,
            false,
        ),
    ] {
        let mut host = FakeHost::bytes(bytes);
        let response =
            client::start_workflow_execution(&mut host, start_request()).expect("start response");
        assert_eq!(response.started, started);
        assert_eq!(response.status.number, number);
        assert_eq!(response.status.label.is_some(), known);
        assert_eq!(response.effect, api::MutationEffect::Applied);
        assert_eq!(host.calls.len(), 1);
    }
    let mut host = FakeHost::bytes(include_bytes!(
        "../conformance/responses/describe-future-status.pb"
    ));
    let response = client::describe_workflow_execution(&mut host, describe_request())
        .expect("future describe");
    assert_eq!(response.status.number, 31_415);
    assert!(response.status.label.is_none());
}

#[test]
fn history_preserves_integer_nanos_binary_metadata_and_payload_order() {
    let mut host = FakeHost::bytes(include_bytes!(
        "../conformance/responses/history-completed.pb"
    ));
    let page =
        client::get_workflow_execution_history(&mut host, history_request(true)).expect("complete");
    assert_eq!(page.events.len(), 1);
    let event = &page.events[0];
    assert_eq!(event.event_id, 9_007_199_254_740_993);
    assert_eq!(event.task_id, i64::MAX);
    assert_eq!(event.event_time.seconds, 1_700_000_000);
    assert_eq!(event.event_time.nanos, 123_456_789);
    let api::EventDetails::WorkflowCompleted(payloads) = &event.details else {
        panic!("completion");
    };
    assert_eq!(payloads.len(), 2);
    assert_eq!(
        payloads[0]
            .metadata
            .iter()
            .map(|entry| entry.key.as_str())
            .collect::<Vec<_>>(),
        ["encoding", "type"]
    );
    assert_eq!(payloads[0].data, br#"{"answer":42}"#);
    assert_eq!(payloads[1].data, b"\0\xff\x80\n\r\"\\");
    assert_eq!(payloads[1].metadata[1].value, b"\0\xff\x80");
}

#[test]
fn failed_history_keeps_separate_activity_source_and_cause_nodes() {
    let mut host = FakeHost::bytes(include_bytes!("../conformance/responses/history-failed.pb"));
    let page = client::get_workflow_execution_history(&mut host, history_request(true))
        .expect("failed event");
    let api::EventDetails::WorkflowFailed(nodes) = &page.events[0].details else {
        panic!("failure details");
    };
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].message, "synthetic activity failure");
    assert_eq!(nodes[0].source.as_deref(), Some("GoSDK"));
    assert_eq!(nodes[0].activity_type.as_deref(), Some("SyntheticActivity"));
    assert_eq!(nodes[1].message, "synthetic root cause");
    assert_eq!(nodes[1].source.as_deref(), Some("JavaSDK"));
    assert!(nodes[1].activity_type.is_none());
}

#[test]
fn caller_pagination_and_future_fields_preserve_exact_values_without_fetching() {
    let mut host = FakeHost::bytes(include_bytes!(
        "../conformance/responses/history-activities-page-one.pb"
    ));
    let page = client::get_workflow_execution_history(&mut host, history_request(false))
        .expect("first page");
    assert_eq!(page.events.len(), 2);
    assert!(
        matches!(&page.events[0].details, api::EventDetails::ActivityScheduled(name) if name == "FirstActivity")
    );
    assert!(
        matches!(&page.events[1].details, api::EventDetails::ActivityScheduled(name) if name == "SecondActivity")
    );
    assert_eq!(page.next_page_token, b"\0next\xff");
    assert_eq!(host.calls.len(), 1);
    for bytes in [
        include_bytes!("../conformance/responses/history-future-event-page-two.pb").as_slice(),
        include_bytes!("../conformance/responses/history-future-fields.pb").as_slice(),
    ] {
        let mut request = history_request(false);
        request.next_page_token = page.next_page_token.clone();
        let mut host = FakeHost::bytes(bytes);
        let page = client::get_workflow_execution_history(&mut host, request).expect("next page");
        assert_eq!(host.calls.len(), 1);
        let request =
            service::GetWorkflowExecutionHistoryRequest::decode(host.calls[0].message.as_slice())
                .expect("request");
        assert_eq!(request.next_page_token, b"\0next\xff");
        let event = &page.events[0];
        assert_eq!(event.event_id, -9_007_199_254_740_993);
        assert_eq!(event.task_id, i64::MIN);
        assert_eq!(event.event_time.nanos, 999_999_999);
        assert_eq!(event.event_type.number, 31_415);
        assert!(event.event_type.label.is_none());
        assert!(matches!(event.details, api::EventDetails::Other));
    }
}

#[test]
fn all_host_failures_remain_infrastructure_with_exact_start_ambiguity() {
    for kind in [
        host::ErrorKind::Denied,
        host::ErrorKind::InvalidRequest,
        host::ErrorKind::Unavailable,
        host::ErrorKind::Timeout,
        host::ErrorKind::Tls,
        host::ErrorKind::Protocol,
        host::ErrorKind::Io,
        host::ErrorKind::Limit,
        host::ErrorKind::CredentialDenied,
        host::ErrorKind::Cancelled,
        host::ErrorKind::Internal,
    ] {
        for sent in [
            host::SendState::NotSent,
            host::SendState::HeadersSent,
            host::SendState::MessagePartial,
            host::SendState::MessageSent,
        ] {
            let mut host = FakeHost {
                calls: Vec::new(),
                response: Some(Err(host::Failure { kind, sent })),
            };
            let error = client::start_workflow_execution(&mut host, start_request())
                .expect_err("host fault");
            assert_eq!(host.calls.len(), 1);
            assert_eq!(error.kind, api::ErrorKind::Infrastructure);
            assert_eq!(error.code, api::ErrorCode::HostFailure);
            assert_eq!(
                error.effect,
                if sent == host::SendState::NotSent {
                    api::MutationEffect::NotSent
                } else {
                    api::MutationEffect::Unknown
                }
            );
            assert!(error.server.is_none());
        }
    }
}

#[test]
fn every_status_class_is_exact_and_nonzero_start_never_claims_not_applied() {
    use api::ServerClass as Class;
    let cases = [
        (1, Class::Cancelled),
        (2, Class::Server),
        (3, Class::Invalid),
        (4, Class::Deadline),
        (5, Class::NotFound),
        (6, Class::Conflict),
        (7, Class::Authorization),
        (8, Class::Exhausted),
        (9, Class::FailedPrecondition),
        (10, Class::Aborted),
        (11, Class::Unknown),
        (12, Class::Unsupported),
        (13, Class::Server),
        (14, Class::Unavailable),
        (15, Class::Server),
        (16, Class::Authentication),
        (42, Class::Unknown),
    ];
    for (status, class) in cases {
        let mut host = FakeHost::bytes(&[]);
        let response = host
            .response
            .as_mut()
            .expect("response")
            .as_mut()
            .expect("ok");
        response.status = status;
        response.message = None;
        response.grpc_message = Some("normalized\\nmessage".into());
        response.status_details_bin = Some(vec![0, 255, 128]);
        let error =
            client::start_workflow_execution(&mut host, start_request()).expect_err("status");
        assert_eq!(error.kind, api::ErrorKind::ServerStatus);
        assert_eq!(error.effect, api::MutationEffect::Unknown);
        let server = error.server.expect("status record");
        assert_eq!(server.code, status);
        assert_eq!(server.class, class);
        assert_eq!(server.name.is_some(), status <= 16);
        assert_eq!(server.message.as_deref(), Some("normalized\\nmessage"));
        assert_eq!(server.details, [0, 255, 128]);
    }
    let mut host = FakeHost::bytes(&[]);
    let response = host
        .response
        .as_mut()
        .expect("response")
        .as_mut()
        .expect("ok");
    response.status = 5;
    response.message = None;
    let error = client::describe_workflow_execution(&mut host, describe_request())
        .expect_err("direct not found");
    assert_eq!(error.effect, api::MutationEffect::NotApplicable);
    assert_eq!(
        error.server.expect("status").name.as_deref(),
        Some("NOT_FOUND")
    );
}

#[test]
fn local_validation_rejects_before_host_and_never_rewrites_json() {
    for change in 0..10 {
        let mut request = start_request();
        match change {
            0 => request.profile = "1invalid".into(),
            1 => request.namespace.clear(),
            2 => request.workflow_id.push('\u{202e}'),
            3 => request.workflow_type.push('\0'),
            4 => request.task_queue = "x".repeat(256),
            5 => request.request_id = "has space".into(),
            6 => request.payloads.pop().map(|_| ()).expect("payload"),
            7 => request.payloads[0].data = vec![255],
            8 => request.payloads[0].data = b"{} {}".to_vec(),
            9 => request.payloads[0].metadata.push(api::MetadataEntry {
                key: "encoding".into(),
                value: b"json/plain".to_vec(),
            }),
            _ => unreachable!(),
        }
        let mut host = FakeHost::bytes(START_SUCCESS);
        let error =
            client::start_workflow_execution(&mut host, request).expect_err("invalid request");
        assert!(host.calls.is_empty());
        assert_eq!(error.effect, api::MutationEffect::NotSent);
        assert!(error.server.is_none());
    }
    let mut request = start_request();
    request.payloads[0].data = b" \n {\"z\": 9007199254740993, \"a\":1} \t".to_vec();
    let exact = request.payloads[0].data.clone();
    let mut host = FakeHost::bytes(START_SUCCESS);
    client::start_workflow_execution(&mut host, request).expect("exact JSON");
    let request = service::StartWorkflowExecutionRequest::decode(host.calls[0].message.as_slice())
        .expect("request");
    assert_eq!(request.input.expect("input").payloads[0].data, exact);
}

#[test]
fn timeout_and_history_shapes_are_lower_only() {
    for timeout in [0, 10_001, u64::MAX] {
        let mut request = describe_request();
        request.timeout_millis = timeout;
        let mut host = FakeHost::bytes(DESCRIBE_SUCCESS);
        assert_eq!(
            client::describe_workflow_execution(&mut host, request)
                .expect_err("timeout")
                .code,
            api::ErrorCode::InvalidTimeout
        );
        assert!(host.calls.is_empty());
    }
    for close in [false, true] {
        let mut request = history_request(close);
        request.timeout_millis = 1;
        let mut host = FakeHost::bytes(EMPTY_PAGE);
        client::get_workflow_execution_history(&mut host, request).expect("lowered timeout");
        assert_eq!(host.calls[0].timeout_millis, 1);
        let mut request = history_request(close);
        request.skip_archival = !request.skip_archival;
        let mut host = FakeHost::bytes(EMPTY_PAGE);
        assert!(client::get_workflow_execution_history(&mut host, request).is_err());
        assert!(host.calls.is_empty());
    }
}

fn simple_event() -> history::HistoryEvent {
    history::HistoryEvent {
        event_id: 1,
        event_time: Some(prost_types::Timestamp {
            seconds: 0,
            nanos: 0,
        }),
        event_type: enums::EventType::WorkflowTaskStarted as i32,
        ..Default::default()
    }
}

fn event_response(event: history::HistoryEvent) -> Vec<u8> {
    service::GetWorkflowExecutionHistoryResponse {
        history: Some(history::History {
            events: vec![event],
        }),
        ..Default::default()
    }
    .encode_to_vec()
}

#[test]
fn response_shape_errors_return_no_partial_page() {
    for nanos in [-1, 1_000_000_000] {
        let mut event = simple_event();
        event.event_time.as_mut().expect("time").nanos = nanos;
        let mut host = FakeHost::bytes(&event_response(event));
        assert_eq!(
            client::get_workflow_execution_history(&mut host, history_request(false))
                .expect_err("nanos")
                .code,
            api::ErrorCode::MalformedResponse
        );
    }
    let mut event = simple_event();
    event.attributes = Some(
        history::history_event::Attributes::WorkflowExecutionCompletedEventAttributes(
            history::WorkflowExecutionCompletedEventAttributes::default(),
        ),
    );
    let mut host = FakeHost::bytes(&event_response(event));
    assert_eq!(
        client::get_workflow_execution_history(&mut host, history_request(false))
            .expect_err("attribute mismatch")
            .code,
        api::ErrorCode::MalformedResponse
    );
    let response = service::GetWorkflowExecutionHistoryResponse {
        raw_history: vec![common::DataBlob::default()],
        ..Default::default()
    };
    let mut host = FakeHost::bytes(&response.encode_to_vec());
    assert_eq!(
        client::get_workflow_execution_history(&mut host, history_request(false))
            .expect_err("raw history")
            .code,
        api::ErrorCode::UnsupportedResponse
    );
}

#[test]
fn history_and_failure_count_boundaries_are_inclusive() {
    for count in [4096, 4097] {
        let response = service::GetWorkflowExecutionHistoryResponse {
            history: Some(history::History {
                events: vec![simple_event(); count],
            }),
            ..Default::default()
        };
        let mut host = FakeHost::bytes(&response.encode_to_vec());
        let result = client::get_workflow_execution_history(&mut host, history_request(false));
        if count == 4096 {
            assert_eq!(result.expect("inclusive events").events.len(), count);
        } else {
            assert_eq!(result.expect_err("event limit").kind, api::ErrorKind::Limit);
        }
    }
    for count in [16, 17] {
        let mut failure = failure::Failure::default();
        for _ in 1..count {
            failure = failure::Failure {
                cause: Some(Box::new(failure)),
                ..Default::default()
            };
        }
        let mut event = simple_event();
        event.event_type = enums::EventType::WorkflowExecutionFailed as i32;
        event.attributes = Some(
            history::history_event::Attributes::WorkflowExecutionFailedEventAttributes(
                history::WorkflowExecutionFailedEventAttributes {
                    failure: Some(failure),
                    ..Default::default()
                },
            ),
        );
        let mut host = FakeHost::bytes(&event_response(event));
        let result = client::get_workflow_execution_history(&mut host, history_request(false));
        if count == 16 {
            assert!(result.is_ok());
        } else {
            assert_eq!(
                result.expect_err("failure limit").kind,
                api::ErrorKind::Limit
            );
        }
    }
}

fn payload_response(payload: common::Payload) -> Vec<u8> {
    let mut event = simple_event();
    event.event_type = enums::EventType::WorkflowExecutionCompleted as i32;
    event.attributes = Some(
        history::history_event::Attributes::WorkflowExecutionCompletedEventAttributes(
            history::WorkflowExecutionCompletedEventAttributes {
                result: Some(common::Payloads {
                    payloads: vec![payload],
                }),
                ..Default::default()
            },
        ),
    );
    event_response(event)
}

#[test]
fn payload_data_and_total_request_boundaries_are_inclusive() {
    for size in [2_097_152, 2_097_153] {
        let bytes = payload_response(common::Payload {
            data: vec![0xff; size],
            ..Default::default()
        });
        let mut host = FakeHost::bytes(&bytes);
        let result = client::get_workflow_execution_history(&mut host, history_request(true));
        if size == 2_097_152 {
            assert!(result.is_ok());
        } else {
            assert_eq!(
                result.expect_err("payload limit").kind,
                api::ErrorKind::Limit
            );
        }
        let mut request = start_request();
        let mut json = vec![b' '; size];
        json[0] = b'"';
        json[size - 1] = b'"';
        request.payloads[0].data = json;
        let mut host = FakeHost::bytes(START_SUCCESS);
        let result = client::start_workflow_execution(&mut host, request);
        if size == 2_097_152 {
            assert!(result.is_ok());
        } else {
            assert_eq!(result.expect_err("input limit").kind, api::ErrorKind::Limit);
            assert!(host.calls.is_empty());
        }
    }
    let mut request = start_request();
    for payload in &mut request.payloads {
        payload.data = vec![b' '; 2_097_152];
        payload.data[0] = b'0';
    }
    let mut host = FakeHost::bytes(START_SUCCESS);
    assert_eq!(
        client::start_workflow_execution(&mut host, request)
            .expect_err("total envelope-independent request limit")
            .kind,
        api::ErrorKind::Limit
    );
    assert!(host.calls.is_empty());
}

#[test]
fn response_metadata_entry_key_and_aggregate_limits_are_independent() {
    for (entries, key_bytes, value_bytes, valid) in [
        (32, 1, 0, true),
        (33, 1, 0, false),
        (1, 256, 0, true),
        (1, 257, 0, false),
        (1, 1, 8191, true),
        (1, 1, 8192, false),
    ] {
        let metadata = (0..entries)
            .map(|index| {
                let key = if entries == 1 {
                    "k".repeat(key_bytes)
                } else {
                    format!("k{index}")
                };
                (key, vec![0xff; value_bytes])
            })
            .collect();
        let mut host = FakeHost::bytes(&payload_response(common::Payload {
            metadata,
            ..Default::default()
        }));
        let result = client::get_workflow_execution_history(&mut host, history_request(true));
        if valid {
            assert!(result.is_ok());
        } else {
            assert_eq!(
                result.expect_err("metadata bound").kind,
                api::ErrorKind::Limit
            );
        }
    }
}

#[test]
fn tokens_strings_and_identifiers_preserve_inclusive_boundaries() {
    for count in [65_536, 65_537] {
        let mut request = history_request(false);
        request.next_page_token = vec![255; count];
        let mut host = FakeHost::bytes(EMPTY_PAGE);
        let result = client::get_workflow_execution_history(&mut host, request);
        if count == 65_536 {
            assert!(result.is_ok());
        } else {
            assert_eq!(result.expect_err("input token").kind, api::ErrorKind::Limit);
            assert!(host.calls.is_empty());
        }
        let response = service::GetWorkflowExecutionHistoryResponse {
            next_page_token: vec![255; count],
            ..Default::default()
        };
        let mut host = FakeHost::bytes(&response.encode_to_vec());
        let result = client::get_workflow_execution_history(&mut host, history_request(false));
        if count == 65_536 {
            assert_eq!(result.expect("output token").next_page_token.len(), count);
        } else {
            assert_eq!(
                result.expect_err("output token").kind,
                api::ErrorKind::Limit
            );
        }
    }
    for (character, count, valid) in [
        ('x', 1024, true),
        ('x', 1025, false),
        ('🦀', 1024, true),
        ('🦀', 1025, false),
    ] {
        let run_id: String = std::iter::repeat_n(character, count).collect();
        let response = service::StartWorkflowExecutionResponse {
            run_id,
            ..Default::default()
        };
        let mut host = FakeHost::bytes(&response.encode_to_vec());
        let result = client::start_workflow_execution(&mut host, start_request());
        if valid {
            assert!(result.is_ok());
        } else {
            assert_eq!(
                result.expect_err("output string").kind,
                api::ErrorKind::Limit
            );
        }
    }
    for (count, valid) in [(255, true), (256, false)] {
        let mut request = describe_request();
        request.workflow_id = "w".repeat(count);
        let mut host = FakeHost::bytes(DESCRIBE_SUCCESS);
        let result = client::describe_workflow_execution(&mut host, request);
        if valid {
            assert!(result.is_ok());
        } else {
            assert_eq!(result.expect_err("identifier").kind, api::ErrorKind::Limit);
            assert!(host.calls.is_empty());
        }
    }
    for (count, valid) in [(64, true), (65, false)] {
        let mut request = start_request();
        request.profile = "p".repeat(count);
        request.request_id = "r".repeat(count);
        let mut host = FakeHost::bytes(START_SUCCESS);
        let result = client::start_workflow_execution(&mut host, request);
        if valid {
            assert!(result.is_ok());
        } else {
            assert_eq!(
                result.expect_err("alias or request id").kind,
                api::ErrorKind::Limit
            );
            assert!(host.calls.is_empty());
        }
    }
}

// These few merge/duplicate vectors use descriptor-derived identities. Normal
// success encodings above use the independent protoc corpus, not this helper.
fn wire_field(message: &str, field: &str, value: &[u8]) -> Vec<u8> {
    let schema = sigil_temporal::proto::wire::MESSAGE_SCHEMA
        .iter()
        .find(|schema| schema.name == message)
        .expect("descriptor message");
    let field = schema
        .fields
        .iter()
        .find(|candidate| candidate.name == field)
        .expect("descriptor field");
    let mut bytes = Vec::new();
    prost::encoding::encode_key(
        field.number,
        prost::encoding::WireType::LengthDelimited,
        &mut bytes,
    );
    prost::encoding::encode_varint(value.len() as u64, &mut bytes);
    bytes.extend_from_slice(value);
    bytes
}

#[test]
fn preflight_preserves_duplicate_metadata_evidence_before_map_decode() {
    let mut entry = wire_field(
        "temporal.api.common.v1.Payload.MetadataEntry",
        "key",
        b"encoding",
    );
    entry.extend(wire_field(
        "temporal.api.common.v1.Payload.MetadataEntry",
        "value",
        b"json/plain",
    ));
    let mut payload = wire_field("temporal.api.common.v1.Payload", "metadata", &entry);
    payload.extend(wire_field(
        "temporal.api.common.v1.Payload",
        "metadata",
        &entry,
    ));
    let payloads = wire_field("temporal.api.common.v1.Payloads", "payloads", &payload);
    let attributes = wire_field(
        "temporal.api.history.v1.WorkflowExecutionCompletedEventAttributes",
        "result",
        &payloads,
    );
    let mut event = simple_event();
    event.event_type = enums::EventType::WorkflowExecutionCompleted as i32;
    let mut event = event.encode_to_vec();
    event.extend(wire_field(
        "temporal.api.history.v1.HistoryEvent",
        "workflow_execution_completed_event_attributes",
        &attributes,
    ));
    let history = wire_field("temporal.api.history.v1.History", "events", &event);
    let response = wire_field(
        "temporal.api.workflowservice.v1.GetWorkflowExecutionHistoryResponse",
        "history",
        &history,
    );
    let mut host = FakeHost::bytes(&response);
    let error = client::get_workflow_execution_history(&mut host, history_request(true))
        .expect_err("duplicate wire key");
    assert_eq!(error.kind, api::ErrorKind::Protocol);
    assert_eq!(error.code, api::ErrorCode::DuplicateMetadataKey);
}

#[test]
fn singular_history_merges_do_not_reset_event_budget() {
    let first = history::History {
        events: vec![simple_event(); 4096],
    }
    .encode_to_vec();
    let second = history::History {
        events: vec![simple_event()],
    }
    .encode_to_vec();
    let mut response = wire_field(
        "temporal.api.workflowservice.v1.GetWorkflowExecutionHistoryResponse",
        "history",
        &first,
    );
    response.extend(wire_field(
        "temporal.api.workflowservice.v1.GetWorkflowExecutionHistoryResponse",
        "history",
        &second,
    ));
    let mut host = FakeHost::bytes(&response);
    assert_eq!(
        client::get_workflow_execution_history(&mut host, history_request(false))
            .expect_err("merged event count")
            .kind,
        api::ErrorKind::Limit
    );
}

#[test]
fn external_payload_references_are_not_partial_payload_success() {
    let payload = common::Payload {
        external_payloads: vec![common::payload::ExternalPayloadDetails { size_bytes: 100 }],
        ..Default::default()
    };
    let mut host = FakeHost::bytes(&payload_response(payload));
    assert_eq!(
        client::get_workflow_execution_history(&mut host, history_request(true))
            .expect_err("external data")
            .code,
        api::ErrorCode::UnsupportedResponse
    );
}
