//! One bounded Temporal operation per semantic host exchange.

use std::collections::{BTreeMap, BTreeSet};

use prost::Message;
use serde::{Deserialize, de::IgnoredAny};

pub use crate::bindings::exports::sigil::temporal::client as api;
pub use crate::bindings::sigil::host::grpc_unary as host;
use crate::proto::temporal::api::workflowservice::v1 as service;
use crate::proto::temporal::api::{common::v1 as common, enums::v1 as enums};
use crate::proto::temporal::api::{failure::v1 as failure, history::v1 as history};

const MESSAGE_BYTES: usize = 4 * 1024 * 1024;
const PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
const METADATA_ENTRIES: usize = 32;
const METADATA_BYTES: usize = 8192;
const PAGE_TOKEN_BYTES: usize = 65_536;
const HISTORY_EVENTS: usize = 4096;
const FAILURE_NODES: usize = 16;
const IDENTITY: &str = "sigil-temporal@0.1.0";

/// The entire external effect boundary. An operation calls this at most once.
pub trait Exchange {
    fn exchange(&mut self, request: host::Call) -> Result<host::Response, host::Failure>;
}

#[derive(Clone, Copy)]
struct Context {
    operation: api::Operation,
    effect: api::MutationEffect,
}

impl Context {
    const fn new(operation: api::Operation) -> Self {
        let effect = match operation {
            api::Operation::StartWorkflowExecution => api::MutationEffect::NotSent,
            _ => api::MutationEffect::NotApplicable,
        };
        Self { operation, effect }
    }

    const fn error(self, kind: api::ErrorKind, code: api::ErrorCode) -> api::Error {
        api::Error {
            kind,
            code,
            operation: self.operation,
            effect: self.effect,
            server: None,
        }
    }

    const fn invalid(self, code: api::ErrorCode) -> api::Error {
        self.error(api::ErrorKind::InvalidRequest, code)
    }

    const fn limit(self) -> api::Error {
        self.error(api::ErrorKind::Limit, api::ErrorCode::LimitExceeded)
    }

    const fn malformed(self) -> api::Error {
        self.error(api::ErrorKind::Protocol, api::ErrorCode::MalformedResponse)
    }

    const fn after_send(mut self, sent: host::SendState) -> Self {
        if matches!(self.operation, api::Operation::StartWorkflowExecution) {
            self.effect = match sent {
                host::SendState::NotSent => api::MutationEffect::NotSent,
                _ => api::MutationEffect::Unknown,
            };
        }
        self
    }
}

/// Start with exactly two JSON payloads and a caller-owned request ID.
pub fn start_workflow_execution(
    exchange: &mut impl Exchange,
    request: api::StartRequest,
) -> Result<api::StartResponse, api::Error> {
    let context = Context::new(api::Operation::StartWorkflowExecution);
    validate_call(
        &request.profile,
        &request.namespace,
        &request.workflow_id,
        request.timeout_millis,
        10_000,
        context,
    )?;
    validate_identifier(&request.workflow_type, context)?;
    validate_identifier(&request.task_queue, context)?;
    if request.request_id.len() > 64 {
        return Err(context.limit());
    }
    if request.request_id.is_empty()
        || !request
            .request_id
            .bytes()
            .all(|byte| byte.is_ascii_graphic())
    {
        return Err(context.invalid(api::ErrorCode::InvalidField));
    }
    if request.payloads.len() != 2 {
        return Err(context.invalid(api::ErrorCode::InvalidPayloadCount));
    }
    for payload in &request.payloads {
        validate_input_payload(payload, context)?;
    }
    let input = common::Payloads {
        payloads: request
            .payloads
            .into_iter()
            .map(|payload| common::Payload {
                metadata: payload
                    .metadata
                    .into_iter()
                    .map(|entry| (entry.key, entry.value))
                    .collect(),
                data: payload.data,
                ..Default::default()
            })
            .collect(),
    };
    let message = service::StartWorkflowExecutionRequest {
        namespace: request.namespace,
        workflow_id: request.workflow_id,
        workflow_type: Some(common::WorkflowType {
            name: request.workflow_type,
        }),
        task_queue: Some(crate::proto::temporal::api::taskqueue::v1::TaskQueue {
            name: request.task_queue,
            kind: enums::TaskQueueKind::Normal as i32,
            ..Default::default()
        }),
        input: Some(input),
        workflow_execution_timeout: Some(prost_types::Duration::default()),
        workflow_run_timeout: Some(prost_types::Duration::default()),
        workflow_task_timeout: Some(prost_types::Duration {
            seconds: 10,
            nanos: 0,
        }),
        identity: IDENTITY.into(),
        request_id: request.request_id,
        header: Some(common::Header::default()),
        ..Default::default()
    };
    let (bytes, context) = invoke(
        exchange,
        request.profile,
        "start",
        request.timeout_millis,
        &message,
        context,
    )?;
    preflight(&bytes, crate::proto::wire::START_RESPONSE, context)?;
    let response = service::StartWorkflowExecutionResponse::decode(bytes.as_slice())
        .map_err(|_| context.malformed())?;
    validate_output_string(&response.run_id, context)?;
    if response.run_id.is_empty() {
        return Err(context.malformed());
    }
    Ok(api::StartResponse {
        run_id: response.run_id,
        started: response.started,
        status: workflow_status(response.status),
        effect: api::MutationEffect::Applied,
    })
}

/// Describe the latest run, selected by workflow ID only.
pub fn describe_workflow_execution(
    exchange: &mut impl Exchange,
    request: api::DescribeRequest,
) -> Result<api::DescribeResponse, api::Error> {
    let context = Context::new(api::Operation::DescribeWorkflowExecution);
    validate_call(
        &request.profile,
        &request.namespace,
        &request.workflow_id,
        request.timeout_millis,
        10_000,
        context,
    )?;
    let message = service::DescribeWorkflowExecutionRequest {
        namespace: request.namespace,
        execution: Some(common::WorkflowExecution {
            workflow_id: request.workflow_id,
            run_id: String::new(),
        }),
    };
    let (bytes, context) = invoke(
        exchange,
        request.profile,
        "describe",
        request.timeout_millis,
        &message,
        context,
    )?;
    preflight(&bytes, crate::proto::wire::DESCRIBE_RESPONSE, context)?;
    let response = service::DescribeWorkflowExecutionResponse::decode(bytes.as_slice())
        .map_err(|_| context.malformed())?;
    let info = response
        .workflow_execution_info
        .ok_or_else(|| context.malformed())?;
    let execution = info.execution.ok_or_else(|| context.malformed())?;
    validate_output_string(&execution.run_id, context)?;
    if execution.run_id.is_empty() {
        return Err(context.malformed());
    }
    Ok(api::DescribeResponse {
        run_id: execution.run_id,
        status: workflow_status(info.status),
    })
}

/// Return one page. A returned token never triggers another exchange.
pub fn get_workflow_execution_history(
    exchange: &mut impl Exchange,
    request: api::HistoryRequest,
) -> Result<api::HistoryPage, api::Error> {
    let context = Context::new(api::Operation::GetWorkflowExecutionHistory);
    let (filter, ceiling) = match request.filter {
        api::HistoryFilter::CloseEvent if request.wait_new_event && request.skip_archival => {
            (enums::HistoryEventFilterType::CloseEvent, 65_000)
        }
        api::HistoryFilter::AllEvents if !request.wait_new_event && !request.skip_archival => {
            (enums::HistoryEventFilterType::AllEvent, 10_000)
        }
        _ => return Err(context.invalid(api::ErrorCode::InvalidField)),
    };
    validate_call(
        &request.profile,
        &request.namespace,
        &request.workflow_id,
        request.timeout_millis,
        ceiling,
        context,
    )?;
    if request.next_page_token.len() > PAGE_TOKEN_BYTES {
        return Err(context.limit());
    }
    let message = service::GetWorkflowExecutionHistoryRequest {
        namespace: request.namespace,
        execution: Some(common::WorkflowExecution {
            workflow_id: request.workflow_id,
            run_id: String::new(),
        }),
        next_page_token: request.next_page_token,
        wait_new_event: request.wait_new_event,
        history_event_filter_type: filter as i32,
        skip_archival: request.skip_archival,
        ..Default::default()
    };
    let (bytes, context) = invoke(
        exchange,
        request.profile,
        "history",
        request.timeout_millis,
        &message,
        context,
    )?;
    preflight(&bytes, crate::proto::wire::HISTORY_RESPONSE, context)?;
    let response = service::GetWorkflowExecutionHistoryResponse::decode(bytes.as_slice())
        .map_err(|_| context.malformed())?;
    if !response.raw_history.is_empty() {
        return Err(context.error(
            api::ErrorKind::Unsupported,
            api::ErrorCode::UnsupportedResponse,
        ));
    }
    let events = response.history.unwrap_or_default().events;
    if events.len() > HISTORY_EVENTS || response.next_page_token.len() > PAGE_TOKEN_BYTES {
        return Err(context.limit());
    }
    let events = events
        .into_iter()
        .map(|event| project_event(event, context))
        .collect::<Result<_, _>>()?;
    Ok(api::HistoryPage {
        events,
        next_page_token: response.next_page_token,
    })
}

fn validate_call(
    profile: &str,
    namespace: &str,
    workflow_id: &str,
    timeout: u64,
    ceiling: u64,
    context: Context,
) -> Result<(), api::Error> {
    if profile.len() > 64 {
        return Err(context.limit());
    }
    if !profile
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_lowercase)
        || !profile.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        return Err(context.invalid(api::ErrorCode::InvalidField));
    }
    validate_identifier(namespace, context)?;
    validate_identifier(workflow_id, context)?;
    if timeout == 0 || timeout > ceiling {
        return Err(context.invalid(api::ErrorCode::InvalidTimeout));
    }
    Ok(())
}

fn validate_identifier(value: &str, context: Context) -> Result<(), api::Error> {
    if value.len() > 255 {
        return Err(context.limit());
    }
    if value.is_empty() || value.chars().any(|character| character.is_control() || matches!(character, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')) {
        return Err(context.invalid(api::ErrorCode::InvalidField));
    }
    Ok(())
}

fn validate_input_payload(payload: &api::Payload, context: Context) -> Result<(), api::Error> {
    if payload.data.len() > PAYLOAD_BYTES || payload.metadata.len() > METADATA_ENTRIES {
        return Err(context.limit());
    }
    let mut keys = BTreeSet::new();
    let mut bytes = 0usize;
    for entry in &payload.metadata {
        validate_metadata_key(&entry.key, context, false)?;
        bytes = bytes
            .checked_add(entry.key.len())
            .and_then(|sum| sum.checked_add(entry.value.len()))
            .filter(|sum| *sum <= METADATA_BYTES)
            .ok_or_else(|| context.limit())?;
        if !keys.insert(entry.key.as_str()) {
            return Err(context.invalid(api::ErrorCode::DuplicateMetadataKey));
        }
    }
    if payload.metadata.len() != 1
        || payload.metadata[0].key != "encoding"
        || payload.metadata[0].value != b"json/plain"
    {
        return Err(context.invalid(api::ErrorCode::InvalidField));
    }
    let json = std::str::from_utf8(&payload.data)
        .map_err(|_| context.error(api::ErrorKind::Encoding, api::ErrorCode::InvalidField))?;
    let mut decoder = serde_json::Deserializer::from_str(json);
    IgnoredAny::deserialize(&mut decoder)
        .map_err(|_| context.error(api::ErrorKind::Encoding, api::ErrorCode::InvalidField))?;
    decoder
        .end()
        .map_err(|_| context.error(api::ErrorKind::Encoding, api::ErrorCode::InvalidField))
}

fn validate_metadata_key(value: &str, context: Context, response: bool) -> Result<(), api::Error> {
    if value.len() > 256 {
        return Err(context.limit());
    }
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(if response {
            context.malformed()
        } else {
            context.invalid(api::ErrorCode::InvalidField)
        });
    }
    Ok(())
}

fn validate_output_string(value: &str, context: Context) -> Result<(), api::Error> {
    if value.len() > 4096 || value.chars().count() > 1024 {
        Err(context.limit())
    } else {
        Ok(())
    }
}

fn workflow_status(number: i32) -> api::ProtoEnum {
    api::ProtoEnum {
        number,
        label: enums::WorkflowExecutionStatus::try_from(number)
            .ok()
            .map(|value| value.as_str_name().to_owned()),
    }
}

fn invoke<M: Message>(
    exchange: &mut impl Exchange,
    profile: String,
    rpc: &str,
    timeout_millis: u64,
    message: &M,
    context: Context,
) -> Result<(Vec<u8>, Context), api::Error> {
    if message.encoded_len() > MESSAGE_BYTES {
        return Err(context.limit());
    }
    let response = exchange
        .exchange(host::Call {
            profile,
            rpc: rpc.into(),
            message: message.encode_to_vec(),
            timeout_millis,
            max_response_bytes: MESSAGE_BYTES as u64,
        })
        .map_err(|failure| {
            context
                .after_send(failure.sent)
                .error(api::ErrorKind::Infrastructure, api::ErrorCode::HostFailure)
        })?;
    let mut context = context.after_send(response.sent);
    if response.status != 0 {
        let (class, name) = grpc_status(response.status);
        if response
            .status_details_bin
            .as_ref()
            .is_some_and(|details| details.len() > PAGE_TOKEN_BYTES)
        {
            return Err(context.limit());
        }
        if let Some(message) = &response.grpc_message {
            validate_output_string(message, context)?;
        }
        let mut error = context.error(api::ErrorKind::ServerStatus, api::ErrorCode::ServerStatus);
        error.server = Some(api::ServerStatus {
            code: response.status,
            class,
            name: name.map(str::to_owned),
            message: response.grpc_message,
            details: response.status_details_bin.unwrap_or_default(),
        });
        return Err(error);
    }
    if matches!(context.operation, api::Operation::StartWorkflowExecution) {
        context.effect = api::MutationEffect::Applied;
    }
    let bytes = response.message.ok_or_else(|| context.malformed())?;
    if bytes.len() > MESSAGE_BYTES {
        return Err(context.limit());
    }
    Ok((bytes, context))
}

const fn grpc_status(code: u32) -> (api::ServerClass, Option<&'static str>) {
    use api::ServerClass as Class;
    match code {
        1 => (Class::Cancelled, Some("CANCELLED")),
        2 => (Class::Server, Some("UNKNOWN")),
        3 => (Class::Invalid, Some("INVALID_ARGUMENT")),
        4 => (Class::Deadline, Some("DEADLINE_EXCEEDED")),
        5 => (Class::NotFound, Some("NOT_FOUND")),
        6 => (Class::Conflict, Some("ALREADY_EXISTS")),
        7 => (Class::Authorization, Some("PERMISSION_DENIED")),
        8 => (Class::Exhausted, Some("RESOURCE_EXHAUSTED")),
        9 => (Class::FailedPrecondition, Some("FAILED_PRECONDITION")),
        10 => (Class::Aborted, Some("ABORTED")),
        11 => (Class::Unknown, Some("OUT_OF_RANGE")),
        12 => (Class::Unsupported, Some("UNIMPLEMENTED")),
        13 => (Class::Server, Some("INTERNAL")),
        14 => (Class::Unavailable, Some("UNAVAILABLE")),
        15 => (Class::Server, Some("DATA_LOSS")),
        16 => (Class::Authentication, Some("UNAUTHENTICATED")),
        _ => (Class::Unknown, None),
    }
}

fn project_event(
    event: history::HistoryEvent,
    context: Context,
) -> Result<api::HistoryEvent, api::Error> {
    use history::history_event::Attributes;
    let event_time = event.event_time.ok_or_else(|| context.malformed())?;
    if !(0..=999_999_999).contains(&event_time.nanos) {
        return Err(context.malformed());
    }
    let known_type = enums::EventType::try_from(event.event_type).ok();
    let details = match (known_type, event.attributes) {
        (
            Some(enums::EventType::WorkflowExecutionCompleted),
            Some(Attributes::WorkflowExecutionCompletedEventAttributes(attributes)),
        ) => {
            let payloads = attributes
                .result
                .unwrap_or_default()
                .payloads
                .into_iter()
                .map(|payload| project_payload(payload, context))
                .collect::<Result<_, _>>()?;
            api::EventDetails::WorkflowCompleted(payloads)
        }
        (
            Some(enums::EventType::WorkflowExecutionFailed),
            Some(Attributes::WorkflowExecutionFailedEventAttributes(attributes)),
        ) => {
            let failure = attributes.failure.ok_or_else(|| context.malformed())?;
            api::EventDetails::WorkflowFailed(project_failure(failure, context)?)
        }
        (
            Some(enums::EventType::ActivityTaskScheduled),
            Some(Attributes::ActivityTaskScheduledEventAttributes(attributes)),
        ) => {
            let activity = attributes
                .activity_type
                .ok_or_else(|| context.malformed())?;
            validate_output_string(&activity.name, context)?;
            if activity.name.is_empty() {
                return Err(context.malformed());
            }
            api::EventDetails::ActivityScheduled(activity.name)
        }
        (
            Some(
                enums::EventType::WorkflowExecutionCompleted
                | enums::EventType::WorkflowExecutionFailed
                | enums::EventType::ActivityTaskScheduled,
            ),
            _,
        ) => return Err(context.malformed()),
        (
            Some(_),
            Some(
                Attributes::WorkflowExecutionCompletedEventAttributes(_)
                | Attributes::WorkflowExecutionFailedEventAttributes(_)
                | Attributes::ActivityTaskScheduledEventAttributes(_),
            ),
        ) => {
            return Err(context.malformed());
        }
        _ => api::EventDetails::Other,
    };
    Ok(api::HistoryEvent {
        event_id: event.event_id,
        event_time: api::Timestamp {
            seconds: event_time.seconds,
            nanos: event_time.nanos,
        },
        task_id: event.task_id,
        event_type: api::ProtoEnum {
            number: event.event_type,
            label: known_type.map(|value| value.as_str_name().to_owned()),
        },
        details,
    })
}

fn project_payload(payload: common::Payload, context: Context) -> Result<api::Payload, api::Error> {
    if !payload.external_payloads.is_empty() {
        return Err(context.error(
            api::ErrorKind::Unsupported,
            api::ErrorCode::UnsupportedResponse,
        ));
    }
    if payload.data.len() > PAYLOAD_BYTES || payload.metadata.len() > METADATA_ENTRIES {
        return Err(context.limit());
    }
    let mut bytes = 0usize;
    for (key, value) in &payload.metadata {
        validate_metadata_key(key, context, true)?;
        bytes = bytes
            .checked_add(key.len())
            .and_then(|sum| sum.checked_add(value.len()))
            .filter(|sum| *sum <= METADATA_BYTES)
            .ok_or_else(|| context.limit())?;
    }
    // The generated map is a BTreeMap. The preflight rejects duplicate wire keys.
    let metadata = payload
        .metadata
        .into_iter()
        .map(|(key, value)| api::MetadataEntry { key, value })
        .collect();
    Ok(api::Payload {
        metadata,
        data: payload.data,
    })
}

fn project_failure(
    failure: failure::Failure,
    context: Context,
) -> Result<Vec<api::FailureNode>, api::Error> {
    let mut nodes = Vec::new();
    let mut next = Some(failure);
    while let Some(failure) = next {
        if nodes.len() == FAILURE_NODES {
            return Err(context.limit());
        }
        validate_output_string(&failure.message, context)?;
        validate_output_string(&failure.source, context)?;
        let activity_type = match failure.failure_info {
            Some(failure::failure::FailureInfo::ActivityFailureInfo(info)) => {
                info.activity_type.map(|activity| activity.name)
            }
            _ => None,
        };
        if let Some(name) = &activity_type {
            validate_output_string(name, context)?;
        }
        nodes.push(api::FailureNode {
            message: failure.message,
            source: (!failure.source.is_empty()).then_some(failure.source),
            activity_type,
        });
        next = failure.cause.map(|cause| *cause);
    }
    Ok(nodes)
}

// Protobuf permits singular message fields to occur more than once and merges
// them. Preserve preflight state across those occurrences, so split History or
// Payload messages cannot reset their event or metadata limits.
#[derive(Default)]
struct ScanState<'a> {
    singular: BTreeMap<u32, Self>,
    metadata_keys: BTreeSet<&'a str>,
    metadata_bytes: usize,
    events: usize,
}

fn preflight(bytes: &[u8], schema: usize, context: Context) -> Result<(), api::Error> {
    // Field ceilings precede prost allocations. This is not a replacement for
    // Wasm memory/fuel accounting: repeated unprojected message fields and
    // completion payload counts have only the enclosing 4 MiB wire ceiling.
    scan_message(bytes, schema, &mut ScanState::default(), 0, 0, context)
}

fn scan_message<'a>(
    mut bytes: &'a [u8],
    schema_index: usize,
    state: &mut ScanState<'a>,
    depth: usize,
    failure_depth: usize,
    context: Context,
) -> Result<(), api::Error> {
    use crate::proto::wire::{Kind, MESSAGE_SCHEMA};
    use prost::encoding::{DecodeContext, WireType, decode_key, skip_field};

    if depth >= 100 {
        return Err(context.malformed());
    }
    let schema = &MESSAGE_SCHEMA[schema_index];
    let is_payload = schema.name == "temporal.api.common.v1.Payload";
    let is_failure = schema.name == "temporal.api.failure.v1.Failure";
    let failure_depth = if is_failure { failure_depth + 1 } else { 0 };
    if failure_depth > FAILURE_NODES {
        return Err(context.limit());
    }
    while !bytes.is_empty() {
        let (number, wire) = decode_key(&mut bytes).map_err(|_| context.malformed())?;
        let Some(field) = schema.fields.iter().find(|field| field.number == number) else {
            skip_field(wire, number, &mut bytes, DecodeContext::default())
                .map_err(|_| context.malformed())?;
            continue;
        };
        if schema.name == "temporal.api.history.v1.History" && field.name == "events" {
            state.events = state
                .events
                .checked_add(1)
                .filter(|count| *count <= HISTORY_EVENTS)
                .ok_or_else(|| context.limit())?;
        }
        if schema.name == "temporal.api.workflowservice.v1.GetWorkflowExecutionHistoryResponse"
            && field.name == "raw_history"
        {
            return Err(context.error(
                api::ErrorKind::Unsupported,
                api::ErrorCode::UnsupportedResponse,
            ));
        }
        match field.kind {
            Kind::Message(child) => {
                if wire != WireType::LengthDelimited {
                    return Err(context.malformed());
                }
                let value = take_delimited(&mut bytes, context)?;
                if is_payload && field.name == "metadata" {
                    let (key, value) = scan_metadata(value, child, context)?;
                    if state.metadata_keys.len() == METADATA_ENTRIES {
                        return Err(context.limit());
                    }
                    state.metadata_bytes = state
                        .metadata_bytes
                        .checked_add(key.len())
                        .and_then(|sum| sum.checked_add(value.len()))
                        .filter(|sum| *sum <= METADATA_BYTES)
                        .ok_or_else(|| context.limit())?;
                    if !state.metadata_keys.insert(key) {
                        return Err(context.error(
                            api::ErrorKind::Protocol,
                            api::ErrorCode::DuplicateMetadataKey,
                        ));
                    }
                } else if field.repeated {
                    scan_message(
                        value,
                        child,
                        &mut ScanState::default(),
                        depth + 1,
                        failure_depth,
                        context,
                    )?;
                } else {
                    scan_message(
                        value,
                        child,
                        state.singular.entry(number).or_default(),
                        depth + 1,
                        failure_depth,
                        context,
                    )?;
                }
            }
            Kind::String | Kind::Bytes => {
                if wire != WireType::LengthDelimited {
                    return Err(context.malformed());
                }
                let value = take_delimited(&mut bytes, context)?;
                scan_bytes(schema.name, field, value, context)?;
            }
            Kind::Varint | Kind::Fixed64 | Kind::Fixed32 => {
                skip_field(wire, number, &mut bytes, DecodeContext::default())
                    .map_err(|_| context.malformed())?;
            }
        }
    }
    Ok(())
}

fn scan_bytes(
    schema: &str,
    field: &crate::proto::wire::Field,
    value: &[u8],
    context: Context,
) -> Result<(), api::Error> {
    if (schema == "temporal.api.common.v1.Payload"
        && field.name == "data"
        && value.len() > PAYLOAD_BYTES)
        || (field.name == "next_page_token" && value.len() > PAGE_TOKEN_BYTES)
    {
        return Err(context.limit());
    }
    if field.kind == crate::proto::wire::Kind::String {
        let text = std::str::from_utf8(value).map_err(|_| context.malformed())?;
        let exported = (schema == "temporal.api.failure.v1.Failure"
            && matches!(field.name, "message" | "source"))
            || (schema == "temporal.api.common.v1.ActivityType" && field.name == "name")
            || (matches!(
                schema,
                "temporal.api.common.v1.WorkflowExecution"
                    | "temporal.api.workflowservice.v1.StartWorkflowExecutionResponse"
            ) && field.name == "run_id");
        if exported {
            validate_output_string(text, context)?;
        }
    }
    Ok(())
}

fn take_delimited<'a>(bytes: &mut &'a [u8], context: Context) -> Result<&'a [u8], api::Error> {
    let length = prost::encoding::decode_varint(bytes).map_err(|_| context.malformed())?;
    let length = usize::try_from(length).map_err(|_| context.limit())?;
    let value = bytes.get(..length).ok_or_else(|| context.malformed())?;
    *bytes = &bytes[length..];
    Ok(value)
}

fn scan_metadata(
    mut bytes: &[u8],
    schema_index: usize,
    context: Context,
) -> Result<(&str, &[u8]), api::Error> {
    use prost::encoding::{DecodeContext, WireType, decode_key, skip_field};
    let schema = &crate::proto::wire::MESSAGE_SCHEMA[schema_index];
    let mut key = "";
    let mut value = &[][..];
    while !bytes.is_empty() {
        let (number, wire) = decode_key(&mut bytes).map_err(|_| context.malformed())?;
        let field = schema.fields.iter().find(|field| field.number == number);
        match field.map(|field| field.name) {
            Some("key") => {
                if wire != WireType::LengthDelimited {
                    return Err(context.malformed());
                }
                key = std::str::from_utf8(take_delimited(&mut bytes, context)?)
                    .map_err(|_| context.malformed())?;
                validate_metadata_key(key, context, true)?;
            }
            Some("value") => {
                if wire != WireType::LengthDelimited {
                    return Err(context.malformed());
                }
                value = take_delimited(&mut bytes, context)?;
                if value.len() > METADATA_BYTES {
                    return Err(context.limit());
                }
            }
            _ => skip_field(wire, number, &mut bytes, DecodeContext::default())
                .map_err(|_| context.malformed())?,
        }
    }
    validate_metadata_key(key, context, true)?;
    Ok((key, value))
}
