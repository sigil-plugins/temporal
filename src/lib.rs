#![deny(unsafe_code)]

//! The measured three-operation Temporal component.
//! Routing, credentials and transport authority stay in the Sigil host.

#[allow(unsafe_code, clippy::all, clippy::nursery, clippy::pedantic)]
pub mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "plugin",
        generate_all,
    });
}

pub mod client;
pub mod proto;

#[cfg(target_arch = "wasm32")]
struct Component;

#[cfg(target_arch = "wasm32")]
struct SemanticHost;

#[cfg(target_arch = "wasm32")]
impl client::Exchange for SemanticHost {
    fn exchange(
        &mut self,
        request: client::host::Call,
    ) -> Result<client::host::Response, client::host::Failure> {
        client::host::exchange(&request)
    }
}

#[cfg(target_arch = "wasm32")]
impl client::api::Guest for Component {
    fn start_workflow_execution(
        request: client::api::StartRequest,
    ) -> Result<client::api::StartResponse, client::api::Error> {
        client::start_workflow_execution(&mut SemanticHost, request)
    }

    fn describe_workflow_execution(
        request: client::api::DescribeRequest,
    ) -> Result<client::api::DescribeResponse, client::api::Error> {
        client::describe_workflow_execution(&mut SemanticHost, request)
    }

    fn get_workflow_execution_history(
        request: client::api::HistoryRequest,
    ) -> Result<client::api::HistoryPage, client::api::Error> {
        client::get_workflow_execution_history(&mut SemanticHost, request)
    }
}

#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code, clippy::all, clippy::nursery, clippy::pedantic)]
mod component_exports {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
