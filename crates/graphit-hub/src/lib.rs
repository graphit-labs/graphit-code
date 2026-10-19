//! Captured adapter selection and bounded artifact projection. No implicit environment.
mod adapter;
mod installation;
pub use installation::InstallationTarget;
pub use native::NativeBinding;
pub mod file_payload;
pub mod materialization;
pub mod native;
pub mod native_package;
pub mod native_root;
pub mod policy;
pub mod project_metadata;
mod projection;
pub mod publication;
pub mod registry;
pub mod remote_metadata;
pub mod remote_native;
pub mod remote_storage;
pub mod remote_version;
mod selection;
mod source;
pub use adapter::{AdapterId, AdapterParseError, Layout, RulePlacement};
pub use projection::{
    NativeCapability, Projection, ProjectionError, ProjectionLimits, ProjectionPlan,
    ProjectionPlanner, ProjectionRequest, ProjectionStage, ProjectionTarget, RulePlan,
    SourceBinding, SourceBindingRecord,
};
pub use selection::{
    AgentMembership, Selection, SelectionError, SelectionOrigin, select_agent, select_agents,
};
pub use source::{
    ArtifactId, ArtifactType, ConcreteVersion, RemoteFileOrigin, ResourceIdentity, ResourcesPage,
    ResourcesQuery, SourceOrigin, SourceRecord, SourceRef, list_resources,
};
#[cfg(test)]
mod projection_tests;
#[cfg(test)]
mod tests;
