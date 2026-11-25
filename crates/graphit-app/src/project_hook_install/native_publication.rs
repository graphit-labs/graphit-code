//! Publish the native hook documents selected by the authenticated installer.
use super::*;
use graphit_core::{
    artifact_store::{
        ArtifactChange, ArtifactEffect, ArtifactKey, ArtifactMode, CopyPolicy, GeneratedNode,
        PayloadMode, PayloadPolicy, StoreOwner, ValidatedPayloadPlan,
    },
    project_identity::ProjectId,
    project_paths::ProjectPaths,
};
use graphit_hub::native::{
    hooks::{CommandHookInput, plan_command_hook},
    mcp_config::NativeMcpPlan,
    opencode_plugin::{
        OpenCodePluginInput, publish_opencode_plugin, remove_opencode_plugin,
        render_opencode_plugin,
    },
};

pub(crate) fn publish(
    resources: &InstallResources<'_>,
    input: &HookRequest,
    project: (&ProjectPaths, &ProjectId, &PublicationScope),
    mcp: Option<&NativeMcpPlan>,
    original: &RequestContext<'_>,
    before: &impl Fn() -> io::Result<()>,
    outcome: &mut HookOutcome,
) -> io::Result<Option<String>> {
    let (paths, id, project_scope) = project;
    let hub = &resources.policy.native_install.hub;
    let core = ArtifactStore::new(project_scope.clone(), hub.artifacts());
    let engine = hook_engine(input.binding);
    let identifier = format!("{}-runtime-context", paths.brand().key());
    if input.binding == NativeBinding::OpenCode {
        let publication = if input.operation == HookOperation::Remove {
            match remove_opencode_plugin(&core, &identifier, original, before) {
                Err(graphit_hub::native::opencode_plugin::PluginPublicationError::Physical(
                    error,
                )) if error.kind() == io::ErrorKind::NotFound
                    && error.effect() == ArtifactEffect::NotApplied =>
                {
                    Ok(ArtifactChange::Unchanged)
                }
                result => result,
            }
        } else {
            let invocation = |event| {
                client::native_invocation(
                    &resources.policy.installation,
                    Some(paths),
                    input.project_version.as_deref(),
                    engine,
                    event,
                    crate::session_hook_entry::Recipient::Coordinator,
                )
            };
            let context = invocation(crate::session_hook_entry::Event::SystemTransform)?;
            let compaction = invocation(crate::session_hook_entry::Event::SessionCompacting)?;
            let idle = invocation(crate::session_hook_entry::Event::Stop)?;
            let deleted = invocation(crate::session_hook_entry::Event::SessionEnd)?;
            let plan = render_opencode_plugin(
                OpenCodePluginInput {
                    identifier: &identifier,
                    context: &context,
                    compaction: &compaction,
                    idle: Some(&idle),
                    deleted: Some(&deleted),
                    timeout: resources.policy.timeout,
                    max_input_bytes: hub
                        .content()
                        .max_input_bytes
                        .min(original.content_limits().max_input_bytes),
                    max_output_bytes: hub
                        .content()
                        .max_output_bytes
                        .min(original.content_limits().max_output_bytes),
                },
                original,
            )?;
            before()?;
            publish_opencode_plugin(&core, paths.root(), &plan, original, before)
        };
        let change = publication
            .inspect_err(|error| {
                use graphit_hub::native::opencode_plugin::PluginPublicationError;
                match error {
                    PluginPublicationError::AfterPublication { change, .. } => {
                        outcome.hooks.push(HookEffect {
                            event: "context+compaction".into(),
                            change: format!("{change:?}"),
                        });
                        outcome.publication = Some("Applied".into());
                    }
                    PluginPublicationError::Physical(error) => {
                        outcome.publication = Some(format!("{:?}", error.effect()));
                    }
                    PluginPublicationError::Input(_) => {}
                }
            })
            .map_err(|error| {
                use graphit_hub::native::opencode_plugin::PluginPublicationError;
                let kind = match &error {
                    PluginPublicationError::Input(error)
                    | PluginPublicationError::AfterPublication { cause: error, .. } => error.kind(),
                    PluginPublicationError::Physical(error) => error.kind(),
                };
                private_error(kind, "OpenCode plugin publication failed", error)
            })?;
        outcome.hooks.push(HookEffect {
            event: "context+compaction".into(),
            change: format!("{change:?}"),
        });
        return publish_mcp(resources, input, paths.root(), &core, mcp, original, before);
    }
    let global = input.binding == NativeBinding::Kimi;
    let config_path = if global {
        input
            .kimi_config_dir
            .as_ref()
            .ok_or(io::ErrorKind::InvalidInput)?
            .join("config.toml")
    } else {
        // The SDK owns the native schema and provider-relative pathname.
        let event = context_events(input.binding)[0];
        let command = client::native_command(
            &resources.policy.installation,
            Some(paths),
            input.project_version.as_deref(),
            engine,
            event,
            crate::session_hook_entry::Recipient::Coordinator,
        )?;
        let plan = plan_command_hook(
            input.binding,
            CommandHookInput {
                identifier: &identifier,
                native_event: event
                    .semantic()
                    .native_name(input.binding)
                    .map_err(io::Error::other)?,
                command: &command,
                matcher: None,
                timeout: Some(resources.policy.timeout),
            },
            hub.shared(),
            original,
        )?;
        paths.root().join(
            plan.project_config_path()
                .ok_or(io::ErrorKind::InvalidInput)?,
        )
    };
    let parent_path = config_path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    if input.operation == HookOperation::Remove {
        before()?;
        match fs::metadata(parent_path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return publish_mcp(
                    resources,
                    input,
                    if global { parent_path } else { paths.root() },
                    &core,
                    mcp,
                    original,
                    before,
                );
            }
            Err(error) => return Err(error),
            Ok(_) => {}
        }
    }
    before()?;
    fs::create_dir_all(parent_path)?;
    before()?;
    let global_core;
    let core = if global {
        let StoreOwner::Installation { installation_id } = resources.auth.installation().owner()
        else {
            return Err(io::ErrorKind::PermissionDenied.into());
        };
        let global_scope = PublicationScope::installation(
            &resources.policy.installation,
            installation_id.clone(),
            [parent_path.to_owned()],
        )?;
        global_core = ArtifactStore::new(global_scope, hub.artifacts());
        &global_core
    } else {
        &core
    };
    let recheck = || -> io::Result<()> {
        before()?;
        original.check()
    };
    recheck()?;
    let shared = SharedDocumentStore::open(&parent_path.join(".graphit-shared-documents"))?;
    recheck()?;
    let file = config_path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    let file = file.to_str().ok_or(io::ErrorKind::InvalidInput)?;
    let mut writer = shared.writer(parent_path, file, core, hub.shared(), original)?;
    let mut referenced = false;
    let mut plans = Vec::new();
    for event in hook_events(input.binding) {
        let command = client::native_command(
            &resources.policy.installation,
            (!global).then_some(paths),
            if global {
                None
            } else {
                input.project_version.as_deref()
            },
            engine,
            event,
            crate::session_hook_entry::Recipient::Coordinator,
        )?;
        let event_name = event
            .semantic()
            .native_name(input.binding)
            .map_err(io::Error::other)?;
        let identifier = format!(
            "{}-{}-context",
            paths.brand().key(),
            event_name.to_ascii_lowercase()
        );
        let plan = plan_command_hook(
            input.binding,
            CommandHookInput {
                identifier: &identifier,
                native_event: event_name,
                command: &command,
                matcher: None,
                timeout: Some(
                    if input.binding == NativeBinding::Codex
                        && matches!(event, crate::session_hook_entry::Event::SessionEnd)
                    {
                        resources
                            .policy
                            .timeout
                            .min(std::time::Duration::from_secs(3))
                    } else {
                        resources.policy.timeout
                    },
                ),
            },
            hub.shared(),
            original,
        )?;
        plans.push(plan);
    }
    recheck()?;
    let sibling_roots: Vec<_> = if global {
        resources
            .projects
            .iter()
            .filter_map(|(other, project, _)| {
                (project == id && other.root() != paths.root()).then_some(other.root())
            })
            .collect()
    } else {
        Vec::new()
    };
    match input.operation {
        HookOperation::Install => {
            let changes = if global {
                writer.sync_checkout_command_hooks(&plans, *id, paths.root(), &sibling_roots)?
            } else {
                writer.sync_command_hooks(&plans, None)?
            };
            outcome
                .hooks
                .extend(plans.iter().zip(changes).map(|(plan, change)| HookEffect {
                    event: plan.native_event().into(),
                    change: format!("{change:?}"),
                }));
            recheck()?;
        }
        HookOperation::Remove => {
            for plan in &plans {
                recheck()?;
                let change = if global {
                    writer.remove_checkout_command_hook(plan, *id, paths.root(), &sibling_roots)?
                } else {
                    writer.remove_command_hook(plan, None)?
                };
                if global {
                    referenced |= writer.command_hook_has_references(plan)?;
                }
                outcome.hooks.push(HookEffect {
                    event: plan.native_event().into(),
                    change: format!("{change:?}"),
                });
                recheck()?;
            }
        }
    }
    if referenced {
        return Ok(Some("RetainedReferences".into()));
    }
    if global {
        // Kimi uses separate documents. Keep the hook reference decision locked
        // until its corresponding MCP change has been published.
        let result = publish_mcp(resources, input, parent_path, core, mcp, original, &recheck);
        drop(writer);
        return result;
    }
    drop(writer);
    if input.binding == NativeBinding::Cursor {
        let limits = resources.policy.native_install.hub.content();
        publish_cursor_workflow(
            core,
            paths,
            input.operation,
            limits,
            original,
            &recheck,
            outcome,
        )?;
    }
    publish_mcp(
        resources,
        input,
        paths.root(),
        core,
        mcp,
        original,
        &recheck,
    )
}

fn publish_cursor_workflow(
    core: &ArtifactStore,
    paths: &ProjectPaths,
    operation: HookOperation,
    content: ContentLimits,
    original: &RequestContext<'_>,
    before: &impl Fn() -> io::Result<()>,
    outcome: &mut HookOutcome,
) -> io::Result<()> {
    const RULE: &str = "---\nalwaysApply: true\n---\n";
    let key = ArtifactKey::new("cursor", "runtime-rule", "workflow")?;
    let destination = paths
        .root()
        .join(".cursor/rules")
        .join(format!("{}-workflow.mdc", paths.brand().key()));
    before()?;
    let mut writer = core
        .writer_with_context(original.snapshot())
        .map_err(io::Error::other)?;
    let change = match operation {
        HookOperation::Install => {
            use graphit_runtime::instructions::InstructionLimits;
            use std::num::{NonZeroU64, NonZeroUsize};
            let nz = |n| NonZeroUsize::new(n).ok_or(io::ErrorKind::InvalidInput);
            let limits = InstructionLimits {
                max_templates: nz(content.max_collection_entries)?,
                max_template_bytes: nz(content.max_input_bytes)?,
                max_values: nz(content.max_nodes)?,
                max_variable_bytes: nz(content.max_scalar_bytes)?,
                max_output_bytes: nz(content.max_output_bytes)?,
                fuel: NonZeroU64::new(10_000).unwrap(),
                recursion: nz(content.max_depth.min(500))?,
            };
            let rule = graphit_runtime::policy_instructions::append_coordinator_mandates(
                RULE.into(),
                paths.brand(),
                limits,
                original.snapshot(),
            )
            .map_err(io::Error::other)?;
            let kind = graphit_fs::NodeKind::File;
            let mode = PayloadMode::private_file();
            let plan = ValidatedPayloadPlan::generated(
                [GeneratedNode::file(
                    PathBuf::new(),
                    rule.into_bytes(),
                    mode,
                )?],
                &PayloadPolicy::new(kind, mode, core.limits().copy_limits()),
                original.snapshot(),
            )?;
            writer
                .ensure_publication_parent(&destination)
                .map_err(io::Error::other)?;
            before()?;
            writer.publish_generated(
                &key,
                &plan,
                &destination,
                kind,
                ArtifactMode::Copy(CopyPolicy::RejectEntryLinks),
            )
        }
        HookOperation::Remove => writer.remove(&key),
    };
    let change = match change {
        Ok(change) => change,
        Err(error)
            if operation == HookOperation::Remove
                && error.kind() == io::ErrorKind::NotFound
                && error.effect() == ArtifactEffect::NotApplied =>
        {
            ArtifactChange::Unchanged
        }
        Err(error) => {
            outcome.publication = Some(format!("{:?}", error.effect()));
            return Err(private_error(
                error.kind(),
                "Cursor workflow rule publication failed",
                error,
            ));
        }
    };
    outcome.hooks.push(HookEffect {
        event: "project-rule".into(),
        change: format!("{change:?}"),
    });
    before()
}

pub(crate) fn publish_mcp(
    resources: &InstallResources<'_>,
    input: &HookRequest,
    root: &Path,
    core: &ArtifactStore,
    mcp: Option<&NativeMcpPlan>,
    original: &RequestContext<'_>,
    before: &impl Fn() -> io::Result<()>,
) -> io::Result<Option<String>> {
    let Some(plan) = mcp else {
        return Ok(None);
    };
    let path = if input.binding == NativeBinding::Kimi {
        root.join("mcp.json")
    } else {
        root.join(
            plan.project_config_path()
                .ok_or(io::ErrorKind::InvalidInput)?,
        )
    };
    if input.operation == HookOperation::Remove {
        before()?;
        match fs::metadata(path.parent().ok_or(io::ErrorKind::InvalidInput)?) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Some("Unchanged".into()));
            }
            Err(error) => return Err(error),
            Ok(_) => {}
        }
    }
    let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    before()?;
    fs::create_dir_all(parent)?;
    before()?;
    let shared = SharedDocumentStore::open(&parent.join(".graphit-shared-documents"))?;
    let mut writer = shared.writer(
        parent,
        path.file_name()
            .and_then(OsStr::to_str)
            .ok_or(io::ErrorKind::InvalidInput)?,
        core,
        resources.policy.native_install.hub.shared(),
        original,
    )?;
    before()?;
    let change = match input.operation {
        HookOperation::Install => writer.sync(plan)?,
        HookOperation::Remove => writer.remove(plan)?,
    };
    before()?;
    Ok(Some(format!("{change:?}")))
}
