//! Transport-free protocol types for the Rust coordination core.

mod external_runtime;
mod memory_space;
mod types;

pub use external_runtime::*;
pub use memory_space::*;
pub use types::*;
