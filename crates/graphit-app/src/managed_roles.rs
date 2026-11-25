//! Install/sync managed roles on the actual daemon issuer and captured project.
use crate::source_server::SourcePolicy;
use graphit_auth::{ExternalEffect, IssuerLimits, Permission, ProjectSelection, RoleDefinition};
use graphit_core::{
    artifact_store::{ArtifactStore, PublicationScope},
    module_settings::{Module, ModuleSettings},
    project_identity::ProjectId,
    project_paths::ProjectPaths,
    version_context::VersionContext,
};
use graphit_daemon::auth_startup::DaemonAuth;
use graphit_hub::{
    ArtifactType, ConcreteVersion, NativeBinding,
    native::{
        AgentDescriptor, NativeAgentRuntime, NativeToolGrant, RenderContext, RenderedContent, Role,
        ToolAccess, ToolGrants, render_agent, render_role_agent_with_mcp,
        role_publication::{self, RolePublicationError},
    },
    policy::RequestContext,
};
use graphit_runtime::{
    ManagedRole,
    instructions::{CapturedInstruction, InstructionLimits, InstructionOrigin},
    role_instructions::{RoleAliases, render_role, role_templates},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    num::{NonZeroU64, NonZeroUsize},
    path::{Component, Path, PathBuf},
};

pub mod client;
pub mod global_paths;
pub mod http;
mod selection;

#[derive(Clone, Copy, Debug, clap::ValueEnum, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRoleName {
    Scout,
    Tracker,
    Scribe,
}
impl ManagedRoleName {
    pub(crate) fn role(self) -> ManagedRole {
        match self {
            Self::Scout => ManagedRole::Scout,
            Self::Tracker => ManagedRole::Tracker,
            Self::Scribe => ManagedRole::Scribe,
        }
    }
    fn authority(self) -> Role {
        match self {
            Self::Scout => Role::Scout,
            Self::Tracker => Role::Tracker,
            Self::Scribe => Role::Scribe,
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuleSelection {
    pub role: ManagedRoleName,
    pub id: String,
    pub version: String,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, clap::ValueEnum, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallDestination {
    #[default]
    Project,
    Global,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstallRequest {
    #[serde(default)]
    pub destination: InstallDestination,
    pub project_dir: Option<PathBuf>,
    pub project_version: Option<String>,
    pub binding: Option<NativeBinding>,
    #[serde(default)]
    pub roles: Vec<ManagedRoleName>,
    #[serde(default)]
    pub skills_only: bool,
    #[serde(default)]
    pub rules: Vec<RuleSelection>,
    #[serde(default)]
    pub skill_aliases: BTreeMap<String, String>,
    pub module_skill: Option<String>,
    #[serde(default)]
    pub skill_bundles: BTreeMap<String, crate::compiled_skills::HubSelection>,
}
#[derive(Serialize, Deserialize)]
pub struct RoleEffect {
    pub role: ManagedRoleName,
    pub name: String,
    pub origin: String,
    pub definition_configured: bool,
    pub publication: Option<String>,
}
#[derive(Default, Serialize, Deserialize)]
pub struct InstallOutcome {
    pub roles: Vec<RoleEffect>,
    pub skills: Vec<crate::compiled_skills::SkillEffect>,
    pub failure: Option<String>,
}
pub struct InstallExecution {
    pub outcome: InstallOutcome,
    pub cause: Option<io::Error>,
}

/// Retained tuples are the Runtime producer's current facts, never caller DTO authority.
pub struct InstallResources<'a> {
    pub auth: &'a DaemonAuth,
    pub global_paths: Option<&'a global_paths::NativeGlobalPaths>,
    pub projects: &'a [(ProjectPaths, ProjectId, VersionContext)],
    pub current: usize,
    pub policy: &'a SourcePolicy,
    pub modules: &'a [ModuleSettings],
    pub memory_read: graphit_records::memory_store::MemoryStoreReadLimits,
}
pub fn execute_install(
    resources: InstallResources<'_>,
    credential: &str,
    input: &InstallRequest,
    docs_dir: Option<&str>,
    original: &RequestContext<'_>,
    native: Option<&crate::compiled_skills::remote::NativeSkillTransport>,
) -> InstallExecution {
    let InstallResources {
        auth,
        projects,
        current,
        policy,
        modules: _,
        memory_read: _,
        global_paths,
    } = resources;
    let mut outcome = InstallOutcome::default();
    let result = (|| -> io::Result<()> {
        original.snapshot().required_deadline()?;
        let caller = auth
            .issuer()
            .authenticate_runtime(credential, original)
            .map_err(io::Error::other)?;
        let session = auth
            .issuer()
            .bootstrap_admin(&caller)
            .map_err(io::Error::other)?;
        let parent = auth
            .issuer()
            .admin_parent(&caller, &session)
            .map_err(io::Error::other)?;
        parent
            .authorize(Permission::ManageAccessGrants)
            .map_err(io::Error::other)?;
        parent
            .authorize(Permission::Configure)
            .map_err(io::Error::other)?;
        parent
            .authorize(Permission::External(ExternalEffect::PublishedFilesystem))
            .map_err(io::Error::other)?;
        let source = instruction_project(projects, current, input)?;
        let docs_dir = if input.destination == InstallDestination::Project {
            Some(docs_dir.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "selected project settings unavailable",
                )
            })?)
        } else {
            None
        };
        let source_scope = source
            .map(|(paths, id, _)| PublicationScope::project(paths, *id))
            .transpose()?;
        let global = if input.destination == InstallDestination::Global {
            Some(global_paths.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "configured native global directories unavailable",
                )
            })?)
        } else {
            None
        };
        let scope = if let Some(global) = global {
            let graphit_core::artifact_store::StoreOwner::Installation { installation_id } =
                auth.installation().owner()
            else {
                return Err(io::ErrorKind::PermissionDenied.into());
            };
            let roots = global.allowed_roots()?;
            PublicationScope::installation_instructions(
                &policy.installation,
                installation_id.clone(),
                roots,
            )?
        } else {
            source_scope.clone().ok_or(io::ErrorKind::InvalidInput)?
        };
        let before = || -> io::Result<()> {
            if let Some(((_, _, version), scope)) = source.zip(source_scope.as_ref()) {
                parent
                    .authorize_project_publication(ProjectSelection { scope, version })
                    .map_err(io::Error::other)?;
            } else {
                parent
                    .authorize(Permission::Configure)
                    .map_err(io::Error::other)?;
                parent
                    .authorize(Permission::External(ExternalEffect::PublishedFilesystem))
                    .map_err(io::Error::other)?;
            }
            original.check()
        };
        before()?;
        if source
            .zip(source_scope.as_ref())
            .is_some_and(|((_, _, version), scope)| version.scope() != scope.owner_root())
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let hub = &policy.native_install.hub;
        let store = ArtifactStore::new(scope.clone(), hub.artifacts());
        let selected = if input.skills_only {
            Vec::new()
        } else if input.roles.is_empty() {
            vec![
                ManagedRoleName::Scout,
                ManagedRoleName::Tracker,
                ManagedRoleName::Scribe,
            ]
        } else {
            input.roles.clone()
        };
        let mut seen = std::collections::BTreeSet::new();
        for role in &selected {
            if !seen.insert(role.role().key()) {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        let (mut captured_input, saved_selection) =
            selection::resolve(input, source.map(|(paths, _, _)| paths), policy)?;
        captured_input.project_version = source.map(|(_, _, version)| version.version().to_owned());
        // The requested choices are canonical configuration even if a later file conflicts.
        // A retry must not revert already-published custom instructions to defaults.
        before()?;
        saved_selection.persist(&captured_input, original)?;
        for selected in selected {
            before()?;
            let CompiledDefinition {
                name,
                origin,
                native,
                definition,
                ..
            } = compile_definition(
                source.map(|(paths, _, _)| paths),
                selected,
                &captured_input,
                policy,
                &parent,
                auth.issuer(),
                original,
            )?;

            before()?;
            let reference = auth
                .issuer()
                .configure_tool_owned_definition(&parent, definition)
                .map_err(io::Error::other)?;
            outcome.roles.push(RoleEffect {
                role: selected,
                name,
                origin,
                definition_configured: true,
                publication: None,
            });
            let recheck = || -> io::Result<()> {
                before()?;

                Ok(())
            };
            let directories = global
                .map(|global| global.directories(native.provenance.binding))
                .transpose()?;
            let target = if let Some(directories) = &directories {
                recheck()?;
                fs::create_dir_all(&directories.agents)?;
                role_publication::RoleDestination::GlobalAgents(&directories.agents)
            } else {
                role_publication::RoleDestination::Project(
                    source.ok_or(io::ErrorKind::InvalidInput)?.0.root(),
                )
            };
            let published = role_publication::publish_role(
                &store, target, native, &reference, original, recheck,
            );
            match published {
                Ok(change) => {
                    outcome.roles.last_mut().unwrap().publication = Some(format!("{change:?}"))
                }
                Err(error) => {
                    if let RolePublicationError::AfterPublication { change, .. } = &error {
                        outcome.roles.last_mut().unwrap().publication = Some(format!("{change:?}"));
                    }
                    return Err(io::Error::other(error));
                }
            }
        }
        let skills = crate::compiled_skills::publish(
            &resources,
            &captured_input,
            &store,
            &parent,
            original,
            docs_dir,
            native.map(|native| (native, credential)),
        );
        outcome.skills = skills.effects;
        if let Some(cause) = skills.cause {
            return Err(cause);
        }
        before()?;
        before()
    })();
    let cause = result.err();
    if let Some(error) = &cause {
        outcome.failure = Some(error.to_string());
    }
    InstallExecution { outcome, cause }
}
pub(crate) fn instruction_project<'a>(
    projects: &'a [(ProjectPaths, ProjectId, VersionContext)],
    current: usize,
    input: &InstallRequest,
) -> io::Result<Option<&'a (ProjectPaths, ProjectId, VersionContext)>> {
    if input.project_dir.is_none() && input.destination == InstallDestination::Global {
        if input.project_version.is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(None)
    } else {
        select_project(projects, current, input).map(Some)
    }
}

pub(crate) fn select_project<'a>(
    projects: &'a [(ProjectPaths, ProjectId, VersionContext)],
    current: usize,
    input: &InstallRequest,
) -> io::Result<&'a (ProjectPaths, ProjectId, VersionContext)> {
    let matching: Vec<_> = projects
        .iter()
        .enumerate()
        .filter(|(_, (path, _, version))| {
            input.project_dir.as_deref() == Some(path.root())
                && input
                    .project_version
                    .as_deref()
                    .is_none_or(|selected| selected == version.version())
        })
        .collect();
    if input.project_version.is_none()
        && let Some((_, project)) = matching.iter().find(|(index, _)| *index == current)
    {
        return Ok(project);
    }
    match matching.as_slice() {
        [(_, project)] => Ok(project),
        [] => Err(io::ErrorKind::NotFound.into()),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ambiguous project version",
        )),
    }
}
pub(crate) struct CompiledDefinition {
    pub(crate) name: String,
    pub(crate) instructions: String,
    pub(crate) origin: String,
    pub(crate) native: RenderedContent,
    pub(crate) definition: RoleDefinition,
}

/// Shared captured source and native policy path for install and an actual role run.
pub(crate) fn compile_definition(
    paths: Option<&ProjectPaths>,
    selected: ManagedRoleName,
    input: &InstallRequest,
    policy: &SourcePolicy,
    parent: &graphit_auth::AdminParent<'_>,
    issuer: &graphit_auth::RoleIssuer,
    original: &RequestContext<'_>,
) -> io::Result<CompiledDefinition> {
    parent
        .authorize(Permission::ManageAccessGrants)
        .map_err(io::Error::other)?;
    let role = selected.role();
    let brand = policy.installation.brand();
    let binding = input.binding.unwrap_or(policy.binding);
    let hub = &policy.native_install.hub;
    let mut renderer = RenderContext::new(
        hub.content(),
        original.cancellation(),
        original.snapshot().required_deadline()?,
    )
    .map_err(io::Error::other)?;
    let defaults = role_templates(role);
    let mut observed = crate::instruction_overrides::local_candidates(
        defaults.candidates(),
        policy.installation.root(),
        paths,
        original,
    )?;
    let matches: Vec<_> = input
        .rules
        .iter()
        .filter(|rule| rule.role.role() == role)
        .collect();
    if matches.len() > 1 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    if let Some(rule) = matches.first() {
        parent
            .authorize(Permission::Read(Module::Hub))
            .map_err(io::Error::other)?;
        let document = policy.registry.load()?;
        let concrete =
            ConcreteVersion::new(&rule.version, hub.projection()).map_err(io::Error::other)?;
        let artifact = document
            .get_version(ArtifactType::Rule, &rule.id, &concrete)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "selected Hub rule not installed")
            })?;
        let mut installations = artifact
            .installations
            .iter()
            .filter(|installation| installation.target.includes(binding));
        let installation = installations.next().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "selected Hub rule binding not installed",
            )
        })?;
        if installations.next().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ambiguous Hub rule",
            ));
        }
        let directory = installation.original_source.directory().as_path();
        for candidate in defaults.candidates() {
            observed.push(capture_template(
                directory,
                Path::new(candidate.name),
                InstructionOrigin::Hub,
                original,
            )?);
        }
        if !observed.iter().any(|capture| {
            capture.origin == InstructionOrigin::Hub
                && capture.name == defaults.entry()
                && capture.body.is_some()
        }) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "selected Hub rule entry missing",
            ));
        }
    }
    let mut candidates: Vec<_> = defaults
        .candidates()
        .iter()
        .map(|candidate| CapturedInstruction {
            name: candidate.name,
            origin: candidate.origin,
            markdown: candidate.markdown,
        })
        .collect();
    for capture in &observed {
        if let Some(body) = capture.body.as_deref() {
            candidates.push(CapturedInstruction {
                name: &capture.name,
                origin: capture.origin,
                markdown: body,
            });
        }
    }
    let broker = crate::instruction_overrides::BrokerInstructions::load(
        issuer,
        policy.installation.root(),
        original,
    )?;
    candidates.extend(broker.candidates(defaults.candidates()));
    let skills: Vec<String> = role
        .modules()
        .iter()
        .map(|module| {
            input
                .skill_aliases
                .get(*module)
                .cloned()
                .map(Ok)
                .unwrap_or_else(|| brand.skill_name(module))
        })
        .collect::<io::Result<_>>()?;
    if input
        .skill_aliases
        .keys()
        .any(|module| !["memory", "task", "ast", "knowledge", "hub"].contains(&module.as_str()))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let skill_refs: Vec<&str> = skills.iter().map(String::as_str).collect();
    let module_skill = input
        .module_skill
        .clone()
        .map(Ok)
        .unwrap_or_else(|| brand.mcp_tool_name(&["module", "skill"]))?;
    let content = hub.content();
    let limits = InstructionLimits {
        max_templates: NonZeroUsize::new(candidates.len().max(9))
            .ok_or(io::ErrorKind::InvalidInput)?,
        max_template_bytes: NonZeroUsize::new(content.max_input_bytes)
            .ok_or(io::ErrorKind::InvalidInput)?,
        max_values: NonZeroUsize::new(content.max_collection_entries)
            .ok_or(io::ErrorKind::InvalidInput)?,
        max_variable_bytes: NonZeroUsize::new(content.max_scalar_bytes)
            .ok_or(io::ErrorKind::InvalidInput)?,
        max_output_bytes: NonZeroUsize::new(content.max_output_bytes)
            .ok_or(io::ErrorKind::InvalidInput)?,
        fuel: NonZeroU64::new(10_000).unwrap(),
        recursion: NonZeroUsize::new(content.max_depth).ok_or(io::ErrorKind::InvalidInput)?,
    };
    let rendered = render_role(
        role,
        brand,
        &candidates,
        RoleAliases {
            module_skill: &module_skill,
            skills: &skill_refs,
        },
        limits,
        original.snapshot(),
    )
    .map_err(io::Error::other)?;
    let descriptor = AgentDescriptor::new(
        rendered.name(),
        rendered.summary(),
        rendered.body(),
        role.read_only(),
        role.allows_shell(),
        &mut renderer,
    )
    .map_err(io::Error::other)?;
    let grants = native_grants(binding, role);
    let native = if matches!(
        binding,
        NativeBinding::KiroV3
            | NativeBinding::OpenCode
            | NativeBinding::Claude
            | NativeBinding::Kimi
    ) {
        use graphit_hub::native::kiro_hooks::{
            KiroHookAction, KiroHookInput, KiroHookTrigger, plan_kiro_hook,
        };
        let server = graphit_hub::native::mcp_config::NativeServerSpec {
            name: if binding == NativeBinding::KiroV3 {
                brand.binary_name().to_owned()
            } else {
                brand.key().to_owned()
            },
            transport: graphit_hub::native::mcp_config::NativeServerTransport::Stdio {
                command: brand.binary_name().to_owned(),
                args: {
                    let mut args = vec!["mcp".into(), "--stdio".into()];
                    if binding == NativeBinding::KiroV3 {
                        args.push("--role".into());
                    }
                    args
                },
                env: BTreeMap::new(),
                env_vars: Vec::new(),
                cwd: None,
            },
        };
        let plan = server.plan(binding).map_err(io::Error::other)?;
        let mut hooks = Vec::new();
        if binding == NativeBinding::KiroV3 {
            for (identifier, event, trigger) in [
                (
                    "project-memory-session",
                    crate::session_hook_entry::Event::SessionStart,
                    KiroHookTrigger::SessionStart,
                ),
                (
                    "project-memory-prompt",
                    crate::session_hook_entry::Event::UserPromptSubmit,
                    KiroHookTrigger::UserPromptSubmit,
                ),
            ] {
                let command = crate::project_hook_install::client::native_command(
                    &policy.installation,
                    if input.destination == InstallDestination::Project {
                        paths
                    } else {
                        None
                    },
                    if input.destination == InstallDestination::Project {
                        input.project_version.as_deref()
                    } else {
                        None
                    },
                    crate::session_hook_entry::Engine::Kiro,
                    event,
                    crate::session_hook_entry::Recipient::Role,
                )?;
                hooks.push(
                    plan_kiro_hook(
                        binding,
                        KiroHookInput {
                            identifier,
                            trigger,
                            matcher: None,
                            action: KiroHookAction::Command(&command),
                            timeout: Some(policy.timeout),
                        },
                        hub.shared(),
                        original,
                    )
                    .map_err(io::Error::other)?,
                );
            }
        }
        render_role_agent_with_mcp(
            binding,
            selected.authority(),
            &descriptor,
            None,
            &grants,
            NativeAgentRuntime {
                mcp: &plan,
                kiro_hooks: &hooks,
            },
            &mut renderer,
        )
    } else {
        render_agent(binding, &descriptor, None, &grants, &mut renderer)
    }
    .map_err(io::Error::other)?;
    let definition = RoleDefinition::tool_owned(
        selected.authority(),
        brand,
        binding,
        &permissions(role),
        native.provenance.input_fingerprint,
        IssuerLimits {
            max_permissions: content.max_collection_entries,
            max_scope_bytes: content.max_input_bytes,
            max_parent_bearer_bytes: content.max_scalar_bytes,
        },
    )
    .map_err(io::Error::other)?;

    original.snapshot().check()?;
    let origin = format!("{:?}", rendered.origin());
    let (name, instructions) = rendered.into_parts();
    Ok(CompiledDefinition {
        name,
        instructions,
        origin,
        native,
        definition,
    })
}

pub(crate) struct TemplateObservation {
    pub(crate) name: String,
    pub(crate) origin: InstructionOrigin,
    pub(crate) body: Option<String>,
}
pub(crate) fn capture_template(
    root: &Path,
    relative: &Path,
    origin: InstructionOrigin,
    original: &RequestContext<'_>,
) -> io::Result<TemplateObservation> {
    original.check()?;
    let parts = relative
        .components()
        .map(|component| match component {
            Component::Normal(value) => Ok(value),
            _ => Err(io::ErrorKind::InvalidInput),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let skip = match origin {
        InstructionOrigin::Project => 2,
        InstructionOrigin::Global => 1,
        _ => 0,
    };
    let name = parts
        .get(skip..)
        .ok_or(io::ErrorKind::InvalidInput)?
        .iter()
        .map(|part| part.to_str().ok_or(io::ErrorKind::InvalidData))
        .collect::<Result<Vec<_>, _>>()?
        .join("/");
    if name.is_empty() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let path = root.join(relative);
    let body = match fs::metadata(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            let mut file = fs::File::open(path)?;
            if !file.metadata()?.is_file() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            let maximum = original.content_limits().max_input_bytes;
            let mut bytes = Vec::new();
            let mut block = [0; 64 * 1024];
            loop {
                original.check()?;
                let count = file.read(&mut block)?;
                if count == 0 {
                    break;
                }
                if count > maximum.saturating_sub(bytes.len()) {
                    return Err(io::ErrorKind::FileTooLarge.into());
                }
                bytes
                    .try_reserve(count)
                    .map_err(|_| io::ErrorKind::OutOfMemory)?;
                bytes.extend_from_slice(&block[..count]);
            }
            Some(
                String::from_utf8(bytes)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            )
        }
    };
    original.check()?;
    Ok(TemplateObservation { name, origin, body })
}
pub(crate) fn permissions(role: ManagedRole) -> Vec<Permission> {
    match role {
        ManagedRole::Scout => vec![
            Permission::Read(Module::Memory),
            Permission::Read(Module::Task),
            Permission::Read(Module::Ast),
            Permission::Read(Module::Knowledge),
            Permission::Read(Module::Hub),
        ],
        ManagedRole::Tracker => vec![
            Permission::Read(Module::Ast),
            Permission::Read(Module::Knowledge),
            Permission::Read(Module::Task),
            Permission::InspectGit,
        ],
        ManagedRole::Scribe => vec![
            Permission::Read(Module::Task),
            Permission::Read(Module::Memory),
            Permission::WriteMemory,
            Permission::WriteOwnTask,
        ],
    }
}
fn native_grants(binding: NativeBinding, role: ManagedRole) -> ToolGrants {
    let names: &[&str] = match binding {
        NativeBinding::Codex | NativeBinding::Cursor => &[],
        NativeBinding::KiroV3 => &["read"],
        NativeBinding::Claude | NativeBinding::Kimi => &["Read", "Grep", "Glob"],
        NativeBinding::Qwen => &["read_file", "grep_search", "glob"],
        NativeBinding::OpenCode => &["read", "grep", "glob"],
        NativeBinding::Antigravity => &["view_file", "grep_search"],
    };
    ToolGrants {
        binding,
        read_only: role.read_only(),
        allows_shell: role.allows_shell(),
        tools: names
            .iter()
            .map(|name| NativeToolGrant {
                name: (*name).into(),
                access: ToolAccess::Read,
            })
            .collect(),
    }
}
