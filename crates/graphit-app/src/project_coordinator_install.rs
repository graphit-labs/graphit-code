//! Publish the product's authorial Coordinator through the real PROJECT SDKs.
use crate::{managed_roles::InstallResources, project_hook_install};
use graphit_auth::ProjectSelection;
use graphit_core::{
    artifact_store::{
        ArtifactEffect, ArtifactMode, ArtifactStore, CopyPolicy, PayloadOverlay, PayloadPolicy,
        PayloadSelection, PublicationScope, RetainedBytesLimits, RetainedFileSet, SnapshotPolicy,
        SourceSnapshot, ValidatedPayloadPlan,
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
        shared_document::SharedDocumentStore,
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
        let scope = PublicationScope::project(paths, *id)?;
        if version.scope() != scope.owner_root() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let before = || -> io::Result<()> {
            parent
                .authorize_project_publication(ProjectSelection {
                    scope: &scope,
                    version,
                })
                .map_err(denied)?;
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
                    }
                    Err(error) => {
                        outcome.publication = Some(format!("{:?}", error.effect));
                        return Err(publication_error(error));
                    }
                }
            }
            if binding == NativeBinding::KiroV3 {
                // The complete Hub profile was removed above. Other owned hook
                // tuples belong to independently installed profile integrations.
                let target = paths.root().join(".kiro/agents");
                let state = paths.root().join(".kiro/.graphit-shared-documents");
                if state.try_exists()? {
                    let shared = SharedDocumentStore::open(&state)?;
                    for profile in
                        shared.owned_kiro_profiles(&target, &core, hub.shared(), original)?
                    {
                        before()?;
                        let request = project_hook_install::HookRequest {
                            profile: Some(profile.clone()),
                            ..hook_request.clone()
                        };
                        request.validate()?;
                        let plans = project_hook_install::kiro_plans(
                            &resources, &request, paths, original,
                        )?;
                        let mut writer =
                            shared.writer(&target, &profile, &core, hub.shared(), original)?;
                        for plan in &plans {
                            before()?;
                            let change = writer.remove_kiro_profile_hook(plan, hub.content())?;
                            outcome.hooks.push(project_hook_install::HookEffect {
                                event: plan.native_trigger().into(),
                                change: format!("{change:?}"),
                            });
                        }
                    }
                }
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
        let retention = RetainedBytesLimits::new(hub.content().max_input_bytes, 2)?;
        let snapshot = SourceSnapshot::capture(
            &source_root,
            graphit_fs::NodeKind::Directory,
            &SnapshotPolicy::new(
                CopyPolicy::FollowLinks,
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
            if hooks_outcome.publication.is_some() {
                outcome.publication = hooks_outcome.publication;
            }
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
            CapturedInstruction {
                name: "shared/agent-workflow.md",
                origin: InstructionOrigin::CompiledDefault,
                markdown: include_str!(
                    "../../../templates/agent-instructions/shared/agent-workflow.md"
                ),
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
        let limits = InstructionLimits {
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
        };
        let instruction = sources
            .render(
                "coordinator/AGENT.md",
                &defaults,
                &variables,
                limits,
                original,
            )
            .map_err(io::Error::other)?;
        let instruction = graphit_runtime::policy_instructions::append_coordinator_mandates(
            instruction.into_body(),
            paths.brand(),
            limits,
            original.snapshot(),
        )
        .map_err(io::Error::other)?;
        let mut renderer = RenderContext::new(
            content,
            original.cancellation(),
            original.snapshot().required_deadline()?,
        )
        .map_err(io::Error::other)?;
        let descriptor = AgentDescriptor::parse(instruction.as_bytes(), &mut renderer)
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
            match writer.publish_portable_derived(
                ProjectionRequest {
                    binding,
                    source: &source,
                    alias: Some(&name),
                    entry_name: None,
                },
                ArtifactMode::Copy(CopyPolicy::FollowLinks),
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
                }
                Err(error) => {
                    if error.effect == ArtifactEffect::Applied {
                        outcome.profile = Some(rendered.relative_entry);
                        outcome.hooks = profile_hooks;
                    }
                    outcome.publication = Some(format!("{:?}", error.effect));
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
        if let Some(applied) = error.get_ref().and_then(|cause| {
            cause.downcast_ref::<
                graphit_hub::native::shared_document::SharedDocumentPublicationError,
            >()
        }) {
            outcome.publication = Some(format!("{:?}", applied.effect()));
        }
        eprintln!(
            "Coordinator installation failure: publication={:?}, io_kind={:?}, os_code={:?}",
            outcome.publication,
            error.kind(),
            error.raw_os_error(),
        );
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
