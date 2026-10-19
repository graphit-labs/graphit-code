//! Live investigations: request-bound history and native execution custody.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]

mod auth;
mod history;
mod journal;
mod lifecycle;
mod preparation;
mod preparation_guard;
mod preparation_lifecycle;
mod read_contexts;
mod runner;
mod service;
mod sink;
mod tail;
#[cfg(test)]
pub(crate) mod tests;
mod wire;
mod workspace;

pub(crate) use auth::Access;
pub(crate) use history::{HistoryOperation, dispatch};
pub(crate) use preparation::{CapturedArtifact, Preparation, PreparedSelection};
pub(crate) use read_contexts::{ReadContext, ReadContexts, ReadContextsSnapshot};
pub(crate) use service::{HistoryLimits, LiveError, LiveSessionService};
pub(crate) use tail::{TailDelivery, TailMessage};
pub use wire::{
    ArtifactSelection, CreateRequest, Event, EventData, FailureCode, KnownTerminal,
    ProjectSelection, Replay, ReplayCursor, SendRequest, Session, SessionState, TurnOutcome,
};
