//! Literal USER Memory data contracts and exact native corpus layout.
//!
//! Owner identifiers, result DTOs and schemas describe data. Access to a USER
//! corpus additionally requires the original request and typed USER admission.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod automatic;
mod embeddings;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod fenced;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod hybrid;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub use hybrid::UserMemoryHybridSearchRead;
mod model;
pub use embeddings::*;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod native;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod references;
pub(crate) mod rows;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod search;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub(crate) mod store;

pub use model::*;
pub use rows::user_memory_schema;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub struct NativeUserMemoryStore;

#[cfg(test)]
#[path = "../tests/user_memory_store/domain.rs"]
mod domain_tests;
