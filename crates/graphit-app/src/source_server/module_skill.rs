//! Root skill guidance under native caller authority and current selected configuration.
use super::*;
use graphit_auth::{Obligation, Permission, ProjectReadModule, ProjectRecordAuthority};
use graphit_core::{
    artifact_store::ScopeContext,
    brand::Brand,
    module_settings::{Module, ModuleSettings},
    native_path::NativePath,
};
use graphit_runtime::{
    instructions::{
        CapturedInstruction, InstructionOrigin, InstructionValue, InstructionVariable,
        render_instruction,
    },
    skill_instructions::skill_templates,
};
use std::collections::BTreeMap;

pub(super) fn definition(brand: &Brand) -> io::Result<Tool> {
    Ok(Tool::new(brand.mcp_tool_name(&["module", "skill"])?, "Read one core module skill or its exact framework reference. Omit project_dir for neutral Global guidance; explicit PROJECT and version select current configuration and observed overrides without granting access.",
        json!({"type":"object","properties":{"module":{"type":"string","enum":["task","memory","ast","hub","knowledge"]},"reference":{"type":"string"},"project_dir":{"type":"string"},"project_version":{"type":"string"}},"required":["module"],"additionalProperties":false})
            .as_object().expect("object schema").clone())
        .with_annotations(rmcp::model::ToolAnnotations::new().read_only(true)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    module: String,
    #[serde(default, deserialize_with = "present")]
    reference: Option<String>,
    #[serde(default, deserialize_with = "present")]
    project_dir: Option<NativePath>,
    #[serde(default, deserialize_with = "present")]
    project_version: Option<String>,
}
impl Arguments {
    fn module(&self) -> Result<Module, &'static str> {
        match self.module.trim().to_ascii_lowercase().as_str() {
            "ast" => Ok(Module::Ast),
            "hub" => Ok(Module::Hub),
            "task" => Ok(Module::Task),
            "memory" => Ok(Module::Memory),
            "knowledge" => Ok(Module::Knowledge),
            _ => Err("Module skill module unsupported"),
        }
    }
    fn select(&self, state: &State, projects: &Projects) -> Result<Option<usize>, &'static str> {
        let max = state.policy.content.max_scalar_bytes;
        if self.module.len() > max
            || self
                .reference
                .as_ref()
                .is_some_and(|v| v.len() > max || v.contains('\0'))
            || self
                .project_version
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.len() > max || v.contains('\0'))
        {
            return Err("Module skill argument limit or encoding invalid");
        }
        self.module()?;
        match &self.project_dir {
            None if self.project_version.is_some() => Err("Module skill version requires PROJECT"),
            None => Ok(None),
            Some(root) if !root.as_path().is_absolute() => {
                Err("Module skill PROJECT must be absolute")
            }
            Some(root) => {
                select_project(projects, root.as_path(), self.project_version.as_deref()).map(Some)
            }
        }
    }
}

pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let arguments = call.arguments.unwrap_or_default();
    if serde_json::to_vec(&arguments).map_or(true, |bytes| {
        bytes.len() > server.0.policy.content.max_input_bytes
    }) {
        return failure("Module skill input limit exceeded");
    }
    let input: Arguments = match serde_json::from_value(Value::Object(arguments)) {
        Ok(input) => input,
        Err(_) => return failure("Module skill arguments invalid"),
    };
    let snapshot = match server.0.live_projects.snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => return failure("Module skill PROJECT selection unavailable"),
    };
    let selected = match input.select(&server.0, &snapshot.projects) {
        Ok(selected) => selected,
        Err(message) => return failure(message),
    };
    let Some(parts) = context.extensions.get::<Parts>() else {
        return failure("Module skill incoming HTTP credentials required");
    };
    let local = match parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|v| v.to_str().ok())
    {
        Some("runtime") => false,
        Some("local") => true,
        _ => return failure("Module skill credential origin missing or unsupported"),
    };
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| !v.is_empty())
    else {
        return failure("Module skill bearer credential required");
    };
    let credential = Zeroizing::new(raw.to_owned());
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let original_cancel = cancel.clone();
    let state = Arc::clone(&server.0);
    let deadline = Instant::now() + state.policy.timeout;
    let (send, receive) = oneshot::channel();
    {
        let mut jobs = tokio::select! {
            jobs = state.jobs.lock() => jobs,
            () = context.ct.cancelled() => return failure("Module skill request cancelled"),
            () = state.stop.cancelled() => return failure("Module skill service stopped"),
            () = tokio::time::sleep_until(deadline.into()) => return failure("Module skill deadline expired"),
        };
        jobs.reap();
        if state.stop.is_cancelled() {
            return failure("Module skill service stopped");
        }
        let owner = Arc::clone(&state);
        jobs.tasks.spawn_blocking(move || {
            #[cfg(test)]
            {
                let gate = owner.gate.lock().unwrap().take();
                if let Some((started, waiting)) = gate {
                    started.send(original_cancel.clone()).unwrap();
                    let _ = waiting.recv();
                }
            }
            let result = RequestContext::new(owner.policy.content, &original_cancel, deadline)
                .map_err(|_| "Module skill request context invalid")
                .and_then(|original| {
                    execute(
                        &owner,
                        selected.map(|i| &snapshot.projects[i]),
                        input,
                        &credential,
                        local,
                        &original,
                    )
                });
            let _ = send.send(result);
        });
    }
    tokio::select! {
        result = receive => match result {
            Ok(_) if context.ct.is_cancelled() || cancel.is_cancelled() => failure("Module skill request cancelled"),
            Ok(_) if state.stop.is_cancelled() => failure("Module skill service stopped"),
            Ok(_) if Instant::now() >= deadline => failure("Module skill deadline expired"),
            Ok(Ok(value)) => crate::mcp_output::typed(value),
            Ok(Err(message)) => failure(message),
            Err(_) => failure("Module skill operation failed"),
        },
        () = context.ct.cancelled() => failure("Module skill request cancelled"),
        () = state.stop.cancelled() => failure("Module skill service stopped"),
        () = tokio::time::sleep_until(deadline.into()) => failure("Module skill deadline expired"),
    }
}

fn execute(
    state: &State,
    selection: Option<&(ProjectPaths, ProjectId, VersionContext)>,
    input: Arguments,
    credential: &str,
    local: bool,
    original: &RequestContext<'_>,
) -> Result<Value, &'static str> {
    let module = input.module()?;
    let caller = if local {
        state.auth.issuer().authenticate_local(credential, original)
    } else {
        state
            .auth
            .issuer()
            .authenticate_runtime(credential, original)
    }
    .map_err(|_| "Module skill authentication denied")?;
    if caller
        .authorize(Permission::Read(module))
        .map_err(|_| "Module skill permission denied")?
        != Obligation::Unconditional
    {
        return Err("Module skill unconditional native read permission required");
    }
    let installation = state.auth.installation().live;
    let scope = ScopeContext::from_snapshot(original.snapshot());
    installation
        .revalidate_bounded(&scope)
        .map_err(|_| "Module skill installation changed")?;
    let root = graphit_core::local_files::canonical_directory(state.policy.installation.root())
        .map_err(|_| "Module skill installation mismatch")?;
    if installation.owner_root() != root {
        return Err("Module skill installation mismatch");
    }
    // Later Global reads keep this authorized destination, not a mutable input alias.
    let installation_paths = graphit_core::project_paths::InstallationPaths::resolve(
        state.policy.installation.brand().clone(),
        state.policy.installation.start_directory(),
        state.policy.installation.home_directory(),
        Some(&root),
    )
    .map_err(|_| "Module skill installation mismatch")?;
    let project = selection
        .map(|(paths, id, version)| {
            if !installation
                .allowed_roots()
                .any(|root| root == paths.root())
                || version.scope() != paths.root()
                || !state
                    .live_projects
                    .is_active(paths, *id, version)
                    .map_err(|_| "Module skill PROJECT selection unavailable")?
            {
                return Err("Module skill PROJECT selection unavailable");
            }
            CapturedPublicationScope::project(paths, *id, state.policy.scope, original.snapshot())
                .map_err(|_| "Module skill PROJECT capture failed")
        })
        .transpose()?;
    let binding = project
        .as_ref()
        .map(CapturedPublicationScope::binding)
        .transpose()
        .map_err(|_| "Module skill PROJECT binding failed")?;
    let authority = ProjectRecordAuthority::new(state.auth.issuer());
    let admission = if let (Some((_, _, version)), Some(live), Some(record)) =
        (selection, project.as_ref(), binding.as_ref())
    {
        let read_module = match module {
            Module::Ast => ProjectReadModule::Ast,
            Module::Hub => ProjectReadModule::Hub,
            Module::Task => ProjectReadModule::Task,
            Module::Memory => ProjectReadModule::Memory,
            Module::Knowledge => ProjectReadModule::Knowledge,
            _ => return Err("Module skill module unsupported"),
        };
        Some(
            authority
                .admit_authenticated_read(
                    &caller,
                    original,
                    ScopeInput {
                        installation: state.auth.installation(),
                        project: Some(ProjectCapture {
                            scope: ScopeCapture { live, record },
                            version,
                        }),
                        providers: &[],
                        binding: state.policy.binding,
                        global_version: "",
                    },
                    read_module,
                )
                .map_err(|_| "Module skill PROJECT read admission denied")?,
        )
    } else {
        None
    };
    let policy = state
        .hook
        .as_ref()
        .ok_or("Module skill startup configuration unavailable")?;
    let config = policy
        .selected_configuration(
            &installation_paths,
            selection.map(|s| &s.0),
            original.snapshot(),
        )
        .map_err(|_| "Module skill configuration unavailable")?;
    let enabled = ModuleSettings::resolve(&config)
        .map_err(|_| "Module skill module configuration invalid")?
        .enabled(module);
    let docs_dir = if module == Module::Knowledge && selection.is_some() {
        Some(
            crate::compiled_skills::docs_dir(&config, state.policy.content, original)
                .map_err(|_| "Module skill docs_dir invalid")?,
        )
    } else {
        None
    };
    let set = skill_templates(module).map_err(|_| "Module skill module unsupported")?;
    let reference = input.reference.as_deref().filter(|value| !value.is_empty());
    let entry = match reference {
        Some(reference) => {
            set.references()
                .iter()
                .find(|item| item.relative_path == reference)
                .ok_or("Module skill reference unknown")?
                .entry
        }
        None => set.body_entry(),
    };
    let mut observed = Vec::new();
    for candidate in set.candidates() {
        original
            .check()
            .map_err(|_| "Module skill request cancelled or expired")?;
        if let Some((paths, _, _)) = selection {
            observed.push(
                crate::managed_roles::capture_template(
                    paths.root(),
                    &Path::new(paths.brand().directory())
                        .join("rules")
                        .join(candidate.name),
                    InstructionOrigin::Project,
                    state.policy.index,
                    original,
                )
                .map_err(|_| "Module skill override capture failed")?,
            );
        }
        observed.push(
            crate::managed_roles::capture_template(
                installation_paths.root(),
                &Path::new("rules").join(candidate.name),
                InstructionOrigin::Global,
                state.policy.index,
                original,
            )
            .map_err(|_| "Module skill override capture failed")?,
        );
    }
    let mut candidates: Vec<_> = set
        .candidates()
        .iter()
        .map(|item| CapturedInstruction {
            name: item.name,
            origin: item.origin,
            markdown: item.markdown,
        })
        .collect();
    for item in &observed {
        if let Some(body) = item.body.as_deref() {
            candidates.push(CapturedInstruction {
                name: &item.name,
                origin: item.origin,
                markdown: body,
            });
        }
    }
    let broker = crate::instruction_overrides::BrokerInstructions::load(
        state.auth.issuer(),
        installation_paths.root(),
        original,
    )
    .map_err(|_| "Broker instruction layer unavailable")?;
    candidates.extend(broker.candidates(set.candidates()));
    let brand = installation_paths.brand();
    let mut values = BTreeMap::from([
        ("tool_transport".to_owned(), "root".to_owned()),
        (
            "publication_scope".to_owned(),
            if selection.is_some() {
                "project"
            } else {
                "global"
            }
            .to_owned(),
        ),
        ("brand".to_owned(), brand.key().to_owned()),
        ("brand_display".to_owned(), brand.key().to_owned()),
        (
            "brand_marker".to_owned(),
            brand.environment_prefix().to_owned(),
        ),
    ]);
    if let Some(docs_dir) = &docs_dir {
        values.insert("docs_dir".to_owned(), docs_dir.clone());
    }
    for candidate in set.candidates() {
        for prefix in ["mcp_tool_", "skill_name_"] {
            for (offset, _) in candidate.markdown.match_indices(prefix) {
                let key = candidate.markdown[offset..]
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .next()
                    .expect("prefix");
                let suffix = key.strip_prefix(prefix).expect("prefix");
                let name = if prefix == "mcp_tool_" {
                    brand.mcp_tool_name(&suffix.split('_').collect::<Vec<_>>())
                } else {
                    brand.skill_name(suffix)
                }
                .map_err(|_| "Module skill template name invalid")?;
                values.insert(key.to_owned(), name);
            }
        }
    }
    let variables: Vec<_> = values
        .iter()
        .map(|(name, value)| InstructionVariable {
            name,
            value: InstructionValue::Text(value),
        })
        .collect();
    let rendered = render_instruction(
        entry,
        &candidates,
        &variables,
        policy.instruction_limits(),
        original.snapshot(),
    )
    .map_err(|_| "Module skill render failed")?;
    let mut payload = json!({"module":module.name(),"name":brand.skill_name(module.name()).map_err(|_| "Module skill name invalid")?,"enabled":enabled,"content":rendered.body()});
    if let Some(reference) = reference {
        payload["reference"] = json!(reference);
    } else {
        let mut references: Vec<_> = set
            .references()
            .iter()
            .map(|item| item.relative_path)
            .collect();
        references.sort_unstable();
        if !references.is_empty() {
            payload["references"] = json!(references);
        }
    }
    crate::task_server::browser::codec::check_response(
        &payload,
        state.policy.content.max_output_bytes,
    )
    .map_err(|_| "Module skill output limit exceeded")?;
    for item in &observed {
        item.revalidate(original)
            .map_err(|_| "Module skill override changed")?;
    }
    if let Some(admission) = &admission {
        admission
            .recheck()
            .map_err(|_| "Module skill PROJECT authority changed")?;
    }
    let current = policy
        .selected_configuration(
            &installation_paths,
            selection.map(|s| &s.0),
            original.snapshot(),
        )
        .map_err(|_| "Module skill configuration changed")?;
    let current_enabled = ModuleSettings::resolve(&current)
        .map_err(|_| "Module skill configuration changed")?
        .enabled(module);
    let current_docs = if docs_dir.is_some() {
        Some(
            crate::compiled_skills::docs_dir(&current, state.policy.content, original)
                .map_err(|_| "Module skill configuration changed")?,
        )
    } else {
        None
    };
    if enabled != current_enabled || docs_dir != current_docs {
        return Err("Module skill configuration changed");
    }
    if caller
        .authorize(Permission::Read(module))
        .map_err(|_| "Module skill authority changed")?
        != Obligation::Unconditional
    {
        return Err("Module skill authority changed");
    }
    installation
        .revalidate_bounded(&scope)
        .map_err(|_| "Module skill installation changed")?;
    if let Some(project) = &project {
        project
            .revalidate_bounded(&scope)
            .map_err(|_| "Module skill PROJECT changed")?;
    }
    if let Some((paths, id, version)) = selection
        && !state
            .live_projects
            .is_active(paths, *id, version)
            .map_err(|_| "Module skill PROJECT selection unavailable")?
    {
        return Err("Module skill PROJECT selection unavailable");
    }
    original
        .check()
        .map_err(|_| "Module skill request cancelled or expired")?;
    Ok(payload)
}

#[cfg(test)]
mod tests;
