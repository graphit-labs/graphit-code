//! Publish the native hook documents selected by the authenticated installer.
use super::*;
use graphit_core::{
    artifact_store::StoreOwner, project_identity::ProjectId, project_paths::ProjectPaths,
};
use graphit_hub::native::{
    hooks::{CommandHookInput, plan_command_hook},
    mcp_config::NativeMcpPlan,
    opencode_plugin::{
        OpenCodePluginInput, publish_opencode_plugin, remove_opencode_plugin,
        render_opencode_plugin,
    },
};

// Retain every ancestor used by a provider document; the selected pathname is not authority.
pub(super) fn capture_directory(
    target: &Path,
    resources: &InstallResources<'_>,
    original: &RequestContext<'_>,
    before: &impl Fn() -> io::Result<()>,
) -> io::Result<Vec<CapturedDocumentDirectory>> {
    if !target.is_absolute()
        || target
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut ancestors = target.ancestors().collect::<Vec<_>>();
    ancestors.reverse();
    let mut directories: Vec<CapturedDocumentDirectory> = Vec::new();
    for path in ancestors {
        before()?;
        for directory in &directories {
            directory.revalidate(original.snapshot())?;
        }
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.is_dir() && !meta.is_symlink() => {}
            Ok(_) => return Err(io::ErrorKind::InvalidData.into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                graphit_fs::create_private_directory(path)?;
            }
            Err(error) => return Err(error),
        }
        let captured =
            CapturedDocumentDirectory::capture(path, resources.policy.index, original.snapshot())?;
        if captured.canonical_path() != path {
            return Err(io::ErrorKind::InvalidData.into());
        }
        for directory in &directories {
            directory.revalidate(original.snapshot())?;
        }
        before()?;
        directories.push(captured);
    }
    Ok(directories)
}

pub(crate) fn publish(
    resources: &InstallResources<'_>,
    input: &HookRequest,
    project: (&ProjectPaths, &ProjectId, &CapturedPublicationScope),
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
            remove_opencode_plugin(&core, &identifier, original, before)
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
            publish_opencode_plugin(
                &core,
                paths.root(),
                &plan,
                resources.policy.index,
                original,
                before,
            )
        };
        let change = publication
        .inspect_err(|error| {
            if let graphit_hub::native::opencode_plugin::PluginPublicationError::AfterPublication { change, .. } = error {
                outcome.hooks.push(HookEffect { event: "context+compaction".into(), change: format!("{change:?}") });
            }
        })
        .map_err(|error| {
            use graphit_hub::native::opencode_plugin::PluginPublicationError;
            let kind = match &error {
                PluginPublicationError::Input(error) | PluginPublicationError::AfterPublication { cause: error, .. } => error.kind(),
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
        match fs::symlink_metadata(parent_path) {
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
    let directories = capture_directory(parent_path, resources, original, before)?;
    let held = directories.last().ok_or(io::ErrorKind::InvalidInput)?;
    let global_core;
    let core = if global {
        let StoreOwner::Installation { installation_id } =
            resources.auth.installation().live.owner()
        else {
            return Err(io::ErrorKind::PermissionDenied.into());
        };
        let global_scope = CapturedPublicationScope::installation(
            &resources.policy.installation,
            installation_id.clone(),
            [held.canonical_path().to_owned()],
            resources.policy.scope,
            original.snapshot(),
        )?;
        global_core = ArtifactStore::new(global_scope, hub.artifacts());
        &global_core
    } else {
        &core
    };
    let recheck = || -> io::Result<()> {
        before()?;
        for directory in &directories {
            directory.revalidate(original.snapshot())?;
        }
        core.scope()
            .revalidate_bounded(&ScopeContext::from_snapshot(original.snapshot()))?;
        Ok(())
    };
    recheck()?;
    let shared =
        SharedDocumentStore::open(&held.canonical_path().join(".graphit-shared-documents"))?;
    recheck()?;
    let file = config_path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    let file = file.to_str().ok_or(io::ErrorKind::InvalidInput)?;
    let mut writer = shared.writer(held, file, core, hub.shared(), original)?;
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
    match input.operation {
        HookOperation::Install => {
            let changes = writer.sync_command_hooks(&plans, global.then_some(id))?;
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
                let change = writer.remove_command_hook(plan, global.then_some(id))?;
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
    drop(writer);
    if referenced {
        return Ok(Some("RetainedReferences".into()));
    }
    publish_mcp(
        resources,
        input,
        if global {
            held.canonical_path()
        } else {
            paths.root()
        },
        core,
        mcp,
        original,
        &recheck,
    )
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
        match fs::symlink_metadata(path.parent().ok_or(io::ErrorKind::InvalidInput)?) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Some("Unchanged".into()));
            }
            Err(error) => return Err(error),
            Ok(_) => {}
        }
    }
    let directories = capture_directory(
        path.parent().ok_or(io::ErrorKind::InvalidInput)?,
        resources,
        original,
        before,
    )?;
    let held = directories.last().ok_or(io::ErrorKind::InvalidInput)?;
    before()?;
    let shared =
        SharedDocumentStore::open(&held.canonical_path().join(".graphit-shared-documents"))?;
    let mut writer = shared.writer(
        held,
        path.file_name()
            .and_then(OsStr::to_str)
            .ok_or(io::ErrorKind::InvalidInput)?,
        core,
        resources.policy.native_install.hub.shared(),
        original,
    )?;
    before()?;
    for directory in &directories {
        directory.revalidate(original.snapshot())?;
    }
    let change = match input.operation {
        HookOperation::Install => writer.sync(plan)?,
        HookOperation::Remove => writer.remove(plan)?,
    };
    before()?;
    for directory in &directories {
        directory.revalidate(original.snapshot())?;
    }
    Ok(Some(format!("{change:?}")))
}
