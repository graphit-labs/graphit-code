//! Logical-reference input adapter to the actual Native Knowledge reader.
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::{
    knowledge_reference_query::{
        self as data, KnowledgeReferenceQueryError, KnowledgeReferencesCursor,
        KnowledgeReferencesRead,
    },
    knowledge_references::{
        KnowledgeReferenceNamespace, KnowledgeReferenceScope, KnowledgeReferenceTargetKind,
    },
};
use graphit_records::reference_projection::{LogicalScope, ReferenceTargetKind};
use std::{num::NonZeroUsize, sync::Arc};

#[derive(Clone, Copy, Debug)]
pub struct KnowledgeLogicalTarget<'a> {
    pub target_kind: ReferenceTargetKind,
    pub scope: LogicalScope<'a>,
    pub id: &'a str,
    pub context: Option<&'a str>,
}
#[derive(Clone, Copy)]
pub struct KnowledgeReferencesQuery<'a> {
    pub outgoing: Option<&'a str>,
    pub incoming: Option<KnowledgeLogicalTarget<'a>>,
    pub page_size: NonZeroUsize,
    pub outgoing_cursor: Option<&'a KnowledgeReferencesCursor>,
    pub incoming_cursor: Option<&'a KnowledgeReferencesCursor>,
}
pub(crate) fn target(input: KnowledgeLogicalTarget<'_>) -> data::KnowledgeReferenceTarget<'_> {
    data::KnowledgeReferenceTarget {
        target_kind: match input.target_kind {
            ReferenceTargetKind::Task => KnowledgeReferenceTargetKind::Task,
            ReferenceTargetKind::Session => KnowledgeReferenceTargetKind::Session,
            ReferenceTargetKind::Memory => KnowledgeReferenceTargetKind::Memory,
            ReferenceTargetKind::Knowledge => KnowledgeReferenceTargetKind::Knowledge,
        },
        namespace: KnowledgeReferenceNamespace {
            scope: match input.scope {
                LogicalScope::Project(id) => KnowledgeReferenceScope::Project(id),
                LogicalScope::User(id) => KnowledgeReferenceScope::User(id),
            },
            context: input.context,
        },
        id: input.id,
    }
}
/// Caller inputs are ordinary logical values; only the supplied admission grants read.
pub async fn read_native_knowledge_references(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    query: KnowledgeReferencesQuery<'_>,
    limits: graphit_data::knowledge_index::KnowledgeIndexReadLimits,
) -> Result<KnowledgeReferencesRead, KnowledgeReferenceQueryError> {
    data::read_native_knowledge_reference_query(
        original,
        admission,
        data::KnowledgeReferenceQuery {
            outgoing: query.outgoing,
            incoming: query.incoming.map(target),
            page_size: query.page_size,
            outgoing_cursor: query.outgoing_cursor,
            incoming_cursor: query.incoming_cursor,
        },
        limits,
    )
    .await
}
