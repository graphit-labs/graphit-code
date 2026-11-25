use super::{State, ToolResult, catalog::Operation, error, parse};
use graphit_auth::{
    ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation, VerifiedRoleGrant,
};
use graphit_hub::policy::RequestContext;
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::*,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::num::NonZeroUsize;
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Class {
    Fact,
    Skill,
    Tension,
    Decision,
    Convention,
    Correction,
}
impl From<Class> for MemoryClass {
    fn from(v: Class) -> Self {
        match v {
            Class::Fact => Self::Fact,
            Class::Skill => Self::Skill,
            Class::Tension => Self::Tension,
            Class::Decision => Self::Decision,
            Class::Convention => Self::Convention,
            Class::Correction => Self::Correction,
        }
    }
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Filter {
    classification: Option<Class>,
    tag: Option<String>,
    mandatory: Option<bool>,
    important: Option<bool>,
    #[serde(default)]
    exclude_mandatory: bool,
}
impl From<Filter> for MemoryListFilter {
    fn from(v: Filter) -> Self {
        Self {
            classification: v.classification.map(Into::into),
            tag: v.tag,
            mandatory: v.mandatory,
            important: v.important,
            exclude_mandatory: v.exclude_mandatory,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Get {
    id: MemoryId,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    id: MemoryId,
    #[serde(default)]
    selectors: graphit_core::text_slice::Request,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    text: String,
    #[serde(default)]
    filter: Filter,
    #[serde(default)]
    include_deleted_history: bool,
    top_k: NonZeroUsize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Insert {
    idempotency_key: String,
    title: String,
    body: String,
    classification: Class,
    #[serde(default)]
    mandatory: bool,
    #[serde(default)]
    important: bool,
    #[serde(default)]
    tags: Vec<String>,
    references: Option<Vec<MemoryReference>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    id: MemoryId,
    expected_revision: u64,
    idempotency_key: String,
    reason: String,
    title: Option<String>,
    body: Option<String>,
    classification: Option<Class>,
    mandatory: Option<bool>,
    important: Option<bool>,
    tags: Option<Vec<String>>,
    references: Option<Vec<MemoryReference>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    id: MemoryId,
    expected_revision: u64,
    idempotency_key: String,
    reason: String,
}
fn generation(v: &MemoryGeneration) -> Value {
    json!({"sdk_version":v.sdk_version,"project_id":v.project_id,"project_version":v.project_version,"physical_branch":v.physical_branch,"inherited_target":v.inherited_target})
}
fn failure(v: MemoryStoreError) -> Value {
    match v {
        MemoryStoreError::Confirmed { version, cause } => {
            json!({"code":"write_confirmed","version":version,"cause":failure(*cause)})
        }
        MemoryStoreError::IndexConfirmed { publication, cause } => {
            json!({"code":"index_confirmed","version":publication.generation.sdk_version,"cause":failure(*cause)})
        }
        MemoryStoreError::EmbeddingConfirmed { publication, cause } => {
            json!({"code":"embedding_confirmed","version":publication.generation.sdk_version,"cause":failure(*cause)})
        }
        MemoryStoreError::Auth(_) => error("authentication_or_permission_denied"),
        MemoryStoreError::Request(_) => error("request_cancelled_or_expired"),
        MemoryStoreError::DifferentRequest => error("different_original_request"),
        MemoryStoreError::WrongAdmission => error("wrong_admission"),
        MemoryStoreError::NotFound => error("memory_not_found"),
        MemoryStoreError::NoChange => error("no_change"),
        MemoryStoreError::RevisionConflict { expected, actual } => {
            json!({"code":"revision_conflict","expected":expected,"actual":actual})
        }
        MemoryStoreError::IdempotencyConflict => error("idempotency_conflict"),
        MemoryStoreError::LexicalIndexUnavailable(_) => error("lexical_index_unavailable"),
        MemoryStoreError::IncompleteCorpus => error("incomplete_corpus"),
        MemoryStoreError::Invalid(_) => error("invalid_native_input"),
        MemoryStoreError::Limit(_) => error("input_or_result_limit"),
        _ => error("native_store_failure"),
    }
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(super) fn request_published_rescan(
    result: &Result<graphit_records::memory_store::Memory, MemoryStoreError>,
    handle: Option<&graphit_daemon::supervisor::ProjectRescanHandle>,
) {
    if matches!(result, Ok(_) | Err(MemoryStoreError::Confirmed { .. }))
        && let Some(handle) = handle
    {
        handle.request_records();
    }
}

pub(super) async fn execute<'a>(
    state: &State,
    authority: &'a ProjectRecordAuthority<'_>,
    grant: &'a VerifiedRoleGrant<'a>,
    operation: Operation,
    input: Option<Value>,
    original: &'a RequestContext<'a>,
    rescan: Option<&graphit_daemon::supervisor::ProjectRescanHandle>,
) -> ToolResult {
    if operation.write() {
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            let _ = (state, authority, grant, input, original, rescan);
            return Err(error("native_write_unavailable"));
        }
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        {
            let admission = authority
                .admit_write(grant, original, ProjectWriteOperation::Memory)
                .map_err(|_| error("authentication_or_permission_denied"))?;
            let result = match operation {
                Operation::MemoryInsert => {
                    let v: Insert = parse(input)?;
                    NativeMemoryStore::create(
                        original.snapshot(),
                        &admission,
                        CreateMemory {
                            idempotency_key: v.idempotency_key,
                            title: v.title,
                            body: v.body,
                            attributes: MemoryAttributes {
                                classification: v.classification.into(),
                                mandatory: v.mandatory,
                                important: v.important,
                            },
                            tags: v.tags,
                            references: v.references,
                        },
                        state.policy.memory_write,
                    )
                    .await
                }
                Operation::MemoryUpdate => {
                    let v: Update = parse(input)?;
                    NativeMemoryStore::update(
                        original.snapshot(),
                        &admission,
                        v.id,
                        UpdateMemory {
                            expected_revision: v.expected_revision,
                            idempotency_key: v.idempotency_key,
                            reason: v.reason,
                            title: v.title,
                            body: v.body,
                            classification: v.classification.map(Into::into),
                            mandatory: v.mandatory,
                            important: v.important,
                            tags: v.tags,
                            references: v.references,
                        },
                        state.policy.memory_write,
                    )
                    .await
                }
                Operation::MemoryDelete => {
                    let v: Change = parse(input)?;
                    NativeMemoryStore::delete(
                        original.snapshot(),
                        &admission,
                        v.id,
                        DeleteMemory {
                            expected_revision: v.expected_revision,
                            idempotency_key: v.idempotency_key,
                            reason: v.reason,
                        },
                        state.policy.memory_write,
                    )
                    .await
                }
                Operation::MemoryMarkMandatory | Operation::MemoryUnmarkMandatory => {
                    let v: Change = parse(input)?;
                    NativeMemoryStore::update(
                        original.snapshot(),
                        &admission,
                        v.id,
                        UpdateMemory {
                            expected_revision: v.expected_revision,
                            idempotency_key: v.idempotency_key,
                            reason: v.reason,
                            title: None,
                            body: None,
                            classification: None,
                            mandatory: Some(operation == Operation::MemoryMarkMandatory),
                            important: None,
                            tags: None,
                            references: None,
                        },
                        state.policy.memory_write,
                    )
                    .await
                }
                Operation::MemoryPromote | Operation::MemoryDemote => {
                    let v: Change = parse(input)?;
                    NativeMemoryStore::update(
                        original.snapshot(),
                        &admission,
                        v.id,
                        UpdateMemory {
                            expected_revision: v.expected_revision,
                            idempotency_key: v.idempotency_key,
                            reason: v.reason,
                            title: None,
                            body: None,
                            classification: None,
                            mandatory: None,
                            important: Some(operation == Operation::MemoryPromote),
                            tags: None,
                            references: None,
                        },
                        state.policy.memory_write,
                    )
                    .await
                }
                _ => return Err(error("unknown_tool")),
            };
            request_published_rescan(&result, rescan);
            let result = result.map_err(failure)?;
            return Ok(json!({"kind":"memory","result":result}));
        }
    }
    let admission = authority
        .admit_read(grant, original, ProjectReadModule::Memory)
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let value = match operation {
        Operation::MemoryGet => {
            let v: Get = parse(input)?;
            match NativeMemoryStore::get(
                original.snapshot(),
                &admission,
                v.id,
                state.policy.memory_read,
            )
            .await
            .map_err(failure)?
            {
                None => json!({"kind":"absent"}),
                Some(v) => {
                    json!({"kind":"found","memory":v.memory,"generation":generation(&v.generation)})
                }
            }
        }
        Operation::MemorySource => {
            let v: Source = parse(input)?;
            match NativeMemoryStore::get(
                original.snapshot(),
                &admission,
                v.id,
                state.policy.memory_read,
            )
            .await
            .map_err(failure)?
            {
                None => json!({"kind":"absent"}),
                Some(m) => {
                    let selection = graphit_core::text_slice::apply(&m.memory.body, &v.selectors)
                        .map_err(|_| error("invalid_selection"))?;
                    json!({"kind":"found","id":m.memory.id,"revision":m.memory.revision,"generation":generation(&m.generation),"selection":selection})
                }
            }
        }
        Operation::MemoryHistory => {
            let v: Get = parse(input)?;
            match NativeMemoryStore::history(
                original.snapshot(),
                &admission,
                v.id,
                state.policy.memory_read,
            )
            .await
            .map_err(failure)?
            {
                None => json!({"kind":"absent"}),
                Some(v) => {
                    json!({"kind":"found","generation":generation(&v.generation),"current":v.current,"deleted":v.deleted,"creation":{"project_id":v.creation.project_id,"operation_version":v.creation.operation_version,"actor":v.creation.actor,"memory":v.creation.memory},"revisions":v.revisions.iter().map(|r|json!({"project_id":r.project_id,"operation_version":r.operation_version,"actor":r.actor,"before":r.before,"after":r.after})).collect::<Vec<_>>()})
                }
            }
        }
        Operation::MemorySearch => {
            let v: Search = parse(input)?;
            match NativeMemoryStore::search_lexical(
                original.snapshot(),
                &admission,
                MemoryLexicalQuery {
                    text: v.text,
                    filter: v.filter.into(),
                    include_deleted_history: v.include_deleted_history,
                    top_k: v.top_k,
                },
                state.policy.memory_read,
            )
            .await
            .map_err(failure)?
            {
                MemoryLexicalSearchRead::NoCorpus => json!({"kind":"no_corpus"}),
                MemoryLexicalSearchRead::PublishedEmpty { generation: g } => {
                    json!({"kind":"published_empty","generation":generation(&g)})
                }
                MemoryLexicalSearchRead::Published {
                    generation: g,
                    total_hits,
                    hits,
                } => {
                    json!({"kind":"published","generation":generation(&g),"total_hits":total_hits,"hits":hits.into_iter().map(|h|json!({"current":h.current,"matched":h.matched,"location":match h.location{MemoryMatchLocation::Current=>json!({"kind":"current"}),MemoryMatchLocation::Archived{revision}=>json!({"kind":"archived","revision":revision})},"deleted":h.deleted,"score":h.score})).collect::<Vec<_>>()})
                }
            }
        }
        Operation::MemoryList | Operation::MemoryMandatory | Operation::MemoryImportant => {
            let mut filter: MemoryListFilter = parse::<Filter>(input)?.into();
            if operation == Operation::MemoryMandatory {
                filter.mandatory = Some(true);
            }
            if operation == Operation::MemoryImportant {
                filter.important = Some(true);
            }
            match NativeMemoryStore::list(
                original.snapshot(),
                &admission,
                filter,
                state.policy.memory_read,
            )
            .await
            .map_err(failure)?
            {
                MemoryListRead::NoCorpus => json!({"kind":"no_corpus"}),
                MemoryListRead::Published {
                    generation: g,
                    memories,
                } => json!({"kind":"published","generation":generation(&g),"memories":memories}),
            }
        }
        _ => return Err(error("unknown_tool")),
    };
    admission
        .recheck()
        .map_err(|_| error("authentication_or_permission_denied"))?;
    Ok(value)
}
pub(super) fn filter_schema() -> Value {
    json!({"type":"object","properties":{"classification":{"enum":["fact","skill","tension","decision","convention","correction",null]},"tag":{"type":["string","null"]},"mandatory":{"type":["boolean","null"]},"important":{"type":["boolean","null"]},"exclude_mandatory":{"type":"boolean"}},"additionalProperties":false})
}
pub(super) fn write_schema(op: Operation) -> Value {
    let mut properties = serde_json::Map::from_iter([
        ("idempotency_key".into(), json!({"type":"string"})),
        ("reason".into(), json!({"type":"string"})),
        ("id".into(), json!({"type":"string"})),
        (
            "expected_revision".into(),
            json!({"type":"integer","minimum":1}),
        ),
    ]);
    let mut required = vec!["id", "expected_revision", "idempotency_key", "reason"];
    if op == Operation::MemoryInsert || op == Operation::MemoryUpdate {
        for name in ["title", "body"] {
            properties.insert(name.into(), json!({"type":"string"}));
        }
        properties.insert(
            "classification".into(),
            json!({"enum":["fact","skill","tension","decision","convention","correction"]}),
        );
        for name in ["mandatory", "important"] {
            properties.insert(name.into(), json!({"type":"boolean"}));
        }
        properties.insert(
            "tags".into(),
            json!({"type":"array","items":{"type":"string"}}),
        );
        properties.insert(
            "references".into(),
            json!({"type":["array","null"],"items":{"type":"object"}}),
        );
    }
    if op == Operation::MemoryInsert {
        properties.remove("id");
        properties.remove("reason");
        properties.remove("expected_revision");
        required = vec!["idempotency_key", "title", "body", "classification"];
    }
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
