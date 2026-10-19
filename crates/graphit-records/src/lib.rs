//! Record-domain decisions and authenticated native persistence.
//!
//! Pure planners operate on immutable observations. Native services use original
//! admissions and captures; lifecycle consumers must enforce ownership, fencing,
//! storage CAS and durable history when applying a proposed decision.
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate self as graphit_records;

pub mod gates;
pub mod git_records;
pub mod memory_attributes;
pub mod memory_store;
pub mod reference_projection;
mod storage;
pub mod task_comment;
pub mod task_protocol;
pub mod task_schedule;
pub mod task_spec;
pub mod task_store;
pub mod user_memory_store;
