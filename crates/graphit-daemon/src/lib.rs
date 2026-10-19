//! Project observation and supervision for the Graphit daemon.

pub mod auth_startup;
pub mod broker_inventory;
pub mod broker_verifiers;
pub mod control;
pub mod dream;
pub mod inventory;
pub mod maintained_inventory;
pub mod model_child;
pub mod model_service;
pub mod model_transport;
pub mod now_publisher;
pub mod now_subscription;
pub mod project_bootstrap;
pub mod project_now;
pub mod services;
pub mod source_discovery;
pub mod supervisor;
pub mod watcher;

#[cfg(test)]
#[path = "../tests/support/model_service_revocation.rs"]
mod model_service_revocation;
