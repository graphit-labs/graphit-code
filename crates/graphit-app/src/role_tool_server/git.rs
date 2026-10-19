//! Readonly Git SDK consumer under the run's actual InspectGit authority.
use super::{SelectedProject, State, catalog::Operation, error, parse, require_active};
use crate::task_server::request::{Credential, Origin};
use base64::{Engine, engine::general_purpose::STANDARD};
use gix::status::{Item, index_worktree, tree_index::TrackRenames};
use graphit_auth::{Permission, ProjectCapture, ScopeCapture, ScopeInput, VerifiedRoleGrant};
use graphit_core::{
    artifact_store::CapturedPublicationScope, file_io::SnapshotContext, file_lock::Cancellation,
};
use graphit_hub::{
    native::{ContentLimits, RoleCredentialReference},
    policy::RequestContext,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use tokio::sync::oneshot;

type Result = std::result::Result<Value, Value>;
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct StatusInput {}
#[derive(Deserialize, Default, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Comparison {
    #[default]
    Worktree,
    Staged,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct DiffInput {
    #[serde(default)]
    comparison: Comparison,
}

/// The existing jobs owner retains this future until the blocking SDK call joins.
#[expect(clippy::too_many_arguments)]
pub(super) async fn run<T: Send>(
    state: Arc<State>,
    selected: SelectedProject,
    operation: Operation,
    input: Option<Value>,
    credential: Credential,
    cancel: Cancellation,
    deadline: Instant,
    sender: &mut oneshot::Sender<T>,
) -> Result {
    let interrupt = Arc::new(AtomicBool::new(false));
    let owner = Arc::clone(&state);
    let original_cancel = cancel.clone();
    let flag = Arc::clone(&interrupt);
    let mut job = tokio::task::spawn_blocking(move || {
        execute(
            &owner,
            selected,
            operation,
            input,
            credential,
            &original_cancel,
            deadline,
            flag,
        )
    });
    tokio::select! {
        result = &mut job => return result.unwrap_or_else(|_| Err(error("git_job_failed"))),
        () = sender.closed() => (),
        () = state.stop.cancelled() => (),
        () = tokio::time::sleep_until(deadline.into()) => (),
    };
    cancel.cancel();
    interrupt.store(true, Ordering::SeqCst);
    // Never abort/detach the SDK worker or close its issuer before this join.
    job.await.unwrap_or_else(|_| Err(error("git_job_failed")))
}
#[expect(clippy::too_many_arguments)]
fn execute(
    state: &State,
    selected: SelectedProject,
    operation: Operation,
    input: Option<Value>,
    credential: Credential,
    cancel: &Cancellation,
    deadline: Instant,
    interrupt: Arc<AtomicBool>,
) -> Result {
    let original = RequestContext::new(state.task.content, cancel, deadline)
        .map_err(|_| error("request_cancelled_or_expired"))?;
    require_active(state, &selected)?;
    let (paths, id, version) = &selected;
    let project =
        CapturedPublicationScope::project(paths, *id, state.task.scope, original.snapshot())
            .map_err(|_| error("project_capture_failed"))?;
    let binding = project
        .binding()
        .map_err(|_| error("project_binding_failed"))?;
    let Origin::Role(reference) = &credential.origin else {
        return Err(error("role_credential_required"));
    };
    let reference = RoleCredentialReference::decode(
        reference.as_bytes(),
        state.task.content.max_input_bytes,
        state.task.role_reference,
        &original,
    )
    .map_err(|_| error("invalid_role_reference"))?;
    let scope = ScopeInput {
        installation: state.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &project,
                record: &binding,
            },
            version,
        }),
        providers: &[],
        binding: credential.role_binding.unwrap_or(state.task.binding),
        global_version: "",
    };
    let grant = state
        .auth
        .issuer()
        .verify(&credential.bearer, &reference, scope, &original)
        .map_err(|_| error("authentication_or_permission_denied"))?;
    grant
        .authorize(Permission::InspectGit)
        .map_err(|_| error("authentication_or_permission_denied"))?;
    #[cfg(test)]
    {
        let gate = state.gate.lock().unwrap().take();
        if let Some((started, release)) = gate {
            started.send(cancel.clone()).unwrap();
            let _ = release.blocking_recv();
        }
    }
    original.snapshot().check().map_err(failure)?;
    grant
        .authorize(Permission::InspectGit)
        .map_err(|_| error("authentication_or_permission_denied"))?;
    require_active(state, &selected)?;
    let comparison = match operation {
        Operation::GitStatus => {
            let _: StatusInput = parse(input)?;
            None
        }
        Operation::GitDiff => Some(parse::<DiffInput>(input)?.comparison),
        _ => return Err(error("unknown_tool")),
    };
    let read = Reader {
        grant: &grant,
        original: Arc::clone(original.snapshot()),
        interrupt,
        limits: state.task.content,
        root: realpath(paths.root()).map_err(failure)?,
    };
    read.check().map_err(failure)?;
    let mut repo = gix::open_opts(paths.root(), gix::open::Options::isolated())
        .map_err(|_| error("git_repository_unavailable"))?;
    read.check_root(&repo).map_err(failure)?;
    if realpath(
        repo.workdir()
            .ok_or_else(|| failure(io::ErrorKind::Unsupported.into()))?,
    )
    .map_err(failure)?
        != read.root
    {
        return Err(failure(io::ErrorKind::PermissionDenied.into()));
    }
    sanitize(&mut repo).map_err(failure)?;
    let result = match comparison {
        None => {
            let changes = scan(&repo, &read, 0).map_err(failure)?;
            let mut entries = Vec::new();
            for item in &changes {
                read.check().map_err(failure)?;
                grant
                    .authorize(Permission::InspectGit)
                    .map_err(|_| error("authentication_or_permission_denied"))?;
                let (phase, path, kind) = status_fact(item);
                if path.len() > read.limits.max_scalar_bytes {
                    return Err(failure(io::ErrorKind::InvalidInput.into()));
                }
                read.push(
                    &mut entries,
                    StatusRow {
                        phase,
                        path_base64: encode(path, &read).map_err(failure)?,
                        kind,
                        submodule: submodule_fact(item),
                        old_entry_kind: entry_kinds(item).0,
                        new_entry_kind: entry_kinds(item).1,
                    },
                )
                .map_err(failure)?;
            }
            render(
                &StatusOutput {
                    kind: "git_status",
                    object_format: format_name(&repo).map_err(failure)?,
                    entries: &entries,
                },
                &read,
            )
            .map_err(failure)?
        }
        Some(comparison) => diff(&repo, &read, comparison).map_err(failure)?,
    };
    #[cfg(test)]
    {
        let gate = state.before_delivery.lock().unwrap().take();
        if let Some((started, release)) = gate {
            started.send(cancel.clone()).unwrap();
            let _ = release.blocking_recv();
        }
    }
    read.check().map_err(failure)?;
    grant
        .authorize(Permission::InspectGit)
        .map_err(|_| error("authentication_or_permission_denied"))?;
    require_active(state, &selected)?;
    Ok(result)
}
fn failure(cause: io::Error) -> Value {
    serde_json::json!({"code":"git_inspection_failed", "io_kind":format!("{:?}", cause.kind())})
}
fn sdk(cause: impl Into<gix::Error>) -> io::Error {
    let cause = cause.into();
    let mut source: &(dyn std::error::Error + 'static) = &cause;
    let kind = loop {
        if let Some(error) = source.downcast_ref::<io::Error>() {
            break error.kind();
        }
        if let Some(next) = source.source() {
            source = next;
        } else {
            break io::ErrorKind::Other;
        }
    };
    io::Error::new(kind, cause)
}
fn realpath(path: &Path) -> io::Result<PathBuf> {
    gix::path::realpath(path).map_err(io::Error::other)
}
fn format_name(repo: &gix::Repository) -> io::Result<&'static str> {
    match repo.object_hash() {
        gix::hash::Kind::Sha1 => Ok("sha1"),
        gix::hash::Kind::Sha256 => Ok("sha256"),
        _ => Err(io::ErrorKind::Unsupported.into()),
    }
}
fn sanitize(repo: &mut gix::Repository) -> io::Result<()> {
    let mut config = repo.config_snapshot_mut();
    let ids: Vec<_> = config
        .sections_and_ids_by_name("filter")
        .into_iter()
        .flatten()
        .map(|(_, id)| id)
        .collect();
    for id in ids {
        config.remove_section_by_id(id);
    }
    let ids: Vec<_> = config
        .sections_and_ids_by_name("diff")
        .into_iter()
        .flatten()
        .map(|(_, id)| id)
        .collect();
    for id in ids {
        if let Some(mut section) = config.section_mut_by_id(id) {
            for key in ["command", "textconv"] {
                while section.remove(key).is_some() {}
            }
        }
    }
    config.commit().map_err(sdk)?;
    Ok(())
}
#[derive(Clone)]
struct Reader<'a> {
    grant: &'a VerifiedRoleGrant<'a>,
    original: Arc<SnapshotContext>,
    interrupt: Arc<AtomicBool>,
    limits: ContentLimits,
    root: PathBuf,
}
impl Reader<'_> {
    fn check(&self) -> io::Result<()> {
        self.original.check()?;
        self.grant
            .authorize(Permission::InspectGit)
            .map(|_| ())
            .map_err(|_| io::Error::from(io::ErrorKind::PermissionDenied))
    }
    fn check_root(&self, repo: &gix::Repository) -> io::Result<()> {
        self.check()?;
        let root = realpath(repo.workdir().ok_or(io::ErrorKind::Unsupported)?)?;
        if !root.starts_with(&self.root) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        self.check()
    }
    fn push<T>(&self, values: &mut Vec<T>, value: T) -> io::Result<()> {
        self.check()?;
        if values.len() >= self.limits.max_collection_entries {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        values
            .try_reserve(1)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        values.push(value);
        Ok(())
    }
}
struct Collect<'a> {
    read: &'a Reader<'a>,
    items: Vec<Item>,
    first: Option<io::Error>,
}
impl<'index> gix::status::plumbing::index_as_worktree_with_renames::VisitEntry<'index>
    for Collect<'_>
{
    type ContentChange = ();
    type SubmoduleStatus = gix::submodule::Status;
    fn visit_entry(
        &mut self,
        entry: gix::status::plumbing::index_as_worktree_with_renames::Entry<
            'index,
            (),
            gix::submodule::Status,
        >,
    ) {
        if self.first.is_some() {
            return;
        }
        let item = index_worktree::Item::from(entry);
        if item.summary().is_none() {
            return;
        }
        if let Err(cause) = self.read.push(&mut self.items, Item::IndexWorktree(item)) {
            self.first = Some(cause);
            self.read.interrupt.store(true, Ordering::SeqCst);
        }
    }
}
#[derive(Clone)]
struct Submodules<'a> {
    repo: gix::ThreadSafeRepository,
    read: Reader<'a>,
    depth: usize,
}
impl gix::status::plumbing::index_as_worktree::traits::SubmoduleStatus for Submodules<'_> {
    type Output = gix::submodule::Status;
    fn status(
        &mut self,
        _: &gix::index::Entry,
        path: &gix::bstr::BStr,
    ) -> gix::Result<Option<Self::Output>> {
        self.read.check().map_err(gix::Error::from_error)?;
        let repo = self.repo.to_thread_local();
        let Some(mut modules) = repo.submodules()? else {
            return Ok(None);
        };
        let Some(module) = modules.find(|module| module.path().is_ok_and(|p| p == path)) else {
            return Ok(None);
        };
        let relative = gix::path::from_bstr(&module.path()?)
            .map_err(gix::Error::from_error)?
            .into_owned();
        if relative.is_absolute()
            || relative
                .components()
                .any(|p| matches!(p, std::path::Component::ParentDir))
        {
            return Err(gix::Error::from_error(io::Error::from(
                io::ErrorKind::PermissionDenied,
            )));
        }
        let candidate = realpath(
            &repo
                .workdir()
                .ok_or_else(|| gix::Error::from_error(io::Error::from(io::ErrorKind::Unsupported)))?
                .join(relative),
        )
        .map_err(gix::Error::from_error)?;
        if !candidate.starts_with(&self.read.root) {
            return Err(gix::Error::from_error(io::Error::from(
                io::ErrorKind::PermissionDenied,
            )));
        }
        self.read.check().map_err(gix::Error::from_error)?;
        // This native SDK call returns before its high-level nested status iterator.
        let mut status = module.status(gix::submodule::config::Ignore::Dirty, false)?;
        if status.state.worktree_checkout
            && status.state.repository_exists
            && let Some(mut nested) = module.open()?
        {
            self.read
                .check_root(&nested)
                .map_err(gix::Error::from_error)?;
            sanitize(&mut nested).map_err(gix::Error::from_error)?;
            status.changes = Some(
                scan(
                    &nested,
                    &self.read,
                    self.depth.checked_add(1).ok_or_else(|| {
                        gix::Error::from_error(io::Error::from(io::ErrorKind::InvalidInput))
                    })?,
                )
                .map_err(gix::Error::from_error)?,
            );
            self.read
                .check_root(&nested)
                .map_err(gix::Error::from_error)?;
        }
        self.read.check().map_err(gix::Error::from_error)?;
        Ok(status.is_dirty().and_then(|dirty| dirty.then_some(status)))
    }
}
fn scan(repo: &gix::Repository, read: &Reader<'_>, depth: usize) -> io::Result<Vec<Item>> {
    read.check()?;
    if depth > read.limits.max_depth {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let index = repo.index_or_empty().map_err(sdk)?;
    let mut collect = Collect {
        read,
        items: Vec::new(),
        first: None,
    };
    let options = gix::status::index_worktree::Options {
        dirwalk_options: Some(
            repo.dirwalk_options()
                .map_err(sdk)?
                .emit_untracked(gix::dir::walk::EmissionMode::CollapseDirectory),
        ),
        rewrites: None,
        ..Default::default()
    };
    let result = repo.index_worktree_status(
        &index,
        Vec::<gix::bstr::BString>::new(),
        &mut collect,
        gix::status::plumbing::index_as_worktree::traits::FastEq,
        Submodules {
            repo: repo.clone().into_sync(),
            read: read.clone(),
            depth,
        },
        &mut gix::progress::Discard,
        &read.interrupt,
        options,
    );
    if let Some(cause) = collect.first {
        return Err(cause);
    }
    read.check()?;
    result.map_err(sdk)?;
    let mut first = None;
    let result = repo.tree_index_status(
        &repo.head_tree_id_or_empty().map_err(sdk)?,
        &index,
        None,
        TrackRenames::Disabled,
        |change, _, _| match read.push(&mut collect.items, Item::TreeIndex(change.into_owned())) {
            Ok(()) => Ok(std::ops::ControlFlow::Continue(())),
            Err(cause) => {
                first = Some(cause);
                Ok(std::ops::ControlFlow::Break(()))
            }
        },
    );
    if let Some(cause) = first {
        return Err(cause);
    }
    read.check()?;
    result.map_err(sdk)?;
    Ok(collect.items)
}
#[derive(Serialize)]
struct StatusRow {
    phase: &'static str,
    path_base64: String,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    submodule: Option<SubmoduleFact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_entry_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_entry_kind: Option<&'static str>,
}
#[derive(Serialize)]
struct SubmoduleFact {
    index_id: Option<String>,
    checked_out_head_id: Option<String>,
    worktree_changed: bool,
}
fn submodule_fact(item: &Item) -> Option<SubmoduleFact> {
    if let Item::IndexWorktree(index_worktree::Item::Modification {
        status:
            gix::status::plumbing::index_as_worktree::EntryStatus::Change(
                gix::status::plumbing::index_as_worktree::Change::SubmoduleModification(status),
            ),
        ..
    }) = item
    {
        Some(SubmoduleFact {
            index_id: status.index_id.map(|id| id.to_string()),
            checked_out_head_id: status.checked_out_head_id.map(|id| id.to_string()),
            worktree_changed: status
                .changes
                .as_ref()
                .is_some_and(|changes| !changes.is_empty()),
        })
    } else {
        None
    }
}
#[derive(Serialize)]
struct StatusOutput<'a> {
    kind: &'static str,
    object_format: &'static str,
    entries: &'a [StatusRow],
}
fn status_fact(item: &Item) -> (&'static str, &[u8], &'static str) {
    match item {
        Item::IndexWorktree(item) => (
            "index_worktree",
            item.rela_path().as_ref(),
            match item.summary() {
                Some(gix::status::plumbing::index_as_worktree_with_renames::Summary::Added) => {
                    "untracked"
                }
                Some(gix::status::plumbing::index_as_worktree_with_renames::Summary::Removed) => {
                    "deleted"
                }
                Some(gix::status::plumbing::index_as_worktree_with_renames::Summary::Conflict) => {
                    "conflict"
                }
                Some(
                    gix::status::plumbing::index_as_worktree_with_renames::Summary::TypeChange,
                ) => "type_changed",
                _ => "modified",
            },
        ),
        Item::TreeIndex(change) => (
            "tree_index",
            change.location().as_ref(),
            match change {
                gix::diff::index::ChangeRef::Addition { .. } => "added",
                gix::diff::index::ChangeRef::Deletion { .. } => "deleted",
                gix::diff::index::ChangeRef::Modification {
                    previous_entry_mode,
                    entry_mode,
                    ..
                } if previous_entry_mode
                    .to_tree_entry_mode()
                    .map(|mode| mode.kind())
                    != entry_mode.to_tree_entry_mode().map(|mode| mode.kind()) =>
                {
                    "type_changed"
                }
                _ => "modified",
            },
        ),
    }
}
fn entry_kinds(item: &Item) -> (Option<&'static str>, Option<&'static str>) {
    let name = |mode: gix::index::entry::Mode| {
        mode.to_tree_entry_mode().map(|m| match m.kind() {
            gix::objs::tree::EntryKind::Blob => "blob",
            gix::objs::tree::EntryKind::BlobExecutable => "blob_executable",
            gix::objs::tree::EntryKind::Link => "link",
            gix::objs::tree::EntryKind::Commit => "commit",
            gix::objs::tree::EntryKind::Tree => "tree",
        })
    };
    match item {
        Item::TreeIndex(change) => match change {
            gix::diff::index::ChangeRef::Addition { entry_mode, .. } => (None, name(*entry_mode)),
            gix::diff::index::ChangeRef::Deletion { entry_mode, .. } => (name(*entry_mode), None),
            gix::diff::index::ChangeRef::Modification {
                previous_entry_mode,
                entry_mode,
                ..
            } => (name(*previous_entry_mode), name(*entry_mode)),
            _ => (None, None),
        },
        _ => (None, None),
    }
}
fn encode(bytes: &[u8], read: &Reader<'_>) -> io::Result<String> {
    read.check()?;
    let size = base64::encoded_len(bytes.len(), true).ok_or(io::ErrorKind::InvalidInput)?;
    if size > read.limits.max_output_bytes {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut output = String::new();
    output
        .try_reserve_exact(size)
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    STANDARD.encode_string(bytes, &mut output);
    read.check()?;
    Ok(output)
}
struct Writer<'a, W> {
    read: &'a Reader<'a>,
    bytes: W,
    written: usize,
    first: Option<io::Error>,
}
impl<W: Write> Write for Writer<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.first.is_some() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let result = (|| {
            self.read.check()?;
            if self
                .written
                .checked_add(bytes.len())
                .is_none_or(|n| n > self.read.limits.max_output_bytes)
            {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            self.bytes.write_all(bytes)?;
            self.written += bytes.len();
            Ok(bytes.len())
        })();
        if let Err(cause) = result {
            self.first = Some(cause);
            return Err(io::ErrorKind::InvalidData.into());
        }
        result
    }
    fn flush(&mut self) -> io::Result<()> {
        self.read.check()
    }
}
fn render(value: &impl Serialize, read: &Reader<'_>) -> io::Result<Value> {
    let mut writer = Writer {
        read,
        bytes: io::sink(),
        written: 0,
        first: None,
    };
    let result = serde_json::to_writer(&mut writer, value);
    if let Some(cause) = writer.first {
        return Err(cause);
    }
    result.map_err(io::Error::other)?;
    read.check()?;
    let value = serde_json::to_value(value).map_err(io::Error::other)?;
    read.check()?;
    Ok(value)
}
#[derive(Serialize)]
struct Patch {
    path_base64: String,
    kind: &'static str,
    old_bytes: usize,
    new_bytes: usize,
    hunks_base64: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    gitlink_old_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gitlink_new_id: Option<String>,
}
#[derive(Serialize)]
struct DiffOutput<'a> {
    kind: &'static str,
    object_format: &'static str,
    comparison: Comparison,
    patches: &'a [Patch],
}
fn diff(repo: &gix::Repository, read: &Reader<'_>, comparison: Comparison) -> io::Result<Value> {
    read.check()?;
    let index = repo.index_or_empty().map_err(sdk)?;
    if index
        .entries()
        .iter()
        .any(|entry| entry.stage() != gix::index::entry::Stage::Unconflicted)
    {
        return Err(io::ErrorKind::Unsupported.into());
    }
    let mut patches = Vec::new();
    match comparison {
        Comparison::Worktree => {
            for item in scan(repo, read, 0)? {
                let Item::IndexWorktree(index_worktree::Item::Modification {
                    entry,
                    rela_path,
                    status,
                    ..
                }) = item
                else {
                    continue;
                };
                read.check()?;
                if matches!(
                    status,
                    gix::status::plumbing::index_as_worktree::EntryStatus::Conflict { .. }
                ) {
                    return Err(io::ErrorKind::Unsupported.into());
                }
                let mode = entry
                    .mode
                    .to_tree_entry_mode()
                    .ok_or(io::ErrorKind::Unsupported)?
                    .kind();
                use gix::status::plumbing::index_as_worktree::{Change, EntryStatus};
                let new_mode = match &status {
                    EntryStatus::Change(Change::Type { worktree_mode }) => worktree_mode
                        .to_tree_entry_mode()
                        .ok_or(io::ErrorKind::Unsupported)?
                        .kind(),
                    _ => mode,
                };
                let new = match &status {
                    EntryStatus::Change(Change::SubmoduleModification(value)) => value
                        .checked_out_head_id
                        .ok_or(io::ErrorKind::Unsupported)?,
                    _ => gix::hash::ObjectId::null(repo.object_hash()),
                };
                let patch = patch(
                    repo,
                    read,
                    rela_path.as_ref(),
                    entry.id,
                    mode,
                    new,
                    new_mode,
                    true,
                )?;
                read.push(&mut patches, patch)?;
            }
        }
        Comparison::Staged => {
            let mut first = None;
            let result = repo.tree_index_status(
                &repo.head_tree_id_or_empty().map_err(sdk)?,
                &index,
                None,
                TrackRenames::Disabled,
                |change, _, _| {
                    let value = (|| {
                        use gix::diff::index::ChangeRef::*;
                        let null = gix::hash::ObjectId::null(repo.object_hash());
                        let (old, old_mode, new, new_mode) = match &change {
                            Addition { id, entry_mode, .. } => {
                                (null, *entry_mode, id.as_ref().to_owned(), *entry_mode)
                            }
                            Deletion { id, entry_mode, .. } => {
                                (id.as_ref().to_owned(), *entry_mode, null, *entry_mode)
                            }
                            Modification {
                                previous_id,
                                previous_entry_mode,
                                id,
                                entry_mode,
                                ..
                            } => (
                                previous_id.as_ref().to_owned(),
                                *previous_entry_mode,
                                id.as_ref().to_owned(),
                                *entry_mode,
                            ),
                            Rewrite { .. } => return Err(io::ErrorKind::Unsupported.into()),
                        };
                        read.push(
                            &mut patches,
                            patch(
                                repo,
                                read,
                                change.location(),
                                old,
                                old_mode
                                    .to_tree_entry_mode()
                                    .ok_or(io::ErrorKind::Unsupported)?
                                    .kind(),
                                new,
                                new_mode
                                    .to_tree_entry_mode()
                                    .ok_or(io::ErrorKind::Unsupported)?
                                    .kind(),
                                false,
                            )?,
                        )
                    })();
                    match value {
                        Ok(()) => Ok(std::ops::ControlFlow::Continue(())),
                        Err(cause) => {
                            first = Some(cause);
                            Ok(std::ops::ControlFlow::Break(()))
                        }
                    }
                },
            );
            if let Some(cause) = first {
                return Err(cause);
            }
            read.check()?;
            result.map_err(sdk)?;
        }
    }
    render(
        &DiffOutput {
            kind: "git_diff",
            object_format: format_name(repo)?,
            comparison,
            patches: &patches,
        },
        read,
    )
}
#[allow(clippy::too_many_arguments)]
fn patch(
    repo: &gix::Repository,
    read: &Reader<'_>,
    path: &gix::bstr::BStr,
    old: gix::hash::ObjectId,
    old_mode: gix::objs::tree::EntryKind,
    new: gix::hash::ObjectId,
    new_mode: gix::objs::tree::EntryKind,
    worktree: bool,
) -> io::Result<Patch> {
    read.check()?;
    if path.len() > read.limits.max_scalar_bytes {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    // Gitlinks are commit identity changes, not blob patches.
    if old_mode == gix::objs::tree::EntryKind::Commit
        || new_mode == gix::objs::tree::EntryKind::Commit
    {
        if old_mode != new_mode {
            return Err(io::ErrorKind::Unsupported.into());
        }
        return Ok(Patch {
            path_base64: encode(path.as_ref(), read)?,
            kind: "gitlink",
            old_bytes: 0,
            new_bytes: 0,
            hunks_base64: String::new(),
            gitlink_old_id: Some(old.to_string()),
            gitlink_new_id: Some(new.to_string()),
        });
    }
    let mut cache = repo
        .diff_resource_cache(
            gix::diff::blob::pipeline::Mode::ToGit,
            gix::diff::blob::pipeline::WorktreeRoots {
                old_root: None,
                new_root: worktree.then(|| repo.workdir().unwrap().to_owned()),
            },
        )
        .map_err(sdk)?;
    cache
        .set_resource(
            old,
            old_mode,
            path,
            gix::diff::blob::ResourceKind::OldOrSource,
            repo,
        )
        .map_err(sdk)?;
    read.check()?;
    cache
        .set_resource(
            new,
            new_mode,
            path,
            gix::diff::blob::ResourceKind::NewOrDestination,
            repo,
        )
        .map_err(sdk)?;
    read.check()?;
    let prepared = cache.prepare_diff().map_err(sdk)?;
    let size = |data: gix::diff::blob::platform::resource::Data<'_>| -> io::Result<usize> {
        match data {
            gix::diff::blob::platform::resource::Data::Missing => Ok(0),
            gix::diff::blob::platform::resource::Data::Buffer { buf, .. } => Ok(buf.len()),
            gix::diff::blob::platform::resource::Data::Binary { size } => {
                usize::try_from(size).map_err(|_| io::ErrorKind::InvalidInput.into())
            }
        }
    };
    let old_bytes = size(prepared.old.data)?;
    let new_bytes = size(prepared.new.data)?;
    if old_bytes > read.limits.max_input_bytes || new_bytes > read.limits.max_input_bytes {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let (kind, hunks) = match prepared.operation {
        gix::diff::blob::platform::prepare_diff::Operation::InternalDiff { algorithm } => {
            let input = prepared.interned_input();
            let diff = gix::diff::blob::diff_with_slider_heuristics(algorithm, &input);
            read.check()?;
            let mut writer = Writer {
                read,
                bytes: Vec::new(),
                written: 0,
                first: None,
            };
            let delegate =
                gix::diff::blob::unified_diff::ConsumeBinaryHunk::new(Hunks(&mut writer), "\n");
            let result =
                gix::diff::blob::UnifiedDiff::new(&diff, &input, delegate, Default::default())
                    .consume()
                    .map(|_| ());
            if let Some(cause) = writer.first.take() {
                return Err(cause);
            }
            result?;
            ("text", writer.bytes)
        }
        gix::diff::blob::platform::prepare_diff::Operation::SourceOrDestinationIsBinary => {
            ("binary", Vec::new())
        }
        gix::diff::blob::platform::prepare_diff::Operation::ExternalCommand { .. } => {
            return Err(io::ErrorKind::Unsupported.into());
        }
    };
    read.check()?;
    Ok(Patch {
        path_base64: encode(path.as_ref(), read)?,
        kind,
        old_bytes,
        new_bytes,
        hunks_base64: encode(&hunks, read)?,
        gitlink_old_id: None,
        gitlink_new_id: None,
    })
}
struct Hunks<'a, 'b>(&'a mut Writer<'b, Vec<u8>>);
impl gix::diff::blob::unified_diff::ConsumeBinaryHunkDelegate for Hunks<'_, '_> {
    fn consume_binary_hunk(
        &mut self,
        _: gix::diff::blob::unified_diff::HunkHeader,
        header: &str,
        hunk: &[u8],
    ) -> io::Result<()> {
        self.0.write_all(header.as_bytes())?;
        self.0.write_all(hunk)
    }
}
