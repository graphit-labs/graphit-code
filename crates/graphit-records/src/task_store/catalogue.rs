//! Catalogue readiness needs the full graph, but only one page needs full heads.
use super::{read::Projected, *};
use crate::{
    gates,
    task_spec::{CheckKind, CheckStatus, TaskId},
};
use futures::TryStreamExt;
use graphit_auth::ProjectReadAdmission;
use graphit_core::{file_io::SnapshotContext, native_path::NativePath};
use graphit_data::lance_module_store::StoreError;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    num::{NonZeroU128, NonZeroUsize},
    sync::Arc,
};

struct GateHead {
    task_id: TaskId,
    id: String,
    session_id: SessionId,
    session: String,
    parent: Option<String>,
    dependencies: Vec<String>,
    status: gates::TaskStatus,
    flagged: bool,
    checks: Vec<gates::CheckFact<'static>>,
    matches: bool,
    created_at: u64,
}
impl GateHead {
    fn from_projected(projected: Projected, filter: &CatalogueFilter, query: Option<&str>) -> Self {
        let task = projected.summary;
        let matches = filter.status.is_none_or(|status| status == task.status)
            && filter
                .session_id
                .is_none_or(|session| session == task.session_id)
            && filter.flagged.is_none_or(|flagged| flagged == task.flagged)
            && query.is_none_or(|query| task.title.to_lowercase().contains(query));
        Self {
            task_id: task.id,
            id: model::id(task.id.get()),
            session_id: task.session_id,
            session: model::id(task.session_id.get()),
            parent: task.parent_id.map(|id| model::id(id.get())),
            dependencies: task
                .depends_on
                .iter()
                .map(|id| model::id(id.get()))
                .collect(),
            status: task.status,
            flagged: task.flagged,
            checks: projected
                .checks
                .into_iter()
                .map(|check| gates::CheckFact {
                    kind: match check.kind {
                        CheckKind::Acceptance => gates::CheckKind::Acceptance,
                        CheckKind::Test => gates::CheckKind::Test,
                    },
                    status: match check.status {
                        CheckStatus::Pending => gates::CheckStatus::Pending,
                        CheckStatus::Passed => gates::CheckStatus::Passed,
                        CheckStatus::Failed => gates::CheckStatus::Failed,
                        CheckStatus::Superseded => gates::CheckStatus::Superseded,
                    },
                    evidence: if check
                        .evidence
                        .is_some_and(|evidence| !evidence.trim().is_empty())
                    {
                        "present"
                    } else {
                        ""
                    },
                })
                .collect(),
            matches,
            created_at: task.created_at,
        }
    }
}
type Selection = BTreeMap<(u64, NonZeroU128), (TaskId, bool)>;
fn select(
    heads: &[GateHead],
    filter: &CatalogueFilter,
    cursor: Option<&CatalogueCursor>,
    limit: usize,
    before: impl Fn() -> Result<(), TaskStoreError>,
) -> Result<Selection, TaskStoreError> {
    before()?;
    let dependencies: Vec<Vec<_>> = heads
        .iter()
        .map(|head| head.dependencies.iter().map(String::as_str).collect())
        .collect();
    let facts: Vec<_> = heads
        .iter()
        .zip(&dependencies)
        .map(|(head, dependencies)| gates::TaskFact {
            id: &head.id,
            session_id: &head.session,
            parent_id: head.parent.as_deref(),
            status: head.status,
            flagged: head.flagged,
            dependencies,
            checks: &head.checks,
        })
        .collect();
    let graph = gates::GateGraph::new(&facts)?;
    before()?;
    let mut selected = BTreeMap::new();
    for head in heads {
        before()?;
        let key = (head.created_at, head.task_id.get());
        if !head.matches
            || cursor.is_some_and(|cursor| key >= (cursor.after_created_at, cursor.after_id.get()))
        {
            continue;
        }
        let ready = graph.ready(&head.id)?;
        if filter
            .blocked
            .is_some_and(|blocked| blocked != (head.status == gates::TaskStatus::Open && !ready))
        {
            continue;
        }
        selected.insert(key, (head.task_id, ready));
        if selected.len() > limit {
            selected.pop_first();
        }
    }
    before()?;
    Ok(selected)
}
impl NativeTaskStore {
    /// Retain readiness facts for the complete graph and full summaries for this page only.
    pub async fn catalogue(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        limits: TaskStoreReadLimits,
        filter: CatalogueFilter,
        page_size: NonZeroUsize,
        cursor: Option<&CatalogueCursor>,
    ) -> Result<CataloguePage, TaskStoreError> {
        native::check_read(original, admission)?;
        let limit = page_size.get().saturating_add(1);
        let Some(capture) = native::read_pin(original, admission, limits).await? else {
            if cursor.is_some() {
                return Err(TaskStoreError::InvalidCursor);
            }
            native::check_read(original, admission)?;
            return Ok(CataloguePage {
                results: vec![],
                next_cursor: None,
            });
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, TaskStoreError>(())
        };
        before()?;
        let project = admission.project()?;
        let root = NativePath::new(project.scope.live.owner_root().to_owned())?;
        let project_id = native::project_id(project.scope.live)?;
        let brand = project.scope.live.brand().directory().to_owned();
        let project_version = project.version.version().to_owned();
        let corpus_key = capture.guard.corpus_key(capture.pin.corpus_uri());
        let version = capture.pin.version();
        let physical_branch = capture.pin.physical_branch().map(str::to_owned);
        let inherited_target = capture.pin.inherited_target().map(str::to_owned);
        if cursor.is_some_and(|cursor| {
            cursor.corpus_key != corpus_key
                || cursor.root != root
                || cursor.project_id != project_id
                || cursor.brand != brand
                || cursor.project_version != project_version
                || cursor.version != version
                || cursor.physical_branch != physical_branch
                || cursor.inherited_target != inherited_target
                || cursor.filter != filter
        }) {
            return Err(TaskStoreError::InvalidCursor);
        }
        let query = filter.query.as_ref().map(|query| query.to_lowercase());
        let mut scan = capture.pin.scan();
        let columns: Vec<_> = std::iter::once("kind")
            .chain(rows::COLUMNS.iter().copied())
            .collect();
        scan.filter("kind IN ('task', 'session')")
            .map_err(StoreError::from)?
            .project(&columns)
            .map_err(StoreError::from)?
            .batch_size(limits.batch_rows.get() as usize);
        let mut stream = crate::storage::read(original, scan.try_into_stream())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let mut heads = Vec::new();
        let mut sessions = HashSet::new();
        loop {
            before()?;
            let batch = crate::storage::read(original, stream.try_next())
                .await?
                .map_err(StoreError::from)?;
            before()?;
            let Some(batch) = batch else { break };
            for row in 0..batch.num_rows() {
                original.check()?;
                if rows::required(&batch, "kind", row)? == "task" {
                    heads.push(GateHead::from_projected(
                        read::project(&batch, row)?,
                        &filter,
                        query.as_deref(),
                    ));
                } else {
                    let id: SessionId =
                        read::identifier(rows::required(&batch, "record_id", row)?)?;
                    let _: gates::TaskStatus =
                        read::identifier(rows::required(&batch, "status", row)?)?;
                    canonical(rows::required(&batch, "title", row)?)?;
                    rows::number(&batch, "updated_at", row)?;
                    rows::ownership(&batch, row)?;
                    rows::progress(&batch, row)?;
                    if rows::number(&batch, "revision", row)? == 0 || !sessions.insert(id) {
                        return Err(TaskStoreError::Invalid("session projection"));
                    }
                }
            }
        }
        before()?;
        if heads
            .iter()
            .any(|head| !sessions.contains(&head.session_id))
        {
            return Err(TaskStoreError::MissingSession);
        }
        let mut selected = select(&heads, &filter, cursor, limit, || Ok(original.check()?))?;
        drop(heads);
        drop(sessions);
        let has_more = selected.len() > page_size.get();
        if has_more {
            selected.pop_first();
        }
        let next_cursor = if has_more {
            let ((after_created_at, _), (after_id, _)) = selected
                .first_key_value()
                .ok_or(TaskStoreError::IncompleteCorpus)?;
            Some(CatalogueCursor {
                corpus_key,
                root,
                project_id,
                brand,
                project_version,
                version,
                physical_branch,
                inherited_target,
                filter,
                after_created_at: *after_created_at,
                after_id: *after_id,
            })
        } else {
            None
        };
        let mut expected: HashMap<_, _> = selected.into_values().collect();
        let mut results = Vec::new();
        if !expected.is_empty() {
            let ids = expected
                .keys()
                .map(|id| format!("'{}'", model::id(id.get())))
                .collect::<Vec<_>>()
                .join(",");
            let mut scan = capture.pin.scan();
            scan.filter(&format!("kind = 'task' AND record_id IN ({ids})"))
                .map_err(StoreError::from)?
                .project(rows::COLUMNS)
                .map_err(StoreError::from)?
                .batch_size(limits.batch_rows.get() as usize);
            before()?;
            let mut stream = crate::storage::read(original, scan.try_into_stream())
                .await?
                .map_err(StoreError::from)?;
            before()?;
            loop {
                before()?;
                let batch = crate::storage::read(original, stream.try_next())
                    .await?
                    .map_err(StoreError::from)?;
                before()?;
                let Some(batch) = batch else { break };
                for row in 0..batch.num_rows() {
                    original.check()?;
                    let mut task = read::project(&batch, row)?.summary;
                    task.ready = expected
                        .remove(&task.id)
                        .ok_or(TaskStoreError::IncompleteCorpus)?;
                    results.push(task);
                }
            }
            if !expected.is_empty() {
                return Err(TaskStoreError::IncompleteCorpus);
            }
        }
        results.sort_unstable_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.get().cmp(&a.id.get()))
        });
        before()?;
        Ok(CataloguePage {
            results,
            next_cursor,
        })
    }
}
