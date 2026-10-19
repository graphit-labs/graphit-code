//! Reads edges from the admitted source corpus; never resolves a foreign target.
use super::{State, ToolResult, catalog::Operation, error, parse};
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, VerifiedRoleGrant};
use graphit_data::knowledge_reference_query::{
    KnowledgeReferenceQueryError, KnowledgeReferencesCursor,
};
use graphit_hub::policy::RequestContext;
use graphit_knowledge::native_reference_query::{
    KnowledgeLogicalTarget, KnowledgeReferencesQuery, read_native_knowledge_references,
};
use graphit_records::{
    reference_projection::{LogicalScope, ReferenceTargetKind},
    task_store::{NativeTaskStore, TaskReferenceKind, TaskReferenceScope, TaskReferencesQuery},
};
use serde::Deserialize;
use serde_json::Value;
use std::num::NonZeroUsize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Incoming {
    target_kind: TaskReferenceKind,
    scope: TaskReferenceScope,
    id: String,
    context: Option<String>,
}
impl Incoming {
    fn borrow(&self) -> KnowledgeLogicalTarget<'_> {
        KnowledgeLogicalTarget {
            target_kind: match self.target_kind {
                TaskReferenceKind::Task => ReferenceTargetKind::Task,
                TaskReferenceKind::Session => ReferenceTargetKind::Session,
                TaskReferenceKind::Memory => ReferenceTargetKind::Memory,
                TaskReferenceKind::Knowledge => ReferenceTargetKind::Knowledge,
            },
            scope: match &self.scope {
                TaskReferenceScope::Project(id) => LogicalScope::Project(*id),
                TaskReferenceScope::User(id) => LogicalScope::User(id),
            },
            id: &self.id,
            context: self.context.as_deref(),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Knowledge {
    outgoing: Option<String>,
    incoming: Option<Incoming>,
    page_size: NonZeroUsize,
    outgoing_cursor: Option<KnowledgeReferencesCursor>,
    incoming_cursor: Option<KnowledgeReferencesCursor>,
}
pub(super) async fn read<'a>(
    state: &State,
    authority: &'a ProjectRecordAuthority<'_>,
    grant: &'a VerifiedRoleGrant<'a>,
    operation: Operation,
    input: Option<Value>,
    original: &'a RequestContext<'a>,
) -> ToolResult {
    let module = match operation {
        Operation::TaskReferences => ProjectReadModule::Task,
        Operation::KnowledgeReferences => ProjectReadModule::Knowledge,
        _ => return Err(error("unknown_tool")),
    };
    let admission = authority
        .admit_read(grant, original, module)
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let result = match operation {
        Operation::TaskReferences => {
            let query: TaskReferencesQuery = parse(input)?;
            let result = NativeTaskStore::references(
                original.snapshot(),
                &admission,
                query,
                state.task.read,
            )
            .await
            .map_err(crate::task_server::Failure::store);
            match result {
                Ok(value) => {
                    serde_json::to_value(value).map_err(|_| error("response_encoding_failed"))?
                }
                Err(cause) => {
                    return Err(serde_json::to_value(cause)
                        .map_err(|_| error("response_encoding_failed"))?);
                }
            }
        }
        Operation::KnowledgeReferences => {
            let query: Knowledge = parse(input)?;
            let result = read_native_knowledge_references(
                original.snapshot(),
                &admission,
                KnowledgeReferencesQuery {
                    outgoing: query.outgoing.as_deref(),
                    incoming: query.incoming.as_ref().map(Incoming::borrow),
                    page_size: query.page_size,
                    outgoing_cursor: query.outgoing_cursor.as_ref(),
                    incoming_cursor: query.incoming_cursor.as_ref(),
                },
                state.policy.knowledge,
            )
            .await
            .map_err(|cause| match cause {
                KnowledgeReferenceQueryError::Data(cause) => super::knowledge::failure(cause),
                KnowledgeReferenceQueryError::InvalidQuery(_) => error("invalid_arguments"),
                KnowledgeReferenceQueryError::InvalidCursor => error("invalid_cursor"),
                KnowledgeReferenceQueryError::UnknownSource => error("unknown_source"),
            })?;
            serde_json::to_value(result).map_err(|_| error("response_encoding_failed"))?
        }
        _ => unreachable!("operation checked before admission"),
    };
    admission
        .recheck()
        .map_err(|_| error("authentication_or_permission_denied"))?;
    original
        .check()
        .map_err(|_| error("request_cancelled_or_expired"))?;
    Ok(result)
}
