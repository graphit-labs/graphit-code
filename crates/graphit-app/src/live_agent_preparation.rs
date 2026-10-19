//! Installed text artifacts for Live. Registry metadata never grants storage access.
use crate::knowledge_live::ArtifactSelection;
use graphit_auth::{InstalledHubReadAdmission, ProjectReadAdmission};
use graphit_core::artifact_store::{
    CapturedSourceReader, CopyPolicy, SourceReadLimits, StoreOwner,
};
use graphit_fs::NodeKind;
use graphit_hub::{
    ArtifactType, ConcreteVersion, NativeBinding, SourceBinding, SourceRef,
    policy::HubResourcePolicy,
    registry::{HubRegistry, HubScope, Installation, InstalledPayload},
};
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) struct CapturedHubText {
    pub selection: ArtifactSelection,
    pub reader: CapturedSourceReader,
    installation: Installation,
    selected: TextBinding,
    source_entry: PathBuf,
}

enum TextBinding {
    Original(SourceBinding),
    Derived([u8; 32]),
}

use crate::live_agent_catalog::selection;

fn selected_installation(
    registry: &HubRegistry,
    selected: &ArtifactSelection,
    binding: NativeBinding,
) -> io::Result<Installation> {
    let kind = match selected.artifact_type.as_str() {
        "agent" => ArtifactType::Agent,
        "skill" => ArtifactType::Skill,
        "rule" => ArtifactType::Rule,
        _ => return Err(io::ErrorKind::InvalidInput.into()),
    };
    let version = ConcreteVersion::new(&selected.version, registry.limits().projection())
        .map_err(|cause| io::Error::new(io::ErrorKind::InvalidInput, cause))?;
    let document = registry.load()?;
    let artifact = document
        .get_version(kind, &selected.id, &version)
        .ok_or(io::ErrorKind::NotFound)?;
    let mut matching = artifact
        .installations
        .iter()
        .filter(|installed| installed.target.includes(binding));
    let installed = matching.next().ok_or(io::ErrorKind::NotFound)?;
    if matching.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ambiguous installed Live artifact binding",
        ));
    }
    if selection(&installed.original_source) != *selected {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(installed.clone())
}

fn check_authority(
    authority: &InstalledHubReadAdmission<'_>,
    registry: &HubRegistry,
) -> io::Result<()> {
    let original = authority.request().snapshot();
    original.check()?;
    authority.recheck().map_err(|cause| {
        original
            .check()
            .err()
            .unwrap_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, cause))
    })?;
    let installation = authority.installation();
    let StoreOwner::Installation { installation_id } = installation.live.owner() else {
        return Err(io::ErrorKind::PermissionDenied.into());
    };
    if !matches!(registry.scope(), HubScope::Global { installation_id: id } if id == installation_id.as_str())
        || registry.path().parent() != Some(installation.live.owner_root())
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(())
}

/// Resolve only installed selection metadata, under its actual installation grant.
pub(crate) fn installed_metadata(
    authority: &InstalledHubReadAdmission<'_>,
    registry: &HubRegistry,
    selected: &ArtifactSelection,
    binding: NativeBinding,
) -> io::Result<Installation> {
    check_authority(authority, registry)?;
    let installed = selected_installation(registry, selected, binding)?;
    check_authority(authority, registry)?;
    Ok(installed)
}

fn check_origin(
    authority: &InstalledHubReadAdmission<'_>,
    project: Option<&ProjectReadAdmission<'_>>,
    source: &SourceRef,
) -> io::Result<()> {
    let original = authority.request().snapshot();
    original.check()?;
    if source
        .directory()
        .as_path()
        .starts_with(authority.installation().live.owner_root())
    {
        return Ok(());
    }
    let project = project.ok_or(io::ErrorKind::PermissionDenied)?;
    authority.check_project_source(project).map_err(|cause| {
        original
            .check()
            .err()
            .unwrap_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, cause))
    })?;
    let scope = project
        .project()
        .map_err(|cause| io::Error::new(io::ErrorKind::PermissionDenied, cause))?
        .scope
        .live;
    let StoreOwner::Project { project_id, .. } = scope.owner() else {
        return Err(io::ErrorKind::PermissionDenied.into());
    };
    if *project_id != source.producer()
        || !source.root().as_path().starts_with(scope.owner_root())
        || !source.directory().as_path().starts_with(scope.owner_root())
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    original.check()
}

pub(crate) fn capture_text(
    authority: &InstalledHubReadAdmission<'_>,
    registry: &HubRegistry,
    policy: &HubResourcePolicy,
    selected: &ArtifactSelection,
    binding: NativeBinding,
    project: Option<&ProjectReadAdmission<'_>>,
) -> io::Result<CapturedHubText> {
    let installation = installed_metadata(authority, registry, selected, binding)?;
    check_origin(authority, project, &installation.original_source)?;
    let source = installation.original_source.directory().as_path();
    let (selected_binding, source_entry, copy_policy) = match &installation.payload {
        InstalledPayload::Original {
            selection_binding: Some(selected),
        } => {
            if selected.file_link
                || !selected.input.as_path().starts_with(source)
                || !selected.canonical.as_path().starts_with(source)
            {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            let entry = match installation.original_source.artifact_type() {
                ArtifactType::Skill
                    if selected.kind == NodeKind::Directory
                        && selected.input.as_path() == source
                        && selected.canonical.as_path() == source =>
                {
                    PathBuf::from("SKILL.md")
                }
                ArtifactType::Agent | ArtifactType::Rule if selected.kind == NodeKind::File => {
                    let relative = selected
                        .input
                        .as_path()
                        .strip_prefix(source)
                        .map_err(|_| io::ErrorKind::PermissionDenied)?;
                    if relative.parent() != Some(Path::new("")) || relative.file_name().is_none() {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    relative.to_owned()
                }
                _ => return Err(io::ErrorKind::InvalidData.into()),
            };
            let selected = SourceBinding::restore(selected.clone(), policy.projection())
                .map_err(|cause| io::Error::new(cause.kind(), cause))?;
            selected
                .revalidate()
                .map_err(|cause| io::Error::new(cause.kind(), cause))?;
            (
                TextBinding::Original(selected),
                entry,
                CopyPolicy::RejectFileLinks,
            )
        }
        InstalledPayload::Original {
            selection_binding: None,
        } => return Err(io::ErrorKind::InvalidData.into()),
        InstalledPayload::Derived(payload) => {
            let input = &payload.original_selection;
            let entry = match installation.original_source.artifact_type() {
                ArtifactType::Agent | ArtifactType::Rule if input.kind() == NodeKind::File => {
                    input.relative().to_owned()
                }
                ArtifactType::Skill
                    if input.kind() == NodeKind::Directory
                        && input.relative().as_os_str().is_empty() =>
                {
                    PathBuf::from("SKILL.md")
                }
                _ => return Err(io::ErrorKind::InvalidData.into()),
            };
            let hex = &payload.source_fingerprint;
            if hex.len() != 64 {
                return Err(io::ErrorKind::InvalidData.into());
            }
            let mut fingerprint = [0; 32];
            for (index, byte) in fingerprint.iter_mut().enumerate() {
                let pair = hex
                    .get(index * 2..index * 2 + 2)
                    .ok_or(io::ErrorKind::InvalidData)?;
                *byte = u8::from_str_radix(pair, 16).map_err(|_| io::ErrorKind::InvalidData)?;
            }
            (
                TextBinding::Derived(fingerprint),
                entry,
                payload.source_policy,
            )
        }
    };
    let snapshot = policy.snapshot_policy(copy_policy, [])?;
    let limits = SourceReadLimits::new(
        policy.content().max_input_bytes,
        policy.content().max_scalar_bytes,
        policy.copy().total_file_bytes_limit(),
    )?;
    let reader = CapturedSourceReader::capture(
        source,
        NodeKind::Directory,
        &snapshot,
        limits,
        Arc::clone(authority.request().snapshot()),
    )?;
    if let TextBinding::Derived(expected) = &selected_binding
        && reader.fingerprint() != *expected
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    reader.open_file(&source_entry)?;
    let captured = CapturedHubText {
        selection: selected.clone(),
        reader,
        installation,
        selected: selected_binding,
        source_entry,
    };
    captured.recheck(authority, registry, binding, project)?;
    Ok(captured)
}

impl CapturedHubText {
    /// Entry relative to the captured artifact root; assets retain that same base.
    pub(crate) fn source_entry(&self) -> &Path {
        &self.source_entry
    }

    pub(crate) fn alias(&self) -> Option<&str> {
        self.installation.alias.as_deref().or_else(|| {
            self.installation
                .entry_name
                .as_deref()
                .filter(|name| !name.is_empty())
        })
    }

    /// Recheck metadata and physical bindings without hashing the text tree again.
    pub(crate) fn recheck(
        &self,
        authority: &InstalledHubReadAdmission<'_>,
        registry: &HubRegistry,
        binding: NativeBinding,
        project: Option<&ProjectReadAdmission<'_>>,
    ) -> io::Result<()> {
        check_authority(authority, registry)?;
        check_origin(authority, project, &self.installation.original_source)?;
        self.reader.check_request(authority.request().snapshot())?;
        if !selected_installation(registry, &self.selection, binding)?
            .same_source(&self.installation)
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        match &self.selected {
            TextBinding::Original(selected) => selected
                .revalidate()
                .map_err(|cause| io::Error::new(cause.kind(), cause))?,
            TextBinding::Derived(expected) if self.reader.fingerprint() != *expected => {
                return Err(io::ErrorKind::InvalidData.into());
            }
            TextBinding::Derived(_) => (),
        }
        self.reader.revalidate_bindings()?;
        check_authority(authority, registry)
    }
}

/// Read one instruction through its captured handle; cancellation is never retried as EOF.
impl CapturedHubText {
    pub(crate) fn markdown(&self, maximum: usize) -> io::Result<String> {
        let file = self.reader.open_file(self.source_entry())?;
        let length = usize::try_from(file.len()).map_err(|_| io::ErrorKind::FileTooLarge)?;
        if length > maximum {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let mut bytes = Vec::with_capacity(length);
        let mut stream = file.stream(0)?;
        let mut block = [0_u8; 8192];
        loop {
            let count = stream.read(&mut block)?;
            if count == 0 {
                break;
            }
            if count > maximum.saturating_sub(bytes.len()) {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            bytes.extend_from_slice(&block[..count]);
        }
        String::from_utf8(bytes).map_err(|cause| io::Error::new(io::ErrorKind::InvalidData, cause))
    }
}

fn instruction_error(cause: graphit_runtime::instructions::InstructionError) -> io::Error {
    use graphit_runtime::instructions::InstructionError;
    match cause {
        InstructionError::Request(cause) => cause,
        InstructionError::LimitExceeded(_) => io::Error::new(io::ErrorKind::FileTooLarge, cause),
        _ => io::Error::new(io::ErrorKind::InvalidData, cause),
    }
}

fn content_error(cause: graphit_hub::native::ContentError) -> io::Error {
    use graphit_hub::native::ContentError;
    let kind = match cause {
        ContentError::Cancelled => io::ErrorKind::Interrupted,
        ContentError::TimedOut => io::ErrorKind::TimedOut,
        ContentError::LimitExceeded => io::ErrorKind::FileTooLarge,
        ContentError::UnsupportedCapability => io::ErrorKind::Unsupported,
        _ => io::ErrorKind::InvalidData,
    };
    io::Error::new(kind, cause)
}

/// Render installed Markdown using explicit values; templates never add tool grants.
pub(crate) fn render_markdown(
    markdown: &str,
    variables: &[graphit_runtime::instructions::InstructionVariable<'_>],
    limits: graphit_runtime::instructions::InstructionLimits,
    original: &graphit_core::file_io::SnapshotContext,
) -> io::Result<String> {
    use graphit_runtime::instructions::{
        CapturedInstruction, InstructionOrigin, render_instruction,
    };
    render_instruction(
        "live-artifact.md",
        &[CapturedInstruction {
            name: "live-artifact.md",
            origin: InstructionOrigin::Hub,
            markdown,
        }],
        variables,
        limits,
        original,
    )
    .map(|rendered| rendered.into_body())
    .map_err(instruction_error)
}

impl CapturedHubText {
    #[expect(
        clippy::too_many_arguments,
        reason = "Rendering combines borrowed instructions, exact MCP grants, and the existing request policies"
    )]
    pub(crate) fn render_agent(
        &self,
        markdown: &str,
        rules: &[&str],
        mcp: &graphit_hub::native::mcp_config::NativeMcpPlan,
        has_ast: bool,
        policy: &HubResourcePolicy,
        instructions: graphit_runtime::instructions::InstructionLimits,
        original: &graphit_core::file_io::SnapshotContext,
    ) -> io::Result<graphit_hub::native::RenderedContent> {
        use graphit_hub::native::{
            AgentDescriptor, NativeToolGrant, RenderContext, ToolAccess, ToolGrants,
            render_agent_with_native_mcp,
        };
        use graphit_runtime::instructions::{
            CapturedInstruction, InstructionOrigin, InstructionValue, InstructionVariable,
            render_instruction,
        };
        original.check()?;
        let binding = mcp.binding();
        let mut context = RenderContext::new(
            policy.content(),
            original.cancellation(),
            original.required_deadline()?,
        )
        .map_err(content_error)?;
        let descriptor =
            AgentDescriptor::parse(markdown.as_bytes(), &mut context).map_err(content_error)?;
        let knowledge_tools = [
            crate::live_agent_tools::CONTEXT,
            crate::live_agent_tools::HEADS,
            crate::live_agent_tools::PAGE,
            crate::live_agent_tools::SEARCH,
        ];
        let ast_tools: &[&str] = if has_ast {
            &[
                "graphit_ast_source",
                "graphit_ast_schema",
                "graphit_ast_query",
                "graphit_ast_search",
            ]
        } else {
            &[]
        };
        let composed = render_instruction(
            "live-agent.md",
            &[CapturedInstruction {
                name: "live-agent.md",
                origin: InstructionOrigin::CompiledDefault,
                markdown: include_str!("../../../templates/agent-instructions/live/agent.md"),
            }],
            &[
                InstructionVariable {
                    name: "agent_body",
                    value: InstructionValue::Text(descriptor.body()),
                },
                InstructionVariable {
                    name: "rules",
                    value: InstructionValue::TextList(rules),
                },
                InstructionVariable {
                    name: "live_knowledge_tools",
                    value: InstructionValue::TextList(&knowledge_tools),
                },
                InstructionVariable {
                    name: "live_ast_tools",
                    value: InstructionValue::TextList(ast_tools),
                },
            ],
            instructions,
            original,
        )
        .map_err(instruction_error)?;
        // Host execution policy is readonly regardless of authored descriptor metadata.
        let effective = AgentDescriptor::new(
            descriptor.name(),
            descriptor.description(),
            composed.body(),
            true,
            false,
            &mut context,
        )
        .map_err(content_error)?;
        let grants = ToolGrants {
            binding,
            read_only: true,
            allows_shell: false,
            // Host approval is confined to the actual readonly MCP catalog.
            tools: knowledge_tools
                .iter()
                .chain(ast_tools.iter())
                .copied()
                .map(|name| NativeToolGrant {
                    name: format!("@{}/{name}", mcp.entry()[1]),
                    access: ToolAccess::Read,
                })
                .collect(),
        };
        let rendered = render_agent_with_native_mcp(
            binding,
            &effective,
            self.alias(),
            &grants,
            graphit_hub::native::NativeAgentRuntime {
                mcp,
                kiro_hooks: &[],
            },
            &mut context,
        )
        .map_err(content_error)?;
        original.check()?;
        Ok(rendered)
    }

    pub(crate) fn render_skill(
        &self,
        markdown: &str,
        binding: NativeBinding,
        policy: &HubResourcePolicy,
        original: &graphit_core::file_io::SnapshotContext,
    ) -> io::Result<graphit_hub::native::RenderedContent> {
        use graphit_hub::native::{RenderContext, SkillDescriptor, render_skill};
        let mut context = RenderContext::new(
            policy.content(),
            original.cancellation(),
            original.required_deadline()?,
        )
        .map_err(content_error)?;
        let descriptor =
            SkillDescriptor::parse(markdown.as_bytes(), &mut context).map_err(content_error)?;
        let rendered = render_skill(binding, &descriptor, self.alias(), &mut context)
            .map_err(content_error)?;
        original.check()?;
        Ok(rendered)
    }
}
