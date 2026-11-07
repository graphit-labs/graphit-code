//! Physical source admissions remain borrowed until native completion.
use super::*;
use crate::{
    knowledge_live::{
        CapturedArtifact, EventData, LiveError, LiveSessionService, Preparation, PreparedSelection,
    },
    live_agent_ast as ast, live_agent_contexts as contexts,
    live_agent_preparation::{self as text, CapturedHubText},
    reference_explorer::{Identity, Resources},
};
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority,
    broker::project_access::{BrokerProjectCaller, BrokerProjectScope, BrokerRecordRead},
};
use graphit_core::artifact_store::CapturedPublicationScope;
use graphit_hub::{
    NativeBinding,
    native::{
        RenderedContent,
        mcp_config::{NativeServerSpec, NativeServerTransport},
    },
    policy::HubResourcePolicy,
};
use graphit_runtime::instructions::{InstructionValue, InstructionVariable};
use std::collections::BTreeSet;

fn live_error(cause: LiveError) -> Error {
    Error::Live(cause.into())
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum ContextKind {
    Knowledge,
    Ast,
}

fn denied() -> Error {
    io_failure(io::ErrorKind::PermissionDenied.into())
}

pub(super) fn replayed_turn(
    live: &LiveSessionService,
    access: &Access<'_>,
    id: &str,
    turn: Option<&SendRequest>,
) -> Result<Option<LiveSession>, Error> {
    let observation = live.observe(access, id, None).map_err(live_error)?;
    if let Some(turn) = turn
        && let Some(previous) = observation.replay.events.iter().find(|event| {
            event.turn_id.as_deref() == Some(&turn.turn_id)
                && matches!(event.data, EventData::Prompt { .. })
        })
    {
        if !matches!(&previous.data, EventData::Prompt { text } if text == &turn.text) {
            return Err(live_error(LiveError::Conflict));
        }
        return Ok(Some(observation.replay.session));
    }
    Ok(None)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Preparation borrows the admitted HTTP job, source resources, and reply owner for the same lifetime"
)]
pub(super) async fn execute<'a>(
    routes: &'a Routes,
    web_id: &'a str,
    entry: &'a Arc<Session>,
    resources: &'a Resources<'a>,
    identity: Identity<'a>,
    original: &'a RequestContext<'a>,
    access: &'a Access<'a>,
    live: &'a LiveSessionService,
    session: LiveSession,
    turn: Option<SendRequest>,
    reply: &mut Option<Reply>,
) -> Result<LiveSession, Error> {
    access.write().map_err(io_failure)?;
    if let Some(replayed) = replayed_turn(live, access, &session.id, turn.as_ref())? {
        return Ok(replayed);
    }
    let selected = resources
        .projects
        .iter()
        .enumerate()
        .filter(|(_, (_, id, version))| {
            id.to_string() == session.project.project_id
                && version.version() == session.project.project_version
        })
        .collect::<Vec<_>>();
    if selected.len() != 1 {
        return Err(denied());
    }
    let selected_index = selected[0].0;
    resources
        .require_active(&resources.projects[selected_index])
        .map_err(|_| denied())?;
    let ask = routes
        .tasks
        .ask()
        .map_err(io_failure)?
        .ok_or_else(|| io_failure(io::ErrorKind::Unsupported.into()))?;
    let service = ask
        .service()
        .ok_or_else(|| io_failure(io::ErrorKind::Unsupported.into()))?;
    use graphit_ai::cli_kiro::KiroEngine;
    if session.binding != resources.policy.binding
        || !matches!(
            (session.binding, service.kiro_engine()),
            (NativeBinding::KiroV3, Some(KiroEngine::V3))
        )
    {
        return Err(io_failure(io::ErrorKind::Unsupported.into()));
    }
    let (caller, verifier, provider) = identity;
    let hub = resources
        .auth
        .issuer()
        .admit_web_installed_hub_read(
            caller,
            verifier,
            original,
            provider,
            resources.auth.installation(),
        )
        .map_err(|_| denied())?;
    let imported = resources
        .policy
        .imported_knowledge
        .as_ref()
        .ok_or_else(|| io_failure(io::ErrorKind::Unsupported.into()))?;
    let registry = &imported.registry;
    let (_, config, instructions) = routes
        .tasks
        .live_configuration(None, original.snapshot())
        .map_err(io_failure)?;
    let policy = HubResourcePolicy::resolve(&config)
        .map_err(|cause| io_failure(io::Error::new(io::ErrorKind::InvalidData, cause)))?;
    let selections = std::iter::once(&session.agent)
        .chain(
            session
                .artifacts
                .iter()
                .filter(|item| matches!(item.artifact_type.as_str(), "agent" | "skill" | "rule")),
        )
        .collect::<Vec<_>>();
    let knowledge_selections = contexts::select_contexts(
        resources,
        &hub,
        session
            .artifacts
            .iter()
            .filter(|item| item.artifact_type == "knowledge"),
    )
    .map_err(io_failure)?;
    let ast_source = routes.source.as_ref();
    let ast_selections = if session
        .artifacts
        .iter()
        .any(|item| item.artifact_type == "ast")
    {
        ast::select_contexts(
            resources,
            &hub,
            ast_source.ok_or_else(|| io_failure(io::ErrorKind::Unsupported.into()))?,
            session
                .artifacts
                .iter()
                .filter(|item| item.artifact_type == "ast"),
        )
        .map_err(io_failure)?
    } else {
        Vec::new()
    };
    let ast_graph = ast_source
        .filter(|_| !ast_selections.is_empty())
        .map(crate::source_server::SourceServer::graph_server);
    let ast_read = if ast_selections
        .iter()
        .any(|selected| selected.imported.is_some())
    {
        Some(
            resources
                .auth
                .issuer()
                .admit_web_imported_ast_read(
                    caller,
                    verifier,
                    original,
                    provider,
                    resources.auth.installation(),
                )
                .map_err(|_| denied())?,
        )
    } else {
        None
    };
    let knowledge_read = if knowledge_selections
        .iter()
        .any(|selected| selected.imported.is_some())
    {
        Some(
            resources
                .auth
                .issuer()
                .admit_web_imported_knowledge_read(
                    caller,
                    verifier,
                    original,
                    provider,
                    resources.auth.installation(),
                )
                .map_err(|_| denied())?,
        )
    } else {
        None
    };
    let mut project_indices = BTreeSet::new();
    let mut project_requests = vec![(selected_index, ProjectReadModule::Knowledge)];
    project_indices.insert(selected_index);
    let mut text_projects = Vec::with_capacity(selections.len());
    for selection in &selections {
        let metadata = text::installed_metadata(&hub, registry, selection, session.binding)
            .map_err(io_failure)?;
        let source = &metadata.original_source;
        let index = if source
            .directory()
            .as_path()
            .starts_with(hub.installation().live.owner_root())
        {
            None
        } else {
            let matching = resources
                .projects
                .iter()
                .enumerate()
                .filter(|(_, (paths, id, _))| {
                    *id == source.producer()
                        && source.root().as_path().starts_with(paths.root())
                        && source.directory().as_path().starts_with(paths.root())
                })
                .collect::<Vec<_>>();
            if matching.len() != 1 {
                return Err(denied());
            }
            let index = matching[0].0;
            resources
                .require_active(&resources.projects[index])
                .map_err(|_| denied())?;
            project_indices.insert(index);
            if !project_requests.contains(&(index, ProjectReadModule::Hub)) {
                project_requests.push((index, ProjectReadModule::Hub));
            }
            Some(index)
        };
        text_projects.push(index);
    }
    for selection in &knowledge_selections {
        if let Some(index) = selection.project_index {
            project_indices.insert(index);
            if !project_requests.contains(&(index, ProjectReadModule::Knowledge)) {
                project_requests.push((index, ProjectReadModule::Knowledge));
            }
        }
    }
    for selection in &ast_selections {
        if let Some(index) = selection.project_index {
            project_indices.insert(index);
            if !project_requests.contains(&(index, ProjectReadModule::Ast)) {
                project_requests.push((index, ProjectReadModule::Ast));
            }
        }
    }
    let mut captures = Vec::with_capacity(project_indices.len());
    for index in project_indices {
        let (paths, id, _) = &resources.projects[index];
        resources
            .require_active(&resources.projects[index])
            .map_err(|_| denied())?;
        let scope = CapturedPublicationScope::project(
            paths,
            *id,
            resources.policy.scope,
            original.snapshot(),
        )
        .map_err(io_failure)?;
        let record = scope.binding().map_err(io_failure)?;
        captures.push((index, scope, record));
    }
    let authority = ProjectRecordAuthority::new(resources.auth.issuer());
    let mut admissions = Vec::with_capacity(project_requests.len());
    for (index, module) in &project_requests {
        let (_, scope, record) = captures
            .iter()
            .find(|(actual, _, _)| actual == index)
            .ok_or_else(|| io_failure(io::ErrorKind::InvalidData.into()))?;
        let admission = authority
            .admit_broker_read(BrokerRecordRead {
                caller: BrokerProjectCaller::Web(caller),
                verifier,
                backend: &routes.backend,
                original,
                scope: BrokerProjectScope {
                    installation: resources.auth.installation(),
                    project: ProjectCapture {
                        scope: ScopeCapture {
                            live: scope,
                            record,
                        },
                        version: &resources.projects[*index].2,
                    },
                    providers: &[],
                },
                provider,
                module: *module,
            })
            .await
            .map_err(|_| denied())?;
        admissions.push((*index, *module, admission));
    }
    let texts = selections
        .iter()
        .zip(&text_projects)
        .map(|(selection, index)| {
            let admission = index.and_then(|index| {
                admissions
                    .iter()
                    .find(|(actual, module, _)| {
                        *actual == index && *module == ProjectReadModule::Hub
                    })
                    .map(|(_, _, grant)| grant)
            });
            text::capture_text(
                &hub,
                registry,
                &policy,
                selection,
                session.binding,
                admission,
            )
            .map_err(io_failure)
        })
        .collect::<Result<Vec<CapturedHubText>, Error>>()?;
    // Each vector is owned before values that borrow it: no self-referential lease object.
    let mut remote_selections = Vec::new();
    if let Some(read) = &knowledge_read {
        for (index, selected) in knowledge_selections.iter().enumerate() {
            let Some((id, version)) = &selected.imported else {
                continue;
            };
            if let Some(selection) = crate::remote_native_mount::Selection::capture(
                crate::remote_native_mount::ReadAdmission::Knowledge(read),
                registry,
                id,
                version,
                imported.directory,
            )
            .map_err(|cause| io_failure(io::Error::other(cause)))?
            {
                remote_selections.push((ContextKind::Knowledge, index, selection));
            }
        }
    }
    if let Some(read) = &ast_read {
        let source = ast_source.ok_or_else(denied)?;
        let registry = &source.policy().registry;
        for (index, selected) in ast_selections.iter().enumerate() {
            let Some((id, version)) = &selected.imported else {
                continue;
            };
            if let Some(selection) = crate::remote_native_mount::Selection::capture(
                crate::remote_native_mount::ReadAdmission::Ast(read),
                registry,
                id,
                version,
                source.policy().index,
            )
            .map_err(|cause| io_failure(io::Error::other(cause)))?
            {
                remote_selections.push((ContextKind::Ast, index, selection));
            }
        }
    }
    let mut remote_services = Vec::with_capacity(remote_selections.len());
    for (kind, _, selection) in &remote_selections {
        let issuer = resources.auth.issuer();
        let service = match kind {
            ContextKind::Knowledge => issuer.capture_imported_knowledge_hub_service(
                knowledge_read.as_ref().ok_or_else(denied)?,
                selection.record().provider_name(),
            ),
            ContextKind::Ast => issuer.capture_imported_ast_hub_service(
                ast_read.as_ref().ok_or_else(denied)?,
                selection.record().provider_name(),
            ),
        }
        .map_err(|cause| io_failure(io::Error::other(cause)))?;
        remote_services.push(service);
    }
    let mut remote_verifiers = Vec::with_capacity(remote_services.len());
    for service in &mut remote_services {
        let verifier = if service
            .mode()
            .map_err(|cause| io_failure(io::Error::other(cause)))?
            == graphit_auth::ServiceMode::BrokerNative
        {
            Some(
                resources
                    .native_verifiers
                    .ok_or_else(|| io_failure(io::ErrorKind::NotConnected.into()))?
                    .get(service)
                    .map_err(|cause| io_failure(io::Error::other(cause)))?,
            )
        } else {
            None
        };
        remote_verifiers.push(verifier);
    }
    let mut remote_leases = Vec::with_capacity(remote_services.len());
    for (service, verifier) in remote_services.iter_mut().zip(&remote_verifiers) {
        let lease = match verifier.as_deref() {
            Some(verifier) => service.native_credential(verifier).await,
            None => service.credential(),
        }
        .map_err(|cause| io_failure(io::Error::other(cause)))?;
        remote_leases.push(lease);
    }
    let mut remote_opened = Vec::with_capacity(remote_selections.len());
    let metadata_limits = crate::native_publication::metadata_limits(
        resources.policy.content,
        registry.limits().projection(),
    );
    for ((kind, index, selection), lease) in remote_selections.into_iter().zip(&remote_leases) {
        let metadata_limits = if kind == ContextKind::Ast {
            let policy = ast_source.ok_or_else(denied)?.policy();
            crate::native_publication::metadata_limits(
                policy.content,
                policy.registry.limits().projection(),
            )
        } else {
            metadata_limits
        };
        let opened = crate::source_server::remote_native::open(
            selection,
            lease,
            resources.native_http,
            resources.native_allow_loopback_http,
            metadata_limits,
        )
        .await
        .map_err(|cause| io_failure(io::Error::other(cause)))?;
        remote_opened.push((kind, index, opened));
    }
    let mut remote_pins = Vec::new();
    for (remote, ((kind, _, opened), lease)) in remote_opened.iter().zip(&remote_leases).enumerate()
    {
        if *kind == ContextKind::Knowledge {
            remote_pins.push((
                remote,
                opened
                    .mount
                    .wiki(&opened.selection, lease)
                    .await
                    .map_err(|cause| io_failure(io::Error::other(cause)))?,
            ));
        }
    }
    let mut knowledge_contexts = Vec::with_capacity(knowledge_selections.len());
    for (index, selected) in knowledge_selections.iter().enumerate() {
        let context = if let Some(project) = selected.project_index {
            let read = admissions
                .iter()
                .find(|(actual, module, _)| {
                    *actual == project && *module == ProjectReadModule::Knowledge
                })
                .map(|(_, _, read)| read)
                .ok_or_else(denied)?;
            contexts::capture_project(selected, original.snapshot(), read, resources.policy).await
        } else if let Some(remote) = remote_opened
            .iter()
            .position(|(kind, actual, _)| *kind == ContextKind::Knowledge && *actual == index)
        {
            contexts::capture_remote_imported(
                selected,
                &remote_opened[remote].2,
                &remote_pins
                    .iter()
                    .find(|(actual, _)| *actual == remote)
                    .ok_or_else(denied)?
                    .1,
                &remote_leases[remote],
            )
            .await
        } else {
            contexts::capture_local_imported(
                selected,
                resources,
                knowledge_read.as_ref().ok_or_else(denied)?,
            )
            .await
        }
        .map_err(io_failure)?;
        knowledge_contexts.push(context);
    }
    let mut ast_contexts = Vec::with_capacity(ast_selections.len());
    for (index, selected) in ast_selections.iter().enumerate() {
        let source = ast_source.ok_or_else(denied)?;
        let graph = ast_graph.as_ref().ok_or_else(denied)?;
        let context = if let Some(project) = selected.project_index {
            let read = admissions
                .iter()
                .find(|(actual, module, _)| *actual == project && *module == ProjectReadModule::Ast)
                .map(|(_, _, read)| read)
                .ok_or_else(denied)?;
            ast::capture_project(
                selected,
                &resources.projects[project],
                graph,
                source,
                read,
                resources.auth.installation(),
                original,
            )
            .await
        } else if let Some(remote) = remote_opened
            .iter()
            .position(|(kind, actual, _)| *kind == ContextKind::Ast && *actual == index)
        {
            ast::capture_remote_imported(
                selected,
                graph,
                source,
                &remote_opened[remote].2,
                &remote_leases[remote],
                ast_read.as_ref().ok_or_else(denied)?,
            )
            .await
        } else {
            ast::capture_local_imported(
                selected,
                graph,
                source,
                ast_read.as_ref().ok_or_else(denied)?,
            )
            .await
        }
        .map_err(io_failure)?;
        ast_contexts.push(context);
    }
    let (installation, _, _) = routes
        .tasks
        .live_configuration(None, original.snapshot())
        .map_err(io_failure)?;
    let mcp = NativeServerSpec {
        name: installation.brand().key().into(),
        transport: NativeServerTransport::Stdio {
            command: installation.brand().binary_name().into(),
            args: vec!["mcp".into(), "--stdio".into(), "--live-user".into()],
            env: Default::default(),
            env_vars: crate::project_hook_install::client::mcp_environment_variables(
                installation.brand(),
                session.binding,
            )
            .map_err(io_failure)?,
            cwd: None,
        },
    }
    .plan(session.binding)
    .map_err(|cause| io_failure(io::Error::new(io::ErrorKind::InvalidInput, cause)))?;
    let brand = installation.brand().key();
    let knowledge_tools = [
        crate::live_agent_tools::CONTEXT,
        crate::live_agent_tools::HEADS,
        crate::live_agent_tools::PAGE,
        crate::live_agent_tools::SEARCH,
    ];
    let ast_tools = if ast_contexts.is_empty() {
        &[][..]
    } else {
        &[
            "graphit_ast_source",
            "graphit_ast_schema",
            "graphit_ast_query",
            "graphit_ast_search",
        ][..]
    };
    let variables = [
        InstructionVariable {
            name: "brand",
            value: InstructionValue::Text(brand),
        },
        InstructionVariable {
            name: "brand_display",
            value: InstructionValue::Text(brand),
        },
        InstructionVariable {
            name: "mcp_server_name",
            value: InstructionValue::Text(brand),
        },
        InstructionVariable {
            name: "mcp_tool_live_context",
            value: InstructionValue::Text(crate::live_agent_tools::CONTEXT),
        },
        InstructionVariable {
            name: "mcp_tool_knowledge_heads",
            value: InstructionValue::Text(crate::live_agent_tools::HEADS),
        },
        InstructionVariable {
            name: "mcp_tool_knowledge_page",
            value: InstructionValue::Text(crate::live_agent_tools::PAGE),
        },
        InstructionVariable {
            name: "mcp_tool_knowledge_search",
            value: InstructionValue::Text(crate::live_agent_tools::SEARCH),
        },
        InstructionVariable {
            name: "live_knowledge_tools",
            value: InstructionValue::TextList(&knowledge_tools),
        },
        InstructionVariable {
            name: "live_ast_tools",
            value: InstructionValue::TextList(ast_tools),
        },
    ];
    let markdown = texts
        .iter()
        .map(|captured| {
            let source = captured
                .markdown(policy.content().max_input_bytes)
                .map_err(io_failure)?;
            text::render_markdown(&source, &variables, instructions, original.snapshot())
                .map_err(io_failure)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let rules = texts
        .iter()
        .zip(&markdown)
        .filter(|(captured, _)| captured.selection.artifact_type == "rule")
        .map(|(_, body)| body.as_str())
        .collect::<Vec<_>>();
    let instruction_sources = crate::instruction_overrides::InstructionSources::capture_project(
        resources.auth.issuer(),
        &resources.projects[selected_index].0,
        original,
    )
    .map_err(io_failure)?;
    let rendered = texts
        .iter()
        .zip(&markdown)
        .map(|(captured, body)| {
            match captured.selection.artifact_type.as_str() {
                "agent" => captured
                    .render_agent(
                        body,
                        &rules,
                        &mcp,
                        !ast_contexts.is_empty(),
                        &policy,
                        instructions,
                        original,
                        &instruction_sources,
                    )
                    .map(Some),
                "skill" => captured
                    .render_skill(body, session.binding, &policy, original.snapshot())
                    .map(Some),
                "rule" => Ok(None),
                _ => Err(io::ErrorKind::InvalidInput.into()),
            }
            .map_err(io_failure)
        })
        .collect::<Result<Vec<Option<RenderedContent>>, _>>()?;
    // Run eligibility is independent of data-source permissions. Each MCP read
    // obtains its own module admission and compares its actual captured stamp.
    let allowed = || {
        !entry.cancelled.is_cancelled()
            && !routes.sessions.0.stop.is_cancelled()
            && resources.check(original).is_ok()
            && hub.recheck().is_ok()
            && resources
                .require_active(&resources.projects[selected_index])
                .is_ok()
            && admissions
                .iter()
                .filter(|(_, module, _)| *module == ProjectReadModule::Hub)
                .all(|(index, _, grant)| {
                    grant.recheck().is_ok()
                        && resources
                            .require_active(&resources.projects[*index])
                            .is_ok()
                })
    };
    // These sources really participated in preparation. Recheck them at Ready
    // and before spawning, without turning them into a gate for unrelated reads.
    let prepared_sources = || {
        allowed()
            && admissions.iter().all(|(index, _, grant)| {
                grant.recheck().is_ok()
                    && resources
                        .require_active(&resources.projects[*index])
                        .is_ok()
            })
            && knowledge_read
                .as_ref()
                .is_none_or(|read| read.recheck().is_ok())
            && ast_read.as_ref().is_none_or(|read| read.recheck().is_ok())
            && knowledge_contexts
                .iter()
                .all(|context| context.recheck().is_ok())
            && ast_contexts.iter().all(|context| context.recheck().is_ok())
    };
    let artifacts = session
        .artifacts
        .iter()
        .map(|selection| {
            if selection.artifact_type == "knowledge" {
                return knowledge_contexts
                    .iter()
                    .find(|context| context.selected().selection == selection)
                    .map(|context| context.prepared())
                    .ok_or_else(|| io_failure(io::ErrorKind::InvalidData.into()));
            }
            if selection.artifact_type == "ast" {
                return ast_contexts
                    .iter()
                    .find(|context| context.selection() == selection)
                    .map(|context| context.prepared())
                    .ok_or_else(|| io_failure(io::ErrorKind::InvalidData.into()));
            }
            let index = texts
                .iter()
                .position(|text| &text.selection == selection)
                .ok_or_else(|| io_failure(io::ErrorKind::Unsupported.into()))?;
            let captured = &texts[index];
            let artifact = CapturedArtifact {
                selection,
                reader: &captured.reader,
            };
            match selection.artifact_type.as_str() {
                "rule" => Ok(PreparedSelection::InlineRule(artifact)),
                "agent" | "skill" => Ok(PreparedSelection::NativeText {
                    artifact,
                    source_entry: captured.source_entry(),
                    rendered: rendered[index]
                        .as_ref()
                        .ok_or_else(|| io_failure(io::ErrorKind::InvalidData.into()))?,
                }),
                _ => Err(io_failure(io::ErrorKind::InvalidInput.into())),
            }
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let session = live
        .prepare(
            access,
            &session.id,
            Preparation {
                agent: CapturedArtifact {
                    selection: &texts[0].selection,
                    reader: &texts[0].reader,
                },
                agent_source_entry: texts[0].source_entry(),
                rendered: rendered[0]
                    .as_ref()
                    .ok_or_else(|| io_failure(io::ErrorKind::InvalidData.into()))?,
                artifacts: &artifacts,
                allowed: &prepared_sources,
            },
        )
        .map_err(live_error)?;
    for (captured, index) in texts.iter().zip(&text_projects) {
        let grant = index.and_then(|index| {
            admissions
                .iter()
                .find(|(actual, module, _)| *actual == index && *module == ProjectReadModule::Hub)
                .map(|(_, _, grant)| grant)
        });
        captured
            .recheck(&hub, registry, session.binding, grant)
            .map_err(io_failure)?;
    }
    // Native files now own the rendered bytes; keep authority and pins, release text buffers.
    drop(artifacts);
    drop(rendered);
    drop(rules);
    drop(markdown);
    access.read().map_err(io_failure)?;
    if !prepared_sources() {
        return Err(denied());
    }
    let Some(turn) = turn else {
        if let Some(reply) = reply.take() {
            let _ = reply.send(Ok(session.clone()));
        }
        return Ok(session);
    };
    let mut route = {
        let snapshot = live
            .read_contexts(access, &session.id, &session.cursor.generation)
            .map_err(live_error)?;
        if snapshot.session != session {
            return Err(live_error(crate::knowledge_live::LiveError::Conflict));
        }
        routes
            .mcp
            .begin_run(super::super::super::live_mcp::Registration {
                sessions: &routes.sessions,
                web_session: web_id,
                entry: Arc::clone(entry),
                session: &snapshot.session,
                read_contexts: &snapshot.contexts,
                turn: &turn,
                identity,
                original,
            })
            .await
            .map_err(io_failure)?
    };
    let launch = route
        .launch(
            identity,
            original,
            routes.address,
            &installation,
            &routes.executable,
        )
        .map_err(io_failure)?;
    let model = routes.tasks.memory_model().map_err(io_failure)?;
    if !prepared_sources() {
        return Err(denied());
    }
    if let Some(reply) = reply.take() {
        let _ = reply.send(Ok(session.clone()));
    }
    let selected_contexts = knowledge_contexts
        .iter()
        .map(|context| context.selected())
        .collect::<Vec<_>>();
    let finish = route.stop_handle();
    let serving = route.serve(super::super::super::live_mcp::RunContext {
        resources,
        backend: &routes.backend,
        original,
        access,
        live,
        session: &session,
        turn: &turn,
        selected: &selected_contexts,
        source: ast_source,
        ast: &ast_selections,
        model: model.as_ref(),
        allowed: &allowed,
    });
    let running = live.run_turn(access, &session.id, &turn, service, &allowed, Some(&launch));
    tokio::pin!(serving, running);
    let receipt = tokio::select! {
        result = &mut running => {
            finish.stop();
            let closed = serving.await;
            let receipt = result.map_err(live_error)?;
            closed.map_err(io_failure)?;
            receipt
        },
        result = &mut serving => {
            original.cancellation().cancel();
            let native = running.await;
            result.map_err(io_failure)?;
            native.map_err(live_error)?
        }
    };
    if let Some(cause) = receipt.persistence_error {
        return Err(live_error(cause));
    }
    Ok(live
        .observe(access, &session.id, None)
        .map_err(live_error)?
        .replay
        .session)
}
