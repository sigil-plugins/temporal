//! Pinned, message-only Temporal API bindings. Normal builds run no codegen.
// Upstream generated declarations retain their shape; lint our generator and
// client separately rather than rewriting generated code to satisfy style lints.
#![allow(clippy::all, clippy::pedantic, clippy::nursery)]

include!("generated/mod.rs");

/// Descriptor-derived field identities for allocation-bounded wire preflight.
pub mod wire {
    include!("generated/wire.rs");
}

#[cfg(test)]
mod tests {
    use super::temporal::api::{
        enums::v1::{EventType, HistoryEventFilterType, TaskQueueKind, WorkflowExecutionStatus},
        failure::v1::failure::FailureInfo,
        history::v1::history_event::Attributes,
        workflowservice::v1::{
            DescribeWorkflowExecutionRequest, DescribeWorkflowExecutionResponse,
            GetWorkflowExecutionHistoryRequest, GetWorkflowExecutionHistoryResponse,
            StartWorkflowExecutionRequest, StartWorkflowExecutionResponse,
        },
    };
    use prost::Message;

    #[test]
    fn protoc_request_oracles_preserve_canonical_bytes_and_presence() {
        let bytes = include_bytes!("../conformance/requests/start-workflow-execution.pb");
        let start = StartWorkflowExecutionRequest::decode(bytes.as_slice()).expect("start oracle");
        assert_eq!(start.encode_to_vec(), bytes);
        assert_eq!(start.namespace, "local-data-execution-gcp.hgzph");
        assert_eq!(start.identity, "sigil-temporal@0.1.0");
        assert_eq!(start.request_id, "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee");
        assert_eq!(
            start.task_queue.expect("task queue").kind,
            TaskQueueKind::Normal as i32
        );
        assert_eq!(
            start.workflow_execution_timeout,
            Some(prost_types::Duration::default())
        );
        assert_eq!(
            start.workflow_run_timeout,
            Some(prost_types::Duration::default())
        );
        assert_eq!(
            start.workflow_task_timeout,
            Some(prost_types::Duration {
                seconds: 10,
                nanos: 0
            })
        );
        assert!(
            start
                .header
                .expect("present empty header")
                .fields
                .is_empty()
        );
        let payloads = start.input.expect("input").payloads;
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0].metadata["encoding"], b"json/plain");
        assert_eq!(payloads[0].data, br#"{"eventFeedViewAssetID":"synthetic"}"#);
        assert_eq!(
            payloads[1].data,
            br#"{"actionID":"synthetic","resourceID":"synthetic"}"#
        );

        let bytes = include_bytes!("../conformance/requests/describe-workflow-execution.pb");
        let describe =
            DescribeWorkflowExecutionRequest::decode(bytes.as_slice()).expect("describe oracle");
        assert_eq!(describe.encode_to_vec(), bytes);
        assert!(describe.execution.expect("execution").run_id.is_empty());
        for (bytes, close) in [
            (
                include_bytes!("../conformance/requests/history-close-event.pb").as_slice(),
                true,
            ),
            (
                include_bytes!("../conformance/requests/history-all-events-page-token.pb")
                    .as_slice(),
                false,
            ),
        ] {
            let history =
                GetWorkflowExecutionHistoryRequest::decode(bytes).expect("history oracle");
            assert_eq!(history.encode_to_vec(), bytes);
            assert_eq!(history.maximum_page_size, 0);
            assert_eq!(history.wait_new_event, close);
            assert_eq!(history.skip_archival, close);
            assert_eq!(
                history.history_event_filter_type,
                if close {
                    HistoryEventFilterType::CloseEvent
                } else {
                    HistoryEventFilterType::AllEvent
                } as i32
            );
            assert_eq!(
                history.next_page_token,
                if close {
                    b"".as_slice()
                } else {
                    b"synthetic-page-token"
                }
            );
            assert!(history.execution.expect("execution").run_id.is_empty());
        }
    }

    #[test]
    fn protoc_start_presence_oracles_preserve_boolean_value() {
        // Bind the synthetic proto2 fragment to the descriptor generated from
        // the pinned official schema, not to the plugin's request encoder.
        let started = super::wire::MESSAGE_SCHEMA[super::wire::START_RESPONSE]
            .fields
            .iter()
            .find(|field| field.name == "started")
            .expect("official started field");
        assert_eq!(started.number, 3);
        assert_eq!(started.kind, super::wire::Kind::Varint);
        assert!(!started.repeated);

        let omitted = include_bytes!("../conformance/responses/start-omitted.pb").as_slice();
        let source_false = include_bytes!("../conformance/responses/start-false.pb").as_slice();
        let wire_false = include_bytes!("../conformance/responses/start-wire-false.pb").as_slice();
        assert_eq!(omitted, source_false, "proto3 omits default false");
        assert_ne!(omitted, wire_false, "fixture must exercise wire presence");
        assert_eq!(
            wire_false.strip_prefix(omitted),
            Some([0x18, 0x00].as_slice()),
            "protoc fragment must encode field 3, varint false"
        );
        for (bytes, expected) in [
            (omitted, false),
            (source_false, false),
            (wire_false, false),
            (
                include_bytes!("../conformance/responses/start-success.pb").as_slice(),
                true,
            ),
        ] {
            let response = StartWorkflowExecutionResponse::decode(bytes).expect("start oracle");
            assert_eq!(response.started, expected);
            assert_eq!(response.status, 1);
            assert_eq!(response.run_id, "11111111-2222-3333-4444-555555555555");
            if !expected {
                // Generated proto3 API does not preserve wire presence.
                assert_eq!(response.encode_to_vec(), omitted);
            }
        }
    }

    #[test]
    fn protoc_start_and_describe_oracles_preserve_status_numbers() {
        for (bytes, started, status) in [
            (
                include_bytes!("../conformance/responses/start-success.pb").as_slice(),
                true,
                1,
            ),
            (
                include_bytes!("../conformance/responses/start-existing.pb").as_slice(),
                false,
                2,
            ),
            (
                include_bytes!("../conformance/responses/start-future-status.pb").as_slice(),
                true,
                31415,
            ),
        ] {
            let response = StartWorkflowExecutionResponse::decode(bytes).expect("start response");
            assert_eq!(response.run_id, "11111111-2222-3333-4444-555555555555");
            assert_eq!(response.started, started);
            assert_eq!(response.status, status);
            if status == 31415 {
                assert!(WorkflowExecutionStatus::try_from(status).is_err());
            }
        }
        for (bytes, status) in [
            (
                include_bytes!("../conformance/responses/describe-running.pb").as_slice(),
                1,
            ),
            (
                include_bytes!("../conformance/responses/describe-completed.pb").as_slice(),
                2,
            ),
            (
                include_bytes!("../conformance/responses/describe-future-status.pb").as_slice(),
                31415,
            ),
        ] {
            let response =
                DescribeWorkflowExecutionResponse::decode(bytes).expect("describe response");
            let info = response
                .workflow_execution_info
                .expect("nested execution info");
            assert_eq!(info.status, status);
            assert_eq!(
                info.execution.expect("execution").run_id,
                "11111111-2222-3333-4444-555555555555"
            );
            if status == 2 {
                assert_eq!(info.history_length, 9_007_199_254_740_993);
                assert_eq!(info.start_time.expect("start timestamp").nanos, 123_456_789);
                assert_eq!(info.close_time.expect("close timestamp").nanos, 987_654_321);
            }
        }
    }

    #[test]
    fn protoc_completion_preserves_int64_nanos_and_binary_payload_metadata() {
        let response = GetWorkflowExecutionHistoryResponse::decode(
            include_bytes!("../conformance/responses/history-completed.pb").as_slice(),
        )
        .expect("completion oracle");
        let history = response.history.expect("history");
        let event = &history.events[0];
        assert_eq!(event.event_id, 9_007_199_254_740_993);
        assert_eq!(event.task_id, i64::MAX);
        assert_eq!(
            event.event_time,
            Some(prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: 123_456_789
            })
        );
        assert_eq!(
            event.event_type,
            EventType::WorkflowExecutionCompleted as i32
        );
        let Some(Attributes::WorkflowExecutionCompletedEventAttributes(details)) =
            &event.attributes
        else {
            panic!("completion attributes");
        };
        let payloads = &details.result.as_ref().expect("result").payloads;
        assert_eq!(payloads.len(), 2);
        assert_eq!(
            payloads[0]
                .metadata
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["encoding", "type"]
        );
        assert_eq!(payloads[0].metadata["type"], b"SyntheticResult");
        assert_eq!(payloads[0].data, br#"{"answer":42}"#);
        assert_eq!(payloads[1].metadata["type"], b"\x00\xff\x80");
        assert_eq!(payloads[1].data, b"\x00\xff\x80\n\r\"\\");
    }

    #[test]
    fn protoc_failure_preserves_cause_order_and_activity_identity() {
        let response = GetWorkflowExecutionHistoryResponse::decode(
            include_bytes!("../conformance/responses/history-failed.pb").as_slice(),
        )
        .expect("failure oracle");
        let event = response.history.expect("history").events.remove(0);
        let Some(Attributes::WorkflowExecutionFailedEventAttributes(details)) = event.attributes
        else {
            panic!("failure attributes");
        };
        let failure = details.failure.expect("failure");
        assert_eq!(failure.message, "synthetic activity failure");
        assert_eq!(failure.source, "GoSDK");
        let Some(FailureInfo::ActivityFailureInfo(activity)) = failure.failure_info else {
            panic!("activity failure info");
        };
        assert_eq!(
            activity.activity_type.expect("activity type").name,
            "SyntheticActivity"
        );
        assert_eq!(activity.scheduled_event_id, 9_007_199_254_740_993);
        let cause = failure.cause.expect("root cause");
        assert_eq!(cause.message, "synthetic root cause");
        assert_eq!(cause.source, "JavaSDK");
        assert!(cause.cause.is_none());
    }

    #[test]
    fn protoc_pages_preserve_activity_order_tokens_and_future_values() {
        let response = GetWorkflowExecutionHistoryResponse::decode(
            include_bytes!("../conformance/responses/history-activities-page-one.pb").as_slice(),
        )
        .expect("page one");
        assert_eq!(response.next_page_token, b"\x00next\xff");
        let names = response
            .history
            .expect("history")
            .events
            .into_iter()
            .map(|event| {
                let Some(Attributes::ActivityTaskScheduledEventAttributes(details)) =
                    event.attributes
                else {
                    panic!("scheduled activity");
                };
                details.activity_type.expect("activity type").name
            })
            .collect::<Vec<_>>();
        assert_eq!(names, ["FirstActivity", "SecondActivity"]);
        let page_two = GetWorkflowExecutionHistoryResponse::decode(
            include_bytes!("../conformance/responses/history-future-event-page-two.pb").as_slice(),
        )
        .expect("page two");
        let with_unknown = GetWorkflowExecutionHistoryResponse::decode(
            include_bytes!("../conformance/responses/history-future-fields.pb").as_slice(),
        )
        .expect("future protobuf field");
        assert_eq!(with_unknown, page_two);
        assert!(page_two.next_page_token.is_empty());
        let event = &page_two.history.expect("history").events[0];
        assert_eq!(event.event_id, -9_007_199_254_740_993);
        assert_eq!(event.task_id, i64::MIN);
        assert_eq!(
            event.event_time,
            Some(prost_types::Timestamp {
                seconds: -62_135_596_800,
                nanos: 999_999_999
            })
        );
        assert_eq!(event.event_type, 31415);
        assert!(EventType::try_from(event.event_type).is_err());
        assert!(event.attributes.is_none());
        let empty = GetWorkflowExecutionHistoryResponse::decode(
            include_bytes!("../conformance/responses/history-empty-page.pb").as_slice(),
        )
        .expect("empty page");
        assert!(
            empty
                .history
                .expect("present empty history")
                .events
                .is_empty()
        );
    }
}
