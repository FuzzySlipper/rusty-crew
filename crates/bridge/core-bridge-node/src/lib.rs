//! Native Node transport placeholder.
//!
//! The napi-rs surface belongs here, not in the core crates. Keeping this crate
//! empty for the first scaffold lets dependency checks catch accidental
//! transport leakage before generated glue exists.

pub use rusty_crew_core_bridge_api::manifest_summary;
