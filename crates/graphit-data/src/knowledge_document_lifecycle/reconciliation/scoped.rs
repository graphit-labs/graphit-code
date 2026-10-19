//! Source membership deltas scoped to genuine file origins.
use super::*;
use crate::knowledge_document_projection::{CompilationDigest32, PhysicalSourceDigest32};
use crate::lance_branch_store::BranchSnapshot;
use crate::lance_module_store::RecordBatchStreamAdapter;
use arrow_array::{Array, DurationNanosecondArray, TimestampNanosecondArray, UInt64Array};
use futures::TryStreamExt;

/// The canonical body is the complete original FILE, already compiled by the
/// caller. Validate its real captured bytes/binding without recapturing siblings.
pub(in crate::knowledge_document_lifecycle) fn validate_source_file(
    original: &SnapshotContext,
    source: &CapturedSourceReader,
    batch: &RecordBatch,
) -> Result<(), KnowledgeIndexError> {
    if batch.schema() != index::physical_schema() || batch.num_rows() != 1 {
        return Err(KnowledgeIndexError::InvalidField("source_file_batch"));
    }
    let paths = index::array::<StringArray>(batch, 7)?;
    let bodies = index::array::<StringArray>(batch, 2)?;
    if paths.null_count() != 0 || bodies.null_count() != 0 {
        return Err(KnowledgeIndexError::InvalidField("source_file_null"));
    }
    let relative = knowledge_source_relative_path(paths.value(0))?;
    source.validate_file_bytes(&relative, bodies.value(0).as_bytes(), original)?;
    Ok(())
}

/// Decode a canonical managed origin on this platform. The result is data;
/// membership, absence and authority still require a genuine captured reader.
pub fn knowledge_source_relative_path(
    origin: &str,
) -> Result<std::path::PathBuf, KnowledgeIndexError> {
    let parts = parse_managed_knowledge_source_path(origin)
        .map_err(|_| KnowledgeIndexError::InvalidField("source_file_origin"))?
        .ok_or(KnowledgeIndexError::InvalidField("source_file_origin"))?;
    let mut relative = std::path::PathBuf::new();
    let super::super::KnowledgeSourcePathComponents::Portable(parts) = parts;
    for part in parts {
        relative.push(part);
    }
    Ok(relative)
}

#[derive(Debug)]
pub struct KnowledgeSourceHead {
    pub slug: String,
    pub source_path: String,
    pub physical_digest: Option<PhysicalSourceDigest32>,
    pub compilation_digest: CompilationDigest32,
    pub recipe: String,
    pub recipe_parameters: index::KnowledgeRecipeParameters,
    pub updated_at: Option<std::time::SystemTime>,
    pub stale_since: Option<std::time::SystemTime>,
    pub stale_after: Option<std::time::Duration>,
}

#[derive(Debug)]
pub enum KnowledgeSourceOrigins {
    NoCorpus,
    Published {
        generation: KnowledgeGeneration,
        origins: HashSet<String>,
        heads: Vec<KnowledgeSourceHead>,
    },
}

/// Read only managed membership metadata; no document bodies are captured.
pub async fn read_native_knowledge_source_origins(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    limits: KnowledgeIndexReadLimits,
) -> Result<KnowledgeSourceOrigins, KnowledgeDocumentLifecycleError> {
    let project = index::check_read_project(original, read)?;
    let scope = project.scope.live;
    let corpus = index::local_corpus(scope, project.version.version(), original, limits.native)?;
    let Some(directory) = index::capture_index(scope, &corpus, original, limits.native, || {
        index::check_read(original, read)
    })?
    else {
        return Ok(KnowledgeSourceOrigins::NoCorpus);
    };
    let check = || -> Result<(), KnowledgeDocumentLifecycleError> {
        index::check_read(original, read)?;
        directory
            .revalidate(original)
            .map_err(KnowledgeIndexError::from)?;
        Ok(())
    };
    check()?;
    let uri = corpus.uri.clone();
    let branch = project.version.lance_branch();
    let pin = stream::guarded(
        BranchSnapshot::capture_observed(
            &uri,
            (!branch.is_empty()).then_some(branch.as_str()),
            None,
        ),
        original,
        &check,
    )
    .await?
    .map_err(KnowledgeIndexError::from)?;
    check()?;
    crate::knowledge_index::semantic::dimensions(pin.schema())?;
    let generation = index::generation(&uri, &pin);
    let mut scanner = pin.scan();
    scanner
        .project(&[
            "slug",
            "source_path",
            "document_state",
            "physical_digest",
            "recipe",
            "recipe_parameters",
            "updated_at",
            "stale_since",
            "stale_after",
            "compilation_digest",
        ])
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?
        .batch_size(limits.batch_rows.get() as usize);
    let mut stream = stream::guarded(scanner.try_into_stream(), original, &check)
        .await?
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?;
    let mut origins = HashSet::new();
    let mut heads = Vec::new();
    let mut slugs = HashSet::new();
    while let Some(batch) = stream::guarded(stream.try_next(), original, &check)
        .await?
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?
    {
        check()?;
        let names = index::array::<StringArray>(&batch, 0)?;
        let paths = index::array::<StringArray>(&batch, 1)?;
        let states = index::array::<UInt8Array>(&batch, 2)?;
        if names.null_count() != 0 || paths.null_count() != 0 || states.null_count() != 0 {
            return Err(KnowledgeIndexError::InvalidField("membership_null").into());
        }
        let physical = index::array::<FixedSizeBinaryArray>(&batch, 3)?;
        let recipes = index::array::<StringArray>(&batch, 4)?;
        let params = index::array::<StructArray>(&batch, 5)?;
        let fallback = params
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(KnowledgeIndexError::InvalidField("recipe_parameters"))?;
        let kind = params
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(KnowledgeIndexError::InvalidField("recipe_parameters"))?;
        let summary = params
            .column(2)
            .as_any()
            .downcast_ref::<UInt64Array>()
            .ok_or(KnowledgeIndexError::InvalidField("recipe_parameters"))?;
        let updated = index::array::<TimestampNanosecondArray>(&batch, 6)?;
        let stale = index::array::<TimestampNanosecondArray>(&batch, 7)?;
        let age = index::array::<DurationNanosecondArray>(&batch, 8)?;
        let compilation = index::array::<FixedSizeBinaryArray>(&batch, 9)?;
        if compilation.null_count() != 0 {
            return Err(KnowledgeIndexError::InvalidField("compilation_digest").into());
        }
        if recipes.null_count() != 0
            || params.null_count() != 0
            || params.columns().iter().any(|value| value.null_count() != 0)
        {
            return Err(KnowledgeIndexError::InvalidField("recipe_parameters_null").into());
        }
        for row in 0..batch.num_rows() {
            check()?;
            if !slugs.insert(names.value(row).to_owned()) {
                return Err(KnowledgeIndexError::DuplicateSlug.into());
            }
            let managed = parse_managed_knowledge_source_path(paths.value(row))
                .map_err(KnowledgeDocumentLifecycleError::Identity)?
                .is_some();
            match states.value(row) {
                value if value == KnowledgeDocumentState::Active as u8 => {
                    if managed {
                        if !origins.insert(paths.value(row).to_owned()) {
                            return Err(KnowledgeDocumentLifecycleError::InvalidInput(
                                "duplicate_origin",
                            ));
                        }
                        heads.push(KnowledgeSourceHead {
                            slug: names.value(row).to_owned(),
                            source_path: paths.value(row).to_owned(),
                            physical_digest: (!physical.is_null(row))
                                .then(|| {
                                    physical
                                        .value(row)
                                        .try_into()
                                        .map(PhysicalSourceDigest32::from_bytes)
                                        .map_err(|_| {
                                            KnowledgeIndexError::InvalidField("physical_digest")
                                        })
                                })
                                .transpose()?,
                            compilation_digest: CompilationDigest32::from_bytes(
                                compilation.value(row).try_into().map_err(|_| {
                                    KnowledgeIndexError::InvalidField("compilation_digest")
                                })?,
                            ),
                            recipe: recipes.value(row).to_owned(),
                            recipe_parameters: index::KnowledgeRecipeParameters {
                                fallback_title: fallback.value(row).to_owned(),
                                kind_on_absent: kind.value(row).to_owned(),
                                max_summary_bytes: usize::try_from(summary.value(row))
                                    .ok()
                                    .and_then(std::num::NonZeroUsize::new)
                                    .ok_or(KnowledgeIndexError::InvalidField(
                                        "max_summary_bytes",
                                    ))?,
                            },
                            updated_at: (!updated.is_null(row))
                                .then(|| index::time(updated.value(row)))
                                .transpose()?,
                            stale_since: (!stale.is_null(row))
                                .then(|| index::time(stale.value(row)))
                                .transpose()?,
                            stale_after: (!age.is_null(row))
                                .then(|| {
                                    u64::try_from(age.value(row))
                                        .map(std::time::Duration::from_nanos)
                                        .map_err(|_| {
                                            KnowledgeIndexError::InvalidField("stale_after")
                                        })
                                })
                                .transpose()?,
                        });
                    }
                }
                value if value == KnowledgeDocumentState::Removed as u8 => {}
                _ => return Err(KnowledgeIndexError::InvalidField("document_state").into()),
            }
        }
    }
    check()?;
    Ok(KnowledgeSourceOrigins::Published {
        generation,
        origins,
        heads,
    })
}

/// Replace the documents of one physical source file, preserving every other origin.
#[allow(clippy::too_many_arguments)]
pub async fn reconcile_native_knowledge_source_file(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    expected: &KnowledgeGeneration,
    source: &CapturedSourceReader,
    batch: RecordBatch,
    limits: KnowledgeDocumentLifecycleLimits,
) -> Result<KnowledgeDocumentUpdateResult, KnowledgeDocumentLifecycleError> {
    if batch.schema() != index::physical_schema() || batch.num_rows() == 0 {
        return Err(KnowledgeDocumentLifecycleError::InvalidInput(
            "source_file_batch",
        ));
    }
    let paths = index::array::<StringArray>(&batch, 7)?;
    let names = index::array::<StringArray>(&batch, 0)?;
    if paths.null_count() != 0 || names.null_count() != 0 {
        return Err(KnowledgeIndexError::InvalidField("membership_null").into());
    }
    let origin = paths.value(0);
    if paths.iter().any(|path| path != Some(origin))
        || parse_managed_knowledge_source_path(origin)
            .map_err(KnowledgeDocumentLifecycleError::Identity)?
            .is_none()
    {
        return Err(KnowledgeDocumentLifecycleError::InvalidInput(
            "source_file_origin",
        ));
    }
    let origins = HashSet::from([origin.to_owned()]);
    let slugs = names
        .iter()
        .map(|value| value.unwrap().to_owned())
        .collect();
    scoped(
        original, read, write, expected, source, batch, &origins, &slugs, limits,
    )
    .await
}

/// Remove only explicit missing source origins after the caller's selection completes.
#[allow(clippy::too_many_arguments)]
pub async fn remove_native_knowledge_source_origins(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    expected: &KnowledgeGeneration,
    source: &CapturedSourceReader,
    origins: &HashSet<String>,
    limits: KnowledgeDocumentLifecycleLimits,
) -> Result<KnowledgeDocumentUpdateResult, KnowledgeDocumentLifecycleError> {
    for origin in origins {
        if parse_managed_knowledge_source_path(origin)
            .map_err(KnowledgeDocumentLifecycleError::Identity)?
            .is_none()
        {
            return Err(KnowledgeDocumentLifecycleError::InvalidInput(
                "source_file_origin",
            ));
        }
    }
    scoped(
        original,
        read,
        write,
        expected,
        source,
        RecordBatch::new_empty(index::physical_schema()),
        origins,
        &HashSet::new(),
        limits,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn scoped(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    expected: &KnowledgeGeneration,
    source: &CapturedSourceReader,
    batch: RecordBatch,
    origins: &HashSet<String>,
    slugs: &HashSet<String>,
    limits: KnowledgeDocumentLifecycleLimits,
) -> Result<KnowledgeDocumentUpdateResult, KnowledgeDocumentLifecycleError> {
    let file = (batch.num_rows() != 0).then(|| batch.clone());
    let input = Box::pin(RecordBatchStreamAdapter::new(
        index::physical_schema(),
        futures::stream::iter([Ok(batch)]),
    ));
    stream::reconcile_stream(
        original,
        read,
        write,
        expected,
        source,
        input,
        (origins, slugs),
        file.as_ref(),
        limits,
    )
    .await
}
