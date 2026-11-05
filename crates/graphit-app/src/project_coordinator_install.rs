//! Publish the product's authorial Coordinator through the real PROJECT SDKs.
use crate::{managed_roles::InstallResources, project_hook_install};
use graphit_auth::{ExternalEffect, Permission};
use graphit_core::{
    artifact_store::{
        ArtifactMode, ArtifactStore, CapturedPublicationScope, CopyPolicy, PayloadOverlay,
        PayloadPolicy, PayloadSelection, RetainedBytesLimits, RetainedFileSet, ScopeContext,
        SnapshotPolicy, SourceSnapshot, ValidatedPayloadPlan,
    },
    native_path::NativePath,
    project_identity::{ProjectId, ProjectIdentityStore},
};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, NativeBinding, ProjectionRequest, SourceRef,
    materialization::DerivedArtifact,
    native::{
        AgentDescriptor, NativeAgentRuntime, RenderContext, ToolGrants,
        mcp_config::{NativeServerSpec, NativeServerTransport},
        render_agent_with_native_mcp,
    },
    policy::RequestContext,
    publication::{PublicationChange, PublicationFailure, ScopedCoordinator},
    registry::{ArtifactDisplayMetadata, HubRegistry},
};
use graphit_runtime::instructions::{
    CapturedInstruction, InstructionLimits, InstructionOrigin, InstructionValue,
    InstructionVariable,
};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    num::{NonZeroU64, NonZeroUsize},
    path::{Path, PathBuf},
};

pub mod client;
pub const ROUTE: &str = "/admin/coordinator";
pub const ARTIFACT_ID: &str = "graphit/coordinator";
pub const ARTIFACT_VERSION: &str = "1.0.0";
pub(crate) const PRODUCER: &str = "01KSH1CRFFG8Z74B5ZS78WW808";
const DESCRIPTOR: &str = include_str!("../../../templates/agent-instructions/coordinator/AGENT.md");
const PROMPT: &str = include_str!("../../../templates/agent-instructions/coordinator/prompt.md");
const PERSISTENCE: &str =
    include_str!("../../../templates/agent-instructions/shared/persistence-boundary.md");

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinatorRequest {
    pub operation: project_hook_install::HookOperation,
    pub binding: NativeBinding,
    pub kimi_config_dir: Option<PathBuf>,
    pub source_dir: Option<PathBuf>,
    pub project_dir: PathBuf,
    pub project_version: Option<String>,
}
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinatorOutcome {
    pub profile: Option<String>,
    pub publication: Option<String>,
    pub mcp: Option<String>,
    pub hooks: Vec<project_hook_install::HookEffect>,
    pub failure: Option<String>,
}
pub struct CoordinatorExecution {
    pub outcome: CoordinatorOutcome,
    pub cause: Option<io::Error>,
}

#[derive(Debug)]
struct CoordinatorCleanupFailure(PublicationFailure);
impl std::fmt::Display for CoordinatorCleanupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Coordinator publication cleanup remains pending")
    }
}
impl std::error::Error for CoordinatorCleanupFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match &self.0 {
            PublicationFailure::Local(error) => error,
            PublicationFailure::Projection(error) => error,
            PublicationFailure::Physical(error) => error,
        })
    }
}
fn cleanup_failure(failure: PublicationFailure) -> io::Error {
    let kind = match &failure {
        PublicationFailure::Local(error) => error.kind(),
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, CoordinatorCleanupFailure(failure))
}

pub fn execute(
    resources: InstallResources<'_>,
    credential: &str,
    input: &CoordinatorRequest,
    original: &RequestContext<'_>,
) -> CoordinatorExecution {
    let mut outcome = CoordinatorOutcome::default();
    let result = (|| -> io::Result<()> {
        original.check()?;
        let caller = resources
            .auth
            .issuer()
            .authenticate_runtime(credential, original)
            .map_err(denied)?;
        let session = resources
            .auth
            .issuer()
            .bootstrap_admin(&caller)
            .map_err(denied)?;
        let parent = resources
            .auth
            .issuer()
            .admin_parent(&caller, &session)
            .map_err(denied)?;
        let selection = crate::managed_roles::InstallRequest {
            destination: crate::managed_roles::InstallDestination::Project,
            project_dir: Some(input.project_dir.clone()),
            project_version: input.project_version.clone(),
            binding: None,
            roles: Vec::new(),
            skills_only: false,
            rules: Vec::new(),
            skill_aliases: Default::default(),
            skill_bundles: Default::default(),
            module_skill: None,
        };
        let (paths, id, version) = crate::managed_roles::select_project(
            resources.projects,
            resources.current,
            &selection,
        )?;
        if !resources
            .auth
            .installation()
            .live
            .allowed_roots()
            .any(|root| root == paths.root())
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let scope = CapturedPublicationScope::project(
            paths,
            *id,
            resources.policy.scope,
            original.snapshot(),
        )?;
        if version.scope() != scope.owner_root() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let scope_context = ScopeContext::from_snapshot(original.snapshot());
        let before = || -> io::Result<()> {
            parent.authorize(Permission::Configure).map_err(denied)?;
            parent
                .authorize(Permission::External(ExternalEffect::PublishedFilesystem))
                .map_err(denied)?;
            scope.revalidate_bounded(&scope_context)?;
            original.check()
        };
        before()?;
        let hub = &resources.policy.native_install.hub;
        let binding = input.binding;
        let name = format!("{}-coordinator", paths.brand().key());
        let server_name = paths.brand().key();
        let hook_request = project_hook_install::HookRequest {
            operation: input.operation,
            binding,
            kimi_config_dir: input.kimi_config_dir.clone(),
            profile: (binding == NativeBinding::KiroV3).then(|| format!("{name}.md")),
            project_dir: input.project_dir.clone(),
            project_version: input.project_version.clone(),
        };
        hook_request.validate()?;
        let mcp = NativeServerSpec {
            name: server_name.into(),
            transport: NativeServerTransport::Stdio {
                command: paths.brand().binary_name().into(),
                args: vec!["mcp".into(), "--stdio".into()],
                env: Default::default(),
                env_vars: project_hook_install::client::mcp_environment_variables(
                    paths.brand(),
                    binding,
                )?,
                cwd: None,
            },
        }
        .plan(binding)
        .map_err(io::Error::other)?;
        if input.operation == project_hook_install::HookOperation::Remove {
            if input.source_dir.is_some() {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let core = ArtifactStore::new(scope.clone(), hub.artifacts());
            if binding != NativeBinding::Antigravity {
                let identity = ProjectIdentityStore::at(paths)
                    .load()?
                    .ok_or(io::ErrorKind::NotFound)?;
                if identity.id() != *id {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                let registry = HubRegistry::project(paths, &identity, hub.hub())?;
                let coordinator = ScopedCoordinator::new(
                    registry,
                    core.clone(),
                    NativePath::new(paths.root().into())?,
                )?;
                let key =
                    graphit_hub::InstallationTarget::for_artifact(binding, ArtifactType::Agent)
                        .key(ArtifactType::Agent, ARTIFACT_ID)?;
                before()?;
                let mut writer = coordinator.writer(original).map_err(publication_error)?;
                match writer.remove(&key) {
                    Ok(result) => {
                        outcome.publication = Some(format!("{:?}", result.change));
                        if let Some(pending) = result.cleanup_pending {
                            return Err(cleanup_failure(pending));
                        }
                    }
                    Err(error) => {
                        if error.committed {
                            outcome.publication = Some("CommittedWithError".into());
                        } else if error.core_pending.is_some() || error.hub_intent.is_some() {
                            outcome.publication = Some("Pending".into());
                        }
                        return Err(publication_error(error));
                    }
                }
            }
            if binding == NativeBinding::KiroV3 {
                // Inline hooks belong to the complete Hub profile removed above.
                outcome.mcp = project_hook_install::native_publication::publish_mcp(
                    &resources,
                    &hook_request,
                    paths.root(),
                    &core,
                    Some(&mcp),
                    original,
                    &before,
                )?;
            } else {
                let mut hooks_outcome = project_hook_install::HookOutcome::default();
                let result = project_hook_install::native_publication::publish(
                    &resources,
                    &hook_request,
                    (paths, id, &scope),
                    Some(&mcp),
                    original,
                    &before,
                    &mut hooks_outcome,
                );
                outcome.hooks = hooks_outcome.hooks;
                outcome.mcp = result?;
            }
            return before();
        }
        let source_root = fs::canonicalize(
            input
                .source_dir
                .as_ref()
                .ok_or(io::ErrorKind::InvalidInput)?,
        )?;
        let retention = RetainedBytesLimits::new(
            hub.content().max_input_bytes,
            2,
            graphit_core::native_path::MAX_PATH_BYTES,
        )?;
        let snapshot = SourceSnapshot::capture(
            &source_root,
            graphit_fs::NodeKind::Directory,
            &SnapshotPolicy::new(
                CopyPolicy::RejectFileLinks,
                hub.materialization().snapshots,
                RetainedFileSet::new(["AGENT.md".into(), "prompt.md".into()], retention)?,
                retention,
            )?,
            original.snapshot(),
        )?;
        if snapshot.retained(Path::new("AGENT.md")) != Some(DESCRIPTOR.as_bytes())
            || snapshot.retained(Path::new("prompt.md")) != Some(PROMPT.as_bytes())
            || snapshot.nodes().any(|node| {
                !matches!(
                    node.relative().to_str(),
                    Some("" | "AGENT.md" | "prompt.md")
                )
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Coordinator source differs from the product artifact",
            ));
        }
        snapshot.revalidate_content(original.snapshot())?;
        before()?;
        if binding == NativeBinding::Antigravity {
            let mut hooks_outcome = project_hook_install::HookOutcome::default();
            let publication = project_hook_install::native_publication::publish(
                &resources,
                &hook_request,
                (paths, id, &scope),
                Some(&mcp),
                original,
                &before,
                &mut hooks_outcome,
            );
            outcome.hooks = hooks_outcome.hooks;
            outcome.mcp = publication?;
            return before();
        }
        let variables = [
            InstructionVariable {
                name: "brand",
                value: InstructionValue::Text(server_name),
            },
            InstructionVariable {
                name: "agent_name",
                value: InstructionValue::Text(&name),
            },
            InstructionVariable {
                name: "mcp_server_name",
                value: InstructionValue::Text(server_name),
            },
        ];
        let defaults = [
            CapturedInstruction {
                name: "coordinator/AGENT.md",
                origin: InstructionOrigin::CompiledDefault,
                markdown: DESCRIPTOR,
            },
            CapturedInstruction {
                name: "coordinator/prompt.md",
                origin: InstructionOrigin::CompiledDefault,
                markdown: PROMPT,
            },
            CapturedInstruction {
                name: "shared/persistence-boundary.md",
                origin: InstructionOrigin::CompiledDefault,
                markdown: PERSISTENCE,
            },
        ];
        let sources = crate::instruction_overrides::InstructionSources::capture(
            resources.auth.issuer(),
            resources.policy,
            Some(paths),
            original,
        )?;
        let content = hub.content();
        let nz = |n| NonZeroUsize::new(n).ok_or(io::ErrorKind::InvalidInput);
        let instruction = sources
            .render(
                "coordinator/AGENT.md",
                &defaults,
                &variables,
                InstructionLimits {
                    max_templates: nz(content.max_collection_entries)?,
                    max_template_bytes: nz(content.max_input_bytes)?,
                    max_values: nz(content.max_nodes)?,
                    max_variable_bytes: nz(content.max_input_bytes)?,
                    max_output_bytes: nz(content.max_output_bytes)?,
                    fuel: NonZeroU64::new(
                        u64::try_from(content.max_nodes)
                            .map_err(io::Error::other)?
                            .checked_mul(16)
                            .ok_or(io::ErrorKind::InvalidInput)?,
                    )
                    .ok_or(io::ErrorKind::InvalidInput)?,
                    recursion: nz(content.max_depth.min(500))?,
                },
                original,
            )
            .map_err(io::Error::other)?;
        let mut renderer = RenderContext::new(
            content,
            original.cancellation(),
            original.snapshot().required_deadline()?,
        )
        .map_err(io::Error::other)?;
        let descriptor = AgentDescriptor::parse(instruction.body().as_bytes(), &mut renderer)
            .map_err(io::Error::other)?;
        let display = ArtifactDisplayMetadata::new(
            descriptor.name(),
            Some(descriptor.description()),
            &[],
            hub.hub(),
        )?;
        let grants = ToolGrants::native(binding, descriptor.read_only(), descriptor.allows_shell());
        let hooks = if binding == NativeBinding::KiroV3 {
            project_hook_install::kiro_plans(&resources, &hook_request, paths, original)?
        } else {
            Vec::new()
        };
        let rendered = render_agent_with_native_mcp(
            binding,
            &descriptor,
            Some(&name),
            &grants,
            NativeAgentRuntime {
                mcp: &mcp,
                kiro_hooks: &hooks,
            },
            &mut renderer,
        )
        .map_err(io::Error::other)?;
        let profile_bytes = rendered.bytes;
        let identity = ProjectIdentityStore::at(paths)
            .load()?
            .ok_or(io::ErrorKind::NotFound)?;
        if identity.id() != *id {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let registry = HubRegistry::project(paths, &identity, hub.hub())?;
        let core = ArtifactStore::new(scope.clone(), hub.artifacts());
        let source = SourceRef::local_link(
            NativePath::new(source_root)?,
            PRODUCER.parse::<ProjectId>().map_err(io::Error::other)?,
            ArtifactType::Agent,
            ArtifactId::new(ARTIFACT_ID, hub.projection()).map_err(io::Error::other)?,
            ConcreteVersion::new(ARTIFACT_VERSION, hub.projection()).map_err(io::Error::other)?,
            hub.projection(),
        )
        .map_err(io::Error::other)?;
        let root_mode = snapshot
            .nodes()
            .next()
            .ok_or(io::ErrorKind::InvalidData)?
            .mode();
        let file_mode = snapshot
            .nodes()
            .find(|node| node.relative() == Path::new("AGENT.md"))
            .ok_or(io::ErrorKind::NotFound)?
            .mode();
        let plan = ValidatedPayloadPlan::new(
            snapshot,
            [PayloadOverlay::new(
                "AGENT.md".into(),
                "rendered.md".into(),
                profile_bytes,
                file_mode,
            )?],
            &PayloadPolicy::new(
                graphit_fs::NodeKind::Directory,
                root_mode,
                hub.materialization().snapshots,
            ),
            original.snapshot(),
        )?;
        let derived = DerivedArtifact::new(
            source.clone(),
            plan,
            binding.schema(),
            &name,
            (
                PayloadSelection::new("AGENT.md".into(), graphit_fs::NodeKind::File)?,
                PayloadSelection::new("rendered.md".into(), graphit_fs::NodeKind::File)?,
            ),
            hub.materialization().lineage,
        )?;
        before()?;
        let coordinator = ScopedCoordinator::new(
            registry,
            core.clone(),
            NativePath::new(paths.root().into())?,
        )?;
        let profile_hooks = hooks
            .iter()
            .map(|hook| project_hook_install::HookEffect {
                event: hook.native_trigger().into(),
                change: "PublishedInProfile".into(),
            })
            .collect();
        {
            let mut writer = coordinator.writer(original).map_err(publication_error)?;
            match writer.publish_derived(
                ProjectionRequest {
                    binding,
                    source: &source,
                    alias: Some(&name),
                    entry_name: None,
                },
                ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
                derived,
                Some(&display),
            ) {
                Ok(result) => {
                    if result.change == PublicationChange::Unsupported {
                        return Err(io::ErrorKind::Unsupported.into());
                    }
                    outcome.profile = Some(rendered.relative_entry);
                    outcome.hooks = profile_hooks;
                    outcome.publication = Some(format!("{:?}", result.change));
                    if let Some(pending) = result.cleanup_pending {
                        return Err(cleanup_failure(pending));
                    }
                }
                Err(error) => {
                    if error.committed {
                        outcome.profile = Some(rendered.relative_entry);
                        outcome.hooks = profile_hooks;
                        outcome.publication = Some("CommittedWithError".into());
                    } else if error.core_pending.is_some() || error.hub_intent.is_some() {
                        outcome.publication = Some("Pending".into());
                    }
                    return Err(publication_error(error));
                }
            }
        }
        before()?;
        if binding == NativeBinding::KiroV3 {
            outcome.mcp = project_hook_install::native_publication::publish_mcp(
                &resources,
                &hook_request,
                paths.root(),
                &core,
                Some(&mcp),
                original,
                &before,
            )?;
        } else {
            let mut hooks_outcome = project_hook_install::HookOutcome::default();
            let publication = project_hook_install::native_publication::publish(
                &resources,
                &hook_request,
                (paths, id, &scope),
                Some(&mcp),
                original,
                &before,
                &mut hooks_outcome,
            );
            outcome.hooks = hooks_outcome.hooks;
            outcome.mcp = publication?;
        }

        before()?;
        before()
    })();
    let cause = result.err();
    if let Some(error) = &cause {
        outcome.failure = Some(
            match error.kind() {
                io::ErrorKind::PermissionDenied => "Coordinator authority denied",
                io::ErrorKind::Interrupted => "Coordinator installation cancelled",
                io::ErrorKind::TimedOut => "Coordinator installation deadline",
                _ => "Coordinator installation failed",
            }
            .into(),
        );
    }
    CoordinatorExecution { outcome, cause }
}
fn denied(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    project_hook_install::private_error(
        io::ErrorKind::PermissionDenied,
        "Coordinator authority denied",
        error,
    )
}

fn publication_error(error: Box<graphit_hub::publication::PublicationError>) -> io::Error {
    let kind = match &error.failure {
        PublicationFailure::Local(error) => error.kind(),
        PublicationFailure::Projection(error) => error.kind(),
        PublicationFailure::Physical(error) => error.kind(),
    };
    project_hook_install::private_error(kind, "Coordinator publication failed", error)
}
