//! Each requested source uses its own native query, admission and continuation.
use crate::{
    foreground_knowledge, foreground_memory, foreground_user_memory,
    reference_explorer::{
        self as explorer, ArtifactSelector, Failure, Identity, Resources, SourceKey, present,
    },
    task_server::{
        browser::codec::{self, AdmittedSelection, ExactSelection},
        request::{Arguments, Selection},
    },
};
use graphit_auth::{
    AuthError, ProjectCapture, ProjectReadAdmission, ProjectReadModule, ProjectRecordAuthority,
    ScopeCapture,
    broker::project_access::{
        BrokerProjectAccess, BrokerProjectCaller, BrokerProjectError, BrokerProjectScope,
        BrokerRecordRead,
    },
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, StoreOwner},
    native_path::NativePath,
};
use graphit_data::knowledge_references::KnowledgeReferenceTargetKind;
use graphit_data::{
    knowledge_index::{KnowledgeGeneration, KnowledgeIndexPageRead},
    knowledge_reference_query::{
        KnowledgeReferenceOwnedScope, KnowledgeReferencePayload, KnowledgeReferenceSource,
        KnowledgeReferencesCursor, KnowledgeReferencesRead,
    },
    knowledge_snapshot::KnowledgeSnapshotPage,
};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use graphit_knowledge::{
    imported_knowledge::{
        ImportedKnowledgeInput, ImportedKnowledgeOpen, ImportedKnowledgeReadLimits,
        open_imported_knowledge,
    },
    imported_reference_query::{
        ImportedKnowledgeReferencesCursor, ImportedKnowledgeReferencesQuery,
        ImportedKnowledgeReferencesRead,
    },
    native_reference_query::{
        KnowledgeLogicalTarget, KnowledgeReferencesQuery, read_native_knowledge_references,
    },
};
use graphit_records::{
    memory_store::{
        MemoryId, MemoryReferenceScope, MemoryReferenceTarget, MemoryReferencesCursor,
        MemoryReferencesQuery, MemoryReferencesRead, NativeMemoryStore,
    },
    reference_projection::{LogicalScope, ReferenceTargetKind},
    task_spec::TaskId,
    task_store::{
        NativeTaskStore, SessionGetInput, SessionId, TaskGetInput, TaskReferenceBacklinksCursor,
        TaskReferenceBacklinksQuery, TaskReferenceBacklinksRead, TaskReferenceKind,
        TaskReferenceRecord, TaskReferenceScope, TaskReferenceTarget, TaskReferencesCursor,
        TaskReferencesQuery, TaskReferencesRead,
    },
    user_memory_store::{
        NativeUserMemoryStore, UserMemoryOwner, UserMemoryReferencesCursor,
        UserMemoryReferencesQuery, UserMemoryReferencesRead, UserMemoryScopeId,
    },
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::value::RawValue;
use std::num::NonZeroUsize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Task,
    Session,
    Memory,
    Knowledge,
}
impl Kind {
    fn task(self) -> TaskReferenceKind {
        match self {
            Self::Task => TaskReferenceKind::Task,
            Self::Session => TaskReferenceKind::Session,
            Self::Memory => TaskReferenceKind::Memory,
            Self::Knowledge => TaskReferenceKind::Knowledge,
        }
    }
    fn native(self) -> ReferenceTargetKind {
        match self {
            Self::Task => ReferenceTargetKind::Task,
            Self::Session => ReferenceTargetKind::Session,
            Self::Memory => ReferenceTargetKind::Memory,
            Self::Knowledge => ReferenceTargetKind::Knowledge,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Scope {
    Project(graphit_core::project_identity::ProjectId),
    User(String),
}
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Focus {
    kind: Kind,
    scope: Scope,
    id: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    context: Option<String>,
}
fn id<T: DeserializeOwned>(value: &str) -> Result<T, &'static str> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .map_err(|_| "invalid_arguments")
}
impl Focus {
    fn validate(&self, limits: ContentLimits) -> Result<(), &'static str> {
        explorer::text(&self.id, limits)?;
        if let Scope::User(scope) = &self.scope {
            explorer::text(scope, limits)?;
        }
        if let Some(context) = &self.context {
            if self.kind != Kind::Knowledge {
                return Err("invalid_arguments");
            }
            explorer::text(context, limits)?;
        }
        Ok(())
    }
    fn own_project(&self, selected: Option<graphit_core::project_identity::ProjectId>) -> bool {
        self.context.is_none()
            && matches!(self.scope,Scope::Project(value) if Some(value)==selected)
    }
    fn readable_id(&self) -> bool {
        match self.kind {
            Kind::Task => id::<TaskId>(&self.id).is_ok(),
            Kind::Session => id::<SessionId>(&self.id).is_ok(),
            Kind::Memory => id::<MemoryId>(&self.id).is_ok(),
            Kind::Knowledge => graphit_data::knowledge_index::validate_slug(&self.id).is_ok(),
        }
    }
    fn own_task(&self, selected: Option<graphit_core::project_identity::ProjectId>) -> bool {
        self.own_project(selected)
            && matches!(self.kind, Kind::Task | Kind::Session)
            && self.readable_id()
    }
    fn task_target(&self) -> TaskReferenceTarget {
        TaskReferenceTarget {
            target_kind: self.kind.task(),
            scope: match &self.scope {
                Scope::Project(p) => TaskReferenceScope::Project(*p),
                Scope::User(u) => TaskReferenceScope::User(u.clone()),
            },
            id: self.id.clone(),
            context: self.context.clone(),
        }
    }
    fn memory_target(&self) -> MemoryReferenceTarget {
        MemoryReferenceTarget {
            target_kind: self.kind.native(),
            scope: match &self.scope {
                Scope::Project(p) => MemoryReferenceScope::Project(*p),
                Scope::User(u) => MemoryReferenceScope::User(u.clone()),
            },
            id: self.id.clone(),
            context: self.context.clone(),
        }
    }
    fn knowledge_target(&self) -> KnowledgeLogicalTarget<'_> {
        KnowledgeLogicalTarget {
            target_kind: self.kind.native(),
            scope: match &self.scope {
                Scope::Project(p) => LogicalScope::Project(*p),
                Scope::User(u) => LogicalScope::User(u),
            },
            id: &self.id,
            context: self.context.as_deref(),
        }
    }
    fn task_record(&self) -> Result<TaskReferenceRecord, Failure> {
        match self.kind {
            Kind::Task => Ok(TaskReferenceRecord::Task(
                id(&self.id).map_err(Failure::Invalid)?,
            )),
            Kind::Session => Ok(TaskReferenceRecord::Session(
                id(&self.id).map_err(Failure::Invalid)?,
            )),
            _ => Err(Failure::Invalid("invalid_arguments")),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Direction {
    Outgoing,
    Incoming,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPage {
    source: SourceKey,
    direction: Direction,
    #[serde(default, deserialize_with = "present")]
    cursor: Option<Box<RawValue>>,
}
#[derive(Clone, Deserialize, Serialize)]
enum Cursor {
    Task(TaskReferencesCursor),
    TaskIncoming(TaskReferenceBacklinksCursor),
    Memory(MemoryReferencesCursor),
    User(UserMemoryReferencesCursor),
    Knowledge(KnowledgeReferencesCursor),
    Imported(ImportedKnowledgeReferencesCursor),
}
#[derive(Deserialize, Serialize)]
struct Page {
    source: SourceKey,
    direction: Direction,
    cursor: Option<Cursor>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInput {
    focus: Focus,
    #[serde(default, deserialize_with = "present")]
    outgoing_source: Option<SourceKey>,
    #[serde(default = "page_size")]
    page_size: NonZeroUsize,
    #[serde(default)]
    include_user: bool,
    #[serde(default)]
    contexts: Vec<ArtifactSelector>,
    #[serde(default, deserialize_with = "present")]
    pages: Option<Vec<RawPage>>,
}
#[derive(Deserialize, Serialize)]
struct Input {
    focus: Focus,
    outgoing_source: Option<SourceKey>,
    page_size: NonZeroUsize,
    include_user: bool,
    contexts: Vec<ArtifactSelector>,
    pages: Option<Vec<Page>>,
}
fn page_size() -> NonZeroUsize {
    NonZeroUsize::new(20).expect("positive page size")
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    #[serde(default, deserialize_with = "present")]
    selection: Option<ExactSelection>,
    input: RawInput,
}

pub(crate) fn parse(
    bytes: &[u8],
    limits: ContentLimits,
    operation: &str,
) -> Result<Arguments, &'static str> {
    if operation != "records_references" {
        return Err("invalid_arguments");
    }
    let body: Body = codec::decode(bytes, limits)?;
    body.input.focus.validate(limits)?;
    if body.input.page_size.get() > 100 || body.input.contexts.len() > limits.max_collection_entries
    {
        return Err("invalid_arguments");
    }
    for (index, artifact) in body.input.contexts.iter().enumerate() {
        artifact.validate(limits)?;
        if body.input.contexts[..index].contains(artifact) {
            return Err("invalid_arguments");
        }
    }
    let selected = body.selection.as_ref().map(|s| s.project_id);
    let task_own = body.input.focus.own_task(selected);
    let pages = body
        .input
        .pages
        .map(|pages| -> Result<Vec<Page>, &'static str> {
            if pages.is_empty() || pages.len() > limits.max_collection_entries {
                return Err("invalid_arguments");
            }
            let mut result = Vec::with_capacity(pages.len());
            for page in pages {
                if result
                    .iter()
                    .any(|p: &Page| p.source == page.source && p.direction == page.direction)
                {
                    return Err("invalid_arguments");
                }
                let cursor = page
                    .cursor
                    .as_ref()
                    .map(|raw| {
                        let decode = |raw: &RawValue| {
                            serde_json::from_str(raw.get()).map_err(|_| "invalid_arguments")
                        };
                        match &page.source {
                            SourceKey::ProjectTask if task_own => decode(raw).map(Cursor::Task),
                            SourceKey::ProjectTask => serde_json::from_str(raw.get())
                                .map(Cursor::TaskIncoming)
                                .map_err(|_| "invalid_arguments"),
                            SourceKey::ProjectMemory => serde_json::from_str(raw.get())
                                .map(Cursor::Memory)
                                .map_err(|_| "invalid_arguments"),
                            SourceKey::UserMemory => serde_json::from_str(raw.get())
                                .map(Cursor::User)
                                .map_err(|_| "invalid_arguments"),
                            SourceKey::ProjectKnowledge => serde_json::from_str(raw.get())
                                .map(Cursor::Knowledge)
                                .map_err(|_| "invalid_arguments"),
                            SourceKey::ImportedKnowledge { .. } => serde_json::from_str(raw.get())
                                .map(Cursor::Imported)
                                .map_err(|_| "invalid_arguments"),
                        }
                    })
                    .transpose()?;
                result.push(Page {
                    source: page.source,
                    direction: page.direction,
                    cursor,
                });
            }
            Ok(result)
        })
        .transpose()?;
    let input = Input {
        focus: body.input.focus,
        outgoing_source: body.input.outgoing_source,
        page_size: body.input.page_size,
        include_user: body.input.include_user,
        contexts: body.input.contexts,
        pages,
    };
    let mut selection = Selection::default();
    if let Some(selected) = body.selection {
        explorer::text(&selected.project_version, limits)?;
        let path_bytes = selected
            .project_dir
            .as_path()
            .to_str()
            .ok_or("invalid_arguments")?
            .len();
        if path_bytes > limits.max_scalar_bytes || limits.max_scalar_bytes < 26 {
            return Err("invalid_arguments");
        }
        selection = Selection {
            project_dir: Some(selected.project_dir),
            project_id: Some(selected.project_id),
            project_version: Some(selected.project_version),
        };
    }
    input.plans(selection.project_id)?;
    Ok(Arguments {
        selection,
        input: Some(serde_json::to_value(input).map_err(|_| "invalid_arguments")?),
    })
}
struct Plan {
    source: SourceKey,
    outgoing: bool,
    incoming: bool,
    outgoing_cursor: Option<Cursor>,
    incoming_cursor: Option<Cursor>,
}
impl Input {
    fn owner(
        &self,
        selected: Option<graphit_core::project_identity::ProjectId>,
    ) -> Option<SourceKey> {
        if let Some(source) = &self.outgoing_source {
            return Some(source.clone());
        }
        if self.focus.own_project(selected) && self.focus.readable_id() {
            return Some(match self.focus.kind {
                Kind::Task | Kind::Session => SourceKey::ProjectTask,
                Kind::Memory => SourceKey::ProjectMemory,
                Kind::Knowledge => SourceKey::ProjectKnowledge,
            });
        }
        if self.focus.kind == Kind::Memory
            && matches!(self.focus.scope, Scope::User(_))
            && self.include_user
            && self.focus.readable_id()
        {
            return Some(SourceKey::UserMemory);
        }
        None
    }
    fn plans(
        &self,
        selected: Option<graphit_core::project_identity::ProjectId>,
    ) -> Result<Vec<Plan>, &'static str> {
        let mut universe = vec![
            SourceKey::ProjectTask,
            SourceKey::ProjectMemory,
            SourceKey::ProjectKnowledge,
        ];
        if self.include_user {
            universe.push(SourceKey::UserMemory);
        }
        universe.extend(
            self.contexts
                .iter()
                .cloned()
                .map(|artifact| SourceKey::ImportedKnowledge { artifact }),
        );
        let owner = self.owner(selected);
        let check = |source: &SourceKey, direction: Direction| -> Result<(), &'static str> {
            if !universe.contains(source) {
                return Err("invalid_arguments");
            }
            if direction == Direction::Outgoing {
                let valid = match source {
                    SourceKey::ProjectTask => {
                        self.focus.own_project(selected)
                            && matches!(self.focus.kind, Kind::Task | Kind::Session)
                    }
                    SourceKey::ProjectMemory => {
                        self.focus.own_project(selected) && self.focus.kind == Kind::Memory
                    }
                    SourceKey::ProjectKnowledge => self.focus.kind == Kind::Knowledge,
                    SourceKey::UserMemory => {
                        self.include_user
                            && self.focus.kind == Kind::Memory
                            && self.focus.context.is_none()
                            && matches!(self.focus.scope, Scope::User(_))
                    }
                    SourceKey::ImportedKnowledge { .. } => self.focus.kind == Kind::Knowledge,
                };
                if !valid
                    || !self.focus.readable_id()
                    || owner.as_ref().is_some_and(|owner| owner != source)
                {
                    return Err("invalid_arguments");
                }
            }
            Ok(())
        };
        if let Some(owner) = &self.outgoing_source {
            check(owner, Direction::Outgoing)?;
        }
        let mut plans = Vec::new();
        for source in universe.iter().cloned() {
            let (outgoing, incoming, outgoing_cursor, incoming_cursor) =
                if let Some(pages) = &self.pages {
                    let out = pages
                        .iter()
                        .find(|p| p.source == source && p.direction == Direction::Outgoing);
                    let inc = pages
                        .iter()
                        .find(|p| p.source == source && p.direction == Direction::Incoming);
                    (
                        out.is_some(),
                        inc.is_some(),
                        out.and_then(|p| p.cursor.clone()),
                        inc.and_then(|p| p.cursor.clone()),
                    )
                } else {
                    (owner.as_ref() == Some(&source), true, None, None)
                };
            if !outgoing && !incoming {
                continue;
            }
            if outgoing {
                check(&source, Direction::Outgoing)?;
            }
            if incoming {
                check(&source, Direction::Incoming)?;
            }
            if matches!(
                source,
                SourceKey::ProjectTask | SourceKey::ProjectMemory | SourceKey::ProjectKnowledge
            ) && selected.is_none()
            {
                return Err("invalid_selection");
            }
            plans.push(Plan {
                source,
                outgoing,
                incoming,
                outgoing_cursor,
                incoming_cursor,
            });
        }
        if let Some(pages) = &self.pages {
            for page in pages {
                check(&page.source, page.direction)?;
            }
        }
        Ok(plans)
    }
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum ImportedRead {
    RegistryAbsent,
    Published {
        producer: graphit_core::project_identity::ProjectId,
        generation: KnowledgeGeneration,
        read: Box<ImportedKnowledgeReferencesRead>,
    },
}
#[derive(Serialize)]
#[serde(untagged)]
enum Native {
    Task(TaskReferencesRead),
    TaskIncoming(TaskReferenceBacklinksRead),
    Memory(MemoryReferencesRead),
    User(UserMemoryReferencesRead),
    Knowledge(KnowledgeReferencesRead),
    Imported(ImportedRead),
}
#[derive(Serialize)]
struct Reader<'a> {
    selection: Option<&'a AdmittedSelection>,
    input: ReaderInput,
    availability: &'static str,
    method: &'static str,
    path: &'static str,
    focus: Focus,
    source: SourceKey,
}
#[derive(Serialize)]
struct Rich<'a> {
    title: String,
    reader: Reader<'a>,
    href: Option<String>,
    generation: Option<KnowledgeGeneration>,
}
#[derive(Serialize)]
struct Navigation<'a> {
    source_href: Option<String>,
    source_reader: Option<Reader<'a>>,
    target_rich: Option<Rich<'a>>,
}
#[derive(Serialize)]
struct NavigationPages<'a> {
    outgoing: Vec<Navigation<'a>>,
    incoming: Vec<Navigation<'a>>,
}
#[derive(Serialize)]
struct Source<'a> {
    source: SourceKey,
    native: Native,
    navigation: NavigationPages<'a>,
}
#[derive(Serialize)]
struct Response<'a> {
    selection: Option<&'a AdmittedSelection>,
    sources: &'a [Source<'a>],
    outgoing_complete: bool,
    incoming_complete: bool,
    warnings: &'a [&'static str],
}

async fn admit<'a>(
    authority: &'a ProjectRecordAuthority<'a>,
    scope: BrokerProjectScope<'a>,
    module: ProjectReadModule,
    identity: Identity<'a>,
    backend: &'a BrokerProjectAccess,
    original: &'a RequestContext<'a>,
) -> Result<ProjectReadAdmission<'a>, BrokerProjectError> {
    let (caller, verifier, provider) = identity;
    authority
        .admit_broker_read(BrokerRecordRead {
            caller: BrokerProjectCaller::Web(caller),
            verifier,
            backend,
            original,
            scope,
            provider,
            module,
        })
        .await
}
fn denied(error: &BrokerProjectError) -> bool {
    matches!(
        error,
        BrokerProjectError::Denied | BrokerProjectError::Authority(AuthError::Denied)
    )
}

pub(crate) async fn read<'a>(
    resources: Resources<'a>,
    arguments: Arguments,
    identity: Identity<'a>,
    backend: &'a BrokerProjectAccess,
    original: &'a RequestContext<'a>,
) -> Result<Vec<u8>, Failure> {
    resources.check(original)?;
    let input: Input = serde_json::from_value(
        arguments
            .input
            .ok_or(Failure::Invalid("invalid_arguments"))?,
    )
    .map_err(|_| Failure::Invalid("invalid_arguments"))?;
    let validated_artifacts = input
        .contexts
        .iter()
        .map(|key| key.native(resources.policy))
        .collect::<Result<Vec<_>, _>>()?;
    let plans = input
        .plans(arguments.selection.project_id)
        .map_err(Failure::Invalid)?;
    let need_project = plans.iter().any(|p| {
        matches!(
            p.source,
            SourceKey::ProjectTask | SourceKey::ProjectMemory | SourceKey::ProjectKnowledge
        )
    });
    let selected = if need_project {
        Some(resources.select_exact(&arguments.selection)?)
    } else {
        None
    };
    if let Some(selected) = selected {
        resources.require_active(selected)?;
    }
    let project = selected
        .map(|(paths, id, _)| {
            CapturedPublicationScope::project(
                paths,
                *id,
                resources.policy.scope,
                original.snapshot(),
            )
        })
        .transpose()
        .map_err(Failure::Physical)?;
    let binding = project
        .as_ref()
        .map(|p| p.binding())
        .transpose()
        .map_err(Failure::Physical)?;
    let selection = selected
        .map(|(paths, id, version)| -> Result<_, Failure> {
            Ok(AdmittedSelection {
                project_dir: NativePath::new(paths.root().to_owned()).map_err(Failure::Physical)?,
                project_id: *id,
                project_version: version.version().to_owned(),
            })
        })
        .transpose()?;
    let capture = project
        .as_ref()
        .zip(binding.as_ref())
        .map(|(live, record)| ScopeCapture { live, record });
    let authority = ProjectRecordAuthority::new(resources.auth.issuer());
    let mut task = None;
    let mut memory = None;
    let mut knowledge = None;
    for plan in &plans {
        let module = match plan.source {
            SourceKey::ProjectTask => Some(ProjectReadModule::Task),
            SourceKey::ProjectMemory => Some(ProjectReadModule::Memory),
            SourceKey::ProjectKnowledge => Some(ProjectReadModule::Knowledge),
            _ => None,
        };
        if let Some(module) = module {
            let admission = admit(
                &authority,
                BrokerProjectScope {
                    installation: resources.auth.installation(),
                    project: ProjectCapture {
                        scope: capture.ok_or(Failure::Invalid("invalid_selection"))?,
                        version: &selected.ok_or(Failure::Invalid("invalid_selection"))?.2,
                    },
                    providers: &[],
                },
                module,
                identity,
                backend,
                original,
            )
            .await?;
            match module {
                ProjectReadModule::Task => task = Some(admission),
                ProjectReadModule::Memory => memory = Some(admission),
                ProjectReadModule::Knowledge => knowledge = Some(admission),
                ProjectReadModule::Ast | ProjectReadModule::Hub => {
                    return Err(Failure::Invalid("unsupported_reference_module"));
                }
            }
        }
    }
    let need_user = plans.iter().any(|p| p.source == SourceKey::UserMemory);
    let user = if need_user {
        Some(
            foreground_user_memory::capture(
                resources.auth.issuer(),
                resources.auth.installation(),
                identity.0,
                identity.1,
                original,
                resources.policy,
            )
            .map_err(Failure::from)?,
        )
    } else {
        None
    };
    let user_binding = user
        .as_ref()
        .map(|u| u.binding())
        .transpose()
        .map_err(Failure::Physical)?;
    let user_admission = user
        .as_ref()
        .zip(user_binding.as_ref())
        .map(|(live, record)| {
            resources.auth.issuer().admit_web_user_memory_read(
                identity.0,
                identity.1,
                original,
                identity.2,
                ScopeCapture { live, record },
            )
        })
        .transpose()?;
    let user_scope = user_admission
        .as_ref()
        .map(|admission| -> Result<_, Failure> {
            let live = admission.user()?.live;
            let StoreOwner::User {
                installation_id,
                identity,
            } = live.owner()
            else {
                return Err(Failure::Invalid("wrong_admission"));
            };
            Ok(UserMemoryScopeId::from_owner(&UserMemoryOwner {
                installation_id: installation_id.clone(),
                identity: identity.clone(),
            })?)
        })
        .transpose()?;
    if plans
        .iter()
        .any(|p| p.source == SourceKey::UserMemory && p.outgoing)
        && !matches!(&input.focus.scope,Scope::User(scope) if user_scope.as_ref().is_some_and(|own|own.as_str()==scope))
    {
        return Err(Failure::Invalid("invalid_user_scope"));
    }
    let imported_keys = plans
        .iter()
        .filter_map(|p| {
            if let SourceKey::ImportedKnowledge { artifact } = &p.source {
                Some(artifact)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let native_keys = imported_keys
        .iter()
        .map(|key| {
            input
                .contexts
                .iter()
                .position(|context| context == *key)
                .map(|index| &validated_artifacts[index])
                .ok_or(Failure::Invalid("invalid_arguments"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let imported_admission = if imported_keys.is_empty() {
        None
    } else {
        Some(resources.auth.issuer().admit_web_imported_knowledge_read(
            identity.0,
            identity.1,
            original,
            identity.2,
            resources.auth.installation(),
        )?)
    };
    let imported_policy = if imported_keys.is_empty() {
        None
    } else {
        Some(
            resources
                .policy
                .imported_knowledge
                .as_ref()
                .ok_or(Failure::Unavailable)?,
        )
    };
    let mut imported_readers = Vec::new();
    let mut sources = Vec::with_capacity(plans.len());
    for plan in &plans {
        resources.check(original)?;
        let native = match &plan.source {
            SourceKey::ProjectTask => {
                let admission = task.as_ref().ok_or(Failure::Invalid("wrong_admission"))?;
                if input
                    .focus
                    .own_task(selection.as_ref().map(|s| s.project_id))
                {
                    let cursor = |c: &Option<Cursor>| match c {
                        None => Ok(None),
                        Some(Cursor::Task(c)) => Ok(Some(c.clone())),
                        _ => Err(Failure::Invalid("invalid_cursor")),
                    };
                    Native::Task(
                        NativeTaskStore::references(
                            original.snapshot(),
                            admission,
                            TaskReferencesQuery {
                                focus: input.focus.task_record()?,
                                page_size: input.page_size,
                                outgoing_cursor: cursor(&plan.outgoing_cursor)?,
                                incoming_cursor: cursor(&plan.incoming_cursor)?,
                            },
                            resources.policy.read,
                        )
                        .await?,
                    )
                } else {
                    let cursor = match &plan.incoming_cursor {
                        None => None,
                        Some(Cursor::TaskIncoming(c)) => Some(c.clone()),
                        _ => return Err(Failure::Invalid("invalid_cursor")),
                    };
                    Native::TaskIncoming(
                        NativeTaskStore::reference_backlinks(
                            original.snapshot(),
                            admission,
                            TaskReferenceBacklinksQuery {
                                target: input.focus.task_target(),
                                page_size: input.page_size,
                                cursor,
                            },
                            resources.policy.read,
                        )
                        .await?,
                    )
                }
            }
            SourceKey::ProjectMemory => {
                let cursor = |c: &Option<Cursor>| match c {
                    None => Ok(None),
                    Some(Cursor::Memory(c)) => Ok(Some(c.clone())),
                    _ => Err(Failure::Invalid("invalid_cursor")),
                };
                Native::Memory(
                    NativeMemoryStore::references(
                        original.snapshot(),
                        memory.as_ref().ok_or(Failure::Invalid("wrong_admission"))?,
                        MemoryReferencesQuery {
                            outgoing: if plan.outgoing {
                                Some(id(&input.focus.id).map_err(Failure::Invalid)?)
                            } else {
                                None
                            },
                            incoming: plan.incoming.then(|| input.focus.memory_target()),
                            page_size: input.page_size,
                            outgoing_cursor: cursor(&plan.outgoing_cursor)?,
                            incoming_cursor: cursor(&plan.incoming_cursor)?,
                        },
                        foreground_memory::limits(resources.policy)?,
                    )
                    .await?,
                )
            }
            SourceKey::UserMemory => {
                let cursor = |c: &Option<Cursor>| match c {
                    None => Ok(None),
                    Some(Cursor::User(c)) => Ok(Some(c.clone())),
                    _ => Err(Failure::Invalid("invalid_cursor")),
                };
                Native::User(
                    NativeUserMemoryStore::references(
                        original.snapshot(),
                        user_admission
                            .as_ref()
                            .ok_or(Failure::Invalid("wrong_admission"))?,
                        UserMemoryReferencesQuery {
                            outgoing: if plan.outgoing {
                                Some(id(&input.focus.id).map_err(Failure::Invalid)?)
                            } else {
                                None
                            },
                            incoming: plan.incoming.then(|| input.focus.memory_target()),
                            page_size: input.page_size,
                            outgoing_cursor: cursor(&plan.outgoing_cursor)?,
                            incoming_cursor: cursor(&plan.incoming_cursor)?,
                        },
                        foreground_memory::limits(resources.policy)?,
                    )
                    .await?,
                )
            }
            SourceKey::ProjectKnowledge => {
                let cursor = knowledge_cursor;
                Native::Knowledge(
                    read_native_knowledge_references(
                        original.snapshot(),
                        knowledge
                            .as_ref()
                            .ok_or(Failure::Invalid("wrong_admission"))?,
                        KnowledgeReferencesQuery {
                            outgoing: plan.outgoing.then_some(input.focus.id.as_str()),
                            incoming: plan.incoming.then(|| input.focus.knowledge_target()),
                            page_size: input.page_size,
                            outgoing_cursor: cursor(&plan.outgoing_cursor)?,
                            incoming_cursor: cursor(&plan.incoming_cursor)?,
                        },
                        foreground_knowledge::limits(resources.policy)?,
                    )
                    .await?,
                )
            }
            SourceKey::ImportedKnowledge { artifact } => {
                let index = imported_keys
                    .iter()
                    .position(|key| *key == artifact)
                    .ok_or(Failure::Invalid("invalid_arguments"))?;
                let (id, version) = native_keys[index];
                let policy = imported_policy.ok_or(Failure::Unavailable)?;
                let reader = open_imported_knowledge(
                    imported_admission
                        .as_ref()
                        .ok_or(Failure::Invalid("wrong_admission"))?,
                    ImportedKnowledgeInput {
                        registry: &policy.registry,
                        id,
                        version,
                        context_alias: artifact.context_alias.as_deref(),
                    },
                    ImportedKnowledgeReadLimits {
                        directory: policy.directory,
                        index: foreground_knowledge::limits(resources.policy)?,
                    },
                )
                .await?;
                let result = match &reader {
                    ImportedKnowledgeOpen::RegistryAbsent => {
                        if plan.outgoing_cursor.is_some() || plan.incoming_cursor.is_some() {
                            return Err(Failure::Invalid("invalid_import_cursor"));
                        }
                        ImportedRead::RegistryAbsent
                    }
                    ImportedKnowledgeOpen::Published(reader) => {
                        let cursor = imported_cursor;
                        let read = reader
                            .references(
                                ImportedKnowledgeReferencesQuery {
                                    outgoing: plan.outgoing.then_some(input.focus.id.as_str()),
                                    incoming: plan.incoming.then(|| input.focus.knowledge_target()),
                                    page_size: input.page_size,
                                    outgoing_cursor: cursor(&plan.outgoing_cursor)?,
                                    incoming_cursor: cursor(&plan.incoming_cursor)?,
                                },
                                foreground_knowledge::limits(resources.policy)?,
                            )
                            .await?;
                        ImportedRead::Published {
                            producer: reader.observation().producer,
                            generation: reader.observation().generation.clone(),
                            read: Box::new(read),
                        }
                    }
                };
                imported_readers.push(reader);
                Native::Imported(result)
            }
        };
        if plan.outgoing {
            let focused = match &native {
                Native::Knowledge(KnowledgeReferencesRead::Published { read, .. }) => {
                    read.focused_source.as_ref()
                }
                Native::Imported(ImportedRead::Published { read, .. }) => {
                    read.focused_source.as_ref()
                }
                _ => None,
            };
            if let Some(focused) = focused
                && focused.namespace.is_some()
                && !focused_matches(&input.focus, focused)
            {
                return Err(Failure::Invalid(if matches!(native, Native::Imported(_)) {
                    "invalid_import_namespace"
                } else {
                    "invalid_knowledge_namespace"
                }));
            }
        }
        sources.push(Source {
            source: plan.source.clone(),
            native,
            navigation: NavigationPages {
                outgoing: Vec::new(),
                incoming: Vec::new(),
            },
        });
    }
    // Rich reads may add another module admission only within an already captured PROJECT.
    let edges = sources.iter().flat_map(edge_focuses).collect::<Vec<_>>();
    if let (Some(capture), Some(selected)) = (capture, selected) {
        for (module, slot) in [
            (ProjectReadModule::Task, &mut task),
            (ProjectReadModule::Memory, &mut memory),
            (ProjectReadModule::Knowledge, &mut knowledge),
        ] {
            if slot.is_some() {
                continue;
            }
            let needed = edges.iter().any(|(_, focus, _)| {
                focus.readable_id()
                    && focus.own_project(Some(selected.1))
                    && match module {
                        ProjectReadModule::Task => matches!(focus.kind, Kind::Task | Kind::Session),
                        ProjectReadModule::Memory => focus.kind == Kind::Memory,
                        ProjectReadModule::Knowledge => focus.kind == Kind::Knowledge,
                        ProjectReadModule::Ast | ProjectReadModule::Hub => false,
                    }
            });
            if needed {
                match admit(
                    &authority,
                    BrokerProjectScope {
                        installation: resources.auth.installation(),
                        project: ProjectCapture {
                            scope: capture,
                            version: &selected.2,
                        },
                        providers: &[],
                    },
                    module,
                    identity,
                    backend,
                    original,
                )
                .await
                {
                    Ok(admission) => *slot = Some(admission),
                    Err(e) if denied(&e) => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }
    let mut rich = Vec::new();
    for (source, focus, own_source) in &edges {
        resources.check(original)?;
        if !focus.readable_id() {
            continue;
        }
        if rich
            .iter()
            .any(|(s, f, own, _): &(SourceKey, Focus, bool, Option<Rich>)| {
                s == source && f == focus && own == own_source
            })
        {
            continue;
        }
        let observed = if focus.kind == Kind::Knowledge {
            let mut candidates = Vec::new();
            if (!*own_source || *source == SourceKey::ProjectKnowledge)
                && focus.own_project(selection.as_ref().map(|s| s.project_id))
                && let Some(admission) = &knowledge
                && let Some((title, generation)) =
                    native_knowledge_rich(&resources, original, admission, focus).await?
            {
                candidates.push((title, Some(generation), SourceKey::ProjectKnowledge));
            }
            let mut physical_imports = Vec::new();
            for (index, artifact) in imported_keys.iter().enumerate() {
                let candidate_source = SourceKey::ImportedKnowledge {
                    artifact: (*artifact).clone(),
                };
                if *own_source && source != &candidate_source {
                    continue;
                }
                // Mount aliases bind cursors, but the registry id/version identifies one physical payload.
                if physical_imports.contains(&(&artifact.id, &artifact.version)) {
                    continue;
                }
                physical_imports.push((&artifact.id, &artifact.version));
                if let ImportedKnowledgeOpen::Published(reader) = &imported_readers[index]
                    && let KnowledgeSnapshotPage::Found {
                        document,
                        focused_source: Some(head),
                    } = reader
                        .page(&focus.id, foreground_knowledge::limits(resources.policy)?)
                        .await?
                    && focused_matches(focus, &head)
                {
                    candidates.push((
                        document.title,
                        Some(reader.observation().generation.clone()),
                        candidate_source,
                    ));
                }
            }
            if candidates.len() == 1 {
                candidates.pop()
            } else {
                None
            }
        } else if focus.own_project(selection.as_ref().map(|s| s.project_id)) {
            match focus.kind {
                Kind::Task => {
                    if let Some(admission) = &task {
                        NativeTaskStore::read_task(
                            original.snapshot(),
                            admission,
                            TaskGetInput::current(id(&focus.id).map_err(Failure::Invalid)?),
                            resources.policy.read,
                        )
                        .await?
                        .map(|d| (d.task.specification.title, None, SourceKey::ProjectTask))
                    } else {
                        None
                    }
                }
                Kind::Session => {
                    if let Some(admission) = &task {
                        NativeTaskStore::read_session(
                            original.snapshot(),
                            admission,
                            SessionGetInput::current(id(&focus.id).map_err(Failure::Invalid)?),
                            resources.policy.read,
                        )
                        .await?
                        .map(|d| (d.session.input.title, None, SourceKey::ProjectTask))
                    } else {
                        None
                    }
                }
                Kind::Memory => {
                    if let Some(admission) = &memory {
                        NativeMemoryStore::get(
                            original.snapshot(),
                            admission,
                            id(&focus.id).map_err(Failure::Invalid)?,
                            foreground_memory::limits(resources.policy)?,
                        )
                        .await?
                        .map(|d| (d.memory.title, None, SourceKey::ProjectMemory))
                    } else {
                        None
                    }
                }
                Kind::Knowledge => unreachable!("Knowledge candidates handled above"),
            }
        } else if focus.kind == Kind::Memory
            && focus.context.is_none()
            && matches!(&focus.scope,Scope::User(value) if user_scope.as_ref().is_some_and(|scope|scope.as_str()==value))
        {
            if let Some(admission) = &user_admission {
                NativeUserMemoryStore::get(
                    original.snapshot(),
                    admission,
                    id(&focus.id).map_err(Failure::Invalid)?,
                    foreground_memory::limits(resources.policy)?,
                )
                .await?
                .map(|d| (d.memory.title, None, SourceKey::UserMemory))
            } else {
                None
            }
        } else {
            None
        };
        rich.push((
            source.clone(),
            focus.clone(),
            *own_source,
            observed.map(|(title, generation, source)| Rich {
                title,
                reader: reader(source, focus.clone(), selection.as_ref()),
                href: None,
                generation,
            }),
        ));
    }
    for source in &mut sources {
        source.navigation = navigation(source, &rich);
    }
    let mut outgoing_complete = true;
    let mut incoming_complete = true;
    for (source, plan) in sources.iter().zip(&plans) {
        let (out, inc) = completeness(&source.native);
        if plan.outgoing {
            outgoing_complete &= out;
        }
        if plan.incoming {
            incoming_complete &= inc;
        }
    }
    if let Some(selected) = selected {
        resources.require_active(selected)?;
    }
    for admission in [&task, &memory, &knowledge].into_iter().flatten() {
        admission.recheck()?;
    }
    if let Some(admission) = &user_admission {
        admission.recheck()?;
    }
    if let Some(admission) = &imported_admission {
        admission.recheck()?;
    }
    for reader in &imported_readers {
        if let ImportedKnowledgeOpen::Published(reader) = reader {
            reader.recheck()?;
        }
    }
    let bytes = explorer::encode(
        &Response {
            selection: selection.as_ref(),
            sources: &sources,
            outgoing_complete,
            incoming_complete,
            warnings: &[],
        },
        original.snapshot(),
        resources.stop,
        resources.policy.content.max_output_bytes,
    )?;
    for admission in [&task, &memory, &knowledge].into_iter().flatten() {
        admission.recheck()?;
    }
    if let Some(admission) = &user_admission {
        admission.recheck()?;
    }
    if let Some(admission) = &imported_admission {
        admission.recheck()?;
    }
    for reader in &imported_readers {
        if let ImportedKnowledgeOpen::Published(reader) = reader {
            reader.recheck()?;
        }
    }
    if let Some(selected) = selected {
        resources.require_active(selected)?;
    }
    resources.check(original)?;
    Ok(bytes)
}

fn focused_matches(focus: &Focus, source: &KnowledgeReferenceSource) -> bool {
    source.slug == focus.id
        && source.namespace.as_ref().is_some_and(|namespace| {
            namespace.context == focus.context
                && match (&namespace.scope, &focus.scope) {
                    (KnowledgeReferenceOwnedScope::Project(actual), Scope::Project(requested)) => {
                        actual == requested
                    }
                    (KnowledgeReferenceOwnedScope::User(actual), Scope::User(requested)) => {
                        actual == requested
                    }
                    _ => false,
                }
        })
}
async fn native_knowledge_rich(
    resources: &Resources<'_>,
    original: &RequestContext<'_>,
    admission: &ProjectReadAdmission<'_>,
    focus: &Focus,
) -> Result<Option<(String, KnowledgeGeneration)>, Failure> {
    match graphit_data::knowledge_index::read_native_knowledge_page(
        original.snapshot(),
        admission,
        &focus.id,
        foreground_knowledge::limits(resources.policy)?,
    )
    .await?
    {
        KnowledgeIndexPageRead::Published {
            generation: page_generation,
            page:
                KnowledgeSnapshotPage::Found {
                    document,
                    focused_source: Some(source),
                },
        } if focused_matches(focus, &source) => Ok(Some((document.title, page_generation))),
        _ => Ok(None),
    }
}

fn task_focus(record: TaskReferenceRecord, scope: Scope) -> Focus {
    fn literal<T: Serialize>(id: T) -> String {
        match serde_json::to_value(id).expect("native identifier serialization") {
            serde_json::Value::String(value) => value,
            _ => unreachable!("native Task/Session identifiers are strings"),
        }
    }
    match record {
        TaskReferenceRecord::Task(id) => Focus {
            kind: Kind::Task,
            scope,
            id: literal(id),
            context: None,
        },
        TaskReferenceRecord::Session(id) => Focus {
            kind: Kind::Session,
            scope,
            id: literal(id),
            context: None,
        },
    }
}
fn task_target(reference: &graphit_records::task_store::TaskReference) -> Focus {
    Focus {
        kind: match reference.target_kind {
            TaskReferenceKind::Task => Kind::Task,
            TaskReferenceKind::Session => Kind::Session,
            TaskReferenceKind::Memory => Kind::Memory,
            TaskReferenceKind::Knowledge => Kind::Knowledge,
        },
        scope: match &reference.scope {
            TaskReferenceScope::Project(p) => Scope::Project(*p),
            TaskReferenceScope::User(u) => Scope::User(u.clone()),
        },
        id: reference.id.clone(),
        context: reference.context.clone(),
    }
}
fn memory_target(reference: &graphit_records::memory_store::MemoryReference) -> Focus {
    Focus {
        kind: match reference.target_kind {
            ReferenceTargetKind::Task => Kind::Task,
            ReferenceTargetKind::Session => Kind::Session,
            ReferenceTargetKind::Memory => Kind::Memory,
            ReferenceTargetKind::Knowledge => Kind::Knowledge,
        },
        scope: match &reference.scope {
            MemoryReferenceScope::Project(p) => Scope::Project(*p),
            MemoryReferenceScope::User(u) => Scope::User(u.clone()),
        },
        id: reference.id.clone(),
        context: reference.context.clone(),
    }
}
fn knowledge_target(payload: &KnowledgeReferencePayload) -> Focus {
    let (KnowledgeReferencePayload::Authored { target, .. }
    | KnowledgeReferencePayload::WikiLink { target }) = payload;
    Focus {
        kind: match target.target_kind {
            KnowledgeReferenceTargetKind::Task => Kind::Task,
            KnowledgeReferenceTargetKind::Session => Kind::Session,
            KnowledgeReferenceTargetKind::Memory => Kind::Memory,
            KnowledgeReferenceTargetKind::Knowledge => Kind::Knowledge,
        },
        scope: match &target.namespace.scope {
            KnowledgeReferenceOwnedScope::Project(p) => Scope::Project(*p),
            KnowledgeReferenceOwnedScope::User(u) => Scope::User(u.clone()),
        },
        id: target.id.clone(),
        context: target.namespace.context.clone(),
    }
}
struct Edge {
    direction: Direction,
    source: Option<Focus>,
    target: Focus,
}
fn edges(source: &Source<'_>) -> Vec<Edge> {
    let mut result = Vec::new();
    match &source.native {
        Native::Task(TaskReferencesRead::Published {
            outgoing,
            incoming,
            coverage,
            ..
        }) => {
            for (direction, page) in [
                (Direction::Outgoing, outgoing),
                (Direction::Incoming, incoming),
            ] {
                for edge in &page.results {
                    result.push(Edge {
                        direction,
                        source: Some(task_focus(edge.source, Scope::Project(coverage.project_id))),
                        target: task_target(&edge.reference),
                    });
                }
            }
        }
        Native::TaskIncoming(TaskReferenceBacklinksRead::Published {
            incoming, coverage, ..
        }) => {
            for edge in &incoming.results {
                result.push(Edge {
                    direction: Direction::Incoming,
                    source: Some(task_focus(edge.source, Scope::Project(coverage.project_id))),
                    target: task_target(&edge.reference),
                });
            }
        }
        Native::Memory(MemoryReferencesRead::Published {
            outgoing,
            incoming,
            coverage,
            ..
        }) => {
            for (direction, page) in [
                (Direction::Outgoing, outgoing),
                (Direction::Incoming, incoming),
            ] {
                if let Some(page) = page {
                    for edge in &page.results {
                        result.push(Edge {
                            direction,
                            source: Some(Focus {
                                kind: Kind::Memory,
                                scope: Scope::Project(coverage.project_id),
                                id: edge.source.to_string(),
                                context: None,
                            }),
                            target: memory_target(&edge.reference),
                        });
                    }
                }
            }
        }
        Native::User(UserMemoryReferencesRead::Published {
            outgoing, incoming, ..
        }) => {
            for (direction, page) in [
                (Direction::Outgoing, outgoing),
                (Direction::Incoming, incoming),
            ] {
                if let Some(page) = page {
                    for edge in &page.results {
                        result.push(Edge {
                            direction,
                            source: Some(Focus {
                                kind: Kind::Memory,
                                scope: Scope::User(edge.source_scope.as_str().to_owned()),
                                id: edge.source.to_string(),
                                context: None,
                            }),
                            target: memory_target(&edge.reference),
                        });
                    }
                }
            }
        }
        Native::Knowledge(KnowledgeReferencesRead::Published { read, .. }) => {
            for (direction, page) in [
                (Direction::Outgoing, &read.outgoing),
                (Direction::Incoming, &read.incoming),
            ] {
                if let Some(page) = page {
                    for edge in &page.results {
                        result.push(Edge {
                            direction,
                            source: knowledge_source(&edge.source),
                            target: knowledge_target(&edge.payload),
                        });
                    }
                }
            }
        }
        Native::Imported(ImportedRead::Published { read, .. }) => {
            for (direction, page) in [
                (Direction::Outgoing, &read.outgoing),
                (Direction::Incoming, &read.incoming),
            ] {
                if let Some(page) = page {
                    for edge in &page.results {
                        result.push(Edge {
                            direction,
                            source: knowledge_source(&edge.source),
                            target: knowledge_target(&edge.payload),
                        });
                    }
                }
            }
        }
        _ => {}
    }
    result
}
fn knowledge_source(source: &KnowledgeReferenceSource) -> Option<Focus> {
    let namespace = source.namespace.as_ref()?;
    Some(Focus {
        kind: Kind::Knowledge,
        scope: match &namespace.scope {
            KnowledgeReferenceOwnedScope::Project(project) => Scope::Project(*project),
            KnowledgeReferenceOwnedScope::User(user) => Scope::User(user.clone()),
        },
        context: namespace.context.clone(),
        id: source.slug.clone(),
    })
}
#[derive(Serialize)]
#[serde(untagged)]
enum ReaderInput {
    Record { id: String },
    Page { slug: String },
    Imported { source: SourceKey, slug: String },
}
fn knowledge_cursor(c: &Option<Cursor>) -> Result<Option<&KnowledgeReferencesCursor>, Failure> {
    match c {
        None => Ok(None),
        Some(Cursor::Knowledge(c)) => Ok(Some(c)),
        _ => Err(Failure::Invalid("invalid_cursor")),
    }
}
fn imported_cursor(
    c: &Option<Cursor>,
) -> Result<Option<&ImportedKnowledgeReferencesCursor>, Failure> {
    match c {
        None => Ok(None),
        Some(Cursor::Imported(c)) => Ok(Some(c)),
        _ => Err(Failure::Invalid("invalid_cursor")),
    }
}
fn edge_focuses(source: &Source<'_>) -> Vec<(SourceKey, Focus, bool)> {
    edges(source)
        .into_iter()
        .flat_map(|edge| {
            edge.source
                .into_iter()
                .map(|focus| (source.source.clone(), focus, true))
                .chain(std::iter::once((source.source.clone(), edge.target, false)))
        })
        .collect()
}
fn reader(source: SourceKey, focus: Focus, selection: Option<&AdmittedSelection>) -> Reader<'_> {
    let path = match (&source, focus.kind) {
        (SourceKey::UserMemory, _) => "/api/user-memory/get",
        (SourceKey::ImportedKnowledge { .. }, _) => "/api/knowledge/imported/page",
        (_, Kind::Task) => "/api/tasks/get",
        (_, Kind::Session) => "/api/tasks/session",
        (_, Kind::Memory) => "/api/memory/get",
        (_, Kind::Knowledge) => "/api/knowledge/page",
    };
    let input = match &source {
        SourceKey::ImportedKnowledge { .. } => ReaderInput::Imported {
            source: source.clone(),
            slug: focus.id.clone(),
        },
        _ if focus.kind == Kind::Knowledge => ReaderInput::Page {
            slug: focus.id.clone(),
        },
        _ => ReaderInput::Record {
            id: focus.id.clone(),
        },
    };
    let selection = if matches!(
        source,
        SourceKey::UserMemory | SourceKey::ImportedKnowledge { .. }
    ) {
        None
    } else {
        selection
    };
    Reader {
        method: "POST",
        path,
        focus,
        source,
        selection,
        input,
        availability: "available",
    }
}
fn navigation<'a>(
    source: &Source<'a>,
    rich: &[(SourceKey, Focus, bool, Option<Rich<'a>>)],
) -> NavigationPages<'a> {
    let mut output = NavigationPages {
        outgoing: Vec::new(),
        incoming: Vec::new(),
    };
    for edge in edges(source) {
        let source_reader = rich
            .iter()
            .find(|(s, f, own, _)| s == &source.source && edge.source.as_ref() == Some(f) && *own)
            .and_then(|(_, _, _, r)| r.as_ref())
            .map(|r| {
                reader(
                    r.reader.source.clone(),
                    r.reader.focus.clone(),
                    r.reader.selection,
                )
            });
        let target_rich = rich
            .iter()
            .find(|(s, f, own, _)| s == &source.source && f == &edge.target && !*own)
            .and_then(|(_, _, _, r)| r.as_ref())
            .map(|r| Rich {
                title: r.title.clone(),
                reader: reader(
                    r.reader.source.clone(),
                    r.reader.focus.clone(),
                    r.reader.selection,
                ),
                href: None,
                generation: r.generation.clone(),
            });
        let navigation = Navigation {
            source_href: None,
            source_reader,
            target_rich,
        };
        match edge.direction {
            Direction::Outgoing => output.outgoing.push(navigation),
            Direction::Incoming => output.incoming.push(navigation),
        }
    }
    output
}
fn completeness(native: &Native) -> (bool, bool) {
    match native {
        Native::Task(TaskReferencesRead::Published {
            outgoing, incoming, ..
        }) => (
            outgoing.complete && outgoing.next_cursor.is_none(),
            incoming.complete && incoming.next_cursor.is_none(),
        ),
        Native::TaskIncoming(TaskReferenceBacklinksRead::Published { incoming, .. }) => {
            (true, incoming.complete && incoming.next_cursor.is_none())
        }
        Native::Memory(MemoryReferencesRead::Published {
            outgoing, incoming, ..
        }) => (
            outgoing
                .as_ref()
                .is_none_or(|p| p.complete && p.next_cursor.is_none()),
            incoming
                .as_ref()
                .is_none_or(|p| p.complete && p.next_cursor.is_none()),
        ),
        Native::User(UserMemoryReferencesRead::Published {
            outgoing, incoming, ..
        }) => (
            outgoing
                .as_ref()
                .is_none_or(|p| p.complete && p.next_cursor.is_none()),
            incoming
                .as_ref()
                .is_none_or(|p| p.complete && p.next_cursor.is_none()),
        ),
        Native::Knowledge(KnowledgeReferencesRead::Published { read, .. }) => (
            read.outgoing
                .as_ref()
                .is_none_or(|p| p.complete && p.next_cursor.is_none()),
            read.incoming
                .as_ref()
                .is_none_or(|p| p.complete && p.next_cursor.is_none()),
        ),
        Native::Imported(ImportedRead::Published { read, .. }) => (
            read.outgoing
                .as_ref()
                .is_none_or(|p| p.complete && p.next_cursor.is_none()),
            read.incoming
                .as_ref()
                .is_none_or(|p| p.complete && p.next_cursor.is_none()),
        ),
        _ => (true, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    fn limits() -> ContentLimits {
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_scalar_bytes: 4096,
            max_collection_entries: 256,
            max_depth: 1,
            max_nodes: 1,
        }
    }
    fn body() -> Value {
        json!({"selection":{"project_dir":NativePath::new(std::env::temp_dir()).unwrap(),"project_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","project_version":"branch/literal"},"input":{"focus":{"kind":"memory","scope":{"project":"01ARZ3NDEKTSV4RRFFQ69G5FAV"},"id":"literal-target"},"pages":[{"source":{"kind":"project_task"},"direction":"incoming"}]}})
    }
    fn parsed(body: &Value) -> Result<Arguments, &'static str> {
        parse(
            &serde_json::to_vec(body).unwrap(),
            limits(),
            "records_references",
        )
    }
    #[test]
    fn logical_incoming_keeps_literal_id_and_source_namespace_without_target_lookup() {
        let arguments = parsed(&body()).unwrap();
        let query: Input = serde_json::from_value(arguments.input.unwrap()).unwrap();
        assert_eq!(query.focus.id, "literal-target");
        assert_eq!(
            arguments.selection.project_version.as_deref(),
            Some("branch/literal")
        );
        assert!(!query.focus.readable_id());
        let plans = query.plans(arguments.selection.project_id).unwrap();
        assert_eq!(plans.len(), 1);
        assert!(!plans[0].outgoing);
        assert!(plans[0].incoming);
        let mut invalid = body();
        invalid["input"]["pages"][0]["direction"] = json!("outgoing");
        assert!(parsed(&invalid).is_err());
    }
    #[test]
    fn user_only_and_import_only_universe_does_not_require_project_selection() {
        let user = json!({"input":{"focus":{"kind":"memory","scope":{"user":"literal user target"},"id":"literal-memory"},"include_user":true,"pages":[{"source":{"kind":"user_memory"},"direction":"incoming"}]}});
        let parsed = parsed(&user).unwrap();
        assert!(parsed.selection.project_dir.is_none());
        let mut false_user = user;
        false_user["input"]["include_user"] = json!(false);
        assert!(self::parsed(&false_user).is_err());
        let import = json!({"input":{"focus":{"kind":"knowledge","scope":{"project":"01ARZ3NDEKTSV4RRFFQ69G5FAV"},"id":"literal page","context":"persisted namespace"},"contexts":[{"id":"library/reference","version":"exact-version","context_alias":"mount alias"}],"pages":[{"source":{"kind":"imported_knowledge","artifact":{"id":"library/reference","version":"exact-version","context_alias":"mount alias"}},"direction":"incoming"}]}});
        let arguments = self::parsed(&import).unwrap();
        assert!(arguments.selection.project_id.is_none());
        let query: Input = serde_json::from_value(arguments.input.unwrap()).unwrap();
        assert_eq!(query.focus.context.as_deref(), Some("persisted namespace"));
        assert_eq!(
            query.contexts[0].context_alias.as_deref(),
            Some("mount alias")
        );
    }
    #[test]
    fn duplicated_unknown_null_and_foreign_cursor_shapes_fail_before_source_io() {
        for field in [
            "pages",
            "outgoing_source",
            "page_size",
            "contexts",
            "include_user",
        ] {
            let mut invalid = body();
            invalid["input"][field] = Value::Null;
            assert!(parsed(&invalid).is_err());
        }
        for focus in [
            json!({"kind":"other"}),
            json!({"context":"unsupported"}),
            json!({"id":null}),
            json!({"scope":{"project":"00000000000000000000000000"}}),
        ] {
            let mut invalid = body();
            invalid["input"]["focus"]
                .as_object_mut()
                .unwrap()
                .extend(focus.as_object().unwrap().clone());
            assert!(parsed(&invalid).is_err());
        }
        let mut duplicate = body();
        duplicate["input"]["pages"] = json!([{"source":{"kind":"project_task"},"direction":"incoming"},{"source":{"kind":"project_task"},"direction":"incoming"}]);
        assert!(parsed(&duplicate).is_err());
        let mut cursor = body();
        cursor["input"]["pages"][0]["cursor"] = json!({"offset":0});
        assert!(parsed(&cursor).is_err());
        let selected = body()["selection"].clone();
        let raw = format!(
            r#"{{"selection":{selected},"input":{{"focus":{{"kind":"memory","scope":{{"project":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}},"id":"literal","\u0069d":"other"}},"pages":[{{"source":{{"kind":"project_task"}},"direction":"incoming"}}]}}}}"#
        );
        assert!(parse(raw.as_bytes(), limits(), "records_references").is_err());
    }
    #[test]
    fn actual_scalar_frame_page_and_native_path_bounds_preserve_exact_boundary() {
        let bytes = serde_json::to_vec(&body()).unwrap();
        let mut cap = limits();
        cap.max_input_bytes = bytes.len();
        assert!(parse(&bytes, cap, "records_references").is_ok());
        cap.max_input_bytes -= 1;
        assert!(parse(&bytes, cap, "records_references").is_err());
        for size in [json!(0), json!(101), json!(20.0), json!("20")] {
            let mut invalid = body();
            invalid["input"]["page_size"] = size;
            assert!(parsed(&invalid).is_err());
        }
        let mut scalar = body();
        scalar["input"]["focus"]["id"] = json!("é".repeat(13));
        cap = limits();
        cap.max_scalar_bytes = 26;
        assert!(
            parse(
                &serde_json::to_vec(&scalar).unwrap(),
                cap,
                "records_references"
            )
            .is_ok()
        );
        scalar["input"]["focus"]["id"] = json!("é".repeat(14));
        assert!(
            parse(
                &serde_json::to_vec(&scalar).unwrap(),
                cap,
                "records_references"
            )
            .is_err()
        );
        let mut input = body();
        input["selection"]["project_dir"] =
            json!(format!("/projetos espaço/café Ω/{}", "x".repeat(64)));
        let bytes = serde_json::to_vec(&input).unwrap();
        cap = limits();
        cap.max_scalar_bytes = input["selection"]["project_dir"].as_str().unwrap().len();
        assert!(parse(&bytes, cap, "records_references").is_ok());
        cap.max_scalar_bytes -= 1;
        assert!(parse(&bytes, cap, "records_references").is_err());
    }
}
