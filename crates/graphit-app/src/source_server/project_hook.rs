//! Readonly hook composition under the Source service's existing authentication.
use super::*;
use crate::{
    project_hook_context::{
        ProjectHookInput, TaskContextSelection, UserContext, render_captured_hook,
    },
    session_hook_entry::{Engine, Event, binding_event},
};
use graphit_auth::{
    ProjectReadModule, ProjectRecordAuthority, ServiceUseError, profile::ProfileError,
};
use graphit_core::artifact_store::{StoreOwner, UserIdentity};
use graphit_core::{
    artifact_store::ScopeContext,
    config::{ConfigSnapshot, ConfigSource, ConfigStore},
    module_settings::{Module, ModuleSettings},
    native_path::NativePath,
};
use graphit_records::{memory_store::MemoryListFilter, user_memory_store::NativeUserMemoryStore};
use graphit_records::{
    memory_store::MemoryStoreReadLimits, task_store::TaskStoreReadLimits,
    user_memory_store::UserMemoryListRead,
};
use graphit_runtime::{
    ManagedRole, OutputLimits,
    instructions::{InstructionLimits, InstructionValue, InstructionVariable},
    policy_instructions::{MandateAudience, PolicyPrompt, module_mandate, policy_prompt},
};
use std::collections::BTreeMap;

pub(super) const TOOL: &str = "graphit_session_hook";

pub(crate) struct HookPolicy {
    base: ConfigSnapshot,
    memory: MemoryStoreReadLimits,
    task: TaskStoreReadLimits,
    instructions: InstructionLimits,
    output: OutputLimits,
    variables: BTreeMap<String, String>,
}
impl HookPolicy {
    pub(crate) fn selected_modules(
        &self,
        installation: &InstallationPaths,
        project: &ProjectPaths,
        original: &Arc<graphit_core::file_io::SnapshotContext>,
    ) -> io::Result<ModuleSettings> {
        ModuleSettings::resolve(&self.selected_configuration(
            installation,
            Some(project),
            original,
        )?)
    }
    pub(crate) fn selected_configuration(
        &self,
        installation: &InstallationPaths,
        project: Option<&ProjectPaths>,
        original: &Arc<graphit_core::file_io::SnapshotContext>,
    ) -> io::Result<ConfigSnapshot> {
        original.check()?;
        let layers = [
            ConfigSource::Command,
            ConfigSource::Environment,
            ConfigSource::Distribution,
        ]
        .into_iter()
        .filter_map(|kind| self.base.layer(kind).cloned())
        .chain(std::iter::once(ConfigStore::global(installation)?.load()?))
        .chain(
            project
                .map(|project| ConfigStore::project(project)?.load())
                .transpose()?,
        );
        let config = ConfigSnapshot::new(self.base.brand(), layers)?;
        original.check()?;
        Ok(config)
    }
    pub(crate) fn instruction_limits(&self) -> InstructionLimits {
        self.instructions
    }
    pub(crate) fn memory_limits(&self) -> MemoryStoreReadLimits {
        self.memory
    }
    pub(crate) fn capture(
        source: &SourcePolicy,
        config: &ConfigSnapshot,
        projects: &Projects,
    ) -> io::Result<Self> {
        let modules = projects
            .iter()
            .map(|(paths, _, _)| {
                let mut layers: Vec<_> = [
                    ConfigSource::Distribution,
                    ConfigSource::Global,
                    ConfigSource::Environment,
                    ConfigSource::Command,
                ]
                .into_iter()
                .filter_map(|source| config.layer(source).cloned())
                .collect();
                layers.push(ConfigStore::project(paths)?.load()?);
                ModuleSettings::resolve(&ConfigSnapshot::new(paths.brand(), layers)?)
            })
            .collect::<io::Result<Vec<_>>>()?;
        let limits =
            crate::role_tool_server::RoleToolPolicy::capture(source.clone(), config.clone())?;
        let brand = source.installation.brand();
        let mut variables = BTreeMap::from([
            ("brand_display".into(), brand.key().to_owned()),
            ("brand_marker".into(), brand.environment_prefix().to_owned()),
        ]);
        for role in [
            ManagedRole::Scout,
            ManagedRole::Tracker,
            ManagedRole::Scribe,
        ] {
            variables.insert(
                format!("role_name_{}", role.key()),
                format!("{}-{}", brand.key(), role.key()),
            );
        }
        // Use the maintained template slots and the same name APIs as Role skill
        // dispatch. These names do not attest that a tool is available on /mcp.
        let sets = std::iter::once(policy_prompt(PolicyPrompt::ProjectHookContext)).chain(
            [
                Module::Task,
                Module::Memory,
                Module::Ast,
                Module::Hub,
                Module::Knowledge,
            ]
            .into_iter()
            .filter(|module| modules.iter().any(|settings| settings.enabled(*module)))
            .map(|module| {
                module_mandate(module, MandateAudience::Coordinator)
                    .expect("compiled policy module")
            }),
        );
        for set in sets {
            for candidate in set.candidates() {
                for prefix in ["mcp_tool_", "skill_name_"] {
                    for (offset, _) in candidate.markdown.match_indices(prefix) {
                        let name = candidate.markdown[offset..]
                            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                            .next()
                            .expect("slot prefix");
                        let suffix = name.strip_prefix(prefix).expect("slot prefix");
                        let value = if prefix == "mcp_tool_" {
                            brand.mcp_tool_name(&suffix.split('_').collect::<Vec<_>>())?
                        } else {
                            brand.skill_name(suffix)?
                        };
                        variables.entry(name.to_owned()).or_insert(value);
                    }
                }
            }
        }
        Ok(Self {
            base: config.clone(),
            memory: limits.memory_read,
            task: crate::foreground_tasks::task_policy(
                &source.native_install.hub,
                source.scope,
                source.index,
                source.binding,
            )?
            .read,
            instructions: limits.instructions,
            output: OutputLimits {
                max_context_bytes: source.content.max_output_bytes,
                max_output_bytes: source.content.max_output_bytes,
            },
            variables,
        })
    }
}

pub(super) fn definition() -> Tool {
    Tool::new(TOOL, "Render readonly PROJECT hook context and the current Native Coordinator mandatory USER selection at native context events. Supported quiet lifecycle events dispatch project rescan only with Configure permission and read no corpus. Event, version and native path are selectors; they grant no access. Unsupported host/event combinations are rejected.",
        json!({"type":"object","properties":{"engine":{"enum":Engine::ALL},"event":{"enum":Event::ALL},"project_dir":{"type":"string"},"project_version":{"type":"string"},"task_context":{"type":"object","properties":{"session_id":{"type":"string"},"task_ids":{"type":"array","items":{"type":"string"},"uniqueItems":true}},"required":["session_id"],"additionalProperties":false}},"required":["engine","event"],"additionalProperties":false}).as_object().expect("object schema").clone())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    engine: Engine,
    event: Event,
    #[serde(default, deserialize_with = "present")]
    project_dir: Option<NativePath>,
    #[serde(default, deserialize_with = "present")]
    project_version: Option<String>,
    #[serde(default, deserialize_with = "present")]
    task_context: Option<TaskContextSelection>,
}
impl Arguments {
    fn select(
        &self,
        state: &State,
        snapshot: &crate::live_project_selections::LiveProjectSnapshot,
    ) -> Result<usize, &'static str> {
        let current = snapshot.current.ok_or("PROJECT selection unavailable")?;
        if self.project_version.as_deref().is_some_and(|v| {
            v.is_empty() || v.contains('\0') || v.len() > state.policy.content.max_scalar_bytes
        }) {
            return Err("Session hook project version invalid");
        }
        let root = self
            .project_dir
            .as_ref()
            .map(NativePath::as_path)
            .unwrap_or(snapshot.projects[current].0.root());
        if self.project_version.is_none() && root == snapshot.projects[current].0.root() {
            return Ok(current);
        }
        select_project(&snapshot.projects, root, self.project_version.as_deref())
    }
}

pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let input: Arguments =
        match serde_json::from_value(Value::Object(call.arguments.unwrap_or_default())) {
            Ok(input) => input,
            Err(_) => return failure("Session hook arguments invalid"),
        };
    let snapshot = match server.0.live_projects.snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => return failure("PROJECT selection unavailable"),
    };
    let selected = match input.select(&server.0, &snapshot) {
        Ok(index) => index,
        Err(message) => return failure(message),
    };
    let (paths, id, version) = &snapshot.projects[selected];
    if !matches!(
        server.0.live_projects.is_active(paths, *id, version),
        Ok(true)
    ) {
        return failure("Session hook PROJECT selection unavailable");
    }
    let Some(parts) = context.extensions.get::<Parts>() else {
        return failure("Session hook incoming HTTP credentials required");
    };
    let local = match parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|v| v.to_str().ok())
    {
        Some("runtime") => false,
        Some("local") => true,
        _ => return failure("Session hook credential origin missing or unsupported"),
    };
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| !v.is_empty())
    else {
        return failure("Session hook bearer credential required");
    };
    let credential = Zeroizing::new(raw.to_owned());
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let original_cancel = cancel.clone();
    let state = Arc::clone(&server.0);
    let deadline = Instant::now() + state.policy.timeout;
    let handle = Handle::current();
    let (send, receive) = oneshot::channel();
    {
        let mut jobs = tokio::select! {
            jobs = state.jobs.lock() => jobs,
            () = context.ct.cancelled() => return failure("Session hook request cancelled"),
            () = state.stop.cancelled() => return failure("Session hook service stopped"),
            () = tokio::time::sleep_until(deadline.into()) => return failure("Session hook deadline expired"),
        };
        jobs.reap();
        if state.stop.is_cancelled() {
            return failure("Session hook service stopped");
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
            let result = handle.block_on(execute(
                &owner,
                &snapshot.projects[selected],
                input,
                &credential,
                local,
                &original_cancel,
                deadline,
            ));
            let _ = send.send(result);
        });
    }
    tokio::select! {
        result = receive => match result {
            Ok(_) if context.ct.is_cancelled() || cancel.is_cancelled() => failure("Session hook request cancelled"),
            Ok(_) if state.stop.is_cancelled() => failure("Session hook service stopped"),
            Ok(_) if Instant::now() >= deadline => failure("Session hook deadline expired"),
            Ok(Ok(text)) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Ok(Err(error)) => failure(&error),
            Err(_) => failure("Session hook operation failed"),
        },
        () = context.ct.cancelled() => failure("Session hook request cancelled"),
        () = state.stop.cancelled() => failure("Session hook service stopped"),
        () = tokio::time::sleep_until(deadline.into()) => failure("Session hook deadline expired"),
    }
}

#[allow(clippy::too_many_arguments)] // Separate retained selection, credential and Original controls.
async fn execute(
    state: &State,
    selection: &(
        graphit_core::project_paths::ProjectPaths,
        graphit_core::project_identity::ProjectId,
        graphit_core::version_context::VersionContext,
    ),
    input: Arguments,
    credential: &str,
    local: bool,
    cancel: &Cancellation,
    deadline: Instant,
) -> Result<String, String> {
    let original = RequestContext::new(state.policy.content, cancel, deadline)
        .map_err(|_| "Session hook request context invalid")?;
    if let Some(selection) = &input.task_context {
        selection
            .validate(&original)
            .map_err(|error| error.to_string())?;
    }
    let caller = if local {
        state
            .auth
            .issuer()
            .authenticate_local(credential, &original)
    } else {
        state
            .auth
            .issuer()
            .authenticate_runtime(credential, &original)
    }
    .map_err(|_| "Session hook authentication denied")?;
    let policy = state
        .hook
        .as_ref()
        .ok_or("Session hook startup configuration unavailable")?;
    let (paths, id, version) = selection;
    let modules = policy
        .selected_modules(&state.policy.installation, paths, original.snapshot())
        .map_err(|_| "Session hook project configuration unavailable")?;
    let live =
        CapturedPublicationScope::project(paths, *id, state.policy.scope, original.snapshot())
            .map_err(|_| "Session hook project capture failed")?;
    let record = live
        .binding()
        .map_err(|_| "Session hook project binding failed")?;
    let (binding, event) = binding_event(input.engine, input.event);
    let channel = event
        .channel(binding)
        .map_err(|_| "Session hook boundary unsupported")?;
    let delivers_context = event
        .accepts_context(binding)
        .map_err(|_| "Session hook boundary unsupported")?;
    // Quiet native boundaries carry no record context and must not open any corpus.
    if !delivers_context {
        caller
            .check()
            .map_err(|_| "Session hook authentication changed")?;
        live.revalidate_bounded(&ScopeContext::from_snapshot(original.snapshot()))
            .map_err(|_| "Session hook physical project changed")?;
        if modules.enabled(Module::Hooks)
            && matches!(
                event,
                graphit_runtime::HookEvent::Stop | graphit_runtime::HookEvent::SessionEnd
            )
        {
            caller
                .authorize(graphit_auth::Permission::Configure)
                .map_err(|_| "Session hook rescan permission denied")?;
            state
                .role_tools
                .as_ref()
                .ok_or("Session hook rescan owner unavailable")?
                .request_project_rescan(paths, *id, version)
                .map_err(|_| "Session hook rescan dispatch failed")?;
        }
        let output = graphit_runtime::OutputContext::new(policy.output, original.snapshot())
            .map_err(|_| "Session hook output context invalid")?;
        let encoded = graphit_runtime::encode(
            binding,
            event,
            graphit_runtime::OutputMessage::Quiet,
            &output,
        )
        .map_err(|_| "Session hook output failed")?;
        original
            .snapshot()
            .check()
            .map_err(|_| "Session hook request cancelled or expired")?;
        return String::from_utf8(encoded.into_bytes())
            .map_err(|_| "Session hook output is not UTF8".into());
    }
    if modules.enabled(Module::Hooks)
        && matches!(event, graphit_runtime::HookEvent::UserPromptSubmit)
    {
        caller
            .check()
            .map_err(|_| "Session hook authentication changed")?;
        live.revalidate_bounded(&ScopeContext::from_snapshot(original.snapshot()))
            .map_err(|_| "Session hook physical project changed")?;
        record_human_activity(state, paths, *id, version, &original, cancel, || {
            caller
                .check()
                .map_err(|_| "Session hook authentication changed".to_owned())?;
            live.revalidate_bounded(&ScopeContext::from_snapshot(original.snapshot()))
                .map_err(|_| "Session hook physical project changed".to_owned())
        })?;
    }
    // Only the initial provider lookup can report legitimate Native absence.
    // Once captured, all Native authentication failures remain request failures.
    let native = if !local && modules.enabled(Module::Hooks) && modules.enabled(Module::Memory) {
        match caller.native_provider() {
            Ok(binding) => Some(binding),
            Err(ServiceUseError::Profile(ProfileError::NoActive) | ServiceUseError::WrongMode) => {
                None
            }
            Err(_) => return Err("Session hook USER Native profile unavailable".into()),
        }
    } else {
        None
    };
    let verifier = native
        .map(|(binding, revision)| {
            state
                .native_verifiers
                .as_ref()
                .ok_or("Session hook USER Native verifier unavailable")?
                .get_native(&binding, revision)
                .map_err(|_| "Session hook USER Native verifier failed")
        })
        .transpose()?;
    let caller = if let Some(verifier) = &verifier {
        state
            .auth
            .issuer()
            .prepare_native_record_access(&caller)
            .await
            .map_err(|_| "Session hook USER Native refresh failed")?;
        caller
            .authenticate_native(verifier)
            .await
            .map_err(|_| "Session hook USER Native authentication failed")?
    } else {
        caller
    };
    let user_live = if verifier.is_some() {
        let verified = state
            .auth
            .issuer()
            .user_from_authenticated(&caller)
            .map_err(|_| "Session hook USER subject denied")?;
        let StoreOwner::Installation { installation_id } = state.auth.installation().live.owner()
        else {
            return Err("Session hook installation owner invalid".into());
        };
        let identity = UserIdentity::new(
            verified
                .issuer()
                .map_err(|_| "Session hook USER subject changed")?
                .to_owned(),
            verified
                .subject()
                .map_err(|_| "Session hook USER subject changed")?
                .to_owned(),
        )
        .map_err(|_| "Session hook USER identity invalid")?;
        Some(
            CapturedPublicationScope::user(
                &state.policy.installation,
                installation_id.clone(),
                identity,
                state.policy.scope,
                original.snapshot(),
            )
            .map_err(|_| "Session hook USER capture failed")?,
        )
    } else {
        None
    };
    let user_record = user_live
        .as_ref()
        .map(|live| live.binding())
        .transpose()
        .map_err(|_| "Session hook USER binding failed")?;
    let user_admission = user_live
        .as_ref()
        .zip(user_record.as_ref())
        .map(|(live, record)| {
            state
                .auth
                .issuer()
                .admit_authenticated_user_memory_read(
                    &caller,
                    &original,
                    ScopeCapture { live, record },
                )
                .map_err(|_| "Session hook USER Memory permission denied")
        })
        .transpose()?;
    let authority = ProjectRecordAuthority::new(state.auth.issuer());
    let admission = if modules.enabled(Module::Hooks) && modules.enabled(Module::Memory) {
        Some(
            authority
                .admit_authenticated_read(
                    &caller,
                    &original,
                    ScopeInput {
                        installation: state.auth.installation(),
                        project: Some(ProjectCapture {
                            scope: ScopeCapture {
                                live: &live,
                                record: &record,
                            },
                            version,
                        }),
                        providers: &[],
                        binding: state.policy.binding,
                        global_version: "",
                    },
                    ProjectReadModule::Memory,
                )
                .map_err(|_| "Session hook Memory permission denied")?,
        )
    } else {
        None
    };
    let task_admission = if modules.enabled(Module::Hooks)
        && modules.enabled(Module::Task)
        && input.task_context.is_some()
    {
        Some(
            authority
                .admit_authenticated_read(
                    &caller,
                    &original,
                    ScopeInput {
                        installation: state.auth.installation(),
                        project: Some(ProjectCapture {
                            scope: ScopeCapture {
                                live: &live,
                                record: &record,
                            },
                            version,
                        }),
                        providers: &[],
                        binding: state.policy.binding,
                        global_version: "",
                    },
                    ProjectReadModule::Task,
                )
                .map_err(|_| "Session hook Task permission denied")?,
        )
    } else {
        None
    };
    let user_memory: Option<UserMemoryListRead> = if let Some(admission) = &user_admission {
        Some(
            NativeUserMemoryStore::list(
                original.snapshot(),
                admission,
                MemoryListFilter {
                    mandatory: Some(true),
                    ..Default::default()
                },
                policy.memory,
            )
            .await
            .map_err(|_| "Session hook USER Memory read failed")?,
        )
    } else {
        None
    };
    let variables: Vec<_> = policy
        .variables
        .iter()
        .map(|(name, value)| InstructionVariable {
            name,
            value: InstructionValue::Text(value),
        })
        .chain(std::iter::once(InstructionVariable {
            name: "project_version",
            value: InstructionValue::Text(version.version()),
        }))
        .collect();
    let encoded = render_captured_hook(
        &original,
        ProjectHookInput {
            task_context: input.task_context.as_ref(),
            binding,
            event,
            modules: &modules,
            audience: MandateAudience::Coordinator,
            variables: &variables,
            instruction_limits: policy.instructions,
            output_limits: policy.output,
        },
        admission
            .as_ref()
            .map(|admission| (admission, policy.memory)),
        task_admission
            .as_ref()
            .map(|admission| (admission, policy.task)),
        Some(match &user_memory {
            Some(read) => UserContext::Captured(read),
            None => UserContext::Unavailable,
        }),
    )
    .await
    .map_err(|error| error.to_string())?;
    if encoded.channel() != channel {
        return Err("Session hook output channel invalid".into());
    }
    caller
        .check()
        .map_err(|_| "Session hook authentication changed")?;
    live.revalidate_bounded(&ScopeContext::from_snapshot(original.snapshot()))
        .map_err(|_| "Session hook physical project changed")?;
    if let Some(admission) = &user_admission {
        admission
            .recheck()
            .map_err(|_| "Session hook USER authority changed")?;
    }
    original
        .snapshot()
        .check()
        .map_err(|_| "Session hook request cancelled or expired")?;
    String::from_utf8(encoded.into_bytes()).map_err(|_| "Session hook output is not UTF8".into())
}

fn record_human_activity(
    state: &State,
    paths: &graphit_core::project_paths::ProjectPaths,
    id: graphit_core::project_identity::ProjectId,
    version: &graphit_core::version_context::VersionContext,
    original: &RequestContext<'_>,
    cancel: &Cancellation,
    revalidate: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if state.stop.is_cancelled() {
        return Err("Session hook Source stopped".into());
    }
    if cancel.is_cancelled() {
        return Err("Session hook request cancelled or expired".into());
    }
    original
        .snapshot()
        .check()
        .map_err(|_| "Session hook request cancelled or expired")?;
    if !matches!(state.live_projects.is_active(paths, id, version), Ok(true)) {
        return Err("Session hook PROJECT selection retired or unavailable".into());
    }
    if state.human_activity.is_none() && state.user_activity.is_none() {
        return Ok(());
    }
    #[cfg(test)]
    {
        let gate = state.gate.lock().unwrap().take();
        if let Some((started, waiting)) = gate {
            started.send(cancel.clone()).unwrap();
            let _ = waiting.recv();
        }
    }
    let selected = state
        .human_activity
        .as_ref()
        .map(|activity| activity.read())
        .transpose()
        .map_err(|_| "Session hook activity unavailable")?;
    let project_sender = selected
        .as_ref()
        .and_then(|selected| selected.as_ref())
        .filter(|(context, _)| {
            context.id == id && context.root() == paths.root() && &context.version == version
        })
        .map(|(_, sender)| sender);
    if project_sender.is_some() || state.user_activity.is_some() {
        original
            .snapshot()
            .check()
            .map_err(|_| "Session hook request cancelled or expired")?;
        if state.stop.is_cancelled() {
            return Err("Session hook Source stopped".into());
        }
        revalidate()?;
        if !matches!(state.live_projects.is_active(paths, id, version), Ok(true)) {
            return Err("Session hook PROJECT selection retired or unavailable".into());
        }
        let observed = Instant::now();
        if let Some(sender) = project_sender {
            sender.send_replace(observed);
        }
        if let Some(sender) = &state.user_activity {
            sender.send_replace(Some(observed));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    mod hosts;
    mod task_context;
    #[cfg(target_os = "linux")]
    mod user_activity;
    mod user_context;
    use super::*;
    use graphit_core::{
        brand::Brand,
        config::{ConfigDocument, ConfigKey, ConfigSource},
    };
    use rmcp::{
        model::ProtocolVersion,
        service::{ClientLifecycleMode, ClientServiceExt},
        transport::{
            StreamableHttpClientTransport,
            streamable_http_client::StreamableHttpClientTransportConfig,
        },
    };

    fn configured(
        permissions: &[graphit_auth::Permission],
        modules: Value,
    ) -> super::super::test_fixture::Fixture {
        configured_fixture(
            super::super::test_fixture::fixture(
                permissions,
                Brand::default(),
                NativeBinding::KiroV3,
            ),
            modules,
        )
    }

    fn configured_fixture(
        mut fixture: super::super::test_fixture::Fixture,
        modules: Value,
    ) -> super::super::test_fixture::Fixture {
        let config = ConfigSnapshot::new(
            fixture.paths.brand(),
            [ConfigDocument::new(
                fixture.paths.brand(),
                ConfigSource::Command,
                modules.as_object().unwrap().iter().map(|(name, value)| {
                    (
                        ConfigKey::new(&format!("modules.{name}")).unwrap(),
                        value.clone(),
                    )
                }),
            )
            .unwrap()],
        )
        .unwrap();
        fixture.server.attach_project_hook(&config).unwrap();
        fixture
    }
    async fn client(
        fixture: &super::super::test_fixture::Fixture,
    ) -> (
        rmcp::service::RunningService<rmcp::RoleClient, ()>,
        tokio::task::JoinHandle<io::Result<()>>,
    ) {
        client_origin(fixture, "runtime").await
    }
    async fn client_origin(
        fixture: &super::super::test_fixture::Fixture,
        origin: &'static str,
    ) -> (
        rmcp::service::RunningService<rmcp::RoleClient, ()>,
        tokio::task::JoinHandle<io::Result<()>>,
    ) {
        let listener = graphit_core::tcp_listener::reserve(
            graphit_core::tcp_listener::Options {
                address: std::net::Ipv4Addr::LOCALHOST.into(),
                ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            &Cancellation::default(),
        )
        .unwrap();
        let uri = format!("http://{}/mcp", listener.address());
        let original = graphit_core::file_io::SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        graphit_daemon::source_discovery::publish_discovery(
            &fixture.auth,
            &listener,
            fixture.server.0.policy.index,
            &original,
        )
        .unwrap();
        let server = fixture.server.clone();
        let serving = tokio::spawn(async move { server.serve(listener).await });
        let mut bearer =
            axum::http::HeaderValue::from_str(&format!("Bearer {}", fixture.raw)).unwrap();
        bearer.set_sensitive(true);
        let headers = std::collections::HashMap::from([
            (axum::http::header::AUTHORIZATION, bearer),
            (
                axum::http::HeaderName::from_static("x-graphit-auth-origin"),
                axum::http::HeaderValue::from_static(origin),
            ),
        ]);
        let client = ()
            .serve_with_lifecycle(
                StreamableHttpClientTransport::with_client(
                    reqwest::Client::new(),
                    StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers),
                ),
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::LATEST],
                },
            )
            .await
            .unwrap();
        (client, serving)
    }
    fn request(value: Value) -> CallToolRequestParams {
        CallToolRequestParams::new(TOOL).with_arguments(value.as_object().unwrap().clone())
    }
    #[cfg(target_os = "linux")]
    fn attach_activity(
        fixture: &mut super::super::test_fixture::Fixture,
    ) -> (
        super::super::HumanActivitySlot,
        tokio::sync::watch::Receiver<Instant>,
    ) {
        let selected = &fixture.projects[0];
        let context = graphit_daemon::supervisor::ProjectContext {
            id: selected.1,
            version: selected.2.clone(),
        };
        let (sender, receiver) = tokio::sync::watch::channel(Instant::now());
        let activity = Arc::new(std::sync::RwLock::new(Some((context, sender))));
        fixture
            .server
            .attach_human_activity(Arc::clone(&activity))
            .unwrap();
        (activity, receiver)
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn root_user_prompt_activity_is_exact_and_independent_of_task() {
        let mut fixture = configured_fixture(
            super::super::test_fixture::fixture_with_projects(
                &[],
                Brand::default(),
                NativeBinding::KiroV3,
                &["project", "second"],
            ),
            json!({"memory":false,"task":false}),
        );
        let (activity, receiver) = attach_activity(&mut fixture);
        let (connection, serving) = client(&fixture).await;
        let before = *receiver.borrow();
        for event in ["SessionStart"] {
            let result = connection
                .call_tool(request(json!({"engine":"kiro","event":event})))
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(false));
            assert_eq!(*receiver.borrow(), before);
        }
        let result = connection
            .call_tool(request(json!({"engine":"kiro","event":"UserPromptSubmit"})))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(false));
        let from_mcp = *receiver.borrow();
        assert!(from_mcp > before);
        let cancel = Cancellation::default();
        let original = RequestContext::new(
            fixture.server.0.policy.content,
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let transport_policy = graphit_hub::policy::HubResourcePolicy::resolve(
            &fixture.server.0.hook.as_ref().unwrap().base,
        )
        .unwrap();
        let arguments = crate::session_hook_entry::Arguments {
            recipient: crate::session_hook_entry::Recipient::Coordinator,
            engine: Engine::Kiro,
            event: Event::UserPromptSubmit,
            project_dir: Some(fixture.projects[0].0.root().to_owned()),
            task_session: None,
            task_ids: Vec::new(),
        };
        let mut cli_bytes = Vec::new();
        crate::session_hook_entry::call(
            &fixture.paths,
            arguments.recipient,
            crate::session_hook_entry::prepare(
                &fixture.paths,
                &arguments,
                Some(fixture.projects[0].2.version()),
                &original,
            )
            .unwrap(),
            fixture.server.0.policy.index,
            &transport_policy,
            &original,
            &mut cli_bytes,
        )
        .await
        .unwrap();
        assert!(
            String::from_utf8(cli_bytes)
                .unwrap()
                .contains("PROJECT Memory state: disabled.")
        );
        let accepted = *receiver.borrow();
        assert!(accepted > from_mcp);
        for value in [
            json!({"engine":"kiro","event":"UserPromptSubmit","project_version":"missing"}),
            json!({"engine":"kiro","event":"UserPromptSubmit","recipient":"role"}),
        ] {
            assert_eq!(
                connection.call_tool(request(value)).await.unwrap().is_error,
                Some(true)
            );
            assert_eq!(*receiver.borrow(), accepted);
        }
        let live = fixture.server.0.live_projects.snapshot().unwrap();
        let sibling = live
            .projects
            .iter()
            .enumerate()
            .find(|(_, selected)| selected.0.root() != fixture.projects[0].0.root())
            .unwrap();
        let (sender, sibling_receiver) = tokio::sync::watch::channel(Instant::now());
        let sibling_before = *sibling_receiver.borrow();
        *activity.write().unwrap() = Some((
            graphit_daemon::supervisor::ProjectContext {
                id: sibling.1.1,
                version: sibling.1.2.clone(),
            },
            sender,
        ));
        assert_eq!(
            connection
                .call_tool(request(json!({"engine":"kiro","event":"UserPromptSubmit"})))
                .await
                .unwrap()
                .is_error,
            Some(false)
        );
        assert_eq!(*sibling_receiver.borrow(), sibling_before);
        fixture
            .server
            .0
            .live_projects
            .replace(Arc::clone(&live.projects), sibling.0)
            .unwrap();
        assert_eq!(
            connection
                .call_tool(request(json!({"engine":"kiro","event":"UserPromptSubmit"})))
                .await
                .unwrap()
                .is_error,
            Some(false)
        );
        let sibling_accepted = *sibling_receiver.borrow();
        assert!(sibling_accepted > sibling_before);
        *activity.write().unwrap() = None;
        assert_eq!(
            connection
                .call_tool(request(json!({"engine":"kiro","event":"UserPromptSubmit"})))
                .await
                .unwrap()
                .is_error,
            Some(false)
        );
        assert_eq!(*sibling_receiver.borrow(), sibling_accepted);
        connection.cancel().await.unwrap();
        fixture.server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn root_user_prompt_activity_survives_later_bootstrap_denial() {
        let mut fixture = configured(&[], json!({}));
        let (_, receiver) = attach_activity(&mut fixture);
        let (connection, serving) = client(&fixture).await;
        let before = *receiver.borrow();
        let result = connection
            .call_tool(request(json!({"engine":"kiro","event":"UserPromptSubmit"})))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(
            result.content[0]
                .as_text()
                .unwrap()
                .text
                .contains("Memory permission denied")
        );
        assert!(*receiver.borrow() > before);
        connection.cancel().await.unwrap();
        fixture.server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn root_user_prompt_activity_rejects_role_origin_and_disabled_hooks() {
        for (origin, hooks) in [("role", true), ("runtime", false)] {
            let mut fixture = configured(&[], json!({"memory":false,"task":false,"hooks":hooks}));
            let (_, receiver) = attach_activity(&mut fixture);
            let (connection, serving) = client_origin(&fixture, origin).await;
            let before = *receiver.borrow();
            let result = connection
                .call_tool(request(json!({"engine":"kiro","event":"UserPromptSubmit"})))
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(origin == "role"));
            assert_eq!(*receiver.borrow(), before);
            connection.cancel().await.unwrap();
            fixture.server.shutdown().await.unwrap();
            serving.await.unwrap().unwrap();
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn root_user_prompt_activity_cancel_deadline_and_retirement_before_pulse() {
        use rmcp::model::{CallToolRequest, ClientRequest};
        use std::sync::mpsc;
        for mode in ["cancel", "deadline", "retire"] {
            let mut fixture = configured(&[], json!({"memory":false,"task":false}));
            if mode == "deadline" {
                Arc::get_mut(&mut fixture.server.0).unwrap().policy.timeout =
                    Duration::from_millis(100);
            }
            let (_, receiver) = attach_activity(&mut fixture);
            let (connection, serving) = client(&fixture).await;
            let before = *receiver.borrow();
            let (started, observed) = mpsc::channel();
            let (release, waiting) = mpsc::channel();
            *fixture.server.0.gate.lock().unwrap() = Some((started, waiting));
            let pending = connection
                .send_cancellable_request(
                    ClientRequest::CallToolRequest(CallToolRequest::new(request(
                        json!({"engine":"kiro","event":"UserPromptSubmit"}),
                    ))),
                    Default::default(),
                )
                .await
                .unwrap();
            let cancelled = tokio::task::spawn_blocking(move || {
                observed.recv_timeout(Duration::from_secs(5)).unwrap()
            })
            .await
            .unwrap();
            let response = match mode {
                "cancel" => {
                    pending.cancel(None).await.unwrap();
                    None
                }
                "deadline" => Some(pending.await_response().await.unwrap()),
                "retire" => {
                    fixture
                        .server
                        .0
                        .live_projects
                        .retire_checkout(fixture.projects[0].1, fixture.projects[0].0.root())
                        .unwrap();
                    release.send(()).unwrap();
                    Some(pending.await_response().await.unwrap())
                }
                _ => unreachable!(),
            };
            if mode != "retire" {
                tokio::time::timeout(Duration::from_secs(5), async {
                    while !cancelled.is_cancelled() {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
                release.send(()).unwrap();
            }
            fixture.server.shutdown().await.unwrap();
            connection.cancel().await.unwrap();
            serving.await.unwrap().unwrap();
            assert_eq!(*receiver.borrow(), before);
            assert!(fixture.server.0.jobs.lock().await.tasks.is_empty());
            if let Some(rmcp::model::ServerResult::CallToolResult(result)) = response {
                assert_eq!(result.is_error, Some(true));
            } else {
                assert_eq!(mode, "cancel");
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn root_user_prompt_activity_invalidated_while_waiting_for_activity_lock() {
        use rmcp::model::{CallToolRequest, ClientRequest};
        use std::sync::mpsc;
        for mode in ["retire", "physical"] {
            let mut fixture = configured(&[], json!({"memory":false,"task":false}));
            let (activity, receiver) = attach_activity(&mut fixture);
            let (connection, serving) = client(&fixture).await;
            let before = *receiver.borrow();
            let (started, observed) = mpsc::channel();
            let (release, waiting) = mpsc::channel();
            *fixture.server.0.gate.lock().unwrap() = Some((started, waiting));
            let pending = connection
                .send_cancellable_request(
                    ClientRequest::CallToolRequest(CallToolRequest::new(request(
                        json!({"engine":"kiro","event":"UserPromptSubmit"}),
                    ))),
                    Default::default(),
                )
                .await
                .unwrap();
            tokio::task::spawn_blocking(move || {
                observed.recv_timeout(Duration::from_secs(5)).unwrap()
            })
            .await
            .unwrap();
            let (entered, checkpoint) = mpsc::channel();
            let (resume, blocked) = mpsc::channel();
            *fixture.server.0.gate.lock().unwrap() = Some((entered, blocked));
            let (locked, held) = mpsc::channel();
            let (unlock, unlock_wait) = mpsc::channel();
            let holder = std::thread::spawn(move || {
                let _guard = activity.write().unwrap();
                locked.send(()).unwrap();
                unlock_wait.recv().unwrap();
            });
            tokio::task::spawn_blocking(move || held.recv_timeout(Duration::from_secs(5)).unwrap())
                .await
                .unwrap();
            release.send(()).unwrap();
            tokio::task::spawn_blocking(move || {
                checkpoint.recv_timeout(Duration::from_secs(5)).unwrap()
            })
            .await
            .unwrap();
            resume.send(()).unwrap();
            let moved = fixture.temp.path().join("moved-during-hook");
            if mode == "retire" {
                fixture
                    .server
                    .0
                    .live_projects
                    .retire_checkout(fixture.projects[0].1, fixture.projects[0].0.root())
                    .unwrap();
            } else {
                std::fs::rename(fixture.projects[0].0.root(), &moved).unwrap();
            }
            unlock.send(()).unwrap();
            holder.join().unwrap();
            let rmcp::model::ServerResult::CallToolResult(result) =
                pending.await_response().await.unwrap()
            else {
                panic!("expected hook result")
            };
            assert_eq!(result.is_error, Some(true));
            assert_eq!(*receiver.borrow(), before);
            if mode == "physical" {
                std::fs::rename(moved, fixture.projects[0].0.root()).unwrap();
            }
            connection.cancel().await.unwrap();
            fixture.server.shutdown().await.unwrap();
            serving.await.unwrap().unwrap();
            assert!(fixture.server.0.jobs.lock().await.tasks.is_empty());
        }
    }

    #[tokio::test]
    async fn real_source_kiro_pairs_return_complete_plaintext() {
        let fixture = configured(&[graphit_auth::Permission::Read(Module::Memory)], json!({}));
        assert!(fixture.server.get_tool(TOOL).is_some());
        let (client, serving) = client(&fixture).await;
        let mut outputs = Vec::new();
        for event in ["SessionStart", "UserPromptSubmit"] {
            let result = client
                .call_tool(request(json!({"engine":"kiro","event":event})))
                .await
                .unwrap();
            assert_eq!(
                result.is_error,
                Some(false),
                "{}",
                result.content[0].as_text().unwrap().text
            );
            let text = &result.content[0].as_text().unwrap().text;
            assert!(text.contains("PROJECT Memory state: no_corpus."));
            assert!(text.contains("graphit_task_session_get"));
            assert!(text.contains("graphit-memory"));
            assert!(text.contains("USER Memory was not loaded"));
            outputs.push(text.clone());
        }
        assert_eq!(outputs[0], outputs[1]);
        assert_eq!(outputs[0], outputs[1]);
        for value in [
            json!({"engine":"kiro","event":"SessionStart","v3":true}),
            json!({"engine":"kiro","event":"SubagentStart"}),
            json!({"engine":"kiro","event":"SessionStart","project_version":"missing"}),
            json!({"engine":"kiro","event":"SessionStart","project_dir":null}),
            json!({"engine":"kiro","event":"SessionStart","scope":"caller-grant"}),
        ] {
            assert_eq!(
                client.call_tool(request(value)).await.unwrap().is_error,
                Some(true)
            );
        }
        client.cancel().await.unwrap();
        fixture.server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
    #[tokio::test]
    async fn memory_permission_is_required_only_for_enabled_memory_read() {
        let fixture = configured(&[], json!({}));
        let (connection, serving) = client(&fixture).await;
        let response = connection
            .call_tool(request(json!({"engine":"kiro","event":"SessionStart"})))
            .await
            .unwrap();
        assert_eq!(response.is_error, Some(true));
        assert!(
            response.content[0]
                .as_text()
                .unwrap()
                .text
                .contains("Memory permission denied")
        );
        connection.cancel().await.unwrap();
        fixture.server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
        for modules in [json!({"memory":false}), json!({"hooks":false})] {
            let fixture = configured(&[], modules.clone());
            let (connection, serving) = client(&fixture).await;
            let response = connection
                .call_tool(request(json!({"engine":"kiro","event":"SessionStart"})))
                .await
                .unwrap();
            assert_eq!(response.is_error, Some(false));
            let text = &response.content[0].as_text().unwrap().text;
            if modules["hooks"] == false {
                assert!(text.trim().is_empty());
            } else {
                assert!(text.contains("PROJECT Memory state: disabled."));
            }
            connection.cancel().await.unwrap();
            fixture.server.shutdown().await.unwrap();
            serving.await.unwrap().unwrap();
        }
    }

    #[tokio::test]
    async fn project_gates_and_template_aliases_are_selected_independently() {
        for disabled in ["memory", "hooks"] {
            for disabled_index in [0, 1] {
                let mut fixture = super::super::test_fixture::fixture_with_projects(
                    &[graphit_auth::Permission::Read(Module::Memory)],
                    Brand::default(),
                    NativeBinding::KiroV3,
                    &["project", "second"],
                );
                let cancel = Cancellation::default();
                let store = ConfigStore::project(&fixture.projects[disabled_index].0).unwrap();
                store
                    .set(
                        ConfigKey::new(&format!("modules.{disabled}")).unwrap(),
                        json!(false),
                        Duration::from_secs(5),
                        &cancel,
                    )
                    .unwrap();
                store
                    .set(
                        ConfigKey::new("modules.task").unwrap(),
                        json!(false),
                        Duration::from_secs(5),
                        &cancel,
                    )
                    .unwrap();
                fixture
                    .server
                    .attach_project_hook(&ConfigSnapshot::new(fixture.paths.brand(), []).unwrap())
                    .unwrap();
                let (connection, serving) = client(&fixture).await;
                for (index, (paths, _, version)) in fixture.projects.iter().enumerate() {
                    let response = connection
                        .call_tool(request(json!({
                            "engine":"kiro", "event":"SessionStart",
                            "project_dir":NativePath::new(paths.root().to_owned()).unwrap(),
                            "project_version":version.version()
                        })))
                        .await
                        .unwrap();
                    assert_eq!(response.is_error, Some(false), "{response:?}");
                    let text = &response.content[0].as_text().unwrap().text;
                    if index == disabled_index && disabled == "hooks" {
                        assert!(text.is_empty());
                    } else {
                        let state = if index == disabled_index {
                            "disabled"
                        } else {
                            "no_corpus"
                        };
                        assert!(text.contains(&format!("PROJECT Memory state: {state}.")));
                        assert_eq!(
                            text.contains("graphit_task_session_get"),
                            index != disabled_index
                        );
                    }
                }
                connection.cancel().await.unwrap();
                fixture.server.shutdown().await.unwrap();
                serving.await.unwrap().unwrap();
            }
        }
    }

    #[tokio::test]
    async fn real_cli_entry_delivers_exact_bytes_and_propagates_write_flush_errors() {
        struct FailedWriter(bool);
        impl io::Write for FailedWriter {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.0 {
                    Ok(bytes.len())
                } else {
                    Err(io::ErrorKind::BrokenPipe.into())
                }
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
        }
        let fixture = configured(&[graphit_auth::Permission::Read(Module::Memory)], json!({}));
        let (connection, serving) = client(&fixture).await;
        let expected = connection
            .call_tool(request(json!({"engine":"kiro", "event":"SessionStart"})))
            .await
            .unwrap();
        let cancel = Cancellation::default();
        let original = RequestContext::new(
            fixture.server.0.policy.content,
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let transport_policy = graphit_hub::policy::HubResourcePolicy::resolve(
            &fixture.server.0.hook.as_ref().unwrap().base,
        )
        .unwrap();
        let arguments = crate::session_hook_entry::Arguments {
            recipient: crate::session_hook_entry::Recipient::Coordinator,
            engine: Engine::Kiro,
            event: Event::SessionStart,
            project_dir: Some(fixture.projects[0].0.root().to_owned()),
            task_session: None,
            task_ids: Vec::new(),
        };
        let mut bytes = Vec::new();
        crate::session_hook_entry::call(
            &fixture.paths,
            arguments.recipient,
            crate::session_hook_entry::prepare(
                &fixture.paths,
                &arguments,
                Some(fixture.projects[0].2.version()),
                &original,
            )
            .unwrap(),
            fixture.server.0.policy.index,
            &transport_policy,
            &original,
            &mut bytes,
        )
        .await
        .unwrap();
        assert_eq!(
            bytes,
            expected.content[0].as_text().unwrap().text.as_bytes()
        );
        for flush_failure in [false, true] {
            let error = crate::session_hook_entry::call(
                &fixture.paths,
                arguments.recipient,
                crate::session_hook_entry::prepare(&fixture.paths, &arguments, None, &original)
                    .unwrap(),
                fixture.server.0.policy.index,
                &transport_policy,
                &original,
                &mut FailedWriter(flush_failure),
            )
            .await
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        }
        let prepared =
            crate::session_hook_entry::prepare(&fixture.paths, &arguments, None, &original)
                .unwrap();
        cancel.cancel();
        let mut rejected = Vec::new();
        assert!(
            crate::session_hook_entry::call(
                &fixture.paths,
                arguments.recipient,
                prepared,
                fixture.server.0.policy.index,
                &transport_policy,
                &original,
                &mut rejected,
            )
            .await
            .is_err()
        );
        assert!(rejected.is_empty());
        connection.cancel().await.unwrap();
        fixture.server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn cancelled_hook_joins_owned_job_and_deadline_refuses_delivery() {
        use rmcp::model::{CallToolRequest, ClientRequest};
        use std::sync::mpsc;
        for deadline in [false, true] {
            let mut fixture = configured(&[], json!({"hooks":false}));
            if deadline {
                Arc::get_mut(&mut fixture.server.0).unwrap().policy.timeout =
                    Duration::from_millis(100);
            }
            let (connection, serving) = client(&fixture).await;
            let (started, observed) = mpsc::channel();
            let (release, waiting) = mpsc::channel();
            *fixture.server.0.gate.lock().unwrap() = Some((started, waiting));
            let params = || request(json!({"engine":"kiro", "event":"SessionStart"}));
            let pending = connection
                .send_cancellable_request(
                    ClientRequest::CallToolRequest(CallToolRequest::new(params())),
                    Default::default(),
                )
                .await
                .unwrap();
            let cancelled = tokio::task::spawn_blocking(move || {
                observed.recv_timeout(Duration::from_secs(5)).unwrap()
            })
            .await
            .unwrap();
            let deadline_response = if deadline {
                Some(pending.await_response().await.unwrap())
            } else {
                pending.cancel(None).await.unwrap();
                None
            };
            let cancellation_observed = tokio::time::timeout(Duration::from_secs(5), async {
                while !cancelled.is_cancelled() {
                    tokio::task::yield_now().await;
                }
            })
            .await;
            let sibling = connection.call_tool(params()).await.unwrap();
            let closing = fixture.server.clone();
            let shutdown = tokio::spawn(async move { closing.shutdown().await });
            tokio::task::yield_now().await;
            let still_joining = !shutdown.is_finished();
            release.send(()).unwrap();
            shutdown.await.unwrap().unwrap();
            connection.cancel().await.unwrap();
            serving.await.unwrap().unwrap();
            assert!(cancellation_observed.is_ok());
            assert!(still_joining);
            assert!(fixture.server.0.jobs.lock().await.tasks.is_empty());
            assert_eq!(sibling.is_error, Some(false));
            assert!(sibling.content[0].as_text().unwrap().text.is_empty());
            if let Some(rmcp::model::ServerResult::CallToolResult(response)) = deadline_response {
                assert_eq!(response.is_error, Some(true));
                assert!(
                    response.content[0]
                        .as_text()
                        .unwrap()
                        .text
                        .contains("deadline")
                );
            } else {
                assert!(!deadline);
            }
        }
    }
    #[test]
    fn selected_modules_reload_only_actual_project_and_preserve_command_environment() {
        let fixture = configured(&[graphit_auth::Permission::Read(Module::Ast)], json!({}));
        let policy = fixture.server.0.hook.as_ref().unwrap();
        let snapshot = fixture.server.0.live_projects.snapshot().unwrap();
        let project = &snapshot.projects[0].0;
        let cancel = Cancellation::default();
        let original = RequestContext::new(
            fixture.server.0.policy.content,
            &cancel,
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
        let global = ConfigStore::global(&fixture.paths).unwrap();
        let local = ConfigStore::project(project).unwrap();
        let set = |store: &ConfigStore, key: &str, value| {
            store
                .set(
                    ConfigKey::new(key).unwrap(),
                    json!(value),
                    Duration::from_secs(5),
                    &cancel,
                )
                .unwrap();
        };
        set(&global, "modules.ast", true);
        set(&global, "modules.embedding", true);
        let current = policy
            .selected_modules(&fixture.paths, project, original.snapshot())
            .unwrap();
        assert!(current.enabled(Module::Ast) && current.enabled(Module::Embedding));
        set(&global, "modules.ast", false);
        set(&global, "modules.embedding", false);
        let current = policy
            .selected_modules(&fixture.paths, project, original.snapshot())
            .unwrap();
        assert!(!current.enabled(Module::Ast) && !current.enabled(Module::Embedding));
        set(&local, "modules.ast", true);
        assert!(
            policy
                .selected_modules(&fixture.paths, project, original.snapshot())
                .unwrap()
                .enabled(Module::Ast)
        );
        let base = ConfigSnapshot::new(
            fixture.paths.brand(),
            [
                ConfigDocument::new(
                    fixture.paths.brand(),
                    ConfigSource::Command,
                    [(ConfigKey::new("modules.ast").unwrap(), json!(true))],
                )
                .unwrap(),
                ConfigDocument::new(
                    fixture.paths.brand(),
                    ConfigSource::Environment,
                    [(ConfigKey::new("modules.embedding").unwrap(), json!(false))],
                )
                .unwrap(),
            ],
        )
        .unwrap();
        let fixed =
            HookPolicy::capture(&fixture.server.0.policy, &base, &snapshot.projects).unwrap();
        set(&local, "modules.ast", false);
        set(&local, "modules.embedding", true);
        let current = fixed
            .selected_modules(&fixture.paths, project, original.snapshot())
            .unwrap();
        assert!(current.enabled(Module::Ast));
        assert!(!current.enabled(Module::Embedding));
        cancel.cancel();
        assert_eq!(
            policy
                .selected_modules(&fixture.paths, project, original.snapshot())
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
    }
}
