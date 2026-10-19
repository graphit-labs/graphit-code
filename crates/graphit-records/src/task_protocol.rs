//! Typed wire operations dispatched directly to the authenticated native Task store.
//! Transport hosts authenticate their real caller before using these operations.
#![doc = include_str!("../../../docs/specs/rust_task_protocol.md")]
mod input;
mod output;
mod read;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod write;
pub use input::*;
pub use output::*;
pub use read::execute_read;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub use write::execute_write;
