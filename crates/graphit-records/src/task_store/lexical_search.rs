//! Same-pin public Task discovery; SDK scores survive unique selection and paging.
use super::{
    NativeTaskStore, TaskLexicalHit, TaskLexicalMatchLocation, TaskLexicalQuery,
    TaskLexicalSearchRead, TaskSearchCursor, TaskStoreError, TaskStoreReadLimits, comments,
    lexical_model::SearchBinding, model::*, native, public_search, read, rows,
};
use crate::task_spec::TaskId;
use arrow_array::{Float32Array, RecordBatch, StringArray};
use futures::TryStreamExt;
use graphit_auth::ProjectReadAdmission;
use graphit_core::{file_io::SnapshotContext, native_path::NativePath};
use graphit_data::{lance_branch_store::JsonFtsIndexState, lance_module_store::StoreError};
use lance_index::scalar::{
    FullTextSearchQuery,
    inverted::query::{FtsQuery, MatchQuery, Operator},
};
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    sync::Arc,
};

const PATHS: [&str; 5] = [
    "title",
    "description",
    "check_text",
    "evidence",
    "comment_body",
];
pub(super) fn expand(text: &str, maximum: usize) -> Result<String, TaskStoreError> {
    if text.len() > maximum {
        return Err(TaskStoreError::Invalid("Task FTS query byte limit"));
    }
    let bytes = text.split(';').try_fold(0usize, |bytes, literal| {
        if literal.trim().is_empty() {
            return Err(TaskStoreError::Invalid("Task FTS blank literal"));
        }
        PATHS.into_iter().try_fold(bytes, |bytes, path| {
            bytes
                .checked_add(path.len() + 5)
                .and_then(|n| n.checked_add(literal.len()))
                .and_then(|n| n.checked_add(usize::from(bytes > 0)))
                .filter(|n| *n <= maximum)
                .ok_or(TaskStoreError::Invalid("Task FTS query byte limit"))
        })
    })?;
    let mut expanded = String::with_capacity(bytes);
    for literal in text.split(';') {
        for path in PATHS {
            if !expanded.is_empty() {
                expanded.push(';');
            }
            expanded.push_str(path);
            expanded.push_str(",str,");
            expanded.push_str(literal);
        }
    }
    Ok(expanded)
}
pub(super) fn prefer(a: TaskLexicalMatchLocation, b: TaskLexicalMatchLocation) -> bool {
    use TaskLexicalMatchLocation::*;
    match (a, b) {
        (Specification { .. }, Comment { .. }) => true,
        (Comment { sequence: a, .. }, Comment { sequence: b, .. }) => a < b,
        _ => false,
    }
}
#[derive(Clone, Copy)]
struct Selected {
    score: f32,
    location: TaskLexicalMatchLocation,
}
struct Candidates {
    seen: HashSet<String>,
    selected: HashMap<TaskId, Selected>,
}
impl Candidates {
    fn new() -> Self {
        Self {
            seen: HashSet::new(),
            selected: HashMap::new(),
        }
    }
    fn batch(
        &mut self,
        batch: &RecordBatch,
        expected: &HashMap<String, (TaskId, TaskLexicalMatchLocation)>,
        summaries: &HashMap<TaskId, TaskSummary>,
        query: &TaskLexicalQuery,
        before: impl Fn() -> Result<(), TaskStoreError>,
    ) -> Result<(), TaskStoreError> {
        before()?;
        if batch.num_columns() != 3 || batch.columns().iter().any(|a| a.null_count() != 0) {
            return Err(TaskStoreError::Invalid("Task FTS candidate projection"));
        }
        let keys = batch
            .column_by_name("row_key")
            .and_then(|a| a.as_any().downcast_ref::<StringArray>())
            .ok_or(TaskStoreError::Invalid("Task FTS row key type"))?;
        let ids = batch
            .column_by_name("record_id")
            .and_then(|a| a.as_any().downcast_ref::<StringArray>())
            .ok_or(TaskStoreError::Invalid("Task FTS record ID type"))?;
        let scores = batch
            .column_by_name("_score")
            .and_then(|a| a.as_any().downcast_ref::<Float32Array>())
            .ok_or(TaskStoreError::Invalid("Task FTS score type"))?;
        for row in 0..batch.num_rows() {
            before()?;
            let key = keys.value(row);
            let encoded = ids.value(row);
            let score = scores.value(row);
            if !score.is_finite() {
                return Err(TaskStoreError::Invalid("Task FTS nonfinite score"));
            }
            if !self.seen.insert(key.to_owned()) {
                return Err(TaskStoreError::Invalid("Task FTS duplicate candidate"));
            }
            let (task_id, location) = expected
                .get(key)
                .ok_or(TaskStoreError::Invalid("Task FTS candidate binding"))?;
            if encoded != id(task_id.get()) {
                return Err(TaskStoreError::Invalid("Task FTS candidate binding"));
            }
            let summary = summaries
                .get(task_id)
                .ok_or(TaskStoreError::IncompleteCorpus)?;
            if !query.filter.matches(summary) {
                continue;
            }
            if self.selected.get(task_id).is_none_or(|old| {
                score > old.score || (score == old.score && prefer(*location, old.location))
            }) {
                self.selected.insert(
                    *task_id,
                    Selected {
                        score,
                        location: *location,
                    },
                );
            }
        }
        before()?;
        Ok(())
    }
}
/// Bind continuation data only after the current caller selects and authorizes
/// the corpus. SDK historical reads preserve pages while other writers commit.
pub(super) async fn bind_pin(
    capture: &mut native::ReadPin,
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    query: &TaskLexicalQuery,
    semantic: Option<super::embeddings::SemanticBinding>,
) -> Result<SearchBinding, TaskStoreError> {
    native::check_read(original, admission)?;
    capture.guard.revalidate(original)?;
    let project = admission.project()?;
    let mut binding = SearchBinding {
        mode: query.mode,
        semantic,
        corpus_key: capture.guard.corpus_key(capture.pin.corpus_uri()),
        project_id: native::project_id(project.scope.live)?,
        root: NativePath::new(project.scope.live.owner_root().to_owned())?,
        project_version: project.version.version().to_owned(),
        generation: TaskGeneration {
            version: capture.pin.version(),
            physical_branch: capture.pin.physical_branch().map(str::to_owned),
            inherited_target: capture.pin.inherited_target().map(str::to_owned),
        },
        branch_provenance: None,
        text: query.text.clone(),
        filter: query.filter.clone(),
        page_size: query.page_size,
        top_k: query.top_k,
    };
    let inherited = if let Some(cursor) = &query.cursor {
        let mut identity = (*cursor.binding).clone();
        identity.generation = binding.generation.clone();
        identity.branch_provenance = None;
        if identity != binding
            || cursor.offset == 0
            || cursor.offset >= query.top_k.get()
            || !cursor.offset.is_multiple_of(query.page_size.get())
        {
            return Err(TaskStoreError::InvalidCursor);
        }
        let logical = project.version.lance_branch();
        let inherited = match (
            cursor.binding.generation.physical_branch.as_deref(),
            cursor.binding.generation.inherited_target.as_deref(),
        ) {
            (None, Some(target)) if !logical.is_empty() && target == logical => true,
            (physical, None) if physical == (!logical.is_empty()).then_some(logical.as_str()) => {
                false
            }
            _ => return Err(TaskStoreError::InvalidCursor),
        };
        if !inherited
            && cursor.binding.generation.physical_branch.is_some()
            && cursor.binding.branch_provenance.is_none()
        {
            return Err(TaskStoreError::InvalidCursor);
        }
        binding = (*cursor.binding).clone();
        inherited
    } else {
        capture.pin.inherited_target().is_some()
    };
    let (pin, provenance) = if inherited && capture.pin.physical_branch().is_some() {
        let (pin, provenance) = crate::storage::read(
            original,
            capture.pin.at_inherited_version(binding.generation.version),
        )
        .await??;
        (pin, Some(provenance))
    } else {
        crate::storage::read(
            original,
            capture
                .pin
                .at_version_with_provenance(binding.generation.version),
        )
        .await??
    };
    native::check_read(original, admission)?;
    capture.guard.revalidate(original)?;
    if pin.version() != binding.generation.version
        || pin.physical_branch() != binding.generation.physical_branch.as_deref()
        || pin.inherited_target() != binding.generation.inherited_target.as_deref()
        || (binding.branch_provenance.is_some() && binding.branch_provenance != provenance)
    {
        return Err(TaskStoreError::InvalidCursor);
    }
    super::rows::validate_schema(pin.schema())?;
    binding.branch_provenance = provenance;
    capture.pin = pin;
    Ok(binding)
}

pub(super) fn page(
    mut hits: Vec<TaskLexicalHit>,
    query: &TaskLexicalQuery,
    binding: SearchBinding,
    before: impl Fn() -> Result<(), TaskStoreError>,
) -> Result<TaskLexicalSearchRead, TaskStoreError> {
    before()?;
    hits.sort_unstable_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.task.id.get().cmp(&b.task.id.get()))
    });
    let total_hits = hits.len();
    hits.truncate(query.top_k.get());
    let offset = match &query.cursor {
        // bind_pin already checked the complete query/corpus binding and
        // historical provenance, including an inherited child's first birth.
        Some(c)
            if c.offset > 0
                && c.offset < hits.len()
                && c.offset.is_multiple_of(query.page_size.get()) =>
        {
            c.offset
        }
        None => 0,
        _ => return Err(TaskStoreError::InvalidCursor),
    };
    let end = offset
        .checked_add(query.page_size.get())
        .ok_or(TaskStoreError::InvalidCursor)?
        .min(hits.len());
    let generation = binding.generation.clone();
    let next_cursor = (end < hits.len()).then(|| TaskSearchCursor {
        binding: Box::new(binding),
        offset: end,
    });
    let results = hits.into_iter().skip(offset).take(end - offset).collect();
    before()?;
    Ok(TaskLexicalSearchRead::Published {
        generation,
        total_hits,
        results,
        next_cursor,
    })
}
impl NativeTaskStore {
    /// Discover public current specification/check text and append-only comments.
    /// Same-pin graph and validated SDK candidates precede unique top-k paging.
    pub async fn search_lexical(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        query: TaskLexicalQuery,
        read_limits: TaskStoreReadLimits,
        max_query_bytes: NonZeroUsize,
    ) -> Result<TaskLexicalSearchRead, TaskStoreError> {
        native::check_read(original, admission)?;
        if query.mode != super::TaskSearchMode::Fts {
            return Err(TaskStoreError::Invalid(
                "Task semantic search requires configured embedding service",
            ));
        }
        if let Some(c) = &query.cursor
            && (c.binding.text != query.text
                || c.binding.filter != query.filter
                || c.binding.page_size != query.page_size
                || c.binding.top_k != query.top_k)
        {
            return Err(TaskStoreError::InvalidCursor);
        }
        let expanded = expand(&query.text, max_query_bytes.get())?;
        let Some(mut capture) =
            native::read_pin_with_parent_check(original, admission, read_limits).await?
        else {
            native::check_read(original, admission)?;
            if query.cursor.is_some() {
                return Err(TaskStoreError::InvalidCursor);
            }
            return Ok(TaskLexicalSearchRead::NoCorpus);
        };
        let binding = bind_pin(&mut capture, original, admission, &query, None).await?;
        let generation = binding.generation.clone();
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, TaskStoreError>(())
        };
        before()?;
        let summaries: HashMap<_, _> =
            read::head_summaries(&capture.pin, original, read_limits, &before)
                .await?
                .into_iter()
                .map(|t| (t.id, t))
                .collect();
        let hits = collect_hits(
            &capture.pin,
            original,
            &summaries,
            &query,
            read_limits,
            expanded,
            &before,
        )
        .await?;
        let Some(hits) = hits else {
            before()?;
            if query.cursor.is_some() {
                return Err(TaskStoreError::InvalidCursor);
            }
            return Ok(TaskLexicalSearchRead::PublishedEmpty { generation });
        };
        page(hits, &query, binding, before)
    }
}

/// Collect every eligible unique Task before the public cap. Hybrid uses this
/// same captured pin and authority boundary rather than calling a paged API.
pub(super) async fn collect_hits(
    pin: &graphit_data::lance_branch_store::BranchSnapshot,
    original: &Arc<SnapshotContext>,
    summaries: &HashMap<TaskId, TaskSummary>,
    query: &TaskLexicalQuery,
    read_limits: TaskStoreReadLimits,
    expanded: String,
    before: impl Fn() -> Result<(), TaskStoreError>,
) -> Result<Option<Vec<TaskLexicalHit>>, TaskStoreError> {
    if summaries.is_empty() {
        before()?;
        return Ok(None);
    }
    before()?;
    let state = pin.json_fts_index_state(public_search::COLUMN).await?;
    before()?;
    match state {
        JsonFtsIndexState::Ready => (),
        JsonFtsIndexState::Missing => {
            return Err(TaskStoreError::Invalid("Task FTS index missing"));
        }
        JsonFtsIndexState::Incompatible => {
            return Err(TaskStoreError::Invalid("Task FTS index incompatible"));
        }
    }
    let matched = MatchQuery::new(expanded)
        .with_column(Some(public_search::COLUMN.into()))
        .with_operator(Operator::Or)
        .with_fuzziness(Some(0));
    let mut scan = pin.scan();
    scan.full_text_search(FullTextSearchQuery::new_query(FtsQuery::Match(matched)).limit(None))
        .map_err(StoreError::from)?;
    // Read public matches and their authoritative typed source together. Historical
    // specifications, coordination events and unrelated bodies are not query input.
    let base = rows::schema();
    let fields: Vec<_> = base
        .fields()
        .iter()
        .map(|f| f.name().as_str())
        .chain(std::iter::once("_score"))
        .collect();
    scan.project(&fields).map_err(StoreError::from)?;
    scan.disable_scoring_autoprojection();
    scan.batch_size(read_limits.batch_rows.get() as usize);
    before()?;
    let mut stream = crate::storage::read(original, scan.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    let mut candidates = Candidates::new();
    loop {
        before()?;
        let next = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = next else { break };
        let expected =
            candidate_sources(pin, original, &batch, summaries, read_limits, &before).await?;
        let scored = batch.project(&[
            batch.schema().index_of("row_key")?,
            batch.schema().index_of("record_id")?,
            batch.schema().index_of("_score")?,
        ])?;
        candidates.batch(&scored, &expected, summaries, query, || {
            Ok(original.check()?)
        })?;
    }
    let mut hits = Vec::new();
    for (task_id, selected) in candidates.selected {
        original.check()?;
        hits.push(TaskLexicalHit {
            task: summaries
                .get(&task_id)
                .ok_or(TaskStoreError::IncompleteCorpus)?
                .clone(),
            score: selected.score,
            location: selected.location,
        });
    }
    before()?;
    Ok(Some(hits))
}

type SourceLocations = HashMap<String, (TaskId, TaskLexicalMatchLocation)>;

/// Validate the rows returned by FTS against this same pin. Comment attribution
/// needs only its actual CommentAdded event, not every lifecycle revision.
async fn candidate_sources(
    pin: &graphit_data::lance_branch_store::BranchSnapshot,
    original: &Arc<SnapshotContext>,
    batch: &RecordBatch,
    summaries: &HashMap<TaskId, TaskSummary>,
    limits: TaskStoreReadLimits,
    before: impl Fn() -> Result<(), TaskStoreError>,
) -> Result<SourceLocations, TaskStoreError> {
    // The typed codec expects only the persistent schema, not the SDK's score.
    let source = batch.project(&(0..rows::schema().fields().len()).collect::<Vec<_>>())?;
    let mut locations = HashMap::new();
    let mut sessions = HashSet::new();
    let mut comment_events = HashMap::new();
    for row in 0..batch.num_rows() {
        original.check()?;
        let key = rows::required(batch, "row_key", row)?.to_owned();
        let (task_id, location) = match rows::decode(&source, row, original)? {
            rows::Row::Task(t) => {
                let task_id = t.record.specification.id;
                let summary = summaries.get(&task_id).ok_or(TaskStoreError::MissingTask)?;
                let mut projected = read::project_task(&t.record).summary;
                projected.ready = summary.ready;
                if projected != *summary {
                    return Err(TaskStoreError::Invalid("Task FTS head binding"));
                }
                sessions.insert(summary.session_id);
                (
                    task_id,
                    TaskLexicalMatchLocation::Specification {
                        revision: summary.revision,
                    },
                )
            }
            rows::Row::TaskComment(c) => {
                let summary = summaries
                    .get(&c.task_id)
                    .ok_or(TaskStoreError::MissingTask)?;
                if c.session_id != summary.session_id
                    || c.revision > summary.revision
                    || c.at < summary.created_at
                    || c.at > summary.updated_at
                {
                    return Err(TaskStoreError::Invalid("Task FTS comment binding"));
                }
                sessions.insert(c.session_id);
                let location = TaskLexicalMatchLocation::Comment {
                    comment_id: c.id,
                    sequence: c.sequence.get(),
                };
                let task_id = c.task_id;
                let event_key = format!("task_event/{}/{}", id(task_id.get()), c.revision);
                if comment_events.insert(event_key, c).is_some() {
                    return Err(TaskStoreError::Invalid("duplicate Task comment revision"));
                }
                (task_id, location)
            }
            _ => return Err(TaskStoreError::Invalid("Task FTS source family")),
        };
        if locations.insert(key, (task_id, location)).is_some() {
            return Err(TaskStoreError::Invalid("Task FTS duplicate candidate"));
        }
    }
    if locations.is_empty() {
        return Ok(locations);
    }
    let mut filter = format!(
        "kind = 'session' AND record_id IN ({})",
        sessions
            .iter()
            .map(|s| format!("'{}'", id(s.get())))
            .collect::<Vec<_>>()
            .join(",")
    );
    if !comment_events.is_empty() {
        filter = format!(
            "({filter}) OR (kind = 'task_event' AND row_key IN ({}))",
            comment_events
                .keys()
                .map(|key| format!("'{key}'"))
                .collect::<Vec<_>>()
                .join(",")
        );
    }
    for source in read::exact_rows(pin, &filter, original, limits, &before).await? {
        original.check()?;
        match source {
            rows::Row::Session(s) if sessions.remove(&s.record.id) => (),
            rows::Row::TaskEvent(e) => {
                let key = format!(
                    "task_event/{}/{}",
                    id(e.record.specification.id.get()),
                    e.record.specification.revision
                );
                let comment = comment_events
                    .remove(&key)
                    .ok_or(TaskStoreError::Invalid("duplicate Task comment event"))?;
                comments::validate_binding(&e, &comment)?;
            }
            _ => return Err(TaskStoreError::Invalid("Task FTS source binding")),
        }
    }
    if !sessions.is_empty() {
        return Err(TaskStoreError::MissingSession);
    }
    if !comment_events.is_empty() {
        return Err(TaskStoreError::Invalid("missing comment event"));
    }
    before()?;
    Ok(locations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gates::TaskStatus;
    use graphit_core::file_lock::Cancellation;
    use std::num::{NonZeroU128, NonZeroUsize};

    fn query() -> TaskLexicalQuery {
        TaskLexicalQuery {
            mode: super::super::TaskSearchMode::Fts,
            text: "needle".into(),
            filter: super::super::TaskSearchFilter::default(),
            page_size: NonZeroUsize::MIN,
            top_k: NonZeroUsize::new(5).unwrap(),
            cursor: None,
        }
    }
    #[test]
    fn task_lexical_search_individual_query_expansion_boundary() {
        let text = "é, literal\nvalue;other";
        let expected = text
            .split(';')
            .flat_map(|literal| PATHS.map(|path| format!("{path},str,{literal}")))
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(expand(text, expected.len()).unwrap(), expected);
        assert!(matches!(
            expand(text, expected.len() - 1),
            Err(TaskStoreError::Invalid("Task FTS query byte limit"))
        ));
        for text in ["", " ", "word;", ";word", "word; "] {
            assert!(matches!(
                expand(text, 1024),
                Err(TaskStoreError::Invalid("Task FTS blank literal"))
            ));
        }
    }
    fn summary() -> TaskSummary {
        TaskSummary {
            id: TaskId::new(NonZeroU128::MIN),
            session_id: SessionId::new(NonZeroU128::MIN),
            title: "needle".into(),
            task_type: "feature".into(),
            priority: 2,
            status: TaskStatus::Open,
            parent_id: None,
            depends_on: vec![],
            schedule: None,
            ownership: Ownership::default(),
            progress: None,
            flagged: false,
            ready: true,
            revision: 4,
            created_at: 1,
            updated_at: 4,
        }
    }
    fn batch(key: &str, encoded: &str, score: Option<f32>) -> RecordBatch {
        use arrow_schema::{DataType, Field, Schema};
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                Field::new("row_key", DataType::Utf8, false),
                Field::new("record_id", DataType::Utf8, false),
                Field::new("_score", DataType::Float32, true),
            ])),
            vec![
                Arc::new(StringArray::from(vec![key])),
                Arc::new(StringArray::from(vec![encoded])),
                Arc::new(Float32Array::from(vec![score])),
            ],
        )
        .unwrap()
    }
    #[test]
    fn task_lexical_search_candidate_late_errors_and_excluded_rows_still_validate() {
        let task = summary();
        let encoded = id(task.id.get());
        let key = format!("task/{encoded}");
        let location = TaskLexicalMatchLocation::Specification {
            revision: task.revision,
        };
        let expected = HashMap::from([(key.clone(), (task.id, location))]);
        let summaries = HashMap::from([(task.id, task)]);
        let q = query();
        let mut candidates = Candidates::new();
        candidates
            .batch(
                &batch(&key, &encoded, Some(1.0)),
                &expected,
                &summaries,
                &q,
                || Ok(()),
            )
            .unwrap();
        assert!(matches!(
            candidates.batch(
                &batch(&key, &encoded, Some(2.0)),
                &expected,
                &summaries,
                &q,
                || Ok(())
            ),
            Err(TaskStoreError::Invalid("Task FTS duplicate candidate"))
        ));
        let mut excluded = q;
        excluded.filter.flagged = Some(true);
        for (k, i, s) in [
            (key.as_str(), encoded.as_str(), Some(f32::NAN)),
            (key.as_str(), encoded.as_str(), Some(f32::INFINITY)),
            (key.as_str(), encoded.as_str(), Some(f32::NEG_INFINITY)),
            (key.as_str(), encoded.as_str(), None),
            ("task_event/foreign", encoded.as_str(), Some(1.0)),
            (key.as_str(), "unknown", Some(1.0)),
        ] {
            assert!(
                Candidates::new()
                    .batch(&batch(k, i, s), &expected, &summaries, &excluded, || Ok(()))
                    .is_err(),
                "excluded invalid candidate must fail"
            );
        }
    }
    #[test]
    fn task_lexical_search_candidate_max_score_and_location_tie_rules() {
        let task = summary();
        let encoded = id(task.id.get());
        let head = format!("task/{encoded}");
        let low = format!("task_comment/{encoded}/1");
        let high = format!("task_comment/{encoded}/2");
        let spec = TaskLexicalMatchLocation::Specification { revision: 4 };
        let c1 = TaskLexicalMatchLocation::Comment {
            comment_id: TaskCommentId::new(NonZeroU128::MIN),
            sequence: 1,
        };
        let c2 = TaskLexicalMatchLocation::Comment {
            comment_id: TaskCommentId::new(NonZeroU128::new(2).unwrap()),
            sequence: 2,
        };
        let expected = HashMap::from([
            (head.clone(), (task.id, spec)),
            (low.clone(), (task.id, c1)),
            (high.clone(), (task.id, c2)),
        ]);
        let summaries = HashMap::from([(task.id, task.clone())]);
        for rows in [
            [(&high, 3.0), (&low, 3.0), (&head, 3.0)],
            [(&head, 3.0), (&high, 3.0), (&low, 3.0)],
        ] {
            let mut c = Candidates::new();
            for (k, s) in rows {
                c.batch(
                    &batch(k, &encoded, Some(s)),
                    &expected,
                    &summaries,
                    &query(),
                    || Ok(()),
                )
                .unwrap();
            }
            assert_eq!(c.selected[&task.id].location, spec);
            assert_eq!(c.selected.len(), 1);
        }
        let mut c = Candidates::new();
        for (k, s) in [(&head, 1.0), (&high, 3.0), (&low, 3.0)] {
            c.batch(
                &batch(k, &encoded, Some(s)),
                &expected,
                &summaries,
                &query(),
                || Ok(()),
            )
            .unwrap();
        }
        assert_eq!(c.selected[&task.id].location, c1);
        assert_eq!(c.selected[&task.id].score, 3.0);
    }
    #[test]
    fn task_lexical_search_actual_cancellation_between_candidates_and_before_delivery() {
        let cancellation = Cancellation::default();
        let ctx = super::super::test_fixture::request(&cancellation);
        let original = ctx.snapshot();
        let task = summary();
        let encoded = id(task.id.get());
        let key = format!("task/{encoded}");
        let location = TaskLexicalMatchLocation::Specification { revision: 4 };
        let expected = HashMap::from([(key.clone(), (task.id, location))]);
        let summaries = HashMap::from([(task.id, task.clone())]);
        let before = || original.check().map_err(TaskStoreError::from);
        let mut candidates = Candidates::new();
        candidates
            .batch(
                &batch(&key, &encoded, Some(1.0)),
                &expected,
                &summaries,
                &query(),
                before,
            )
            .unwrap();
        cancellation.cancel();
        assert!(matches!(
            candidates.batch(
                &batch("late", &encoded, Some(2.0)),
                &expected,
                &summaries,
                &query(),
                before
            ),
            Err(TaskStoreError::Request(_))
        ));
        let binding = SearchBinding {
            mode: super::super::TaskSearchMode::Fts,
            semantic: None,
            corpus_key: "test-corpus".into(),
            project_id: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            root: NativePath::new(std::path::PathBuf::from("/task-test")).unwrap(),
            project_version: "main".into(),
            generation: TaskGeneration {
                version: 1,
                physical_branch: None,
                inherited_target: None,
            },
            branch_provenance: None,
            text: "needle".into(),
            filter: query().filter,
            page_size: query().page_size,
            top_k: query().top_k,
        };
        // Cancel after page construction at its real final-delivery recheck.
        // This private boundary composes an actual Original; it does not intercept SDK.
        let delivery_cancel = Cancellation::default();
        let delivery_ctx = super::super::test_fixture::request(&delivery_cancel);
        let calls = std::cell::Cell::new(0);
        let deliver = || {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                delivery_cancel.cancel();
            }
            delivery_ctx
                .snapshot()
                .check()
                .map_err(TaskStoreError::from)
        };
        assert!(matches!(
            page(
                vec![TaskLexicalHit {
                    task,
                    score: 1.0,
                    location
                }],
                &query(),
                binding,
                deliver
            ),
            Err(TaskStoreError::Request(_))
        ));
        assert_eq!(calls.get(), 2);
    }
}
