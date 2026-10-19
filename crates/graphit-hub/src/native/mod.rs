//! Native content codecs, host configuration plans and managed document publication.
//! Publication uses the Core writer; host qualification and MCP dispatch use
//! their dedicated runtime APIs.
mod binding;
mod content;
mod document;
pub mod hooks;
pub mod kiro_hooks;
pub mod mcp_config;
pub mod opencode_plugin;
mod render;
mod role;
pub mod role_publication;
pub mod shared_document;
pub use binding::{BindingParseError, NativeBinding};
pub use content::{
    AgentDescriptor, ContentError, ContentLimits, RenderContext, SkillDescriptor,
    inspect_kiro_coordinator_profile,
};
pub use document::{DocumentError, DocumentFormat, FieldValue, NativeDocument};
pub use render::{
    ContentProvenance, NativeAgentRuntime, NativeToolGrant, RenderedContent, ToolAccess,
    ToolGrants, render_agent, render_agent_with_native_mcp, render_role_agent_with_mcp,
    render_skill,
};
pub use role::{Role, RoleCredentialReference, RoleReferenceError, RoleSource};
#[cfg(test)]
mod tests;
