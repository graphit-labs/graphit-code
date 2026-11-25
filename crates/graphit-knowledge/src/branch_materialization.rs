//! Explicit Knowledge branch materialization; document publication is a separate effect.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]
use graphit_auth::{ProjectReadAdmission, ProjectWriteAdmission};
use graphit_core::file_io::SnapshotContext;
use graphit_data::{
    knowledge_branch_materialization::{
        KnowledgeBranchMaterialization, KnowledgeBranchMaterializationError,
        materialize_native_knowledge_branch,
    },
    knowledge_document_update::KnowledgeDocumentUpdateLimits,
    knowledge_index::KnowledgeGeneration,
};
use std::sync::Arc;

/// Delegate to Data using both real Knowledge admissions and the expected
/// generation. After Materialized, use its actual generation for republication.
pub async fn materialize_native_document_branch(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    expected: &KnowledgeGeneration,
    limits: KnowledgeDocumentUpdateLimits,
) -> Result<KnowledgeBranchMaterialization, KnowledgeBranchMaterializationError> {
    materialize_native_knowledge_branch(original, read, write, expected, limits).await
}
