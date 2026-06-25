//! Stable bridge-facing API surface.
//!
//! This crate intentionally has no native transport dependency. napi-rs, CLI,
//! and test transports live in sibling crates.

mod brain_stream;
mod buffers;

pub use brain_stream::{
    brain_wake_stream_channel, BrainWakeStream, BrainWakeStreamProducer, BrainWakeStreamSender,
};
pub use buffers::{
    BrainWakeBufferInput, BufferedBrainWakeRequest, RuntimeBufferLease, RuntimeBufferStore,
    APPLICATION_JSON, TEXT_PLAIN,
};
pub use rusty_crew_core_protocol::*;

pub const MANIFEST_VERSION: u32 = 1;
pub const MANIFEST_TEXT: &str = include_str!("../bridge-manifest.toml");
pub const OPERATION_NAMES: &[&str] = &[
    "initialize_engine",
    "shutdown_engine",
    "register_brain_implementation",
    "replace_brain_implementation",
    "wake_brain",
    "submit_brain_event",
    "submit_brain_actions",
    "register_platform_adapter",
    "validate_runtime_config_draft",
    "plan_runtime_config",
    "plan_create_profile",
    "inject_external_event",
    "inject_den_data_update",
    "enqueue_body_follow_up_message",
    "archive_session",
    "ensure_configured_session",
    "register_scheduled_wake_job",
    "register_scheduled_host_job",
    "list_scheduled_jobs",
    "list_scheduled_runs",
    "claim_scheduled_host_runs",
    "request_scheduled_host_job_run",
    "complete_scheduled_host_run",
    "run_scheduler_tick",
    "request_scheduled_job_run",
    "pause_scheduled_job",
    "resume_scheduled_job",
    "cancel_delegated_session",
    "request_delegated_checkpoint",
    "drain_delegated_sessions",
    "cleanup_delegated_resources",
    "delegated_session_status",
    "list_sessions",
    "provider_state_diagnostics",
    "database_size",
    "run_maintenance",
    "subscribe_events",
    "unsubscribe_events",
    "get_buffer",
    "release_buffer",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeManifestSummary {
    pub version: u32,
    pub owning_crate: &'static str,
    pub native_package: &'static str,
    pub operation_names: &'static [&'static str],
}

pub fn manifest_summary() -> BridgeManifestSummary {
    BridgeManifestSummary {
        version: MANIFEST_VERSION,
        owning_crate: "rusty-crew-core-bridge-api",
        native_package: "@rusty-crew/native-bridge",
        operation_names: OPERATION_NAMES,
    }
}
