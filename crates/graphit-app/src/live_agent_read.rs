//! Request-owned Knowledge reads shared by Live MCP and the historical viewer.
//! Source tuples and persisted observations never substitute for fresh admissions.
#![cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod reference;
use crate::{
    foreground_knowledge,
    knowledge_live::{Access, ArtifactSelection, LiveSessionService, ReadContextsSnapshot},
    live_agent_contexts::{self as contexts, CapturedKnowledgeContext, EvidenceDocument},
    live_agent_tools::Error,
    reference_explorer::{self, Identity, Resources, SourceKey},
    task_server::browser::codec::AdmittedSelection,
};
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture,
    broker::project_access::{
        BrokerProjectAccess, BrokerProjectCaller, BrokerProjectScope, BrokerRecordRead,
    },
};
use graphit_core::{artifact_store::CapturedPublicationScope, native_path::NativePath};
use graphit_hub::policy::RequestContext;
pub(crate) use reference::ReferenceLookup;
use serde_json::json;
use std::{io, num::NonZeroUsize, sync::Arc};

pub(crate) enum Command {
    Viewer,
    Heads,
    Page(String),
    Search(foreground_knowledge::search::Input),
    Evidence(String),
}
pub(crate) enum Output {
    Bytes(Vec<u8>),
    Evidence(Option<Box<EvidenceDocument>>),
}
pub(crate) struct ReadContext<'a> {
    pub(crate) resources: &'a Resources<'a>,
    pub(crate) backend: &'a BrokerProjectAccess,
    pub(crate) identity: Identity<'a>,
    pub(crate) original: &'a RequestContext<'a>,
    pub(crate) access: &'a Access<'a>,
    pub(crate) live: &'a LiveSessionService,
    pub(crate) stored: &'a ReadContextsSnapshot,
    pub(crate) model: Option<&'a Arc<crate::foreground_memory_jobs::MemoryJobResources>>,
}
impl ReadContext<'_> {
    pub(crate) fn check(&self) -> Result<(), Error> {
        self.resources.check(self.original)?;
        if !Arc::ptr_eq(self.original.snapshot(), self.access.original().snapshot()) {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied).into());
        }
        self.access.read()?;
        self.identity
            .0
            .check(
                self.identity.1,
                graphit_auth::broker::BrokerRequestContext::new(self.original.snapshot()),
            )
            .map_err(Error::Web)?;
        Ok(())
    }
    pub(crate) fn check_current(&self) -> Result<(), Error> {
        self.check()?;
        let current = self
            .live
            .read_contexts(
                self.access,
                &self.stored.session.id,
                &self.stored.session.cursor.generation,
            )
            .map_err(Error::Live)?;
        if current.session.project != self.stored.session.project
            || current.session.artifacts != self.stored.session.artifacts
            || current.contexts != self.stored.contexts
        {
            return Err(Error::ContextChanged);
        }
        Ok(())
    }
    fn require_selection(&self, selection: &ArtifactSelection) -> Result<(), Error> {
        if selection == &contexts::project_selection(&self.stored.session.project) {
            return Ok(());
        }
        if !self.stored.session.artifacts.contains(selection)
            || self
                .stored
                .contexts
                .selected
                .iter()
                .filter(|context| &context.selection == selection)
                .count()
                != 1
        {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied).into());
        }
        Ok(())
    }
}
pub(crate) fn resources<'a>(r: &Resources<'a>) -> Resources<'a> {
    Resources {
        server: r.server,
        native_http: r.native_http,
        native_allow_loopback_http: r.native_allow_loopback_http,
        native_verifiers: r.native_verifiers,
        live_projects: r.live_projects,
        auth: r.auth,
        policy: r.policy,
        stop: r.stop,
        projects: r.projects,
        current: r.current,
    }
}
fn invalid(code: &'static str) -> Error {
    reference_explorer::Failure::Invalid(code).into()
}

async fn evidence(
    context: &ReadContext<'_>,
    capture: Option<&CapturedKnowledgeContext<'_>>,
    slug: &str,
) -> Result<Output, Error> {
    let page = if let Some(capture) = capture {
        capture
            .page(
                slug,
                foreground_knowledge::limits(context.resources.policy)
                    .map_err(foreground_knowledge::Failure::from)
                    .map_err(Error::Knowledge)?,
                NonZeroUsize::new(20).expect("positive page size"),
            )
            .await?
    } else {
        None
    };
    Ok(Output::Evidence(page.map(Box::new)))
}

pub(crate) async fn read(
    context: &ReadContext<'_>,
    selection: &ArtifactSelection,
    command: Command,
) -> Result<Output, Error> {
    context.check()?;
    context.require_selection(selection)?;
    // The project and imported readers contain independent native query state.
    // Box the selected branch instead of carrying both states in this dispatcher.
    let output = if selection.source == "project" {
        Box::pin(project(context, selection, command)).await
    } else if selection.source == "hub" {
        Box::pin(imported(context, selection, command)).await
    } else {
        return Err(invalid("unsupported_live_context"));
    };
    context.check()?;
    output
}
async fn project(
    context: &ReadContext<'_>,
    selection: &ArtifactSelection,
    command: Command,
) -> Result<Output, Error> {
    let r = context.resources;
    if selection.artifact_type != "knowledge"
        || selection.project_id.as_deref() != Some(selection.id.as_str())
        || selection.instance_id.is_some()
        || selection.project_kind.is_some()
    {
        return Err(invalid("selected_context_changed"));
    }
    let mut matches = r
        .projects
        .iter()
        .enumerate()
        .filter(|(_, (_, id, version))| {
            id.to_string() == selection.id && version.version() == selection.version
        });
    let (index, selected) = matches
        .next()
        .ok_or_else(|| invalid("selection_unavailable"))?;
    if matches.next().is_some() {
        return Err(invalid("selection_ambiguous"));
    }
    r.require_active(selected)?;
    let (paths, id, version) = selected;
    let scope =
        CapturedPublicationScope::project(paths, *id, r.policy.scope, context.original.snapshot())?;
    let binding = scope.binding()?;
    let authority = ProjectRecordAuthority::new(r.auth.issuer());
    let (caller, verifier, provider) = context.identity;
    let read = authority
        .admit_broker_read(BrokerRecordRead {
            caller: BrokerProjectCaller::Web(caller),
            verifier,
            backend: context.backend,
            original: context.original,
            scope: BrokerProjectScope {
                installation: r.auth.installation(),
                project: ProjectCapture {
                    scope: ScopeCapture {
                        live: &scope,
                        record: &binding,
                    },
                    version,
                },
                providers: &[],
            },
            provider,
            module: ProjectReadModule::Knowledge,
        })
        .await
        .map_err(reference_explorer::Failure::from)?;
    let admitted = AdmittedSelection {
        project_dir: NativePath::new(paths.root().to_owned())?,
        project_id: *id,
        project_version: version.version().to_owned(),
    };
    let query = match command {
        command @ (Command::Evidence(_) | Command::Viewer) => {
            let selector = contexts::ContextSelection {
                selection,
                source: SourceKey::ProjectKnowledge,
                imported: None,
                project_index: Some(index),
            };
            let capture = contexts::capture_project_optional(
                &selector,
                context.original.snapshot(),
                &read,
                r.policy,
            )
            .await?;
            context.require_selection(selection)?;
            let output = match command {
                Command::Evidence(slug) => evidence(context, capture.as_ref(), &slug).await?,
                Command::Viewer => Output::Bytes(serde_json::to_vec(&admitted)?),
                _ => return Err(invalid("invalid_live_read")),
            };
            read.recheck()?;
            r.require_active(selected)?;
            return Ok(output);
        }
        Command::Heads => foreground_knowledge::Query::Heads,
        Command::Page(slug) => {
            foreground_knowledge::Query::Page(serde_json::from_value(json!({"slug":slug}))?)
        }
        Command::Search(input) => foreground_knowledge::Query::Search(input),
    };
    let bytes = foreground_knowledge::read(
        context.original,
        caller,
        &read,
        r.policy,
        &admitted,
        query,
        context.model.map(Arc::as_ref),
        &foreground_knowledge::search::BrowserSearchContext {
            server: r.server,
            auth: r.auth,
            project: paths,
            verifiers: r.native_verifiers,
        },
    )
    .await
    .map_err(Error::Knowledge)?;
    context.require_selection(selection)?;
    read.recheck()?;
    r.require_active(selected)?;
    Ok(Output::Bytes(bytes))
}
async fn imported(
    context: &ReadContext<'_>,
    selection: &ArtifactSelection,
    command: Command,
) -> Result<Output, Error> {
    let r = context.resources;
    let (caller, verifier, provider) = context.identity;
    let hub = r.auth.issuer().admit_web_installed_hub_read(
        caller,
        verifier,
        context.original,
        provider,
        r.auth.installation(),
    )?;
    let selectors = contexts::select_contexts(r, &hub, std::iter::once(selection))?;
    let selected = selectors
        .first()
        .ok_or_else(|| invalid("selected_context_unavailable"))?;
    if !matches!(command, Command::Evidence(_) | Command::Viewer) {
        let output = imported_query(context, selected, command).await?;
        contexts::select_contexts(r, &hub, std::iter::once(selection))?;
        hub.recheck()?;
        return Ok(output);
    }
    let read = r.auth.issuer().admit_web_imported_knowledge_read(
        caller,
        verifier,
        context.original,
        provider,
        r.auth.installation(),
    )?;
    let registry = &r
        .policy
        .imported_knowledge
        .as_ref()
        .ok_or_else(|| invalid("selected_context_unavailable"))?
        .registry;
    let (id, version) = selected
        .imported
        .as_ref()
        .ok_or_else(|| invalid("selected_context_changed"))?;
    let remote = crate::remote_native_mount::Selection::capture(
        crate::remote_native_mount::ReadAdmission::Knowledge(&read),
        registry,
        id,
        version,
    )
    .map_err(io::Error::other)?;
    let output = if let Some(selection_remote) = remote {
        let mut service = r
            .auth
            .issuer()
            .capture_imported_knowledge_hub_service(
                &read,
                selection_remote.record().provider_name(),
            )
            .map_err(io::Error::other)?;
        let verifier = if service.mode().map_err(io::Error::other)?
            == graphit_auth::ServiceMode::BrokerNative
        {
            Some(
                r.native_verifiers
                    .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?
                    .get(&mut service)
                    .map_err(io::Error::other)?,
            )
        } else {
            None
        };
        let lease = match verifier.as_deref() {
            Some(v) => service.native_credential(v).await,
            None => service.credential(),
        }
        .map_err(io::Error::other)?;
        let opened = crate::source_server::remote_native::open(
            selection_remote,
            &lease,
            r.native_http,
            r.native_allow_loopback_http,
            crate::native_publication::metadata_limits(
                r.policy.content,
                registry.limits().projection(),
            ),
        )
        .await
        .map_err(io::Error::other)?;
        let pin = opened
            .mount
            .wiki(&opened.selection, &lease)
            .await
            .map_err(io::Error::other)?;
        let capture = contexts::capture_remote_imported(selected, &opened, &pin, &lease).await?;
        captured_imported_query(context, selected, &capture, command).await?
    } else {
        let capture = contexts::capture_local_imported(selected, r, &read).await?;
        captured_imported_query(context, selected, &capture, command).await?
    };
    read.recheck()?;
    hub.recheck()?;
    Ok(output)
}
async fn captured_imported_query(
    context: &ReadContext<'_>,
    selected: &contexts::ContextSelection<'_>,
    capture: &CapturedKnowledgeContext<'_>,
    command: Command,
) -> Result<Output, Error> {
    context.require_selection(selected.selection)?;
    match command {
        Command::Evidence(slug) => evidence(context, Some(capture), &slug).await,
        Command::Viewer => Ok(Output::Bytes(serde_json::to_vec(
            &json!({"context":selected.selection,"source":selected.source}),
        )?)),
        _ => Err(invalid("invalid_live_read")),
    }
}
async fn imported_query(
    context: &ReadContext<'_>,
    selected: &contexts::ContextSelection<'_>,
    command: Command,
) -> Result<Output, Error> {
    let (operation, input) = match command {
        Command::Evidence(_) | Command::Viewer => return Err(invalid("invalid_live_read")),
        Command::Heads => (
            "imported_knowledge_heads",
            json!({"source":selected.source}),
        ),
        Command::Page(slug) => (
            "imported_knowledge_page",
            json!({"source":selected.source,"slug":slug}),
        ),
        Command::Search(search) => (
            "imported_knowledge_search",
            json!({"source":selected.source,"search":search}),
        ),
    };
    let args = reference_explorer::parse(
        &serde_json::to_vec(&json!({"input":input}))?,
        context.resources.policy.content,
        operation,
    )
    .map_err(invalid)?;
    let bytes = reference_explorer::read(
        resources(context.resources),
        args,
        context.identity,
        context.original,
        || Ok(context.model.cloned()),
    )
    .await?;
    context.require_selection(selected.selection)?;
    Ok(Output::Bytes(bytes))
}

#[derive(serde::Serialize)]
struct LiveEvidencePage {
    title: String,
    path: String,
    source_path: String,
    content: String,
    context: ArtifactSelection,
    generation: String,
    refs: contexts::EvidenceReferences,
}
#[derive(serde::Serialize)]
struct Candidate {
    title: String,
    path: String,
    source_path: String,
    context: ArtifactSelection,
}

/// Read a persisted context outside Running under fresh USER and source admissions.
#[expect(
    clippy::too_many_arguments,
    reason = "The viewer borrows fresh admission inputs and the explicit persisted page selection without retaining another request object"
)]
pub(crate) async fn page<'a>(
    tasks: &'a crate::task_server::TaskServer,
    identity: Identity<'a>,
    backend: &'a BrokerProjectAccess,
    original: &'a RequestContext<'a>,
    id: &str,
    generation: &str,
    slug: &str,
    wanted: Option<&ArtifactSelection>,
    reference: Option<&ReferenceLookup>,
) -> Result<Vec<u8>, Error> {
    let snapshot = tasks
        .record_snapshot()
        .map_err(|_| io::Error::from(io::ErrorKind::NotConnected))?;
    let resources = tasks.record_resources(&snapshot);
    resources.check(original)?;
    reference_explorer::text(slug, resources.policy.content).map_err(invalid)?;
    graphit_data::knowledge_index::validate_slug(slug).map_err(|_| invalid("invalid_arguments"))?;
    let (caller, verifier, provider) = identity;
    let issuer = resources.auth.issuer();
    let scope = crate::foreground_user_memory::capture(
        issuer,
        resources.auth.installation(),
        caller,
        verifier,
        original,
        resources.policy,
    )
    .map_err(|cause| match cause {
        crate::foreground_user_memory::CaptureError::Physical(cause) => cause,
        crate::foreground_user_memory::CaptureError::Auth(cause) => original
            .check()
            .err()
            .unwrap_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, cause)),
        crate::foreground_user_memory::CaptureError::Subject(cause) => original
            .check()
            .err()
            .unwrap_or_else(|| io::Error::other(cause)),
    })?;
    let binding = scope.binding()?;
    let admission = issuer.admit_web_user_knowledge_read(
        caller,
        verifier,
        original,
        provider,
        ScopeCapture {
            live: &scope,
            record: &binding,
        },
    )?;
    let access = Access::user(&admission, None)?;
    let live = tasks
        .live_sessions()
        .ok_or_else(|| io::Error::from(io::ErrorKind::Unsupported))?;
    let stored = live
        .read_contexts(&access, id, generation)
        .map_err(Error::Live)?;
    let context = ReadContext {
        resources: &resources,
        backend,
        identity,
        original,
        access: &access,
        live,
        stored: &stored,
        model: None,
    };
    let default = contexts::project_selection(&stored.session.project);
    let selections = if let Some(wanted) = wanted {
        if wanted.artifact_type != "knowledge" {
            return Err(invalid("selected_context_changed"));
        }
        context.require_selection(wanted)?;
        vec![wanted]
    } else {
        let mut selections = vec![&default];
        for selected in stored
            .contexts
            .selected
            .iter()
            .filter(|selected| selected.selection.artifact_type == "knowledge")
        {
            if !selections.contains(&&selected.selection) {
                selections.push(&selected.selection);
            }
        }
        selections
    };
    if let Some(reference) = reference {
        return reference::resolve(&context, selections, slug, generation, reference).await;
    }
    let mut candidates = Vec::new();
    let mut only = None;
    for selection in selections {
        let Output::Evidence(found) =
            read(&context, selection, Command::Evidence(slug.to_owned())).await?
        else {
            return Err(invalid("invalid_live_read"));
        };
        if let Some(found) = found {
            candidates.push(Candidate {
                title: found.document.title.clone(),
                path: found.document.slug.clone(),
                source_path: found.document.source_path.clone(),
                context: selection.clone(),
            });
            if candidates.len() == 1 {
                only = Some((selection.clone(), found));
            } else {
                only = None;
            }
        }
    }
    context.check_current()?;
    let maximum = resources.policy.content.max_output_bytes;
    if candidates.is_empty() {
        return Err(io::Error::from(io::ErrorKind::NotFound).into());
    }
    if candidates.len() > 1 {
        return crate::task_server::browser::codec::encode_response(
            &json!({"generation":generation,"candidates":candidates}),
            maximum,
        )
        .map_err(Error::from);
    }
    let (selection, found) = only.ok_or_else(|| invalid("invalid_live_read"))?;
    let document = found.document;
    crate::task_server::browser::codec::encode_response(
        &LiveEvidencePage {
            title: document.title,
            path: document.slug,
            source_path: document.source_path,
            content: document.body,
            context: selection,
            generation: generation.to_owned(),
            refs: found.refs,
        },
        maximum,
    )
    .map_err(Error::from)
}
