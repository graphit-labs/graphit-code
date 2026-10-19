//! Lexical destination advice through the current PROJECT admission.
use super::super::{SelectedProject, State, ToolResult, error, parse};
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, VerifiedRoleGrant};
use graphit_core::{artifact_store::ArtifactMode, native_path::NativePath};
use graphit_fs::NodeKind;
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, NativeBinding, ProjectionError,
    ProjectionPlanner, ProjectionRequest, ProjectionStage, SourceRef, policy::RequestContext,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Component;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TypePath {
    artifact_type: ArtifactType,
    name: String,
}

/// `binding` is the closed binding of the ScopeInput verified for this grant.
pub(super) fn read<'a>(
    state: &State,
    authority: &'a ProjectRecordAuthority<'_>,
    grant: &'a VerifiedRoleGrant<'a>,
    selected: &SelectedProject,
    binding: NativeBinding,
    input: Option<Value>,
    original: &'a RequestContext<'a>,
) -> ToolResult {
    let query: TypePath = parse(input)?;
    let admission = authority
        .admit_read(grant, original, ProjectReadModule::Hub)
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let check = || {
        original
            .check()
            .map_err(|_| error("request_cancelled_or_expired"))?;
        admission
            .recheck()
            .map_err(|_| error("authentication_or_permission_denied"))
    };
    check()?;
    let policy = &state.policy.source.native_install.hub;
    let root =
        NativePath::new(selected.0.root().to_owned()).map_err(|_| error("hub_path_unavailable"))?;
    let project = admission
        .project()
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let runtime = NativePath::new(project.scope.live.runtime_root().to_owned())
        .map_err(|_| error("hub_path_unavailable"))?;
    // An ephemeral candidate for preflight, never an observed installation or IO authority.
    let source = SourceRef::local_link(
        root.clone(),
        selected.1,
        query.artifact_type,
        ArtifactId::new(&query.name, policy.projection()).map_err(projection_failure)?,
        ConcreteVersion::new(selected.2.version(), policy.projection())
            .map_err(projection_failure)?,
        policy.projection(),
    )
    .map_err(projection_failure)?;
    // Mode and runtime are unused by preflight; no capture, link or publication follows.
    let planner = ProjectionPlanner::new(
        root,
        runtime,
        policy.projection(),
        policy.artifacts(),
        ArtifactMode::Link,
    );
    let targets = planner
        .preflight(&[ProjectionRequest {
            binding,
            source: &source,
            alias: None,
            entry_name: None,
        }])
        .map_err(projection_failure)?;
    let target = targets
        .first()
        .ok_or_else(|| error("hub_path_unavailable"))?;
    let path = target
        .destination()
        .map(|destination| {
            let relative = destination
                .as_path()
                .strip_prefix(selected.0.root())
                .map_err(|_| error("hub_path_unavailable"))?;
            relative
                .components()
                .map(|component| match component {
                    Component::Normal(name) => {
                        name.to_str().ok_or_else(|| error("hub_path_unavailable"))
                    }
                    _ => Err(error("hub_path_unavailable")),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|parts| parts.join("/"))
        })
        .transpose()?;
    let placement = if path.is_some() {
        "physical"
    } else if query.artifact_type == ArtifactType::Rule {
        "dynamic_context"
    } else {
        "no_physical_path"
    };
    let node_kind = target.kind().map(|kind| match kind {
        NodeKind::Directory => "directory",
        NodeKind::File => "file",
    });
    let result = json!({
        "artifact_type":query.artifact_type,
        "binding":binding,
        "target":InstallationTarget::for_artifact(binding,query.artifact_type),
        "name":target.name(),
        "placement":placement,
        "path":path,
        "node_kind":node_kind,
        "agent_schema":(query.artifact_type==ArtifactType::Agent).then(||binding.schema())
    });
    check()?;
    Ok(result)
}

fn projection_failure(cause: ProjectionError) -> Value {
    if cause.stage() == ProjectionStage::Unsupported {
        error("unsupported_artifact_type")
    } else {
        error("invalid_arguments")
    }
}
