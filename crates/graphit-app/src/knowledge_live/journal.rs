//! Live session history, serialized as private JSON with atomic content replacement.
use super::wire::{CreateRequest, Event, EventData, Replay, ReplayCursor, Session, SessionState};
use graphit_core::{
    artifact_store::{PublicationScope, StoreOwner},
    file_io::SnapshotContext,
    file_lock::FileLock,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, File},
    io::{self, Read, Write},
    num::NonZeroUsize,
    path::PathBuf,
    sync::Arc,
};

#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub json_bytes: NonZeroUsize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum HistoryEffect {
    Unchanged,
    Applied,
}
#[derive(Debug)]
pub(crate) struct Failure {
    pub cause: io::Error,
    pub effect: HistoryEffect,
    pub secondary: Option<io::ErrorKind>,
}
impl Failure {
    fn before(cause: impl Into<io::Error>) -> Self {
        Self {
            cause: cause.into(),
            effect: HistoryEffect::Unchanged,
            secondary: None,
        }
    }
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Live history {:?} ({:?})",
            self.cause.kind(),
            self.effect
        )
    }
}
impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    version: u8,
    sessions: BTreeMap<String, StoredSession>,
}
impl Body {
    fn empty() -> Self {
        Self {
            version: 1,
            sessions: BTreeMap::new(),
        }
    }
    fn validate(&self) -> io::Result<()> {
        if self.version != 1 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        for (id, session) in &self.sessions {
            if id != &session.session.id
                || id != &session.request.request_id
                || session.session.project != session.request.project
                || session.session.binding != session.request.binding
                || session.session.agent != session.request.agent
                || session.session.artifacts != session.request.artifacts
                || session.session.title != session.request.title
                || session.session.model != session.request.model
                || session.session.cursor.generation.is_empty()
                || session.events.is_empty()
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
            if let Some(contexts) = &session.read_contexts {
                contexts.validate(&session.session)?;
            }
            let prepared = session.events.iter().any(|event| {
                matches!(
                    event.data,
                    EventData::State {
                        state: SessionState::Ready
                    }
                )
            });
            if session.prepared_fingerprint.is_some() != session.read_contexts.is_some()
                || (prepared && session.read_contexts.is_none())
                || (session.session.state == SessionState::Running
                    && session.read_contexts.is_none())
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
            let mut sequence = 0_u64;
            let mut state = SessionState::Preparing;
            for event in &session.events {
                sequence = sequence.checked_add(1).ok_or(io::ErrorKind::InvalidData)?;
                if event.sequence != sequence {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                if let EventData::State { state: next } = &event.data {
                    if state == SessionState::Closed && *next != SessionState::Closed {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    state = *next;
                }
            }
            if sequence != session.session.cursor.sequence || state != session.session.state {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        Ok(())
    }
}

/// Persisted values cannot restore a scope, admission, child or live workspace pin.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredSession {
    pub session: Session,
    pub request: CreateRequest,
    pub events: Vec<Event>,
    pub native_session: Option<String>,
    /// Private correlation data, never sufficient to restore workspace custody.
    pub workspace: Option<super::workspace::WorkspaceRecord>,
    pub effective_agent: Option<String>,
    pub prepared_fingerprint: Option<String>,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub read_contexts: Option<super::ReadContexts>,
}

impl StoredSession {
    /// A journal state name is not proof that the last native execution joined.
    pub(super) fn native_quiescent(&self) -> bool {
        let Some(prompt) = self
            .events
            .iter()
            .rev()
            .find(|event| matches!(event.data, EventData::Prompt { .. }))
        else {
            return self.session.state != SessionState::Running && self.native_session.is_none();
        };
        let Some(turn_id) = prompt.turn_id.as_ref() else {
            return false;
        };
        self.events
            .iter()
            .rev()
            .find(|event| {
                event.sequence > prompt.sequence
                    && event.turn_id.as_ref() == Some(turn_id)
                    && matches!(event.data, EventData::TurnDone { .. })
            })
            .is_some_and(|event| {
                matches!(
                    event.data,
                    EventData::TurnDone {
                        joined: true,
                        clean: true,
                        ..
                    }
                )
            })
    }
}
/// Entry handlers retain and recheck their real Auth admission for every operation.
pub(super) struct Journal {
    parent: PathBuf,
    limits: Limits,
}
impl Journal {
    pub fn new(scope: &PublicationScope, limits: Limits) -> io::Result<Self> {
        let root = match scope.owner() {
            StoreOwner::Installation { .. } => scope.owner_root(),
            StoreOwner::User { .. } => scope.user_root()?,
            StoreOwner::Project { .. } => return Err(io::ErrorKind::PermissionDenied.into()),
        };
        Ok(Self {
            parent: root.join("knowledge-live"),
            limits,
        })
    }
    pub(super) fn path(&self) -> PathBuf {
        self.parent.join("history.json")
    }
    fn bytes(&self, current: &SnapshotContext) -> io::Result<Option<Vec<u8>>> {
        current.check()?;
        // Inspect the selected file type before opening; special files are not JSON history.
        match fs::metadata(self.path()) {
            Ok(m) if !m.is_file() => return Err(io::ErrorKind::InvalidData.into()),
            Ok(_) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        }
        let mut file = File::open(self.path())?;
        graphit_fs::verify_private_file(&file)?;
        let mut output = Vec::new();
        let mut block = [0; 64 * 1024];
        loop {
            current.check()?;
            let count = file.read(&mut block)?;
            if count == 0 {
                break;
            }
            if count > self.limits.json_bytes.get().saturating_sub(output.len()) {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            output
                .try_reserve(count)
                .map_err(|_| io::ErrorKind::OutOfMemory)?;
            output.extend_from_slice(&block[..count]);
        }
        current.check()?;
        Ok(Some(output))
    }
    fn body(bytes: Option<&[u8]>) -> io::Result<Body> {
        let body = match bytes {
            Some(bytes) => serde_json::from_slice(bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?,
            None => Body::empty(),
        };
        body.validate()?;
        Ok(body)
    }
    fn read<T>(
        &self,
        current: &Arc<SnapshotContext>,
        access: impl Fn() -> io::Result<()>,
        inspect: impl FnOnce(&Body) -> io::Result<T>,
    ) -> io::Result<T> {
        current.check()?;
        access()?;
        let bytes = self.bytes(current)?;
        let result = inspect(&Self::body(bytes.as_deref())?)?;
        access()?;
        current.check()?;
        Ok(result)
    }
    pub fn list(
        &self,
        current: &Arc<SnapshotContext>,
        access: impl Fn() -> io::Result<()>,
    ) -> io::Result<Vec<Session>> {
        self.read(current, access, |body| {
            Ok(body
                .sessions
                .values()
                .map(|value| value.session.clone())
                .collect())
        })
    }
    pub fn replay(
        &self,
        id: &str,
        cursor: Option<&ReplayCursor>,
        current: &Arc<SnapshotContext>,
        access: impl Fn() -> io::Result<()>,
    ) -> io::Result<Replay> {
        self.read(current, access, |body| {
            let value = body.sessions.get(id).ok_or(io::ErrorKind::NotFound)?;
            if cursor.is_some_and(|cursor| cursor.generation != value.session.cursor.generation) {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let after = cursor.map_or(0, |cursor| cursor.sequence);
            Ok(Replay {
                session: value.session.clone(),
                events: value
                    .events
                    .iter()
                    .filter(|event| event.sequence > after)
                    .cloned()
                    .collect(),
            })
        })
    }
    pub fn existing(
        &self,
        id: &str,
        current: &Arc<SnapshotContext>,
        access: impl Fn() -> io::Result<()>,
    ) -> io::Result<Option<StoredSession>> {
        self.read(current, access, |body| Ok(body.sessions.get(id).cloned()))
    }
    pub fn stored(
        &self,
        id: &str,
        current: &Arc<SnapshotContext>,
        access: impl Fn() -> io::Result<()>,
    ) -> io::Result<StoredSession> {
        self.read(current, access, |body| {
            body.sessions
                .get(id)
                .cloned()
                .ok_or_else(|| io::ErrorKind::NotFound.into())
        })
    }
    pub fn change<T>(
        &self,
        current: &Arc<SnapshotContext>,
        access: impl Fn() -> io::Result<()>,
        change: impl FnOnce(&mut BTreeMap<String, StoredSession>) -> io::Result<T>,
    ) -> Result<T, Failure> {
        self.change_with_effect(current, access, change)
            .map(|(value, _)| value)
    }
    pub fn change_with_effect<T>(
        &self,
        current: &Arc<SnapshotContext>,
        access: impl Fn() -> io::Result<()>,
        change: impl FnOnce(&mut BTreeMap<String, StoredSession>) -> io::Result<T>,
    ) -> Result<(T, HistoryEffect), Failure> {
        current.check().map_err(Failure::before)?;
        access().map_err(Failure::before)?;
        fs::create_dir_all(&self.parent).map_err(Failure::before)?;
        // This lock serializes read-modify-write of actual history across services/processes.
        // Readers need only the complete file delivered by atomic replacement.
        let _lock = FileLock::acquire_in_existing_parent_until(
            &self.parent.join("history.lock"),
            current.deadline(),
            current.cancellation(),
        )
        .map_err(Failure::before)?;
        access().map_err(Failure::before)?;
        let before = self.bytes(current).map_err(Failure::before)?;
        let mut body = Self::body(before.as_deref()).map_err(Failure::before)?;
        let result = change(&mut body.sessions).map_err(Failure::before)?;
        body.validate().map_err(Failure::before)?;
        let mut writer = BoundedJson {
            bytes: Vec::new(),
            ceiling: self.limits.json_bytes.get(),
        };
        serde_json::to_writer(&mut writer, &body).map_err(|error| {
            Failure::before(io::Error::new(
                error.io_error_kind().unwrap_or(io::ErrorKind::InvalidData),
                error,
            ))
        })?;
        access().map_err(Failure::before)?;
        current.check().map_err(Failure::before)?;
        if self.bytes(current).map_err(Failure::before)? != before {
            return Err(Failure::before(io::ErrorKind::AlreadyExists));
        }
        let effect = if before.as_deref() == Some(writer.bytes.as_slice()) {
            HistoryEffect::Unchanged
        } else {
            let mut temporary = tempfile::Builder::new()
                .prefix(".live-history-")
                .make_in(&self.parent, graphit_fs::create_private_file)
                .map_err(Failure::before)?;
            for chunk in writer.bytes.chunks(64 * 1024) {
                current.check().map_err(Failure::before)?;
                temporary.write_all(chunk).map_err(Failure::before)?;
            }
            temporary.as_file().sync_all().map_err(Failure::before)?;
            // Detect uncoordinated edits to history content, without path/identity receipts.
            if self.bytes(current).map_err(Failure::before)? != before {
                return Err(Failure::before(io::ErrorKind::AlreadyExists));
            }
            access().map_err(Failure::before)?;
            current.check().map_err(Failure::before)?;
            temporary
                .into_temp_path()
                .persist(self.path())
                .map_err(|e| Failure::before(e.error))?;
            HistoryEffect::Applied
        };
        if let Err(cause) = access().and_then(|()| current.check()) {
            return Err(Failure {
                cause,
                effect,
                secondary: None,
            });
        }
        Ok((result, effect))
    }
}

struct BoundedJson {
    bytes: Vec<u8>,
    ceiling: usize,
}
impl io::Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|size| size > self.ceiling)
        {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    fn journal(parent: PathBuf) -> Journal {
        Journal {
            parent,
            limits: Limits {
                json_bytes: NonZeroUsize::new(1024).unwrap(),
            },
        }
    }
    #[test]
    fn unchanged_history_detects_an_uncoordinated_content_edit() {
        let temp = tempfile::tempdir().unwrap();
        let journal = journal(temp.path().to_owned());
        let current = Arc::new(SnapshotContext::service(Cancellation::default()));
        journal.change(&current, || Ok(()), |_| Ok(())).unwrap();
        let edited = br#"{"version":2,"sessions":{}}"#;
        let error = journal
            .change_with_effect(
                &current,
                || Ok(()),
                |_| {
                    fs::write(journal.path(), edited)?;
                    Ok(())
                },
            )
            .unwrap_err();
        assert_eq!(error.cause.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(error.effect, HistoryEffect::Unchanged);
        assert_eq!(fs::read(journal.path()).unwrap(), edited);
    }
    #[test]
    fn cancelled_history_write_does_not_create_a_namespace() {
        let temp = tempfile::tempdir().unwrap();
        let journal = journal(temp.path().join("history"));
        let current = Arc::new(SnapshotContext::service(Cancellation::default()));
        current.cancellation().cancel();
        let error = journal.change(&current, || Ok(()), |_| Ok(())).unwrap_err();
        assert_eq!(error.cause.kind(), io::ErrorKind::Interrupted);
        assert_eq!(error.effect, HistoryEffect::Unchanged);
        assert!(!journal.parent.exists());
    }
    #[test]
    fn json_limit_failure_keeps_the_previous_private_history() {
        let temp = tempfile::tempdir().unwrap();
        let mut journal = journal(temp.path().to_owned());
        let current = Arc::new(SnapshotContext::service(Cancellation::default()));
        journal.change(&current, || Ok(()), |_| Ok(())).unwrap();
        let before = fs::read(journal.path()).unwrap();
        graphit_fs::verify_private_file(&File::open(journal.path()).unwrap()).unwrap();
        journal.limits.json_bytes = NonZeroUsize::new(before.len() - 1).unwrap();
        let error = journal.change(&current, || Ok(()), |_| Ok(())).unwrap_err();
        assert_eq!(error.cause.kind(), io::ErrorKind::FileTooLarge);
        assert_eq!(error.effect, HistoryEffect::Unchanged);
        assert_eq!(fs::read(journal.path()).unwrap(), before);
    }
}
