//! Stable bridge-facing API surface.
//!
//! This crate intentionally has no native transport dependency. napi-rs, CLI,
//! and test transports live in sibling crates.

mod brain_stream;
mod buffers;
mod scheduler_wire;

pub use brain_stream::{
    brain_wake_stream_channel, BrainWakeStream, BrainWakeStreamProducer, BrainWakeStreamSender,
};
pub use buffers::{
    BrainWakeBufferInput, BufferedBrainWakeRequest, RuntimeBufferLease, RuntimeBufferStore,
    APPLICATION_JSON, TEXT_PLAIN,
};
pub use rusty_crew_core_config::{ClockConfig, EngineConfig, EngineStorageConfig};
pub use rusty_crew_core_protocol::*;
pub use scheduler_wire::{ScheduledJobWireOutput, ScheduledRunWireOutput, SchedulerTickWireOutput};

pub const MANIFEST_TEXT: &str = include_str!("../bridge-manifest.toml");
pub const WIRE_SHAPE_FINGERPRINT_TEXT: &str = include_str!("../bridge-wire-shape-fingerprint.txt");
include!(concat!(env!("OUT_DIR"), "/bridge_operation_names.rs"));

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

pub fn wire_shape_fingerprint() -> &'static str {
    WIRE_SHAPE_FINGERPRINT_TEXT.trim()
}
