use super::{
    catalogue,
    embeddings::{SourceOwner, query::EmbeddingScan},
    history, native, rows, *,
};
use arrow_array::{Float32Array, RecordBatch, StringArray};
use futures::TryStreamExt;
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::lance_branch_store::JsonFtsIndexState;
use lance_index::scalar::{
    FullTextSearchQuery,
    inverted::query::{FtsQuery, MatchQuery, Operator},
};
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU64,
    sync::Arc,
};

const PATHS: [&str; 12] = [
    "title",
    "body",
    "tags",
    "attributes.classification",
    "memory.title",
    "memory.body",
    "memory.tags",
    "memory.attributes.classification",
    "after.title",
    "after.body",
    "after.tags",
    "after.attributes.classification",
];

pub(crate) fn expand(text: &str, maximum: usize) -> Result<String, MemoryStoreError> {
    nonblank(text)?;
    if text.len() > maximum {
        return Err(MemoryStoreError::Limit("lexical query text bytes"));
    }
    let mut bytes = 0;
    let mut terms = 0usize;
    for literal in text.split(';').filter(|s| !s.trim().is_empty()) {
        for path in PATHS {
            if terms != 0 {
                charge(&mut bytes, 1, maximum, "lexical expansion bytes")?;
            }
            charge(&mut bytes, path.len(), maximum, "lexical expansion bytes")?;
            charge(&mut bytes, 5, maximum, "lexical expansion bytes")?;
            charge(
                &mut bytes,
                literal.len(),
                maximum,
                "lexical expansion bytes",
            )?;
            terms = terms
                .checked_add(1)
                .ok_or(MemoryStoreError::Limit("lexical terms"))?;
        }
    }
    if terms == 0 {
        return Err(MemoryStoreError::Invalid("blank lexical segments"));
    }
    let mut expanded = String::with_capacity(bytes);
    for literal in text.split(';').filter(|s| !s.trim().is_empty()) {
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

#[derive(Clone, Copy)]
struct Selected {
    location: MemoryMatchLocation,
    score: f32,
}

#[derive(Clone, Copy)]
pub(crate) enum HistoryView<'a> {
    Project(&'a history::Chain),
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    User(&'a crate::user_memory_store::store::Chain),
}
impl<'a> HistoryView<'a> {
    pub(crate) fn current(self) -> &'a Memory {
        match self {
            Self::Project(c) => &c.current,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            Self::User(c) => &c.current,
        }
    }
    pub(crate) fn creation(self) -> &'a Memory {
        match self {
            Self::Project(c) => &c.creation.memory,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            Self::User(c) => &c.creation.memory,
        }
    }
    pub(crate) fn deleted(self) -> bool {
        match self {
            Self::Project(c) => c.deleted,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            Self::User(c) => c.deleted,
        }
    }
    fn revision(self, number: u64) -> Option<&'a Memory> {
        match self {
            Self::Project(c) => c
                .revisions
                .binary_search_by_key(&number, |e| e.after.revision)
                .ok()
                .map(|i| &c.revisions[i].after),
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            Self::User(c) => c
                .revisions
                .binary_search_by_key(&number, |e| e.after.revision)
                .ok()
                .map(|i| &c.revisions[i].after),
        }
    }
    pub(crate) fn sources(self) -> impl Iterator<Item = &'a Memory> {
        let length = match self {
            Self::Project(c) => c.revisions.len(),
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            Self::User(c) => c.revisions.len(),
        };
        std::iter::once(self.creation()).chain((0..length).filter_map(move |i| match self {
            Self::Project(c) => {
                let e = &c.revisions[i];
                (!e.operation.deleted()).then_some(&e.after)
            }
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            Self::User(c) => {
                let e = &c.revisions[i];
                (!e.operation.deleted()).then_some(&e.after)
            }
        }))
    }
}
fn match_view<'a>(
    chain: HistoryView<'a>,
    key: &str,
) -> Result<(MemoryMatchLocation, &'a Memory), MemoryStoreError> {
    let id = chain.current().id;
    if key == format!("head/{id}") {
        return Ok((MemoryMatchLocation::Current, chain.current()));
    }
    if key == format!("creation/{id}") {
        return Ok((
            MemoryMatchLocation::Archived {
                revision: NonZeroU64::MIN,
            },
            chain.creation(),
        ));
    }
    if let Some(encoded) = key.strip_prefix(&format!("revision/{id}/")) {
        let number = encoded
            .parse::<u64>()
            .map_err(|_| MemoryStoreError::Invalid("lexical archive address"))?;
        if number < 2 || encoded != number.to_string() {
            return Err(MemoryStoreError::Invalid("lexical archive address"));
        }
        if let Some(memory) = chain.revision(number) {
            return Ok((
                MemoryMatchLocation::Archived {
                    revision: NonZeroU64::new(number)
                        .ok_or(MemoryStoreError::Invalid("zero Memory archive revision"))?,
                },
                memory,
            ));
        }
    }
    Err(MemoryStoreError::Invalid("unknown lexical physical row"))
}

fn prefer(new: MemoryMatchLocation, old: MemoryMatchLocation) -> bool {
    match (new, old) {
        (MemoryMatchLocation::Current, MemoryMatchLocation::Archived { .. }) => true,
        (
            MemoryMatchLocation::Archived { revision: a },
            MemoryMatchLocation::Archived { revision: b },
        ) => a > b,
        _ => false,
    }
}

type RankKey = (u8, std::cmp::Reverse<u64>, MemoryId);

fn rank_key(memory: &Memory) -> RankKey {
    (
        catalogue::category(memory),
        std::cmp::Reverse(memory.updated_at),
        memory.id,
    )
}

#[derive(Clone, Copy)]
pub(crate) struct RankedLexicalMatch {
    pub id: MemoryId,
    pub matched_revision: NonZeroU64,
    pub location: MemoryMatchLocation,
    pub score: f32,
}

struct Candidates {
    seen: HashSet<String>,
    selected: HashMap<MemoryId, Selected>,
}
impl Candidates {
    fn new() -> Self {
        Self {
            seen: HashSet::new(),
            selected: HashMap::new(),
        }
    }
    #[cfg(test)]
    fn batch(
        &mut self,
        batch: &RecordBatch,
        chains: &HashMap<MemoryId, history::Chain>,
        query: &MemoryLexicalQuery,
        before: impl Fn() -> Result<(), MemoryStoreError>,
    ) -> Result<(), MemoryStoreError> {
        self.batch_view(
            batch,
            |id| chains.get(&id).map(HistoryView::Project),
            &query.filter,
            query.include_deleted_history,
            before,
        )
    }
    fn batch_view<'a>(
        &mut self,
        batch: &RecordBatch,
        chain_for: impl Fn(MemoryId) -> Option<HistoryView<'a>>,
        filter: &MemoryListFilter,
        include_deleted_history: bool,
        before: impl Fn() -> Result<(), MemoryStoreError>,
    ) -> Result<(), MemoryStoreError> {
        before()?;
        if batch.num_columns() != 3 || batch.columns().iter().any(|a| a.null_count() != 0) {
            return Err(MemoryStoreError::Invalid("lexical candidate projection"));
        }
        let keys = batch
            .column_by_name("row_key")
            .and_then(|a| a.as_any().downcast_ref::<StringArray>())
            .ok_or(MemoryStoreError::Invalid("lexical row key type"))?;
        let ids = batch
            .column_by_name("record_id")
            .and_then(|a| a.as_any().downcast_ref::<StringArray>())
            .ok_or(MemoryStoreError::Invalid("lexical record ID type"))?;
        let scores = batch
            .column_by_name("_score")
            .and_then(|a| a.as_any().downcast_ref::<Float32Array>())
            .ok_or(MemoryStoreError::Invalid("lexical score type"))?;
        for row in 0..batch.num_rows() {
            before()?;

            let key = keys.value(row);
            let encoded_id = ids.value(row);

            let score = scores.value(row);
            if !score.is_finite() {
                return Err(MemoryStoreError::Invalid("nonfinite lexical score"));
            }
            if !self.seen.insert(key.to_owned()) {
                return Err(MemoryStoreError::Invalid("duplicate lexical physical row"));
            }
            let id: MemoryId = encoded_id.parse()?;
            let chain =
                chain_for(id).ok_or(MemoryStoreError::Invalid("unknown lexical Memory ID"))?;
            let (location, matched) = match_view(chain, key)?;
            if matched.id != id
                || match location {
                    MemoryMatchLocation::Current => matched.revision != chain.current().revision,
                    MemoryMatchLocation::Archived { revision } => {
                        matched.revision != revision.get()
                    }
                }
            {
                return Err(MemoryStoreError::Invalid("lexical row binding"));
            }
            if chain.deleted()
                && (!include_deleted_history || location == MemoryMatchLocation::Current)
            {
                continue;
            }
            if !filter.matches(chain.current()) {
                continue;
            }
            if self
                .selected
                .get(&id)
                .is_none_or(|old| prefer(location, old.location))
            {
                self.selected.insert(id, Selected { location, score });
            }
        }
        before()?;
        Ok(())
    }
}

impl NativeMemoryStore {
    /// Search literal content and archives, validating candidate histories on the same snapshot.
    /// Filtering and unique top-k happen after the entire SDK candidate stream.
    pub async fn search_lexical(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        query: MemoryLexicalQuery,
        limits: MemoryStoreReadLimits,
    ) -> Result<MemoryLexicalSearchRead, MemoryStoreError> {
        native::check_read(original, admission)?;
        if let Some(tag) = &query.filter.tag {
            nonblank(tag)?;
        }
        let expanded = expand(&query.text, limits.max_query_bytes.get())?;
        let Some(capture) = native::read_pin(original, admission, limits.native).await? else {
            native::check_read(original, admission)?;
            return Ok(MemoryLexicalSearchRead::NoCorpus);
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, MemoryStoreError>(())
        };
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        let generation = native::generation(&capture.pin, project_id, project.version.version());
        if !has_memories(&capture.pin, original, rows::schema().as_ref(), &before).await? {
            return Ok(MemoryLexicalSearchRead::PublishedEmpty { generation });
        }
        before()?;
        let state = capture.pin.json_fts_index_state("payload").await?;
        before()?;
        if state != JsonFtsIndexState::Ready {
            return Err(MemoryStoreError::LexicalIndexUnavailable(state));
        }
        let (total_hits, hits) = search_candidates(
            &EmbeddingScan {
                pin: &capture.pin,
                original,
                owner: SourceOwner::Project(project_id),
                limits,
            },
            expanded,
            &query,
            &before,
        )
        .await?;
        Ok(MemoryLexicalSearchRead::Published {
            generation,
            total_hits,
            hits,
        })
    }
}

/// The empty-corpus distinction needs only the native row count, never payload decoding.
/// A physical schema mismatch still fails even for an empty table.
pub(crate) async fn has_memories(
    pin: &graphit_data::lance_branch_store::BranchSnapshot,
    original: &SnapshotContext,
    expected: &arrow_schema::Schema,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<bool, MemoryStoreError> {
    before()?;
    if pin.schema() != expected {
        return Err(MemoryStoreError::Invalid("Memory batch schema"));
    }
    let mut scanner = pin.scan();
    scanner
        .filter("kind IN ('head', 'creation', 'revision')")
        .map_err(StoreError::from)?;
    let count = crate::storage::read(original, scanner.count_rows())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    Ok(count != 0)
}

/// Read candidate histories once, ordered by ID, retaining only the requested top-k bodies.
pub(crate) async fn search_candidates(
    scan: &EmbeddingScan<'_>,
    expanded: String,
    query: &MemoryLexicalQuery,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<(usize, Vec<MemoryLexicalHit>), MemoryStoreError> {
    use lance::dataset::scanner::ColumnOrdering;
    use std::{collections::BTreeMap, fmt::Write};
    let mut candidates =
        candidate_batches(scan.pin, scan.original, expanded, scan.limits, &before).await?;
    if candidates.is_empty() {
        before()?;
        return Ok((0, Vec::new()));
    }
    let mut filter = String::from("record_id IN (");
    for (index, id) in candidates.keys().enumerate() {
        scan.original.check()?;
        if index != 0 {
            filter.push(',');
        }
        // MemoryId has a validated canonical alphabet; this literal cannot contain SQL syntax.
        write!(&mut filter, "'{id}'").expect("writing into a String");
    }
    filter.push(')');
    let mut scanner = scan.pin.scan();
    scanner
        .filter(&filter)
        .map_err(StoreError::from)?
        .order_by(Some(vec![ColumnOrdering::asc_nulls_last(
            "record_id".into(),
        )]))
        .map_err(StoreError::from)?
        .batch_size(scan.limits.batch_rows.get() as usize)
        .fragment_readahead(1)
        .batch_readahead(1);
    before()?;
    let mut stream = crate::storage::read(scan.original, scanner.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    let mut current = None;
    let mut group = Vec::new();
    let mut total = 0;
    let mut hits = BTreeMap::new();
    let check_cpu = || Ok(scan.original.check()?);
    let mut consume = |id, batches: Vec<RecordBatch>| -> Result<(), MemoryStoreError> {
        let candidate = candidates
            .remove(&id)
            .ok_or(MemoryStoreError::Invalid("unknown lexical Memory ID"))?;
        let mut keys = HashSet::new();
        let rank = |chain: HistoryView<'_>| -> Result<(), MemoryStoreError> {
            if let Some((key, selected)) = rank_group(
                candidate,
                chain,
                &query.filter,
                query.include_deleted_history,
                check_cpu,
            )? {
                total += 1;
                if hits.len() < query.top_k.get()
                    || hits.last_key_value().is_some_and(|(last, _)| &key < last)
                {
                    hits.insert(key, lexical_hit(chain, selected)?);
                    if hits.len() > query.top_k.get() {
                        hits.pop_last();
                    }
                }
            }
            Ok(())
        };
        match scan.owner {
            SourceOwner::Project(project_id) => {
                let mut sources = Vec::new();
                for batch in &batches {
                    for row in 0..batch.num_rows() {
                        scan.original.check()?;
                        if !keys.insert(rows::text(batch, 0, row)?.to_owned()) {
                            return Err(MemoryStoreError::Invalid("duplicate Memory row key"));
                        }
                        if let Some(source) = rows::decode(batch, row, scan.original, project_id)? {
                            sources.push(source);
                        }
                    }
                }
                let chain = history::assemble(sources, project_id, check_cpu)?
                    .ok_or(MemoryStoreError::IncompleteCorpus)?;
                rank(HistoryView::Project(&chain))?;
            }
            SourceOwner::User(owner) => {
                let mut sources = Vec::new();
                for batch in &batches {
                    for row in 0..batch.num_rows() {
                        scan.original.check()?;
                        if !keys.insert(rows::text(batch, 0, row)?.to_owned()) {
                            return Err(MemoryStoreError::Invalid("duplicate Memory row key"));
                        }
                        if let Some(source) = crate::user_memory_store::rows::decode(
                            batch,
                            row,
                            scan.original,
                            owner,
                        )? {
                            sources.push(source);
                        }
                    }
                }
                let chain = crate::user_memory_store::store::assemble(sources, check_cpu)?
                    .ok_or(MemoryStoreError::IncompleteCorpus)?;
                rank(HistoryView::User(&chain))?;
            }
        }
        check_cpu()
    };
    loop {
        before()?;
        let batch = crate::storage::read(scan.original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = batch else { break };
        rows::validate_schema(&batch, scan.owner.schema().as_ref())?;
        let mut start = 0;
        while start < batch.num_rows() {
            scan.original.check()?;
            let id: MemoryId = rows::text(&batch, 2, start)?.parse()?;
            if let Some(prior) = current {
                if id < prior {
                    return Err(MemoryStoreError::Invalid("Memory group ordering"));
                }
                if id != prior {
                    consume(prior, std::mem::take(&mut group))?;
                }
            }
            current = Some(id);
            let mut end = start + 1;
            while end < batch.num_rows()
                && rows::text(&batch, 2, end)? == rows::text(&batch, 2, start)?
            {
                scan.original.check()?;
                end += 1;
            }
            group.push(batch.slice(start, end - start));
            start = end;
        }
    }
    if let Some(id) = current {
        consume(id, group)?;
    }
    if !candidates.is_empty() {
        return Err(MemoryStoreError::Invalid("unknown lexical Memory ID"));
    }
    before()?;
    Ok((total, hits.into_values().collect()))
}

fn lexical_hit(
    chain: HistoryView<'_>,
    selected: RankedLexicalMatch,
) -> Result<MemoryLexicalHit, MemoryStoreError> {
    let matched = match selected.location {
        MemoryMatchLocation::Current => chain.current(),
        MemoryMatchLocation::Archived { revision } if revision.get() == 1 => chain.creation(),
        MemoryMatchLocation::Archived { revision } => chain
            .revision(revision.get())
            .ok_or(MemoryStoreError::Invalid("lexical archive binding"))?,
    };
    Ok(MemoryLexicalHit {
        current: chain.current().clone(),
        matched: matched.clone(),
        location: selected.location,
        deleted: chain.deleted(),
        score: selected.score,
    })
}

/// SDK candidates only; source bodies and vectors remain in their captured table.
pub(crate) async fn candidate_batches(
    pin: &graphit_data::lance_branch_store::BranchSnapshot,
    original: &SnapshotContext,
    expanded: String,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<HashMap<MemoryId, Vec<RecordBatch>>, MemoryStoreError> {
    let matched = MatchQuery::new(expanded)
        .with_column(Some("payload".into()))
        .with_operator(Operator::Or)
        .with_fuzziness(Some(0));
    let mut scanner = pin.scan();
    scanner
        .filter("kind IN ('head', 'creation', 'revision')")
        .map_err(StoreError::from)?
        .full_text_search(FullTextSearchQuery::new_query(FtsQuery::Match(matched)).limit(None))
        .map_err(StoreError::from)?
        .project(&["row_key", "record_id", "_score"])
        .map_err(StoreError::from)?;
    scanner.disable_scoring_autoprojection();
    scanner
        .batch_size(limits.batch_rows.get() as usize)
        .fragment_readahead(1)
        .batch_readahead(1);
    before()?;
    let mut stream = crate::storage::read(original, scanner.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    let mut groups: HashMap<MemoryId, Vec<RecordBatch>> = HashMap::new();
    loop {
        before()?;
        let batch = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = batch else { break };
        if batch.num_columns() != 3 || batch.columns().iter().any(|a| a.null_count() != 0) {
            return Err(MemoryStoreError::Invalid("lexical candidate projection"));
        }
        let ids = batch
            .column_by_name("record_id")
            .and_then(|a| a.as_any().downcast_ref::<StringArray>())
            .ok_or(MemoryStoreError::Invalid("lexical record ID type"))?;
        for row in 0..batch.num_rows() {
            original.check()?;
            groups
                .entry(ids.value(row).parse()?)
                .or_default()
                .push(batch.slice(row, 1));
        }
    }
    before()?;
    Ok(groups)
}

pub(crate) fn rank_group(
    batches: Vec<RecordBatch>,
    chain: HistoryView<'_>,
    filter: &MemoryListFilter,
    include_deleted_history: bool,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<(RankKey, RankedLexicalMatch)>, MemoryStoreError> {
    let id = chain.current().id;
    let mut candidates = Candidates::new();
    for batch in batches {
        candidates.batch_view(
            &batch,
            |candidate| (candidate == id).then_some(chain),
            filter,
            include_deleted_history,
            &before,
        )?;
    }
    let Some(selected) = candidates.selected.remove(&id) else {
        return Ok(None);
    };
    let matched_revision = match selected.location {
        MemoryMatchLocation::Current => NonZeroU64::new(chain.current().revision)
            .ok_or(MemoryStoreError::Invalid("zero Memory revision"))?,
        MemoryMatchLocation::Archived { revision } => revision,
    };
    before()?;
    Ok(Some((
        rank_key(chain.current()),
        RankedLexicalMatch {
            id,
            matched_revision,
            location: selected.location,
            score: selected.score,
        },
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_schema::{DataType, Field, Schema};
    use graphit_core::file_lock::Cancellation;
    use graphit_hub::policy::RequestContext;
    use std::{
        num::NonZeroU128,
        time::{Duration, Instant},
    };

    fn query() -> MemoryLexicalQuery {
        MemoryLexicalQuery {
            text: "needle".into(),
            filter: MemoryListFilter::default(),
            include_deleted_history: false,
            top_k: NonZeroUsize::MIN,
        }
    }
    fn chain() -> history::Chain {
        let memory = Memory {
            id: MemoryId::new(NonZeroU128::new(1).unwrap()),
            revision: 1,
            title: "needle".into(),
            body: "body".into(),
            attributes: crate::memory_attributes::MemoryAttributes {
                classification: crate::memory_attributes::MemoryClass::Fact,
                mandatory: false,
                important: false,
            },
            tags: vec![],
            references: vec![],
            reference_generation: 1,
            created_by: ulid::Ulid::from(1u128).to_string(),
            updated_by: ulid::Ulid::from(1u128).to_string(),
            created_at: 1,
            updated_at: 1,
        };
        history::Chain {
            creation: CreationMemory {
                project_id: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
                operation_version: "main".into(),
                actor: ulid::Ulid::from(1u128).to_string(),
                input: CreateMemory {
                    idempotency_key: "attempt".into(),
                    title: memory.title.clone(),
                    body: memory.body.clone(),
                    attributes: memory.attributes,
                    tags: vec![],
                    references: None,
                },
                memory: memory.clone(),
            },
            current: memory,
            deleted: false,
            revisions: vec![],
        }
    }
    fn batch(key: &str, id: &str, score: Option<f32>) -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                Field::new("row_key", DataType::Utf8, false),
                Field::new("record_id", DataType::Utf8, false),
                Field::new("_score", DataType::Float32, true),
            ])),
            vec![
                Arc::new(StringArray::from(vec![key])),
                Arc::new(StringArray::from(vec![id])),
                Arc::new(Float32Array::from(vec![score])),
            ],
        )
        .unwrap()
    }
    #[test]
    fn lexical_candidate_late_duplicate_nonfinite_null_unknown_binding() {
        let c = chain();
        let id = c.current.id;
        let chains = HashMap::from([(id, c)]);
        let head = format!("head/{id}");
        let creation = format!("creation/{id}");
        let q = query();
        let mut candidates = Candidates::new();
        candidates
            .batch(
                &batch(&head, &id.to_string(), Some(1.0)),
                &chains,
                &q,
                || Ok(()),
            )
            .unwrap();
        assert!(matches!(
            candidates.batch(
                &batch(&head, &id.to_string(), Some(2.0)),
                &chains,
                &q,
                || Ok(())
            ),
            Err(MemoryStoreError::Invalid("duplicate lexical physical row"))
        ));
        for score in [
            Some(f32::NAN),
            Some(f32::INFINITY),
            Some(f32::NEG_INFINITY),
            None,
        ] {
            let mut candidates = Candidates::new();
            candidates
                .batch(
                    &batch(&head, &id.to_string(), Some(1.0)),
                    &chains,
                    &q,
                    || Ok(()),
                )
                .unwrap();
            assert!(matches!(
                candidates.batch(
                    &batch(&creation, &id.to_string(), score),
                    &chains,
                    &q,
                    || Ok(())
                ),
                Err(MemoryStoreError::Invalid(_))
            ));
        }
        for (key, encoded) in [
            (
                format!("head/{}", MemoryId::new(NonZeroU128::new(2).unwrap())),
                id.to_string(),
            ),
            (
                creation.clone(),
                MemoryId::new(NonZeroU128::new(2).unwrap()).to_string(),
            ),
            (format!("revision/{id}/01"), id.to_string()),
        ] {
            let mut candidates = Candidates::new();
            candidates
                .batch(
                    &batch(&head, &id.to_string(), Some(1.0)),
                    &chains,
                    &q,
                    || Ok(()),
                )
                .unwrap();
            assert!(matches!(
                candidates.batch(&batch(&key, &encoded, Some(1.0)), &chains, &q, || Ok(())),
                Err(MemoryStoreError::Invalid(_))
            ));
        }
    }
    #[test]
    fn lexical_candidate_actual_request_cancel_after_valid_prefix() {
        let cancel = Cancellation::default();
        let ctx = RequestContext::new(
            graphit_hub::native::ContentLimits {
                max_input_bytes: 4096,
                max_output_bytes: 4096,
                max_depth: 16,
                max_nodes: 1024,
                max_scalar_bytes: 1024,
                max_collection_entries: 1024,
            },
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let c = chain();
        let id = c.current.id;
        let chains = HashMap::from([(id, c)]);
        let mut candidates = Candidates::new();
        let before = || ctx.snapshot().check().map_err(MemoryStoreError::Request);
        candidates
            .batch(
                &batch(&format!("head/{id}"), &id.to_string(), Some(1.0)),
                &chains,
                &query(),
                before,
            )
            .unwrap();
        cancel.cancel();
        assert!(matches!(
            candidates.batch(
                &batch(&format!("creation/{id}"), &id.to_string(), Some(1.0)),
                &chains,
                &query(),
                before
            ),
            Err(MemoryStoreError::Request(_))
        ));
    }
    #[test]
    fn lexical_expansion_counts_utf8_and_preserves_literal_triplet_values() {
        let text = " ; CAFÉ,β\nOR ; ; word ; ";
        let expanded = expand(text, 4096).unwrap();
        assert!(expanded.contains("title,str, CAFÉ,β\nOR "));
        assert!(expanded.contains("after.attributes.classification,str, word "));
        assert_eq!(expanded.split(';').count(), 24);
        assert_eq!(expand(text, expanded.len()).unwrap(), expanded);
        assert!(matches!(
            expand(text, expanded.len() - 1),
            Err(MemoryStoreError::Limit("lexical expansion bytes"))
        ));
        assert!(matches!(
            expand(&"a".repeat(101), 100),
            Err(MemoryStoreError::Limit("lexical query text bytes"))
        ));
    }

    #[test]
    fn lexical_same_category_date_descending_id_ascending_ignore_score() {
        let memory = chain().current;
        let mut old = MemoryLexicalHit {
            current: memory.clone(),
            matched: memory,
            location: MemoryMatchLocation::Current,
            deleted: false,
            score: 99.0,
        };
        old.current.id = MemoryId::new(NonZeroU128::new(1).unwrap());
        old.current.updated_at = 1;
        let mut later_high_id = old.clone();
        later_high_id.current.id = MemoryId::new(NonZeroU128::new(3).unwrap());
        later_high_id.current.updated_at = 2;
        later_high_id.score = 10.0;
        let mut later_low_id = later_high_id.clone();
        later_low_id.current.id = MemoryId::new(NonZeroU128::new(2).unwrap());
        later_low_id.score = 1.0;
        let mut hits = [old, later_high_id, later_low_id];
        hits.sort_by_key(|hit| rank_key(&hit.current));
        assert_eq!(hits.map(|h| h.current.id.get().get()), [2, 3, 1]);
    }
}
