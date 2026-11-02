//! Git transitions over authorized Task and Memory Lance snapshots.
use crate::{memory_store, task_store};
use arrow_array::{Array, RecordBatch, StringArray};
use graphit_auth::{ProjectWriteAdmission, ProjectWriteOperation};
use graphit_core::file_io::SnapshotContext;
use graphit_data::lance_branch_store::{BranchError, BranchSnapshot};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fmt, sync::Arc};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum GitRecordsTransition {
    Initialize {
        commit: String,
    },
    Commit {
        commit: String,
    },
    Checkout {
        commit: String,
        previous_commit: Option<String>,
    },
    Merge {
        base: String,
        theirs: String,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitRecordModule {
    Memory,
    Task,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitRecordEffect {
    Unchanged,
    Created,
    Tagged,
    Branched,
    Restored,
    Merged,
    SchemaExtended,
    Indexed,
}
#[derive(Clone, Debug, Serialize)]
pub struct GitModuleReport {
    pub module: GitRecordModule,
    pub effect: GitRecordEffect,
    pub version: u64,
    pub commit: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct GitRecordConflict {
    pub module: GitRecordModule,
    pub record_id: String,
    pub reason: &'static str,
}
#[derive(Clone, Debug, Serialize)]
pub struct GitMissingSnapshot {
    pub module: GitRecordModule,
    pub commit: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GitRecordsOutcome {
    Applied {
        memory: GitModuleReport,
        task: GitModuleReport,
    },
    Blocked {
        conflicts: Vec<GitRecordConflict>,
        missing: Vec<GitMissingSnapshot>,
    },
}
#[derive(Debug)]
pub enum GitRecordsCause {
    Memory(memory_store::MemoryStoreError),
    Task(task_store::TaskStoreError),
    Branch(BranchError),
    Delta(graphit_data::lance_branch_store::DeltaCommitFailure),
    Request(std::io::Error),
    Arrow(arrow_schema::ArrowError),
    Invalid(&'static str),
}
#[derive(Debug)]
pub struct GitRecordsError {
    pub cause: GitRecordsCause,
    pub partial: Vec<GitModuleReport>,
}
impl fmt::Display for GitRecordsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Git records transition failed: {:?}; confirmed module effects: {}",
            self.cause,
            self.partial.len()
        )
    }
}
impl Error for GitRecordsError {}
impl GitRecordsError {
    /// Native recovery identity for an uncertain SDK commit; callers must not
    /// replace this receipt with the SDK error's formatted message.
    pub fn recovery_key(&self) -> Option<&graphit_data::lance_branch_store::DeltaRecoveryKey> {
        match &self.cause {
            GitRecordsCause::Delta(error)
            | GitRecordsCause::Memory(memory_store::MemoryStoreError::Commit(error))
            | GitRecordsCause::Task(task_store::TaskStoreError::Commit(error)) => {
                Some(error.recovery_key())
            }
            _ => None,
        }
    }
    pub fn code(&self) -> &'static str {
        if !self.partial.is_empty() {
            return "git_records_partial";
        }
        match &self.cause {
            GitRecordsCause::Memory(memory_store::MemoryStoreError::Auth(_))
            | GitRecordsCause::Task(task_store::TaskStoreError::Auth(_)) => "access_denied",
            GitRecordsCause::Request(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::Interrupted | std::io::ErrorKind::TimedOut
                ) =>
            {
                "request_cancelled_or_expired"
            }
            GitRecordsCause::Memory(memory_store::MemoryStoreError::Request(error))
            | GitRecordsCause::Task(task_store::TaskStoreError::Request(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::Interrupted | std::io::ErrorKind::TimedOut
                ) =>
            {
                "request_cancelled_or_expired"
            }
            GitRecordsCause::Memory(memory_store::MemoryStoreError::Artifact(error))
            | GitRecordsCause::Task(task_store::TaskStoreError::Artifact(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::Interrupted | std::io::ErrorKind::TimedOut
                ) =>
            {
                "request_cancelled_or_expired"
            }
            GitRecordsCause::Delta(_)
            | GitRecordsCause::Memory(memory_store::MemoryStoreError::Commit(_))
            | GitRecordsCause::Task(task_store::TaskStoreError::Commit(_)) => {
                "git_records_commit_uncertain"
            }
            GitRecordsCause::Branch(BranchError::GitSnapshotMissing) => {
                "git_records_missing_snapshot"
            }
            GitRecordsCause::Invalid(_) => "invalid_git_records_transition",
            _ => "git_records_failed",
        }
    }
}
impl From<memory_store::MemoryStoreError> for GitRecordsError {
    fn from(cause: memory_store::MemoryStoreError) -> Self {
        Self {
            cause: GitRecordsCause::Memory(cause),
            partial: vec![],
        }
    }
}
impl From<task_store::TaskStoreError> for GitRecordsError {
    fn from(cause: task_store::TaskStoreError) -> Self {
        Self {
            cause: GitRecordsCause::Task(cause),
            partial: vec![],
        }
    }
}
impl From<BranchError> for GitRecordsError {
    fn from(cause: BranchError) -> Self {
        Self {
            cause: GitRecordsCause::Branch(cause),
            partial: vec![],
        }
    }
}
impl From<std::io::Error> for GitRecordsError {
    fn from(cause: std::io::Error) -> Self {
        Self {
            cause: GitRecordsCause::Request(cause),
            partial: vec![],
        }
    }
}
impl From<arrow_schema::ArrowError> for GitRecordsError {
    fn from(cause: arrow_schema::ArrowError) -> Self {
        Self {
            cause: GitRecordsCause::Arrow(cause),
            partial: vec![],
        }
    }
}
fn invalid(cause: &'static str) -> GitRecordsError {
    GitRecordsError {
        cause: GitRecordsCause::Invalid(cause),
        partial: vec![],
    }
}

#[derive(Clone)]
pub(crate) struct Capture {
    pub module: GitRecordModule,
    pub limits: ReadLimits,
    pub pin: BranchSnapshot,
    pub guard: Arc<crate::storage::Guard>,
}
#[derive(Clone, Copy)]
pub(crate) enum ReadLimits {
    Memory(memory_store::MemoryStoreReadLimits),
    Task(task_store::TaskStoreReadLimits),
}
impl Capture {
    fn check(
        &self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
    ) -> Result<(), GitRecordsError> {
        original.check()?;
        admission
            .recheck()
            .map_err(|error| invalid_auth(self.module, error))?;
        self.guard.revalidate(original)?;
        Ok(())
    }
}
fn invalid_auth(module: GitRecordModule, error: graphit_auth::AuthError) -> GitRecordsError {
    match module {
        GitRecordModule::Memory => memory_store::MemoryStoreError::Auth(error).into(),
        GitRecordModule::Task => task_store::TaskStoreError::Auth(error).into(),
    }
}
fn text<'a>(batch: &'a RecordBatch, name: &str, row: usize) -> Result<&'a str, GitRecordsError> {
    let array = batch
        .column_by_name(name)
        .and_then(|array| array.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| invalid("Git records string column"))?;
    if array.is_null(row) {
        return Err(invalid("Git records null string"));
    }
    Ok(array.value(row))
}
fn oid(value: &str) -> Result<(), GitRecordsError> {
    if matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(invalid("Git object ID"))
    }
}

fn check_admissions(
    original: &Arc<SnapshotContext>,
    memory: &ProjectWriteAdmission<'_>,
    task: &ProjectWriteAdmission<'_>,
) -> Result<(), GitRecordsError> {
    original.check()?;
    if memory.operation() != ProjectWriteOperation::Memory
        || !matches!(
            task.operation(),
            ProjectWriteOperation::Task | ProjectWriteOperation::CoordinateTask
        )
    {
        return Err(invalid(
            "Git records require Memory and Task write admissions",
        ));
    }
    let memory_project = memory
        .project()
        .map_err(|e| invalid_auth(GitRecordModule::Memory, e))?;
    let task_project = task
        .project()
        .map_err(|e| invalid_auth(GitRecordModule::Task, e))?;
    if !Arc::ptr_eq(
        original,
        memory
            .request()
            .map_err(|e| invalid_auth(GitRecordModule::Memory, e))?
            .snapshot(),
    ) || !Arc::ptr_eq(
        original,
        task.request()
            .map_err(|e| invalid_auth(GitRecordModule::Task, e))?
            .snapshot(),
    ) || memory_project.scope.live.owner() != task_project.scope.live.owner()
        || memory_project.scope.live.runtime_root() != task_project.scope.live.runtime_root()
        || memory_project.version.version() != task_project.version.version()
    {
        return Err(invalid("Git records admissions differ"));
    }
    Ok(())
}

/// Probe the two authorized datasets, including remote tags created by another
/// instance. Native tags are the authoritative Git snapshot association.
pub async fn has_git_commit(
    original: &Arc<SnapshotContext>,
    memory: &ProjectWriteAdmission<'_>,
    task: &ProjectWriteAdmission<'_>,
    commit: &str,
    memory_limits: memory_store::MemoryStoreWriteLimits,
    task_limits: task_store::TaskStoreWriteLimits,
) -> Result<bool, GitRecordsError> {
    check_admissions(original, memory, task)?;
    oid(commit)?;
    let memory_capture = memory_store::git::capture(original, memory, memory_limits).await?;
    let memory_tag = if let Some(capture) = memory_capture {
        tagged(original, memory, &capture, commit).await?
    } else {
        None
    };
    if memory_tag.is_none() {
        return Ok(false);
    }
    let task_capture = task_store::git::capture(original, task, task_limits).await?;
    let task_tag = if let Some(capture) = task_capture {
        tagged(original, task, &capture, commit).await?
    } else {
        None
    };
    Ok(task_tag.is_some())
}

pub async fn apply(
    original: &Arc<SnapshotContext>,
    memory: &ProjectWriteAdmission<'_>,
    task: &ProjectWriteAdmission<'_>,
    transition: GitRecordsTransition,
    memory_limits: memory_store::MemoryStoreWriteLimits,
    task_limits: task_store::TaskStoreWriteLimits,
) -> Result<GitRecordsOutcome, GitRecordsError> {
    check_admissions(original, memory, task)?;
    for value in match &transition {
        GitRecordsTransition::Commit { commit } | GitRecordsTransition::Initialize { commit } => {
            vec![commit]
        }
        GitRecordsTransition::Checkout {
            commit,
            previous_commit,
        } => std::iter::once(commit)
            .chain(previous_commit.iter())
            .collect(),
        GitRecordsTransition::Merge { base, theirs } => vec![base, theirs],
    } {
        oid(value)?;
    }
    let mut conflicts = Vec::new();
    let mut missing = Vec::new();
    let memory_plan = prepare(
        original,
        memory,
        GitRecordModule::Memory,
        memory_store::git::capture(original, memory, memory_limits).await?,
        &transition,
        &mut conflicts,
        &mut missing,
        ReadLimits::Memory(memory_limits.read),
    )
    .await?;
    let task_plan = prepare(
        original,
        task,
        GitRecordModule::Task,
        task_store::git::capture(original, task, task_limits).await?,
        &transition,
        &mut conflicts,
        &mut missing,
        ReadLimits::Task(task_limits.read),
    )
    .await?;
    if !conflicts.is_empty() || !missing.is_empty() {
        return Ok(GitRecordsOutcome::Blocked { conflicts, missing });
    }
    let memory_report =
        memory_store::git::execute(original, memory, memory_limits, memory_plan).await?;
    let task_report = task_store::git::execute(original, task, task_limits, task_plan)
        .await
        .map_err(|mut error| {
            if memory_report.effect != GitRecordEffect::Unchanged {
                error.partial.insert(0, memory_report.clone());
            }
            error
        })?;
    Ok(GitRecordsOutcome::Applied {
        memory: memory_report,
        task: task_report,
    })
}

pub(crate) enum VectorRef {
    Memory {
        id: memory_store::MemoryId,
        revision: u64,
        identity: memory_store::MemoryEmbeddingIdentity,
        fingerprint: [u8; 32],
    },
    Task(task_store::TaskEmbeddingDocument),
}
#[derive(Default)]
struct Index {
    public: BTreeMap<String, [u8; 32]>,
    keys: BTreeMap<String, std::collections::BTreeSet<String>>,
    vectors: BTreeMap<String, VectorRef>,
}
async fn index(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    module: GitRecordModule,
    capture: &Capture,
    batch_rows: u32,
) -> Result<Index, GitRecordsError> {
    use futures::TryStreamExt;
    use sha2::{Digest, Sha256};
    capture.check(original, admission)?;
    let project = match admission
        .project()
        .map_err(|e| invalid_auth(module, e))?
        .scope
        .live
        .owner()
    {
        graphit_core::artifact_store::StoreOwner::Project { project_id } => *project_id,
        _ => return Err(invalid("Git records require PROJECT")),
    };
    let mut scan = capture.pin.scan();
    scan.order_by(Some(vec![
        lance::dataset::scanner::ColumnOrdering::asc_nulls_last("row_key".into()),
    ]))
    .map_err(BranchError::from)?
    .batch_size(batch_rows as usize)
    .fragment_readahead(1)
    .batch_readahead(1);
    let mut stream = crate::storage::read(original, scan.try_into_stream())
        .await?
        .map_err(BranchError::from)?;
    let mut digests: BTreeMap<String, Sha256> = BTreeMap::new();
    let mut result = Index::default();
    let mut prior: Option<String> = None;
    loop {
        capture.check(original, admission)?;
        let next = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(BranchError::from)?;
        capture.check(original, admission)?;
        let Some(batch) = next else { break };
        for row in 0..batch.num_rows() {
            original.check()?;
            let key = text(&batch, "row_key", row)?;
            if prior.as_deref().is_some_and(|p| p >= key) {
                return Err(invalid("duplicate or unordered Git record row"));
            }
            prior = Some(key.into());
            let (family, vector) = match module {
                GitRecordModule::Memory => {
                    memory_store::git::validate_row(original, project, &batch, row)?
                }
                GitRecordModule::Task => task_store::git::validate_row(original, &batch, row)?,
            };
            result
                .keys
                .entry(family.clone())
                .or_default()
                .insert(key.into());
            if let Some(vector) = vector {
                result.vectors.insert(key.into(), vector);
                continue;
            }
            let hash = digests.entry(family).or_default();
            let payload = text(&batch, "payload", row)?;
            hash.update((key.len() as u64).to_le_bytes());
            hash.update(key.as_bytes());
            hash.update((payload.len() as u64).to_le_bytes());
            hash.update(payload.as_bytes());
        }
    }
    capture.check(original, admission)?;
    result.public = digests
        .into_iter()
        .map(|(id, hash)| (id, hash.finalize().into()))
        .collect();
    Ok(result)
}

pub(crate) struct Prepared {
    module: GitRecordModule,
    pub expected: Option<BranchSnapshot>,
    action: Action,
}
enum Action {
    Noop {
        report: GitModuleReport,
        check_working: bool,
    },
    Tag {
        commit: String,
    },
    Branch {
        commit: String,
        branch: String,
        vector_source: Capture,
        vector_keys: std::collections::BTreeSet<String>,
    },
    Restore {
        commit: String,
        vector_source: Capture,
        vector_keys: std::collections::BTreeSet<String>,
    },
    Delta {
        source: Capture,
        retained_guard: Arc<crate::storage::Guard>,
        source_keys: std::collections::BTreeSet<String>,
        keep_keys: std::collections::BTreeSet<String>,
        delete_keys: std::collections::BTreeSet<String>,
        effect: GitRecordEffect,
        commit: Option<String>,
        vector_schema: bool,
    },
}
impl Prepared {
    pub(crate) fn noop(&self) -> Option<(GitModuleReport, bool)> {
        if let Action::Noop {
            report,
            check_working,
        } = &self.action
        {
            Some((report.clone(), *check_working))
        } else {
            None
        }
    }
    pub(crate) fn index_before(&self) -> bool {
        // Tagging an existing snapshot does not publish derived indexes into
        // the working head. A new corpus keeps its normal index bootstrap.
        matches!(&self.action, Action::Delta { .. })
            || (self.expected.is_none() && matches!(&self.action, Action::Tag { .. }))
    }
    pub(crate) fn index_after(&self) -> bool {
        matches!(&self.action, Action::Branch { .. } | Action::Restore { .. })
    }
    pub(crate) fn needs_vectors(&self) -> bool {
        matches!(
            &self.action,
            Action::Delta {
                vector_schema: true,
                ..
            }
        )
    }
}
fn report(
    module: GitRecordModule,
    effect: GitRecordEffect,
    pin: &BranchSnapshot,
    commit: Option<String>,
) -> GitModuleReport {
    GitModuleReport {
        module,
        effect,
        version: pin.version(),
        commit,
    }
}
fn blocked_plan(module: GitRecordModule, pin: Option<BranchSnapshot>) -> Prepared {
    Prepared {
        module,
        expected: pin.clone(),
        action: Action::Noop {
            report: GitModuleReport {
                module,
                effect: GitRecordEffect::Unchanged,
                version: pin.as_ref().map_or(0, BranchSnapshot::version),
                commit: None,
            },
            check_working: false,
        },
    }
}
fn changed(left: &Index, right: &Index) -> Vec<String> {
    left.public
        .keys()
        .chain(right.public.keys())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|id| left.public.get(*id) != right.public.get(*id))
        .cloned()
        .collect()
}

async fn tagged(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    capture: &Capture,
    commit: &str,
) -> Result<Option<Capture>, GitRecordsError> {
    capture.check(original, admission)?;
    if let Some(pin) = crate::storage::read(original, capture.pin.at_git_commit(commit)).await?? {
        capture.check(original, admission)?;
        return Ok(Some(Capture {
            pin,
            ..capture.clone()
        }));
    }
    capture.check(original, admission)?;
    Ok(None)
}
#[allow(clippy::too_many_arguments)]
async fn prepare(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    module: GitRecordModule,
    capture: Option<Capture>,
    transition: &GitRecordsTransition,
    conflicts: &mut Vec<GitRecordConflict>,
    missing: &mut Vec<GitMissingSnapshot>,
    read_limits: ReadLimits,
) -> Result<Prepared, GitRecordsError> {
    let batch_rows = match read_limits {
        ReadLimits::Memory(limits) => limits.batch_rows.get(),
        ReadLimits::Task(limits) => limits.batch_rows.get(),
    };
    let Some(capture) = capture else {
        return match transition {
            GitRecordsTransition::Commit { commit }
            | GitRecordsTransition::Initialize { commit } => Ok(Prepared {
                module,
                expected: None,
                action: Action::Tag {
                    commit: commit.clone(),
                },
            }),
            _ => {
                let commits: Vec<&String> = match transition {
                    GitRecordsTransition::Checkout { commit, .. } => vec![commit],
                    GitRecordsTransition::Merge { base, theirs } => vec![base, theirs],
                    GitRecordsTransition::Commit { .. }
                    | GitRecordsTransition::Initialize { .. } => unreachable!(),
                };
                missing.extend(commits.into_iter().map(|commit| GitMissingSnapshot {
                    module,
                    commit: commit.clone(),
                }));
                Ok(blocked_plan(module, None))
            }
        };
    };
    let expected = Some(capture.pin.clone());
    match transition {
        GitRecordsTransition::Commit { commit } | GitRecordsTransition::Initialize { commit } => {
            if let Some(existing) = tagged(original, admission, &capture, commit).await? {
                if matches!(transition, GitRecordsTransition::Initialize { .. })
                    && let Some(branch) = capture.pin.inherited_target()
                {
                    return branch_plan(
                        original, admission, &capture, &existing, commit, branch, batch_rows,
                    )
                    .await;
                }
                if !matches!(transition, GitRecordsTransition::Initialize { .. })
                    && (existing.pin.physical_branch() != capture.pin.physical_branch()
                        || existing.pin.version() != capture.pin.version())
                {
                    let ours = index(original, admission, module, &capture, batch_rows).await?;
                    let saved = index(original, admission, module, &existing, batch_rows).await?;
                    conflicts.extend(changed(&ours, &saved).into_iter().map(|record_id| {
                        GitRecordConflict {
                            module,
                            record_id,
                            reason: "immutable_git_snapshot",
                        }
                    }));
                }
                return Ok(Prepared {
                    module,
                    expected,
                    action: Action::Noop {
                        report: report(
                            module,
                            GitRecordEffect::Unchanged,
                            &existing.pin,
                            Some(commit.clone()),
                        ),
                        check_working: false,
                    },
                });
            }
            Ok(Prepared {
                module,
                expected,
                action: Action::Tag {
                    commit: commit.clone(),
                },
            })
        }
        GitRecordsTransition::Checkout {
            commit,
            previous_commit,
        } => {
            let Some(target) = tagged(original, admission, &capture, commit).await? else {
                missing.push(GitMissingSnapshot {
                    module,
                    commit: commit.clone(),
                });
                return Ok(blocked_plan(module, expected));
            };
            if let Some(branch) = capture.pin.inherited_target() {
                return branch_plan(
                    original, admission, &capture, &target, commit, branch, batch_rows,
                )
                .await;
            }
            let ours = index(original, admission, module, &capture, batch_rows).await?;
            let target_index = index(original, admission, module, &target, batch_rows).await?;
            let changed_ids = changed(&ours, &target_index);
            if changed_ids.is_empty() {
                return make_delta(
                    original,
                    admission,
                    module,
                    &capture,
                    ours,
                    &target,
                    target_index,
                    vec![],
                    GitRecordEffect::Restored,
                    Some(commit.clone()),
                    batch_rows,
                )
                .await;
            }
            let clean = if capture.pin.inherited_target().is_some() {
                true
            } else if let Some(previous) = previous_commit {
                if let Some(previous_pin) = tagged(original, admission, &capture, previous).await? {
                    ours.public
                        == index(original, admission, module, &previous_pin, batch_rows)
                            .await?
                            .public
                } else {
                    missing.push(GitMissingSnapshot {
                        module,
                        commit: previous.clone(),
                    });
                    false
                }
            } else {
                false
            };
            if !clean {
                conflicts.extend(changed_ids.into_iter().map(|record_id| GitRecordConflict {
                    module,
                    record_id,
                    reason: "dirty_records",
                }));
                return Ok(blocked_plan(module, expected));
            }
            make_delta(
                original,
                admission,
                module,
                &capture,
                ours,
                &target,
                target_index,
                changed_ids,
                GitRecordEffect::Restored,
                Some(commit.clone()),
                batch_rows,
            )
            .await
        }
        GitRecordsTransition::Merge { base, theirs } => {
            let base_pin = tagged(original, admission, &capture, base).await?;
            let theirs_pin = tagged(original, admission, &capture, theirs).await?;
            if base_pin.is_none() {
                missing.push(GitMissingSnapshot {
                    module,
                    commit: base.clone(),
                });
            }
            if theirs_pin.is_none() {
                missing.push(GitMissingSnapshot {
                    module,
                    commit: theirs.clone(),
                });
            }
            let (Some(base_pin), Some(theirs_pin)) = (base_pin, theirs_pin) else {
                return Ok(blocked_plan(module, expected));
            };
            let ours = index(original, admission, module, &capture, batch_rows).await?;
            let base_index = index(original, admission, module, &base_pin, batch_rows).await?;
            let theirs_index = index(original, admission, module, &theirs_pin, batch_rows).await?;
            let mut selected = Vec::new();
            let ids = base_index
                .public
                .keys()
                .chain(ours.public.keys())
                .chain(theirs_index.public.keys())
                .collect::<std::collections::BTreeSet<_>>();
            for id in ids {
                original.check()?;
                let (base, ours, theirs) = (
                    base_index.public.get(id),
                    ours.public.get(id),
                    theirs_index.public.get(id),
                );
                if ours == theirs || theirs == base {
                    continue;
                }
                if ours == base {
                    selected.push(id.clone());
                } else {
                    conflicts.push(GitRecordConflict {
                        module,
                        record_id: id.clone(),
                        reason: "concurrent_record_edit",
                    });
                }
            }
            make_delta(
                original,
                admission,
                module,
                &capture,
                ours,
                &theirs_pin,
                theirs_index,
                selected,
                GitRecordEffect::Merged,
                None,
                batch_rows,
            )
            .await
        }
    }
}
/// Only derivative association metadata is retained, never source bodies or vectors.
async fn vector_metadata(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    capture: &Capture,
    batch_rows: u32,
) -> Result<BTreeMap<String, VectorRef>, GitRecordsError> {
    use futures::TryStreamExt;
    let project = match admission
        .project()
        .map_err(|error| invalid_auth(capture.module, error))?
        .scope
        .live
        .owner()
    {
        graphit_core::artifact_store::StoreOwner::Project { project_id } => *project_id,
        _ => return Err(invalid("Git records require PROJECT")),
    };
    let mut scan = capture.pin.scan();
    scan.filter("kind = 'embedding'")
        .map_err(BranchError::from)?
        .batch_size(batch_rows as usize)
        .fragment_readahead(1)
        .batch_readahead(1);
    capture.check(original, admission)?;
    let mut stream = crate::storage::read(original, scan.try_into_stream())
        .await?
        .map_err(BranchError::from)?;
    let mut vectors = BTreeMap::new();
    loop {
        capture.check(original, admission)?;
        let next = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(BranchError::from)?;
        capture.check(original, admission)?;
        let Some(batch) = next else { break };
        for row in 0..batch.num_rows() {
            original.check()?;
            let (_, vector) = match capture.module {
                GitRecordModule::Memory => {
                    memory_store::git::validate_row(original, project, &batch, row)?
                }
                GitRecordModule::Task => task_store::git::validate_row(original, &batch, row)?,
            };
            let key = text(&batch, "row_key", row)?.to_owned();
            if vectors
                .insert(key, vector.ok_or_else(|| invalid("Git embedding family"))?)
                .is_some()
            {
                return Err(invalid("duplicate Git embedding row"));
            }
        }
    }
    Ok(vectors)
}
async fn compatible(
    original: &Arc<SnapshotContext>,
    capture: &Capture,
    target: &Capture,
    vectors: &BTreeMap<String, VectorRef>,
) -> Result<std::collections::BTreeSet<String>, GitRecordsError> {
    let candidates = vectors
        .iter()
        .map(|(key, vector)| (key.clone(), vector))
        .collect();
    target.guard.revalidate(original)?;
    let result = match capture.module {
        GitRecordModule::Memory => {
            memory_store::git::compatible_vectors(
                original,
                &target.pin,
                &candidates,
                capture.limits,
            )
            .await
        }
        GitRecordModule::Task => {
            task_store::git::compatible_vectors(original, &target.pin, &candidates, capture.limits)
                .await
        }
    };
    target.guard.revalidate(original)?;
    result
}
#[allow(clippy::too_many_arguments)]
async fn branch_plan(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    capture: &Capture,
    target: &Capture,
    commit: &str,
    branch: &str,
    batch_rows: u32,
) -> Result<Prepared, GitRecordsError> {
    let vectors = vector_metadata(original, admission, capture, batch_rows).await?;
    let vector_keys = compatible(original, capture, target, &vectors).await?;
    capture.check(original, admission)?;
    Ok(Prepared {
        module: capture.module,
        expected: Some(capture.pin.clone()),
        action: Action::Branch {
            commit: commit.into(),
            branch: branch.into(),
            vector_source: capture.clone(),
            vector_keys,
        },
    })
}

#[allow(clippy::too_many_arguments)]
async fn make_delta(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    module: GitRecordModule,
    capture: &Capture,
    ours: Index,
    target: &Capture,
    theirs: Index,
    selected: Vec<String>,
    effect: GitRecordEffect,
    commit: Option<String>,
    batch_rows: u32,
) -> Result<Prepared, GitRecordsError> {
    if effect == GitRecordEffect::Restored
        && !selected.is_empty()
        && capture.pin.corpus_uri() == target.pin.corpus_uri()
        && capture.pin.physical_branch() == target.pin.physical_branch()
    {
        let vector_keys = compatible(original, capture, target, &ours.vectors).await?;
        capture.check(original, admission)?;
        return Ok(Prepared {
            module,
            expected: Some(capture.pin.clone()),
            action: Action::Restore {
                commit: commit.ok_or_else(|| invalid("Git restore commit"))?,
                vector_source: capture.clone(),
                vector_keys,
            },
        });
    }
    let mut source_keys = std::collections::BTreeSet::new();
    let mut delete_keys = std::collections::BTreeSet::new();
    let mut candidates = BTreeMap::new();
    for id in &selected {
        if let Some(keys) = theirs.keys.get(id) {
            source_keys.extend(keys.iter().cloned());
        }
        if let Some(keys) = ours.keys.get(id) {
            delete_keys.extend(keys.iter().cloned());
            for key in keys {
                if let Some(vector) = ours.vectors.get(key) {
                    candidates.insert(key.clone(), vector);
                }
            }
        }
    }
    // Derivative publication alone is neither a user edit nor a reason to redo inference.
    let other_candidates = theirs
        .vectors
        .iter()
        .filter(|(key, _)| !ours.vectors.contains_key(*key) && !source_keys.contains(*key))
        .map(|(key, value)| (key.clone(), value))
        .collect();
    let other_compatible = match module {
        GitRecordModule::Memory => {
            memory_store::git::compatible_vectors(
                original,
                &capture.pin,
                &other_candidates,
                capture.limits,
            )
            .await?
        }
        GitRecordModule::Task => {
            task_store::git::compatible_vectors(
                original,
                &capture.pin,
                &other_candidates,
                capture.limits,
            )
            .await?
        }
    };
    source_keys.extend(other_compatible);
    let keep_keys = match module {
        GitRecordModule::Memory => {
            memory_store::git::compatible_vectors(
                original,
                &target.pin,
                &candidates,
                capture.limits,
            )
            .await?
        }
        GitRecordModule::Task => {
            task_store::git::compatible_vectors(original, &target.pin, &candidates, capture.limits)
                .await?
        }
    };
    target.check(original, admission)?;
    // Prefer already calculated compatible vectors from the working branch,
    // including when the tagged snapshot carries an older model at the same key.
    for key in &keep_keys {
        source_keys.remove(key);
    }
    capture.check(original, admission)?;
    if source_keys.is_empty() && delete_keys.is_empty() && keep_keys.is_empty() {
        return Ok(Prepared {
            module,
            expected: Some(capture.pin.clone()),
            action: Action::Noop {
                report: report(module, GitRecordEffect::Unchanged, &capture.pin, commit),
                check_working: true,
            },
        });
    }
    if module == GitRecordModule::Task {
        task_store::git::validate_delta(
            original,
            &capture.pin,
            &target.pin,
            &delete_keys,
            &source_keys,
            batch_rows,
        )
        .await?;
    }
    target.check(original, admission)?;
    capture.check(original, admission)?;
    Ok(Prepared {
        module,
        expected: Some(capture.pin.clone()),
        action: Action::Delta {
            source: target.clone(),
            retained_guard: Arc::clone(&capture.guard),
            source_keys,
            keep_keys,
            delete_keys,
            effect,
            commit,
            vector_schema: target
                .pin
                .schema()
                .field_with_name("embedding_vector")
                .is_ok(),
        },
    })
}

pub(crate) fn check_expected(
    current: &BranchSnapshot,
    prepared: &Prepared,
) -> Result<(), GitRecordsError> {
    // Native tags refer to the captured immutable snapshot, not the latest
    // working version. State-changing transitions still require that version.
    if let Some(expected) = prepared.expected.as_ref()
        && (current.corpus_uri() != expected.corpus_uri()
            || (current.version() != expected.version()
                && !matches!(&prepared.action, Action::Tag { .. }))
            || current.physical_branch() != expected.physical_branch()
            || current.inherited_target() != expected.inherited_target())
    {
        return Err(invalid("Git records changed during transition preparation"));
    }
    Ok(())
}

fn key_expression(
    keys: &std::collections::BTreeSet<String>,
) -> graphit_data::lance_branch_store::Expr {
    use graphit_data::lance_branch_store::{col, lit};
    col("row_key").in_list(keys.iter().map(|key| lit(key.as_str())).collect(), false)
}
async fn patch_stream(
    original: &Arc<SnapshotContext>,
    pin: &BranchSnapshot,
    guard: Arc<crate::storage::Guard>,
    keys: std::collections::BTreeSet<String>,
    schema: Arc<arrow_schema::Schema>,
) -> Result<graphit_data::lance_module_store::SendableRecordBatchStream, GitRecordsError> {
    use futures::StreamExt;
    use graphit_data::lance_module_store::{DataFusionError, RecordBatchStreamAdapter};
    if keys.is_empty() {
        return Ok(Box::pin(RecordBatchStreamAdapter::new(
            Arc::clone(&schema),
            futures::stream::iter(vec![Ok(RecordBatch::new_empty(schema))]),
        )));
    }
    guard.revalidate(original)?;
    let mut scan = pin.scan();
    scan.filter_expr(key_expression(&keys));
    scan.fragment_readahead(1).batch_readahead(1);
    let stream = crate::storage::read(original, scan.try_into_stream())
        .await?
        .map_err(BranchError::from)?;
    let request = Arc::clone(original);
    let output_schema = Arc::clone(&schema);
    let stream = stream.map(move |result| {
        guard
            .revalidate(&request)
            .map_err(|error| DataFusionError::External(Box::new(error)))?;
        let batch = result.map_err(|error| DataFusionError::External(Box::new(error)))?;
        let mut columns = Vec::with_capacity(output_schema.fields().len());
        for field in output_schema.fields() {
            columns.push(
                batch
                    .column_by_name(field.name())
                    .cloned()
                    .unwrap_or_else(|| {
                        arrow_array::new_null_array(field.data_type(), batch.num_rows())
                    }),
            );
        }
        RecordBatch::try_new(Arc::clone(&output_schema), columns).map_err(DataFusionError::from)
    });
    Ok(Box::pin(RecordBatchStreamAdapter::new(schema, stream)))
}

/// Add only validated, compatible derivative rows after an exact native restore
/// or shallow branch. Immutable commit tags and all public rows remain untouched.
async fn graft_vectors(
    original: &Arc<SnapshotContext>,
    current: &mut BranchSnapshot,
    source: &Capture,
    keys: &std::collections::BTreeSet<String>,
    before: &impl Fn() -> Result<(), GitRecordsError>,
    partial: &mut Vec<GitModuleReport>,
    module: GitRecordModule,
) -> Result<(), GitRecordsError> {
    if keys.is_empty() {
        return Ok(());
    }
    before()?;
    if current
        .schema()
        .field_with_name("embedding_vector")
        .is_err()
    {
        let field = source
            .pin
            .schema()
            .field_with_name("embedding_vector")?
            .clone();
        *current = current
            .add_nullable_columns(Arc::new(arrow_schema::Schema::new(vec![field])), original)
            .await?;
        partial.push(report(
            module,
            GitRecordEffect::SchemaExtended,
            current,
            None,
        ));
    }
    before()?;
    let stream = patch_stream(
        original,
        &source.pin,
        Arc::clone(&source.guard),
        keys.clone(),
        Arc::new(current.schema().clone()),
    )
    .await?;
    let pending = crate::storage::read(
        original,
        current.prepare_merge_stream(stream, vec!["row_key".into()], None),
    )
    .await??;
    before()?;
    source.guard.revalidate(original)?;
    *current = pending.commit().await.map_err(|cause| GitRecordsError {
        cause: GitRecordsCause::Delta(cause),
        partial: vec![],
    })?;
    Ok(())
}

pub(crate) async fn perform(
    original: &Arc<SnapshotContext>,
    current: &mut BranchSnapshot,
    prepared: &Prepared,
    before: impl Fn() -> Result<(), GitRecordsError>,
) -> Result<GitModuleReport, GitRecordsError> {
    use futures::StreamExt;
    before()?;
    check_expected(current, prepared)?;
    let module = prepared.module;
    let mut partial = Vec::new();
    // SDK merge planning has a large concrete future. Keep this operation on
    // the heap rather than embedding it in every caller's async state/stack.
    let result = Box::pin(async {
        match &prepared.action {
            Action::Noop { report, .. } => Ok(report.clone()),
            Action::Tag { commit } => {
                let source_pin = prepared.expected.as_ref().unwrap_or(current);
                let source_pin = if source_pin.inherited_target().is_some() {
                    before()?;
                    let pin = source_pin.clone().materialize().await?;
                    partial.push(report(module, GitRecordEffect::Branched, &pin, None));
                    pin
                } else {
                    source_pin.clone()
                };
                before()?;
                let tagged = source_pin.tag_git_commit(commit).await?;
                Ok(report(
                    module,
                    GitRecordEffect::Tagged,
                    &tagged,
                    Some(commit.clone()),
                ))
            }
            Action::Branch {
                commit,
                branch,
                vector_source,
                vector_keys,
            } => {
                before()?;
                *current = current.materialize_git_branch(branch, commit).await?;
                partial.push(report(
                    module,
                    GitRecordEffect::Branched,
                    current,
                    Some(commit.clone()),
                ));
                graft_vectors(
                    original,
                    current,
                    vector_source,
                    vector_keys,
                    &before,
                    &mut partial,
                    module,
                )
                .await?;
                Ok(report(
                    module,
                    GitRecordEffect::Branched,
                    current,
                    Some(commit.clone()),
                ))
            }
            Action::Restore {
                commit,
                vector_source,
                vector_keys,
            } => {
                before()?;
                let pending =
                    crate::storage::read(original, current.prepare_restore_git_commit(commit))
                        .await??;
                before()?;
                *current = pending.commit().await.map_err(|cause| GitRecordsError {
                    cause: GitRecordsCause::Delta(cause),
                    partial: vec![],
                })?;
                partial.push(report(
                    module,
                    GitRecordEffect::Restored,
                    current,
                    Some(commit.clone()),
                ));
                graft_vectors(
                    original,
                    current,
                    vector_source,
                    vector_keys,
                    &before,
                    &mut partial,
                    module,
                )
                .await?;
                Ok(report(
                    module,
                    GitRecordEffect::Restored,
                    current,
                    Some(commit.clone()),
                ))
            }
            Action::Delta {
                source,
                retained_guard,
                source_keys,
                keep_keys,
                delete_keys,
                effect,
                commit,
                ..
            } => {
                if current.inherited_target().is_some() {
                    before()?;
                    *current = current.clone().materialize().await?;
                    partial.push(report(module, GitRecordEffect::Branched, current, None));
                }
                let schema = Arc::new(current.schema().clone());
                let incoming = patch_stream(
                    original,
                    &source.pin,
                    Arc::clone(&source.guard),
                    source_keys.clone(),
                    Arc::clone(&schema),
                )
                .await?;
                let retained = patch_stream(
                    original,
                    current,
                    Arc::clone(retained_guard),
                    keep_keys.clone(),
                    Arc::clone(&schema),
                )
                .await?;
                let stream = Box::pin(
                    graphit_data::lance_module_store::RecordBatchStreamAdapter::new(
                        schema,
                        incoming.chain(retained),
                    ),
                );
                let pending = crate::storage::read(
                    original,
                    current.prepare_merge_stream(
                        stream,
                        vec!["row_key".into()],
                        (!delete_keys.is_empty()).then(|| key_expression(delete_keys)),
                    ),
                )
                .await??;
                before()?;
                source.guard.revalidate(original)?;
                retained_guard.revalidate(original)?;
                let next = pending.commit().await.map_err(|cause| GitRecordsError {
                    cause: GitRecordsCause::Delta(cause),
                    partial: vec![],
                })?;
                *current = next;
                Ok(report(module, *effect, current, commit.clone()))
            }
        }
    })
    .await;
    result.map_err(|mut error: GitRecordsError| {
        error.partial.splice(0..0, partial);
        error
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_schema::{DataType, Field, Schema};
    use futures::TryStreamExt;
    use graphit_core::file_lock::Cancellation;
    use graphit_data::lance_module_store::LanceModuleStore;
    use std::time::{Duration, Instant};

    fn image(value: &str) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Utf8, false)]));
        RecordBatch::try_new(schema, vec![Arc::new(StringArray::from(vec![value]))]).unwrap()
    }

    async fn value(snapshot: &BranchSnapshot) -> String {
        let batches: Vec<RecordBatch> = snapshot
            .scan()
            .try_into_stream()
            .await
            .unwrap()
            .try_collect()
            .await
            .unwrap();
        assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
        batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
            .value(0)
            .to_owned()
    }

    #[tokio::test]
    async fn captured_git_tags_survive_head_updates_without_replacing_live_rows() {
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        for module in [GitRecordModule::Memory, GitRecordModule::Task] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("records.lance");
            let uri = path.to_str().unwrap();
            let initial = image("captured");
            LanceModuleStore::create(uri, initial.schema(), vec![initial])
                .await
                .unwrap();
            let captured = BranchSnapshot::capture_observed(uri, None, None)
                .await
                .unwrap();
            // A normal writer advances this dataset after Git captured its
            // snapshot, while the other module can still be publishing its tag.
            let mut current = captured
                .prepare_overwrite(vec![image("later write")])
                .await
                .unwrap()
                .commit()
                .await
                .unwrap();
            let live_version = current.version();
            assert_ne!(captured.version(), live_version);
            let prepared = Prepared {
                module,
                expected: Some(captured.clone()),
                action: Action::Tag {
                    commit: "0123456789012345678901234567890123456789".into(),
                },
            };
            let report = perform(&original, &mut current, &prepared, || {
                original.check().map_err(GitRecordsError::from)
            })
            .await
            .unwrap();
            assert_eq!(report.effect, GitRecordEffect::Tagged);
            assert_eq!(report.version, captured.version());
            let tagged = current
                .at_git_commit("0123456789012345678901234567890123456789")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(value(&tagged).await, "captured");
            let live = BranchSnapshot::capture_observed(uri, None, None)
                .await
                .unwrap();
            assert_eq!(live.version(), live_version);
            assert_eq!(value(&live).await, "later write");
            // Other actions retain their exact-version guard. Only publishing
            // an immutable captured tag is independent of the working head.
            let strict = Prepared {
                module,
                expected: Some(captured),
                action: Action::Noop {
                    report,
                    check_working: true,
                },
            };
            assert!(check_expected(&live, &strict).is_err());
        }
    }
}
