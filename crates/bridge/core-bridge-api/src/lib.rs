//! Stable bridge-facing API surface.
//!
//! This crate intentionally has no native transport dependency. napi-rs, CLI,
//! and test transports live in sibling crates.

pub use rusty_crew_core_protocol::*;

pub const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeManifestSummary {
    pub version: u32,
    pub owning_crate: &'static str,
    pub native_package: &'static str,
}

pub fn manifest_summary() -> BridgeManifestSummary {
    BridgeManifestSummary {
        version: MANIFEST_VERSION,
        owning_crate: "rusty-crew-core-bridge-api",
        native_package: "@rusty-crew/native-bridge",
    }
}
