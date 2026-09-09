//! Benign, offline functional checks against an actual compiled component.
#![forbid(unsafe_code)]

use std::{collections::VecDeque, path::Path};

use anyhow::{Context, Result, bail, ensure};
use wasmtime::{
    Config, Engine, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, HasSelf, Linker},
};

mod bindings {
    wasmtime::component::bindgen!({
        path: "../../wit",
        world: "sigil:temporal/plugin@0.1.0",
        imports: { default: trappable },
    });
}

use bindings::{Plugin, exports::sigil::temporal::client as api, sigil::host::grpc_unary as host};

macro_rules! request_fixture {
    ($name:literal) => {
        include_bytes!(concat!("../../../conformance/requests/", $name, ".pb"))
    };
}
macro_rules! response_fixture {
    ($name:literal) => {
        include_bytes!(concat!("../../../conformance/responses/", $name, ".pb"))
    };
}

const PROFILE: &str = "temporal-public-fixture";
const NAMESPACE: &str = "local-data-execution-gcp.hgzph";
const RUN_ID: &str = "11111111-2222-3333-4444-555555555555";
// Frozen aliases only: the real host owns full RPC path selection.
const START: &str = "start";
const DESCRIBE: &str = "describe";
const HISTORY: &str = "history";
const FUEL: u64 = 100_000_000;
const MEMORY_BYTES: usize = 64 * 1024 * 1024;

#[derive(Default)]
struct Mock {
    replies: VecDeque<host::Response>,
    calls: Vec<host::Call>,
    limits: StoreLimits,
}

impl host::Host for Mock {
    fn exchange(
        &mut self,
        request: host::Call,
    ) -> wasmtime::Result<Result<host::Response, host::Failure>> {
        self.calls.push(request);
        let response = self.replies.pop_front().ok_or_else(|| {
            wasmtime::Error::msg("unexpected host call: retry or implicit pagination")
        })?;
        Ok(Ok(response))
    }
}

struct Harness {
    store: Store<Mock>,
    plugin: Plugin,
    checked: usize,
}

impl Harness {
    fn new(path: &Path) -> Result<Self> {
        ensure!(
            path.is_file(),
            "component artifact absent: {} (build an actual component first)",
            path.display()
        );
        let bytes =
            std::fs::read(path).with_context(|| format!("read component {}", path.display()))?;
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.consume_fuel(true);
        let engine = Engine::new(&config)?;
        let component = Component::new(&engine, bytes).map_err(|error| {
            error.context("compile supplied component (not a core Wasm module)")
        })?;
        let mut linker = Linker::new(&engine);
        Plugin::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)?;
        let mut store = Store::new(
            &engine,
            Mock {
                limits: StoreLimitsBuilder::new().memory_size(MEMORY_BYTES).build(),
                ..Mock::default()
            },
        );
        store.limiter(|state| &mut state.limits);
        store.set_fuel(FUEL)?;
        let plugin = Plugin::instantiate(&mut store, &component, &linker).map_err(|error| {
            error.context("instantiate exact Temporal WIT; no WASI or other imports supplied")
        })?;
        ensure!(
            store.data().calls.is_empty(),
            "component called host during instantiation"
        );
        Ok(Self {
            store,
            plugin,
            checked: 0,
        })
    }

    fn queue(&mut self, response: host::Response) -> Result<()> {
        ensure!(
            self.store.data().replies.is_empty() && self.store.data().calls.is_empty(),
            "previous exchange not checked"
        );
        self.store.data_mut().replies.push_back(response);
        Ok(())
    }

    fn checked_call(
        &mut self,
        label: &str,
        rpc: &str,
        timeout: u64,
        expected: &[u8],
    ) -> Result<()> {
        let state = self.store.data_mut();
        ensure!(state.replies.is_empty(), "{label}: response not consumed");
        ensure!(
            state.calls.len() == 1,
            "{label}: expected exactly one host call, got {}",
            state.calls.len()
        );
        let call = state.calls.pop().context("recorded call")?;
        ensure!(call.profile == PROFILE, "{label}: profile changed");
        ensure!(call.rpc == rpc, "{label}: RPC changed");
        ensure!(
            call.timeout_millis == timeout,
            "{label}: timeout changed: {}",
            call.timeout_millis
        );
        ensure!(
            call.max_response_bytes == 4_194_304,
            "{label}: response limit changed"
        );
        ensure!(
            call.message == expected,
            "{label}: request differs from independent fixture oracle"
        );
        self.checked += 1;
        println!("ok {}: {label}", self.checked);
        Ok(())
    }
}

fn success(bytes: &[u8]) -> host::Response {
    host::Response {
        status: 0,
        sent: host::SendState::MessageSent,
        message: Some(bytes.to_vec()),
        grpc_message: None,
        status_details_bin: None,
        initial_metadata: vec![],
        trailing_metadata: vec![],
    }
}

fn server_status(status: u32) -> host::Response {
    host::Response {
        status,
        sent: host::SendState::MessageSent,
        message: None,
        grpc_message: Some("public synthetic server status".into()),
        status_details_bin: Some(b"\0public\xff".to_vec()),
        initial_metadata: vec![],
        trailing_metadata: vec![],
    }
}

fn start_request() -> api::StartRequest {
    api::StartRequest {
        profile: PROFILE.into(),
        namespace: NAMESPACE.into(),
        workflow_id: "capi-m5-00000000-x1".into(),
        workflow_type: "WorkflowForChildWorkflow".into(),
        task_queue: "task-queue-for-child-workflow".into(),
        payloads: [
            br#"{"eventFeedViewAssetID":"synthetic"}"#.as_slice(),
            br#"{"actionID":"synthetic","resourceID":"synthetic"}"#.as_slice(),
        ]
        .into_iter()
        .map(|data| api::Payload {
            metadata: vec![api::MetadataEntry {
                key: "encoding".into(),
                value: b"json/plain".to_vec(),
            }],
            data: data.to_vec(),
        })
        .collect(),
        request_id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into(),
        timeout_millis: 10_000,
    }
}

fn describe_request() -> api::DescribeRequest {
    api::DescribeRequest {
        profile: PROFILE.into(),
        namespace: NAMESPACE.into(),
        workflow_id: "capi-m5-00000000-x1".into(),
        timeout_millis: 10_000,
    }
}

fn history_request(close: bool, token: Vec<u8>) -> api::HistoryRequest {
    api::HistoryRequest {
        profile: PROFILE.into(),
        namespace: NAMESPACE.into(),
        workflow_id: "capi-m5-00000000-r1".into(),
        wait_new_event: close,
        filter: if close {
            api::HistoryFilter::CloseEvent
        } else {
            api::HistoryFilter::AllEvents
        },
        skip_archival: close,
        next_page_token: token,
        timeout_millis: if close { 65_000 } else { 10_000 },
    }
}

// Derive only the token variant from an independent protoc fixture. No field
// numbers or plugin encoder are used: the unique known length+token is replaced,
// retaining every other byte. All tokens used here fit one-byte lengths.
fn history_oracle(token: &[u8]) -> Result<Vec<u8>> {
    let original = request_fixture!("history-all-events-page-token");
    let old = b"synthetic-page-token";
    let positions: Vec<_> = original
        .windows(old.len())
        .enumerate()
        .filter_map(|(i, part)| (part == old).then_some(i))
        .collect();
    ensure!(
        positions.len() == 1,
        "independent history token must be unique"
    );
    let start = positions[0].checked_sub(1).context("token length prefix")?;
    ensure!(
        original[start] as usize == old.len() && token.len() < 128,
        "one-byte fixture token length required"
    );
    let mut expected = original.to_vec();
    if token.is_empty() {
        // Omitted proto3 bytes field: remove its existing one-byte key too.
        // The key is obtained from the fixture, never from a handwritten tag.
        ensure!(
            start > 0 && original[start - 1] & 7 == 2 && original[start - 1] < 128,
            "one-byte bytes-field key required"
        );
        expected.drain(start - 1..start + 1 + old.len());
    } else {
        expected.splice(
            start..start + 1 + old.len(),
            std::iter::once(token.len() as u8).chain(token.iter().copied()),
        );
    }
    Ok(expected)
}

fn check_enum(value: &api::ProtoEnum, number: i32, label: Option<&str>) -> Result<()> {
    ensure!(
        value.number == number && value.label.as_deref() == label,
        "enum number/label changed: {value:?}"
    );
    Ok(())
}

fn run(path: &Path) -> Result<()> {
    let mut h = Harness::new(path)?;
    for (name, bytes, started, status, label) in [
        (
            "start/defaults",
            response_fixture!("start-success").as_slice(),
            true,
            1,
            Some("WORKFLOW_EXECUTION_STATUS_RUNNING"),
        ),
        (
            "start/existing",
            response_fixture!("start-existing").as_slice(),
            false,
            2,
            Some("WORKFLOW_EXECUTION_STATUS_COMPLETED"),
        ),
        (
            "start/future-enum",
            response_fixture!("start-future-status").as_slice(),
            true,
            31415,
            None,
        ),
    ] {
        h.queue(success(bytes))?;
        let result = h
            .plugin
            .sigil_temporal_client()
            .call_start_workflow_execution(&mut h.store, &start_request())?
            .map_err(|error| anyhow::anyhow!("{name}: {error:?}"))?;
        ensure!(
            result.run_id == RUN_ID
                && result.started == started
                && result.effect == api::MutationEffect::Applied,
            "{name}: start result/effect changed"
        );
        check_enum(&result.status, status, label)?;
        h.checked_call(
            name,
            START,
            10_000,
            request_fixture!("start-workflow-execution"),
        )?;
    }
    for (name, bytes, number, label) in [
        (
            "describe/latest-running",
            response_fixture!("describe-running").as_slice(),
            1,
            Some("WORKFLOW_EXECUTION_STATUS_RUNNING"),
        ),
        (
            "describe/latest-completed",
            response_fixture!("describe-completed").as_slice(),
            2,
            Some("WORKFLOW_EXECUTION_STATUS_COMPLETED"),
        ),
        (
            "describe/future-enum",
            response_fixture!("describe-future-status").as_slice(),
            31415,
            None,
        ),
    ] {
        h.queue(success(bytes))?;
        let result = h
            .plugin
            .sigil_temporal_client()
            .call_describe_workflow_execution(&mut h.store, &describe_request())?
            .map_err(|error| anyhow::anyhow!("{name}: {error:?}"))?;
        ensure!(
            result.run_id == RUN_ID,
            "{name}: nested execution run ID changed"
        );
        check_enum(&result.status, number, label)?;
        h.checked_call(
            name,
            DESCRIBE,
            10_000,
            request_fixture!("describe-workflow-execution"),
        )?;
    }

    h.queue(success(response_fixture!("history-completed")))?;
    let page = h
        .plugin
        .sigil_temporal_client()
        .call_get_workflow_execution_history(&mut h.store, &history_request(true, vec![]))?
        .map_err(|error| anyhow::anyhow!("close history: {error:?}"))?;
    ensure!(
        page.events.len() == 1 && page.next_page_token.is_empty(),
        "completion page shape changed"
    );
    let event = &page.events[0];
    ensure!(
        event.event_id == 9_007_199_254_740_993
            && event.task_id == i64::MAX
            && event.event_time.seconds == 1_700_000_000
            && event.event_time.nanos == 123_456_789,
        "completion int64/nanos changed"
    );
    check_enum(
        &event.event_type,
        2,
        Some("EVENT_TYPE_WORKFLOW_EXECUTION_COMPLETED"),
    )?;
    let api::EventDetails::WorkflowCompleted(payloads) = &event.details else {
        bail!("completion not typed");
    };
    ensure!(payloads.len() == 2, "completion payload count changed");
    for (payload, data, encoding, kind) in [
        (
            &payloads[0],
            br#"{"answer":42}"#.as_slice(),
            b"json/plain".as_slice(),
            b"SyntheticResult".as_slice(),
        ),
        (
            &payloads[1],
            b"\0\xff\x80\n\r\"\\".as_slice(),
            b"binary/plain".as_slice(),
            b"\0\xff\x80".as_slice(),
        ),
    ] {
        ensure!(
            payload.data == data && payload.metadata.len() == 2,
            "payload bytes/metadata count changed"
        );
        ensure!(
            payload.metadata[0].key == "encoding"
                && payload.metadata[0].value == encoding
                && payload.metadata[1].key == "type"
                && payload.metadata[1].value == kind,
            "sorted metadata exact bytes changed"
        );
    }
    h.checked_call(
        "history/typed-completion-exact-values",
        HISTORY,
        65_000,
        request_fixture!("history-close-event"),
    )?;

    h.queue(success(response_fixture!("history-activities-page-one")))?;
    let page = h
        .plugin
        .sigil_temporal_client()
        .call_get_workflow_execution_history(&mut h.store, &history_request(false, vec![]))?
        .map_err(|error| anyhow::anyhow!("activity page: {error:?}"))?;
    ensure!(
        page.events.len() == 2 && page.next_page_token == b"\0next\xff",
        "activity page/token changed"
    );
    for (event, name) in page.events.iter().zip(["FirstActivity", "SecondActivity"]) {
        ensure!(
            matches!(&event.details, api::EventDetails::ActivityScheduled(actual) if actual == name),
            "activity order/type changed"
        );
        check_enum(
            &event.event_type,
            10,
            Some("EVENT_TYPE_ACTIVITY_TASK_SCHEDULED"),
        )?;
    }
    h.checked_call(
        "history/activities-no-implicit-pagination",
        HISTORY,
        10_000,
        &history_oracle(&[])?,
    )?;
    let token = page.next_page_token;
    h.queue(success(response_fixture!("history-future-event-page-two")))?;
    let page = h
        .plugin
        .sigil_temporal_client()
        .call_get_workflow_execution_history(&mut h.store, &history_request(false, token.clone()))?
        .map_err(|error| anyhow::anyhow!("explicit next page: {error:?}"))?;
    ensure!(
        page.events.len() == 1 && page.next_page_token.is_empty(),
        "future page shape changed"
    );
    let event = &page.events[0];
    ensure!(
        event.event_id == -9_007_199_254_740_993
            && event.task_id == i64::MIN
            && event.event_time.seconds == -62_135_596_800
            && event.event_time.nanos == 999_999_999,
        "signed int64/time values changed"
    );
    check_enum(&event.event_type, 31415, None)?;
    ensure!(
        matches!(event.details, api::EventDetails::Other),
        "future event not Other"
    );
    h.checked_call(
        "history/explicit-binary-token-future-enum",
        HISTORY,
        10_000,
        &history_oracle(&token)?,
    )?;

    h.queue(success(response_fixture!("history-failed")))?;
    let page = h
        .plugin
        .sigil_temporal_client()
        .call_get_workflow_execution_history(
            &mut h.store,
            &history_request(false, b"synthetic-page-token".to_vec()),
        )?
        .map_err(|error| anyhow::anyhow!("failure page: {error:?}"))?;
    ensure!(page.events.len() == 1, "failure page shape changed");
    let api::EventDetails::WorkflowFailed(nodes) = &page.events[0].details else {
        bail!("failure not typed");
    };
    ensure!(
        nodes.len() == 2
            && nodes[0].message == "synthetic activity failure"
            && nodes[0].source.as_deref() == Some("GoSDK")
            && nodes[0].activity_type.as_deref() == Some("SyntheticActivity")
            && nodes[1].message == "synthetic root cause"
            && nodes[1].source.as_deref() == Some("JavaSDK")
            && nodes[1].activity_type.is_none(),
        "failure chain values/order changed"
    );
    h.checked_call(
        "history/typed-failure-independent-all-events",
        HISTORY,
        10_000,
        request_fixture!("history-all-events-page-token"),
    )?;

    h.queue(server_status(6))?;
    let error = h
        .plugin
        .sigil_temporal_client()
        .call_start_workflow_execution(&mut h.store, &start_request())?
        .expect_err("nonzero Start status must not succeed");
    check_error(
        &error,
        api::Operation::StartWorkflowExecution,
        api::MutationEffect::Unknown,
        6,
        api::ServerClass::Conflict,
        "ALREADY_EXISTS",
    )?;
    h.checked_call(
        "start/server-status-unknown-mutation-effect",
        START,
        10_000,
        request_fixture!("start-workflow-execution"),
    )?;
    h.queue(server_status(5))?;
    let error = h
        .plugin
        .sigil_temporal_client()
        .call_describe_workflow_execution(&mut h.store, &describe_request())?
        .expect_err("nonzero Describe status must not succeed");
    check_error(
        &error,
        api::Operation::DescribeWorkflowExecution,
        api::MutationEffect::NotApplicable,
        5,
        api::ServerClass::NotFound,
        "NOT_FOUND",
    )?;
    h.checked_call(
        "describe/server-status-read-effect",
        DESCRIBE,
        10_000,
        request_fixture!("describe-workflow-execution"),
    )?;
    ensure!(h.checked == 12, "scenario count changed");
    println!(
        "bounds: fuel_consumed={} fuel_limit={FUEL} memory_limit_bytes={MEMORY_BYTES}",
        FUEL - h.store.get_fuel()?
    );
    println!(
        "PASS: {} actual component export calls; exact WIT + one in-process host exchange each; artifact={}",
        h.checked,
        path.display()
    );
    Ok(())
}

fn check_error(
    error: &api::Error,
    operation: api::Operation,
    effect: api::MutationEffect,
    code: u32,
    class: api::ServerClass,
    name: &str,
) -> Result<()> {
    ensure!(
        error.kind == api::ErrorKind::ServerStatus
            && error.code == api::ErrorCode::ServerStatus
            && error.operation == operation
            && error.effect == effect,
        "server error classification/effect changed: {error:?}"
    );
    let server = error.server.as_ref().context("server status missing")?;
    ensure!(
        server.code == code
            && server.class == class
            && server.name.as_deref() == Some(name)
            && server.message.as_deref() == Some("public synthetic server status")
            && server.details == b"\0public\xff",
        "server status fields changed"
    );
    Ok(())
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .context("usage: temporal-component-conformance <actual-component.wasm>")?;
    ensure!(args.next().is_none(), "expected exactly one component path");
    run(Path::new(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_token_oracle_preserves_original_fixture() -> Result<()> {
        ensure!(
            history_oracle(b"synthetic-page-token")?
                == request_fixture!("history-all-events-page-token")
        );
        Ok(())
    }

    #[test]
    fn derived_token_variants_keep_fixture_prefix_and_suffix() -> Result<()> {
        let original = request_fixture!("history-all-events-page-token");
        let empty = history_oracle(&[])?;
        let binary = history_oracle(b"\0next\xff")?;
        ensure!(empty.len() + b"synthetic-page-token".len() + 2 == original.len());
        ensure!(binary.len() + b"synthetic-page-token".len() == original.len() + 6);
        let shared_prefix = original
            .iter()
            .zip(&empty)
            .take_while(|(a, b)| a == b)
            .count();
        ensure!(shared_prefix > NAMESPACE.len());
        ensure!(binary.starts_with(&empty[..shared_prefix]));
        ensure!(binary.ends_with(&empty[shared_prefix..]));
        ensure!(binary.windows(6).any(|part| part == b"\0next\xff"));
        Ok(())
    }
}
