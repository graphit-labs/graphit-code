//! Publish complete source files from a metadata capture without retaining a corpus.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]
use crate::{
    desired_corpus::{
        CapturedKnowledgeSources, DesiredCorpusError, KnowledgeSourceSelection, authored_id, bound,
        copy_text,
    },
    desired_reconciliation::{
        KnowledgeDesiredReconciliationError as E, KnowledgeDesiredReconciliationLimits,
        KnowledgeDesiredReconciliationResult, auth_error, check, convert_authored_references,
    },
    document::{DocumentParseError, envelope, parse_document},
    document_authored_references::project_document_authored_references,
    native_document_mapping::{
        AuthoredDocumentInput, prepare_native_authored_mapping_with_targets,
        relative_markdown_target,
    },
    publication_metadata::{PublicationMetadataPolicy, compile_captured_file_bytes},
};
use arrow_array::{
    Array, ArrayRef, DurationNanosecondArray, FixedSizeBinaryArray, RecordBatch, StringArray,
    TimestampNanosecondArray,
};
use futures::stream;
use graphit_auth::{ProjectReadAdmission, ProjectWriteAdmission};
use graphit_core::{
    artifact_store::StoreOwner, file_io::SnapshotContext, project_identity::ProjectId,
};
use graphit_data::{
    knowledge_document_lifecycle::{
        KnowledgeSourceIdentity, publish_native_knowledge_source_stream,
    },
    knowledge_document_projection as projection,
    knowledge_document_update::{
        KnowledgeDocumentAuthoredUpdate, KnowledgeDocumentReferenceUpdate,
    },
    knowledge_first_publication::{encode_knowledge_source_batch, native_knowledge_source_schema},
    knowledge_index::{KnowledgeDocumentDates, KnowledgeGeneration},
    lance_module_store::RecordBatchStreamAdapter,
};
use graphit_fs::NodeKind;
use graphit_records::reference_projection::{LogicalScope, SourceNamespace};
use pulldown_cmark::{Event, Tag};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

type Result<T> = std::result::Result<T, E>;

// One compact, request-local identity lookup. Native captures belong only to
// the FILE being published, never to this passage-wide metadata.
struct TargetIdentity {
    slug: String,
    digest: [u8; 32],
}

fn generation(result: &KnowledgeDesiredReconciliationResult) -> &KnowledgeGeneration {
    match result {
        KnowledgeDesiredReconciliationResult::First { publication, .. } => {
            publication.named.as_ref().unwrap_or(&publication.default)
        }
        KnowledgeDesiredReconciliationResult::Existing(
            graphit_data::knowledge_document_update::KnowledgeDocumentUpdateResult::Unchanged {
                generation,
            }
            | graphit_data::knowledge_document_update::KnowledgeDocumentUpdateResult::Updated {
                generation,
                ..
            },
        ) => generation,
    }
}

/// Publish complete FILE batches as soon as their local preparation succeeds.
/// A later failed FILE preserves the last actual receipt instead of rolling
/// back already queryable documents or turning a failed tail into healthy EOF.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn publish_source_changes(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    selection: &KnowledgeSourceSelection,
    kind_on_absent: &str,
    changes: Option<&BTreeSet<PathBuf>>,
    on_file: &(dyn Fn(&str) + Sync),
    before_file: &(dyn Fn() -> std::io::Result<()> + Sync),
    limits: KnowledgeDesiredReconciliationLimits,
) -> Result<KnowledgeDesiredReconciliationResult> {
    use graphit_data::{
        knowledge_branch_materialization::KnowledgeBranchMaterialization,
        knowledge_document_lifecycle::{
            KnowledgeSourceOrigins, publish_native_knowledge_source_file,
            read_native_knowledge_source_origins, reconcile_native_knowledge_source_file,
            remove_native_knowledge_source_origins,
        },
        knowledge_document_update::{KnowledgeDocumentUpdateLimits, KnowledgeDocumentUpdateResult},
    };
    let before = || check(original, read, write);
    before()?;
    let (mut published, mut old_origins, heads) =
        match read_native_knowledge_source_origins(original, read, limits.index.read)
            .await
            .map_err(E::DataLifecycle)?
        {
            KnowledgeSourceOrigins::NoCorpus => (None, HashSet::new(), Vec::new()),
            KnowledgeSourceOrigins::Published {
                mut generation,
                origins,
                heads,
            } => {
                if generation.inherited_target.is_some() {
                    generation =
                        match crate::branch_materialization::materialize_native_document_branch(
                            original,
                            read,
                            write,
                            &generation,
                            KnowledgeDocumentUpdateLimits {
                                read: limits.index.read,
                                write: limits.index.write,
                            },
                        )
                        .await
                        .map_err(|cause| E::Request(std::io::Error::other(cause)))?
                        {
                            KnowledgeBranchMaterialization::Unchanged { generation }
                            | KnowledgeBranchMaterialization::Materialized { generation, .. } => {
                                generation
                            }
                        };
                }
                (
                    Some(KnowledgeDesiredReconciliationResult::Existing(
                        KnowledgeDocumentUpdateResult::Unchanged { generation },
                    )),
                    origins,
                    heads,
                )
            }
        };
    let dates: BTreeMap<_, _> = heads
        .iter()
        .map(|head| {
            (
                head.slug.clone(),
                KnowledgeDocumentDates {
                    slug: head.slug.clone(),
                    updated_at: head.updated_at,
                    stale_since: head.stale_since,
                    stale_after: head.stale_after,
                },
            )
        })
        .collect();
    let mut dates = Arc::new(dates);
    let target_selection = Arc::new(selection.clone());
    let known_targets = Arc::new(
        heads
            .iter()
            .map(|head| {
                (
                    head.source_path.clone(),
                    (
                        head.physical_digest.as_ref().map(|d| *d.as_bytes()),
                        head.slug.clone(),
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>(),
    );
    let mut target_lookup = BTreeMap::new();
    let mut selected = HashSet::new();
    let mut slugs = HashMap::new();
    let mut origins = HashMap::new();
    let mut ordinal = 0;
    let requested = |relative: &Path| {
        changes.is_none_or(|paths| {
            paths
                .iter()
                .any(|path| relative == path || relative.starts_with(path))
        })
    };
    let mut pending_paths: BTreeSet<_> = selection.paths.iter().cloned().collect();
    let mut missing_paths = BTreeSet::new();
    for origin in &old_origins {
        let relative =
            graphit_data::knowledge_document_lifecycle::knowledge_source_relative_path(origin)
                .map_err(E::DataImage)?;
        if requested(&relative) && !pending_paths.contains(&relative) {
            missing_paths.insert(relative);
        }
    }
    pending_paths.extend(missing_paths.iter().cloned());
    let result: Result<()> = async {
        for relative in &selection.paths {
            before()?;
            before_file().map_err(E::Request)?;
            // Include only absent origins alongside this FILE so an authorized
            // rename can retain its slug, without hashing unrelated FILEs.
            let mut capture = missing_paths.clone();
            capture.insert(relative.clone());
            let sources = selection.capture_worker(read, &capture).await?;
            let source = sources.checked_reader(original, read)?;
            let node = source
                .nodes()
                .find(|node| node.kind() == NodeKind::File && node.relative() == relative)
                .ok_or(DesiredCorpusError::MissingRetainedFile)?;
            let identity = KnowledgeSourceIdentity::from_relative_path(
                relative,
                None,
                limits.preparation.mapping.max_field_bytes,
            )
            .map_err(DesiredCorpusError::Identity)?;
            selected.insert(identity.source_path().to_owned());
            // An unchanged source digest does not certify the identities of
            // linked FILEs. Prepare its projection; Data still detects true
            // no-ops and retains compatible semantic fields and vectors.
            let project = read
                .project()
                .map_err(|cause| auth_error(original, cause))?;
            let StoreOwner::Project { project_id } = project.scope.live.owner() else {
                return Err(E::WrongAdmission);
            };
            let project_id = *project_id;
            let request = Arc::clone(original);
            let captured = Arc::clone(&sources);
            let relative = node.relative().to_owned();
            let kind = kind_on_absent.to_owned();
            let temporal = Arc::clone(&dates);
            let targets = Arc::clone(&target_selection);
            let known = Arc::clone(&known_targets);
            let work = tokio::task::spawn_blocking({
                let dates = temporal;
                move || {
                    let mut slugs = slugs;
                    let mut origins = origins;
                    let mut target_lookup = target_lookup;
                    let batch = prepare_file(
                        &request,
                        project_id,
                        &captured,
                        &targets,
                        &known,
                        &mut target_lookup,
                        &relative,
                        &kind,
                        &dates,
                        limits,
                        ordinal,
                        &mut slugs,
                        &mut origins,
                    )?;
                    Ok::<_, E>((batch, slugs, origins, target_lookup))
                }
            });
            let ((batch, dependencies), next_slugs, next_origins, next_targets) =
                work.await.map_err(E::FileWorker)??;
            slugs = next_slugs;
            origins = next_origins;
            target_lookup = next_targets;
            let slug = batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| {
                    E::DataImage(
                        graphit_data::knowledge_index::KnowledgeIndexError::InvalidField("slug"),
                    )
                })?
                .value(0)
                .to_owned();
            before()?;
            check_target_bindings(original, read, dependencies.as_ref())?;
            let next = match published.as_ref() {
                Some(current) => KnowledgeDesiredReconciliationResult::Existing(
                    reconcile_native_knowledge_source_file(
                        original,
                        read,
                        write,
                        generation(current),
                        source,
                        batch,
                        limits.index,
                    )
                    .await
                    .map_err(E::DataLifecycle)?,
                ),
                None => {
                    match publish_native_knowledge_source_file(
                        original,
                        read,
                        write,
                        source,
                        batch.clone(),
                        limits.index.write,
                    )
                    .await
                    {
                        Ok((publication, recovery_key)) => {
                            KnowledgeDesiredReconciliationResult::First {
                                publication,
                                recovery_key,
                            }
                        }
                        Err(cause) if first_target_changed(&cause) => {
                            // Another authorized writer won Create while this FILE
                            // was prepared. Reopen the current branch and use its
                            // ordinary delta/CAS path, never replay an exclusive Create.
                            let (current, inherited_origins, current_dates) =
                                reopen_first_target(original, read, write, Some(&batch), limits)
                                    .await?;
                            Arc::make_mut(&mut dates).extend(
                                current_dates.into_iter().map(|date| (date.slug.clone(), date)),
                            );
                            let batch = with_source_dates(batch, &dates)?;
                            for origin in inherited_origins {
                                let relative = graphit_data::knowledge_document_lifecycle::knowledge_source_relative_path(&origin)
                                    .map_err(E::DataImage)?;
                                if requested(&relative) && !pending_paths.contains(&relative) {
                                    missing_paths.insert(relative.clone());
                                    pending_paths.insert(relative);
                                }
                                old_origins.insert(origin);
                            }
                            KnowledgeDesiredReconciliationResult::Existing(
                                reconcile_native_knowledge_source_file(
                                    original,
                                    read,
                                    write,
                                    &current,
                                    source,
                                    batch,
                                    limits.index,
                                )
                                .await
                                .map_err(E::DataLifecycle)?,
                            )
                        }
                        Err(cause) => return Err(E::DataFirst(cause)),
                    }
                }
            };
            if published.is_none()
                || !matches!(
                    &next,
                    KnowledgeDesiredReconciliationResult::Existing(
                        KnowledgeDocumentUpdateResult::Unchanged { .. }
                    )
                )
            {
                published = Some(next);
            }
            check_target_bindings(original, read, dependencies.as_ref())?;
            pending_paths.remove(node.relative());
            on_file(&slug);
            ordinal += 1;
        }
        before_file().map_err(E::Request)?;
        selection.revalidate_worker().await?;
        let sources = selection.capture_worker(read, &missing_paths).await?;
        let source = sources.checked_reader(original, read)?;
        if source.nodes().any(|node| node.kind() == NodeKind::File) {
            return Err(E::Request(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Knowledge source appeared before removal",
            )));
        }
        before()?;
        let mut removed = HashSet::new();
        for origin in old_origins.difference(&selected) {
            let relative =
                graphit_data::knowledge_document_lifecycle::knowledge_source_relative_path(origin)
                    .map_err(E::DataImage)?;
            if source.covers_selected_path(&relative).map_err(|cause| {
                DesiredCorpusError::Source {
                    operation: "selection_coverage",
                    relative: Some(relative),
                    cause,
                }
            })? {
                removed.insert(origin.clone());
            }
        }
        if let Some(current) = published.as_ref() {
            if !removed.is_empty() {
                let removed = remove_native_knowledge_source_origins(
                    original,
                    read,
                    write,
                    generation(current),
                    source,
                    &removed,
                    limits.index,
                )
                .await
                .map_err(E::DataLifecycle)?;
                if !matches!(&removed, KnowledgeDocumentUpdateResult::Unchanged { .. }) {
                    published = Some(KnowledgeDesiredReconciliationResult::Existing(removed));
                }
            }
        } else {
            let batch = RecordBatch::new_empty(native_knowledge_source_schema());
            let input = Box::pin(RecordBatchStreamAdapter::new(
                batch.schema(),
                stream::iter([Ok(batch)]),
            ));
            published = Some(
                match publish_native_knowledge_source_stream(
                    original,
                    read,
                    write,
                    source,
                    input,
                    limits.index.write,
                )
                .await
                {
                    Ok((publication, recovery_key)) => {
                        KnowledgeDesiredReconciliationResult::First {
                            publication,
                            recovery_key,
                        }
                    }
                    Err(cause) if first_target_changed(&cause) => {
                        let (generation, _, _) =
                            reopen_first_target(original, read, write, None, limits).await?;
                        KnowledgeDesiredReconciliationResult::Existing(
                            KnowledgeDocumentUpdateResult::Unchanged { generation },
                        )
                    }
                    Err(cause) => return Err(E::DataFirst(cause)),
                },
            );
        }
        pending_paths.clear();
        Ok(())
    }
    .await;
    match (result, published) {
        (Ok(()), Some(publication)) => Ok(publication),
        (Err(cause), Some(publication)) => Err(E::Published {
            publication: Box::new(publication),
            pending_paths,
            cause: Box::new(cause),
        }),
        (Err(cause), None) => Err(cause),
        (Ok(()), None) => unreachable!("successful source publication returns its receipt"),
    }
}

fn first_target_changed(
    cause: &graphit_data::knowledge_document_lifecycle::KnowledgeSourceFirstPublicationError,
) -> bool {
    use graphit_data::knowledge_document_lifecycle::KnowledgeSourcePublicationEffect;
    cause.effect() == KnowledgeSourcePublicationEffect::BeforeCommit
        && matches!(
            cause.cause(),
            graphit_data::knowledge_index::KnowledgeIndexError::InitialTargetChanged
        )
}

async fn reopen_first_target(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    file: Option<&RecordBatch>,
    limits: KnowledgeDesiredReconciliationLimits,
) -> Result<(
    KnowledgeGeneration,
    HashSet<String>,
    Vec<KnowledgeDocumentDates>,
)> {
    use graphit_data::{
        knowledge_branch_materialization::KnowledgeBranchMaterialization,
        knowledge_document_lifecycle::{
            KnowledgeSourceOrigins, read_native_knowledge_source_origins,
        },
        knowledge_document_update::KnowledgeDocumentUpdateLimits,
        knowledge_index::KnowledgeIndexError,
    };
    check(original, read, write)?;
    let KnowledgeSourceOrigins::Published {
        mut generation,
        origins,
        heads,
    } = read_native_knowledge_source_origins(original, read, limits.index.read)
        .await
        .map_err(E::DataLifecycle)?
    else {
        return Err(E::DataImage(KnowledgeIndexError::InitialTargetChanged));
    };
    check(original, read, write)?;
    if let Some(file) = file {
        // An inherited branch can legitimately differ from this checkout. An
        // already materialized target with different bytes is a concurrent FILE
        // update, not permission to overwrite it using newly adopted expectations.
        if generation.inherited_target.is_none() {
            let paths = file
                .column(7)
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or(E::DataImage(KnowledgeIndexError::InvalidField(
                    "source_path",
                )))?;
            let digest = file
                .column(9)
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .ok_or(E::DataImage(KnowledgeIndexError::InvalidField(
                    "compilation_digest",
                )))?;
            if heads.iter().any(|head| {
                head.source_path == paths.value(0)
                    && (digest.is_null(0)
                        || head.compilation_digest.as_bytes().as_slice() != digest.value(0))
            }) {
                return Err(E::DataImage(KnowledgeIndexError::InitialTargetChanged));
            }
        }
    } else if !origins.is_empty() {
        // A stale observation of no corpus cannot erase or certify empty over
        // documents published by a competitor after the Source capture.
        return Err(E::DataImage(KnowledgeIndexError::InitialTargetChanged));
    }
    let inherited_origins = if generation.inherited_target.is_some() {
        origins
    } else {
        HashSet::new()
    };
    if generation.inherited_target.is_some() {
        generation = match crate::branch_materialization::materialize_native_document_branch(
            original,
            read,
            write,
            &generation,
            KnowledgeDocumentUpdateLimits {
                read: limits.index.read,
                write: limits.index.write,
            },
        )
        .await
        .map_err(|cause| E::Request(std::io::Error::other(cause)))?
        {
            KnowledgeBranchMaterialization::Unchanged { generation }
            | KnowledgeBranchMaterialization::Materialized { generation, .. } => generation,
        };
    }
    check(original, read, write)?;
    let dates = heads
        .into_iter()
        .map(|head| KnowledgeDocumentDates {
            slug: head.slug,
            updated_at: head.updated_at,
            stale_since: head.stale_since,
            stale_after: head.stale_after,
        })
        .collect();
    Ok((generation, inherited_origins, dates))
}

fn with_source_dates(
    batch: RecordBatch,
    dates: &BTreeMap<String, KnowledgeDocumentDates>,
) -> Result<RecordBatch> {
    use graphit_data::knowledge_index::KnowledgeIndexError;
    let invalid = |field| E::DataImage(KnowledgeIndexError::InvalidField(field));
    let slugs = batch
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| invalid("slug"))?;
    let rows: Vec<_> = slugs
        .iter()
        .map(|slug| slug.and_then(|slug| dates.get(slug)))
        .collect();
    let stamps =
        |select: fn(&KnowledgeDocumentDates) -> Option<std::time::SystemTime>| -> Result<ArrayRef> {
            let values: Result<Vec<_>> = rows
                .iter()
                .map(|row| {
                    row.and_then(&select)
                        .map(|time| {
                            chrono::DateTime::<chrono::Utc>::from(time)
                                .timestamp_nanos_opt()
                                .ok_or_else(|| invalid("timestamp_range"))
                        })
                        .transpose()
                })
                .collect();
            Ok(Arc::new(
                TimestampNanosecondArray::from(values?).with_timezone("UTC"),
            ))
        };
    let ages: Result<Vec<_>> = rows
        .iter()
        .map(|row| {
            row.and_then(|date| date.stale_after)
                .map(|age| i64::try_from(age.as_nanos()).map_err(|_| invalid("stale_after")))
                .transpose()
        })
        .collect();
    // Arrow arrays share the prepared FILE's bodies and projection. Only the
    // temporal columns adopt metadata from the reopened, CAS-checked head.
    let mut columns = batch.columns().to_vec();
    columns[12] = stamps(|date| date.updated_at)?;
    columns[13] = stamps(|date| date.stale_since)?;
    columns[14] = Arc::new(DurationNanosecondArray::from(ages?));
    RecordBatch::try_new(batch.schema(), columns).map_err(|cause| E::DataImage(cause.into()))
}

#[allow(clippy::too_many_arguments)]
fn prepare_file(
    original: &Arc<SnapshotContext>,
    project_id: ProjectId,
    sources: &CapturedKnowledgeSources,
    selection: &KnowledgeSourceSelection,
    known_targets: &BTreeMap<String, (Option<[u8; 32]>, String)>,
    target_lookup: &mut BTreeMap<PathBuf, TargetIdentity>,
    relative: &Path,
    kind_on_absent: &str,
    dates: &BTreeMap<String, KnowledgeDocumentDates>,
    limits: KnowledgeDesiredReconciliationLimits,
    ordinal: usize,
    slugs: &mut HashMap<String, usize>,
    origins: &mut HashMap<String, usize>,
) -> Result<(RecordBatch, Option<CapturedKnowledgeSources>)> {
    let before = || sources.worker_reader(original).map(|_| ()).map_err(E::from);
    before()?;
    let source = sources.worker_reader(original).map_err(E::from)?;
    let file = source
        .open_file(relative)
        .map_err(|cause| DesiredCorpusError::Source {
            operation: "open_file",
            relative: Some(relative.to_path_buf()),
            cause,
        })?;
    let length = usize::try_from(file.len()).map_err(|_| E::Overflow("FILE bytes"))?;
    let bytes = file
        .read_range(0, length)
        .map_err(|cause| DesiredCorpusError::Source {
            operation: "read_range",
            relative: Some(relative.to_path_buf()),
            cause,
        })?;
    before()?;
    let provisional = relative
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("");
    let parsed = parse_document(
        original,
        bytes.as_ref(),
        provisional,
        limits.preparation.parser,
    )
    .map_err(DesiredCorpusError::Parser)?;
    let identity = KnowledgeSourceIdentity::from_relative_path(
        relative,
        authored_id(&parsed, limits.preparation.parser)?,
        limits.preparation.mapping.max_field_bytes,
    )
    .map_err(DesiredCorpusError::Identity)?;
    let fallback = relative
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(identity.slug());
    let compiled = compile_captured_file_bytes(
        source,
        relative,
        bytes.as_ref(),
        original,
        PublicationMetadataPolicy {
            fallback_title: copy_text(fallback)?,
            kind_on_absent: copy_text(kind_on_absent)?,
        },
        limits.preparation.parser,
        limits.preparation.metadata,
    )
    .map_err(DesiredCorpusError::Compilation)?;
    for field in [
        identity.slug(),
        identity.source_path(),
        compiled.whole_source(),
        compiled.title(),
        compiled.kind(),
        compiled.summary(),
        compiled.description().unwrap_or(""),
        fallback,
        kind_on_absent,
    ] {
        bound(
            field.len(),
            limits.preparation.mapping.max_field_bytes,
            "field bytes",
        )?;
    }
    slugs.try_reserve(1).map_err(E::Allocation)?;
    origins.try_reserve(1).map_err(E::Allocation)?;
    if let Some(first) = slugs.insert(copy_text(identity.slug())?, ordinal) {
        return Err(DesiredCorpusError::DuplicateSlug {
            first,
            second: ordinal,
        }
        .into());
    }
    if let Some(first) = origins.insert(copy_text(identity.source_path())?, ordinal) {
        return Err(DesiredCorpusError::DuplicateOrigin {
            first,
            second: ordinal,
        }
        .into());
    }
    let previous = dates.get(identity.slug());
    let inputs = [AuthoredDocumentInput {
        compiled: &compiled,
        slug: identity.slug(),
        source_path: identity.source_path(),
        updated_at: previous.and_then(|dates| dates.updated_at),
        stale_since: previous.and_then(|dates| dates.stale_since),
        stale_after: previous.and_then(|dates| dates.stale_after),
    }];
    let mut target_paths = BTreeSet::new();
    for event in &parsed.markdown {
        before()?;
        if let Event::Start(Tag::Link {
            link_type,
            dest_url,
            ..
        }) = &event.event
            && let Some(path) = relative_markdown_target(
                dest_url,
                *link_type,
                identity.source_path(),
                ordinal,
                limits.preparation.mapping,
            )
            .map_err(E::Mapping)?
            && path != relative
        {
            target_paths.insert(path);
        }
    }
    let mut target_ids = BTreeMap::new();
    // One genuine capture covers this FILE's distinct destinations. It survives
    // both effect guards and is dropped before advancing to the next FILE.
    let target_capture = if target_paths.is_empty() {
        None
    } else {
        Some(selection.capture_targets(&target_paths)?)
    };
    for path in &target_paths {
        before()?;
        let reader = target_capture
            .as_ref()
            .expect("nonempty destination capture")
            .worker_reader(original)?;
        let node = reader
            .nodes()
            .find(|node| node.kind() == NodeKind::File && node.relative() == path);
        if let Some(node) = node {
            let digest = node.digest();
            let cached = target_lookup
                .get(path)
                .filter(|target| Some(target.digest) == digest);
            let slug = if let Some(target) = cached {
                copy_text(&target.slug)?
            } else {
                let identity = KnowledgeSourceIdentity::from_relative_path(
                    path,
                    None,
                    limits.preparation.mapping.max_field_bytes,
                )
                .map_err(DesiredCorpusError::Identity)?;
                if let Some((Some(digest), slug)) = known_targets.get(identity.source_path())
                    && node.digest() == Some(*digest)
                {
                    copy_text(slug)?
                } else {
                    read_target_slug(original, reader, path, limits)?
                }
            };
            if let Some(digest) = digest {
                target_lookup.insert(
                    path.clone(),
                    TargetIdentity {
                        slug: copy_text(&slug)?,
                        digest,
                    },
                );
            }
            target_ids.insert(path.clone(), slug);
        } else {
            target_lookup.remove(path);
        }
    }
    let mapping = prepare_native_authored_mapping_with_targets(
        original,
        &inputs,
        limits.preparation.parser,
        limits.preparation.mapping,
        limits.fingerprint,
        limits.references,
        &target_ids,
    )
    .map_err(E::Mapping)?;
    let mapped = mapping.documents(original).map_err(E::Mapping)?;
    let resolved = mapped[0].resolved_wiki_targets();
    let mut view = Vec::new();
    view.try_reserve_exact(resolved.len())
        .map_err(E::Allocation)?;
    view.extend(resolved.iter().map(String::as_str));
    let targets = [view];
    let fields = mapping
        .authored_fields(original, &targets)
        .map_err(E::Mapping)?;
    let rows =
        projection::prepare(original, &fields, &[], limits.projection).map_err(E::Projection)?;
    let authored = project_document_authored_references(
        original,
        &parsed,
        SourceNamespace {
            scope: LogicalScope::Project(project_id),
            context: None,
        },
        limits.preparation.parser,
        limits.authored_references,
    )
    .map_err(E::Authored)?;
    let refs = convert_authored_references(authored, &before)?;
    let updates = [KnowledgeDocumentReferenceUpdate {
        slug: identity.slug(),
        authored: match &refs {
            None => KnowledgeDocumentAuthoredUpdate::Preserve,
            Some(refs) => KnowledgeDocumentAuthoredUpdate::Replace(refs),
        },
    }];
    before()?;
    let batch = encode_knowledge_source_batch(
        original,
        project_id,
        &rows,
        &updates,
        limits.index.write.content,
    )
    .map_err(E::DataImage)?;
    Ok((batch, target_capture))
}

fn check_target_bindings(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    targets: Option<&CapturedKnowledgeSources>,
) -> Result<()> {
    if let Some(targets) = targets {
        targets
            .checked_reader(original, read)?
            .revalidate_bindings()
            .map_err(|cause| DesiredCorpusError::Source {
                operation: "revalidate_link_target",
                relative: None,
                cause,
            })?;
    }
    Ok(())
}

fn read_target_slug(
    original: &Arc<SnapshotContext>,
    reader: &graphit_core::artifact_store::CapturedSourceReader,
    path: &Path,
    limits: KnowledgeDesiredReconciliationLimits,
) -> Result<String> {
    let source_error = |operation, cause| DesiredCorpusError::Source {
        operation,
        relative: Some(path.to_path_buf()),
        cause,
    };
    let file = reader
        .open_file(path)
        .map_err(|cause| source_error("open_link_target", cause))?;
    let length = usize::try_from(file.len()).map_err(|_| E::Overflow("FILE bytes"))?;
    // Include the opening delimiter/BOM even for an insufficient YAML cap so
    // the existing parser reports its bound instead of treating it as no YAML.
    let cap = length.min(limits.preparation.parser.max_frontmatter_bytes.get().max(8));
    let mut bytes = Vec::new();
    let mut next = cap.min(256);
    loop {
        original.check().map_err(E::Request)?;
        bytes
            .try_reserve(next - bytes.len())
            .map_err(E::Allocation)?;
        let chunk = file
            .read_range(bytes.len() as u64, next - bytes.len())
            .map_err(|cause| source_error("read_link_frontmatter", cause))?;
        bytes.extend_from_slice(chunk.as_ref());
        // A chunk may end inside a body codepoint. Only the complete YAML
        // envelope is parsed; the destination body is never decoded here.
        let (source, utf8_error) = match std::str::from_utf8(&bytes) {
            Ok(source) => (source, None),
            Err(error) => (
                std::str::from_utf8(&bytes[..error.valid_up_to()]).unwrap(),
                Some(error),
            ),
        };
        // A delimiter at the end of a partial line is not a YAML closing
        // delimiter. Only complete lines (or actual FILE EOF) certify it.
        let source = if bytes.len() < length {
            source.rfind('\n').map_or(source, |end| &source[..=end])
        } else {
            source
        };
        match envelope(
            original,
            source,
            limits.preparation.parser.max_frontmatter_bytes,
        ) {
            Ok(envelope) => {
                let end = envelope.frontmatter.map_or(0, |range| range.end);
                let parsed = parse_document(original, &bytes[..end], "", limits.preparation.parser)
                    .map_err(DesiredCorpusError::Parser)?;
                let identity = KnowledgeSourceIdentity::from_relative_path(
                    path,
                    authored_id(&parsed, limits.preparation.parser)?,
                    limits.preparation.mapping.max_field_bytes,
                )
                .map_err(DesiredCorpusError::Identity)?;
                return copy_text(identity.slug()).map_err(E::from);
            }
            Err(DocumentParseError::UnclosedFrontmatter)
                if next < cap && utf8_error.is_none_or(|e| e.error_len().is_none()) =>
            {
                next = cap.min(next.saturating_mul(2));
            }
            Err(DocumentParseError::UnclosedFrontmatter)
                if cap < length && utf8_error.is_none_or(|e| e.error_len().is_none()) =>
            {
                return Err(
                    DesiredCorpusError::Parser(DocumentParseError::LimitExceeded(
                        "frontmatter bytes",
                    ))
                    .into(),
                );
            }
            Err(error) => {
                return Err(DesiredCorpusError::Parser(
                    utf8_error.map_or(error, DocumentParseError::InvalidUtf8),
                )
                .into());
            }
        }
    }
}
