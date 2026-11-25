//! Materialize an inherited native Knowledge branch from its observed SDK pin.

use crate::{
    knowledge_document_update::KnowledgeDocumentUpdateLimits,
    knowledge_index::{self as index, KnowledgeGeneration, KnowledgeIndexError},
    lance_module_store::StoreError,
};
use arrow_array::StringArray;
use futures::TryStreamExt;
use graphit_auth::{ProjectReadAdmission, ProjectWriteAdmission};
use graphit_core::{artifact_store::StoreOwner, file_io::SnapshotContext};
use std::{collections::BTreeSet, error::Error, fmt, sync::Arc};

#[derive(Debug)]
pub enum KnowledgeBranchMaterialization {
    Unchanged {
        generation: KnowledgeGeneration,
    },
    Materialized {
        generation: KnowledgeGeneration,
        inherited: KnowledgeGeneration,
    },
}
#[derive(Debug)]
pub enum KnowledgeBranchMaterializationError {
    Data(KnowledgeIndexError),
    ScopeMismatch,
    GenerationChanged {
        observed: KnowledgeGeneration,
    },
    NoCorpus,
    Materialized {
        generation: KnowledgeGeneration,
        cause: Box<Self>,
    },
}
impl fmt::Display for KnowledgeBranchMaterializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Knowledge branch materialization: {self:?}")
    }
}
impl Error for KnowledgeBranchMaterializationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Data(e) => Some(e),
            Self::Materialized { cause, .. } => Some(cause.as_ref()),
            _ => None,
        }
    }
}
impl From<KnowledgeIndexError> for KnowledgeBranchMaterializationError {
    fn from(e: KnowledgeIndexError) -> Self {
        Self::Data(e)
    }
}
fn check_authority(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
) -> Result<(), KnowledgeBranchMaterializationError> {
    index::check_read(original, read)?;
    index::check_write(original, write)?;
    let r = read.project().map_err(KnowledgeIndexError::from)?;
    let w = write.project().map_err(KnowledgeIndexError::from)?;
    if r.scope.owner() != w.scope.owner()
        || r.scope.runtime_root() != w.scope.runtime_root()
        || r.version.version() != w.version.version()
        || r.version.lance_branch() != w.version.lance_branch()
    {
        return Err(KnowledgeBranchMaterializationError::ScopeMismatch);
    }
    Ok(())
}

/// Check the expected generation and the entire canonical18 image before a no-op
/// or an SDK branch-reference creation. This clones the captured version, even
/// if a noncooperative writer later advances default. It is not a parent CAS.
/// Read.content limits the actual Full18 data projected for materialization.
/// Branch creation holds only its corpus writer lock; it does not read an
/// artifact catalogue or stage installation content, and never overwrites documents.
/// A confirmed SDK effect followed by failed checks returns Materialized error
/// with the actual generation. SDK bootstrap errors retain their own uncertainty.
pub async fn materialize_native_knowledge_branch(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    expected: &KnowledgeGeneration,
    limits: KnowledgeDocumentUpdateLimits,
) -> Result<KnowledgeBranchMaterialization, KnowledgeBranchMaterializationError> {
    materialize_inner(original, read, write, expected, limits, &|_| {}).await
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MaterializationPhase {
    BeforeWriter,
    WriterAcquired,
    Captured,
    BatchValidated,
    ImageValidated,
    BeforeMaterialize,
    Materialized,
    Delivery,
}
async fn materialize_inner(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    expected: &KnowledgeGeneration,
    limits: KnowledgeDocumentUpdateLimits,
    phase: &(dyn Fn(MaterializationPhase) + Sync),
) -> Result<KnowledgeBranchMaterialization, KnowledgeBranchMaterializationError> {
    check_authority(original, read, write)?;
    let project = write.project().map_err(KnowledgeIndexError::from)?;
    let scope = project.scope;
    let StoreOwner::Project { project_id } = scope.owner() else {
        return Err(KnowledgeIndexError::WrongAdmission.into());
    };
    let branch = project.version.lance_branch();
    let encoded_branch = (!branch.is_empty()).then_some(branch.as_str());
    let corpus = index::local_corpus(scope)?;
    let uri = corpus.uri.clone();
    let check = || -> Result<(), KnowledgeIndexError> {
        index::check_read(original, read)?;
        index::check_write(original, write)
    };
    phase(MaterializationPhase::BeforeWriter);
    check()?;
    let writer = index::acquire_local_writer_lock(original, scope).await?;
    phase(MaterializationPhase::WriterAcquired);
    check_authority(original, read, write)?;
    if !graphit_core::local_files::directory_exists(&corpus.path)
        .map_err(KnowledgeIndexError::from)?
    {
        check()?;
        return Err(KnowledgeBranchMaterializationError::NoCorpus);
    };
    let before = || -> Result<(), KnowledgeIndexError> {
        check()?;

        Ok(())
    };
    before()?;
    let pin =
        crate::lance_branch_store::BranchSnapshot::capture_observed(&uri, encoded_branch, None)
            .await
            .map_err(KnowledgeIndexError::from)?;
    index::semantic::dimensions(pin.schema())?;
    before()?;
    let inherited = index::generation(&uri, &pin);
    if &inherited != expected {
        return Err(KnowledgeBranchMaterializationError::GenerationChanged {
            observed: inherited,
        });
    }
    phase(MaterializationPhase::Captured);
    before()?;
    let mut scanner = pin.scan();
    scanner.batch_size(limits.read.batch_rows.get() as usize);
    let mut stream = scanner
        .try_into_stream()
        .await
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?;
    before()?;
    let mut counts = index::NamespaceValidation::bound(Some(*project_id));
    let mut slugs = BTreeSet::new();
    loop {
        before()?;
        let next = stream
            .try_next()
            .await
            .map_err(StoreError::from)
            .map_err(KnowledgeIndexError::from)?;
        before()?;
        let Some(batch) = next else { break };
        index::validate_full_batch(&batch, limits.read.content, &mut counts, before)?;
        let column: &StringArray = index::array(&batch, 0)?;
        for slug in column.iter() {
            before()?;
            let slug = slug.ok_or(KnowledgeIndexError::InvalidField("slug"))?;
            if !slugs.insert(slug.to_owned()) {
                return Err(KnowledgeIndexError::DuplicateSlug.into());
            }
        }
        phase(MaterializationPhase::BatchValidated);
        before()?;
    }
    drop(stream);
    drop(scanner);
    before()?;
    phase(MaterializationPhase::ImageValidated);
    before()?;
    if pin.inherited_target().is_none() {
        phase(MaterializationPhase::Delivery);
        before()?;
        drop(writer);
        return Ok(KnowledgeBranchMaterialization::Unchanged {
            generation: inherited,
        });
    }
    phase(MaterializationPhase::BeforeMaterialize);
    before()?;
    let pin = pin.materialize().await.map_err(KnowledgeIndexError::from)?;
    let generation = index::generation(&uri, &pin);
    phase(MaterializationPhase::Materialized);
    let confirmed_check = || -> Result<(), KnowledgeBranchMaterializationError> {
        before().map_err(|cause| KnowledgeBranchMaterializationError::Materialized {
            generation: generation.clone(),
            cause: Box::new(cause.into()),
        })
    };
    confirmed_check()?;
    phase(MaterializationPhase::Delivery);
    confirmed_check()?;
    drop(pin);
    drop(writer);
    Ok(KnowledgeBranchMaterialization::Materialized {
        generation,
        inherited,
    })
}

#[cfg(test)]
mod tests;
