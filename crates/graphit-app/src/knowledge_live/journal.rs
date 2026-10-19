//! Live's JSON body over Core's existing private, conditional journal.
use super::wire::{CreateRequest, Event, EventData, Replay, ReplayCursor, Session, SessionState};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeContext, StoreOwner},
    file_io::SnapshotContext,
    native_document_file::{
        NativeDocumentFileLimits, NativeDocumentJournalEffect, NativeDocumentJournalFailure,
        NativeDocumentJournalWriteBounds, NativeDocumentNamespace,
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, io, num::NonZeroUsize, sync::Arc};

#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub native: NativeDocumentFileLimits,
    /// Whole Core envelope, including its header; an IO ceiling, not a reservation.
    pub envelope_bytes: NonZeroUsize,
}

#[derive(Debug)]
pub(crate) struct Failure {
    pub cause: io::Error,
    pub effect: NativeDocumentJournalEffect,
    pub secondary: Option<io::ErrorKind>,
}
impl Failure {
    fn before(cause: impl Into<io::Error>) -> Self {
        Self {
            cause: cause.into(),
            effect: NativeDocumentJournalEffect::Unchanged,
            secondary: None,
        }
    }
}
impl From<NativeDocumentJournalFailure> for Failure {
    fn from(error: NativeDocumentJournalFailure) -> Self {
        let effect = error.effect();
        let secondary = error.secondary_kind();
        Self {
            cause: io::Error::new(error.kind(), error),
            effect,
            secondary,
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
/// Mechanical IO only. Entry handlers retain and recheck their current Auth admission.
pub(super) struct Journal {
    scope: CapturedPublicationScope,
    original: Arc<SnapshotContext>,
    limits: Limits,
}
impl Journal {
    pub fn capture(
        scope: CapturedPublicationScope,
        original: Arc<SnapshotContext>,
        limits: Limits,
    ) -> io::Result<Self> {
        if !matches!(
            scope.owner(),
            StoreOwner::Installation { .. } | StoreOwner::User { .. }
        ) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let value = Self {
            scope,
            original,
            limits,
        };
        value.check(&value.original)?;
        Ok(value)
    }
    fn check(&self, current: &Arc<SnapshotContext>) -> io::Result<()> {
        if !Arc::ptr_eq(current, &self.original) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        current.check()?;
        self.scope
            .revalidate_bounded(&ScopeContext::from_snapshot(current))
    }
    fn parent(&self) -> io::Result<std::path::PathBuf> {
        let root = match self.scope.owner() {
            StoreOwner::Installation { .. } => self.scope.owner_root(),
            StoreOwner::User { .. } => self.scope.user_root()?,
            StoreOwner::Project { .. } => return Err(io::ErrorKind::PermissionDenied.into()),
        };
        Ok(root.join("knowledge-live"))
    }
    fn namespace(&self, current: &Arc<SnapshotContext>) -> io::Result<NativeDocumentNamespace> {
        self.check(current)?;
        NativeDocumentNamespace::capture(&self.scope, &self.parent()?, self.limits.native, current)
    }
    fn read_body(
        snapshot: &graphit_core::native_document_file::NativeDocumentJournalSnapshot,
    ) -> io::Result<Body> {
        let body = match snapshot.body()? {
            Some(raw) => serde_json::from_str(raw.get())
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
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
        self.check(current)?;
        access()?;
        match graphit_core::native_document_file::CapturedDocumentDirectory::capture(
            &self.parent()?,
            self.limits.native,
            current,
        ) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let result = inspect(&Body::empty())?;
                access()?;
                self.check(current)?;
                return Ok(result);
            }
            Err(error) => return Err(error),
        }
        let namespace = self.namespace(current)?;
        let guard = namespace.lock(current)?;
        access()?;
        let snapshot = guard.read_journal(self.limits.envelope_bytes.get())?;
        let body = Self::read_body(&snapshot)?;
        let result = inspect(&body)?;
        access()?;
        self.check(current)?;
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
    ) -> Result<(T, NativeDocumentJournalEffect), Failure> {
        self.check(current).map_err(Failure::before)?;
        access().map_err(Failure::before)?;
        let namespace = self.namespace(current).map_err(Failure::before)?;
        let guard = namespace.lock(current).map_err(Failure::before)?;
        access().map_err(Failure::before)?;
        let ceiling = self.limits.envelope_bytes.get();
        let before = guard.read_journal(ceiling).map_err(Failure::before)?;
        let mut body = Self::read_body(&before).map_err(Failure::before)?;
        let result = change(&mut body.sessions).map_err(Failure::before)?;
        body.validate().map_err(Failure::before)?;
        let mut writer = BoundedJson {
            bytes: Vec::new(),
            ceiling,
        };
        serde_json::to_writer(&mut writer, &body).map_err(|error| {
            Failure::before(io::Error::new(
                error.io_error_kind().unwrap_or(io::ErrorKind::InvalidData),
                error,
            ))
        })?;
        let text = String::from_utf8(writer.bytes)
            .map_err(|error| Failure::before(io::Error::new(io::ErrorKind::InvalidData, error)))?;
        let raw = serde_json::value::RawValue::from_string(text)
            .map_err(|error| Failure::before(io::Error::new(io::ErrorKind::InvalidData, error)))?;
        let bounds = NativeDocumentJournalWriteBounds::new(ceiling, ceiling, ceiling)
            .map_err(Failure::before)?;
        access().map_err(Failure::before)?;
        self.check(current).map_err(Failure::before)?;
        let effect = if before
            .body()
            .map_err(Failure::before)?
            .is_some_and(|old| old.get() == raw.get())
        {
            NativeDocumentJournalEffect::Unchanged
        } else {
            NativeDocumentJournalEffect::Applied
        };
        guard.replace_journal(&before, &raw, bounds)?;
        if let Err(cause) = access().and_then(|()| self.check(current)) {
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
