//! Application session history. Every operation borrows a fresh real admission.
use super::{
    Access, CreateRequest, Event, EventData, KnownTerminal, Replay, ReplayCursor, Session,
    SessionState,
    journal::{self, Journal, StoredSession},
};
use graphit_core::{
    file_lock::Cancellation,
    native_document_file::{NativeDocumentFileLimits, NativeDocumentJournalEffect},
};
use std::{
    collections::BTreeMap,
    fmt, io,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

#[derive(Clone, Copy)]
pub(crate) struct HistoryLimits {
    pub native: NativeDocumentFileLimits,
    pub envelope_bytes: NonZeroUsize,
    pub id_bytes: NonZeroUsize,
    pub prompt_bytes: NonZeroUsize,
    pub event_bytes: NonZeroUsize,
}
impl HistoryLimits {
    /// Reuse Source's concrete JSON IO ceilings and Core's existing native limits.
    /// No session-count, retention-age or allocation-reservation quota is introduced.
    pub fn capture(
        native: NativeDocumentFileLimits,
        content: graphit_hub::native::ContentLimits,
    ) -> io::Result<Self> {
        let positive = |value| {
            NonZeroUsize::new(value).ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))
        };
        Ok(Self {
            native,
            envelope_bytes: positive(content.max_output_bytes)?,
            id_bytes: positive(content.max_scalar_bytes)?,
            prompt_bytes: positive(content.max_input_bytes)?,
            event_bytes: positive(content.max_output_bytes)?,
        })
    }
}

#[derive(Debug)]
pub(crate) enum LiveError {
    Io(io::Error),
    History(journal::Failure),
    Preparation {
        cause: Box<LiveError>,
        persistence: Option<Box<LiveError>>,
        effect: NativeDocumentJournalEffect,
    },
    Busy,
    Conflict,
    Closed,
    NotReady,
}
impl From<io::ErrorKind> for LiveError {
    fn from(kind: io::ErrorKind) -> Self {
        Self::Io(kind.into())
    }
}
impl From<io::Error> for LiveError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<journal::Failure> for LiveError {
    fn from(error: journal::Failure) -> Self {
        Self::History(error)
    }
}
impl LiveError {
    pub fn kind(&self) -> io::ErrorKind {
        match self {
            Self::Io(e) => e.kind(),
            Self::History(e) => e.cause.kind(),
            Self::Preparation { cause, .. } => cause.kind(),
            Self::Busy => io::ErrorKind::WouldBlock,
            Self::Conflict => io::ErrorKind::AlreadyExists,
            Self::Closed | Self::NotReady => io::ErrorKind::InvalidInput,
        }
    }
    pub(crate) fn after_effect(cause: io::Error, effect: NativeDocumentJournalEffect) -> Self {
        Self::History(journal::Failure {
            cause,
            effect,
            secondary: None,
        })
    }
    pub(crate) fn retain_prior_effect(self, prior: NativeDocumentJournalEffect) -> Self {
        if prior == NativeDocumentJournalEffect::Unchanged
            || self.effect() != NativeDocumentJournalEffect::Unchanged
        {
            return self;
        }
        Self::after_effect(io::Error::new(self.kind(), self), prior)
    }
    /// Classify the domain cause without discarding effects or persistence failures.
    pub fn primary_cause(&self) -> &LiveError {
        match self {
            Self::Preparation { cause, .. } => cause.primary_cause(),
            Self::History(failure) => failure
                .cause
                .get_ref()
                .and_then(|cause| cause.downcast_ref::<LiveError>())
                .map_or(self, LiveError::primary_cause),
            _ => self,
        }
    }
    pub fn preparation_persistence_error(&self) -> Option<&LiveError> {
        match self {
            Self::Preparation { persistence, .. } => persistence.as_deref(),
            Self::History(failure) => failure
                .cause
                .get_ref()
                .and_then(|cause| cause.downcast_ref::<LiveError>())
                .and_then(LiveError::preparation_persistence_error),
            _ => None,
        }
    }
    /// Safe diagnostics preserve typed persistence failures without exposing native text.
    pub fn diagnostic(&self) -> ErrorDiagnostic {
        ErrorDiagnostic {
            kind: format!("{:?}", self.kind()),
            journal_effect: format!("{:?}", self.effect()),
            secondary_kind: self.secondary_kind().map(|kind| format!("{kind:?}")),
            persistence: self
                .preparation_persistence_error()
                .map(|error| Box::new(error.diagnostic())),
        }
    }
    fn secondary_kind(&self) -> Option<io::ErrorKind> {
        match self {
            Self::History(failure) => failure.secondary.or_else(|| {
                failure
                    .cause
                    .get_ref()
                    .and_then(|cause| cause.downcast_ref::<LiveError>())
                    .and_then(LiveError::secondary_kind)
            }),
            Self::Preparation { cause, .. } => cause.secondary_kind(),
            _ => None,
        }
    }
    pub fn effect(&self) -> NativeDocumentJournalEffect {
        match self {
            Self::History(e) => e.effect,
            Self::Preparation { effect, .. } => *effect,
            _ => NativeDocumentJournalEffect::Unchanged,
        }
    }
}
#[derive(serde::Serialize)]
pub(crate) struct ErrorDiagnostic {
    kind: String,
    journal_effect: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    secondary_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    persistence: Option<Box<ErrorDiagnostic>>,
}
impl fmt::Display for LiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Do not place native errors, private paths or credentials in public responses.
        write!(f, "Live {:?} ({:?})", self.kind(), self.effect())
    }
}
impl std::error::Error for LiveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::History(e) => Some(e),
            Self::Preparation { cause, .. } => Some(cause.as_ref()),
            _ => None,
        }
    }
}

pub(crate) struct Observation {
    pub replay: Replay,
    /// Subscribe before the journal observation. Changes carry no native payload.
    pub changed: watch::Receiver<()>,
    pub terminal: Option<KnownTerminal>,
}

pub(super) struct SessionSlot {
    pub(super) generation: Option<String>,
    pub changed: watch::Sender<()>,
    pub active: Option<Cancellation>,
    pub(super) preparing_original: Option<Arc<graphit_core::file_io::SnapshotContext>>,
    pub process: Option<Arc<crate::knowledge_kiro_process::KiroProcessOwner>>,
    pub workspace: Option<Arc<Mutex<super::workspace::Workspace>>>,
    pub prepared_original: Option<Arc<graphit_core::file_io::SnapshotContext>>,
    pub terminal: Option<KnownTerminal>,
    pub preparation_failure: Option<super::FailureCode>,
    pub native_session: Option<zeroize::Zeroizing<String>>,
    pub abandoned: bool,
}
impl SessionSlot {
    fn new() -> Self {
        let (changed, _) = watch::channel(());
        Self {
            generation: None,
            changed,
            active: None,
            preparing_original: None,
            process: None,
            workspace: None,
            prepared_original: None,
            terminal: None,
            preparation_failure: None,
            native_session: None,
            abandoned: false,
        }
    }
    pub(super) fn bind_generation(&mut self, generation: &str) -> io::Result<()> {
        if self.generation.as_deref() == Some(generation) {
            return Ok(());
        }
        if self.generation.is_some()
            && (self.active.is_some()
                || self.process.is_some()
                || self.workspace.is_some()
                || self.preparing_original.is_some())
        {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        self.terminal = None;
        self.preparation_failure = None;
        self.native_session = None;
        self.prepared_original = None;
        self.abandoned = false;
        self.generation = Some(generation.to_owned());
        Ok(())
    }
}

type SessionSlots = BTreeMap<(String, String), Arc<Mutex<SessionSlot>>>;

pub(crate) struct LiveSessionService {
    pub(super) limits: HistoryLimits,
    pub(super) slots: Mutex<SessionSlots>,
    pub(super) stop: Cancellation,
}
impl LiveSessionService {
    pub fn new(limits: HistoryLimits) -> Self {
        Self {
            limits,
            slots: Mutex::new(BTreeMap::new()),
            stop: Cancellation::default(),
        }
    }
    pub(super) fn journal(&self, access: &Access<'_>) -> io::Result<Journal> {
        access.read()?;
        Journal::capture(
            access.scope().clone(),
            Arc::clone(access.original().snapshot()),
            journal::Limits {
                native: self.limits.native,
                envelope_bytes: self.limits.envelope_bytes,
            },
        )
    }
    pub(super) fn slot(
        &self,
        access: &Access<'_>,
        id: &str,
    ) -> io::Result<Arc<Mutex<SessionSlot>>> {
        access.read()?;
        self.identifier(id)?;
        // This lookup key is data only; admission and the physical scope are checked on each IO.
        let owner = serde_json::to_string(access.scope().owner()).map_err(io::Error::other)?;
        let mut slots = self
            .slots
            .lock()
            .map_err(|_| io::Error::other("Live custody lock"))?;
        Ok(Arc::clone(
            slots
                .entry((owner, id.to_owned()))
                .or_insert_with(|| Arc::new(Mutex::new(SessionSlot::new()))),
        ))
    }
    pub(super) fn history_slot(
        &self,
        access: &Access<'_>,
        id: &str,
    ) -> io::Result<Arc<Mutex<SessionSlot>>> {
        access.read()?;
        self.identifier(id)?;
        let owner = serde_json::to_string(access.scope().owner()).map_err(io::Error::other)?;
        if let Some(slot) = self
            .slots
            .lock()
            .map_err(|_| io::Error::other("Live custody lock"))?
            .get(&(owner, id.to_owned()))
            .cloned()
        {
            return Ok(slot);
        }
        // An unknown read must not allocate a permanent custody entry. The final
        // observation still subscribes before reading, so this existence probe loses no event.
        self.journal(access)?
            .stored(id, access.original().snapshot(), || access.read())?;
        self.slot(access, id)
    }
    pub(super) fn identifier(&self, value: &str) -> io::Result<()> {
        if value.is_empty() || value.len() > self.limits.id_bytes.get() || value.contains('\0') {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(())
    }
    fn selection(&self, value: &super::ArtifactSelection) -> io::Result<()> {
        for field in [
            &value.source,
            &value.artifact_type,
            &value.id,
            &value.version,
        ] {
            self.identifier(field)?;
        }
        for field in [&value.project_id, &value.instance_id, &value.project_kind]
            .into_iter()
            .flatten()
        {
            self.identifier(field)?;
        }
        Ok(())
    }
    fn request(&self, value: &CreateRequest) -> io::Result<()> {
        self.identifier(&value.request_id)?;
        self.identifier(&value.project.project_id)?;
        self.identifier(&value.project.project_version)?;
        self.identifier(&value.model)?;
        self.selection(&value.agent)?;
        for selected in &value.artifacts {
            self.selection(selected)?;
        }
        if value
            .title
            .as_ref()
            .is_some_and(|s| s.len() > self.limits.id_bytes.get())
            || value
                .prompt
                .as_ref()
                .is_some_and(|s| s.len() > self.limits.prompt_bytes.get())
        {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        Ok(())
    }
    pub fn create(
        &self,
        access: &Access<'_>,
        request: CreateRequest,
    ) -> Result<Session, LiveError> {
        access.write()?;
        if self.stop.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.request(&request)?;
        let slot = self.slot(access, &request.request_id)?;
        let mut guard = slot
            .lock()
            .map_err(|_| io::Error::other("Live custody lock"))?;
        let journal = self.journal(access)?;
        if super::preparation_guard::preparation_matches(&guard, access).is_err()
            && journal
                .existing(&request.request_id, access.original().snapshot(), || {
                    access.read()
                })?
                .is_none()
        {
            return Err(LiveError::Busy);
        }
        let result = journal.change_with_effect(
            access.original().snapshot(),
            || access.write(),
            |sessions| {
                if let Some(existing) = sessions.get(&request.request_id) {
                    return if existing.request == request {
                        Ok(existing.session.clone())
                    } else {
                        Err(io::ErrorKind::AlreadyExists.into())
                    };
                }
                if super::preparation_guard::preparation_matches(&guard, access).is_err() {
                    return Err(io::Error::new(io::ErrorKind::WouldBlock, LiveError::Busy));
                }
                if guard.active.is_some() || guard.process.is_some() || guard.workspace.is_some() {
                    return Err(io::ErrorKind::WouldBlock.into());
                }
                let session = Session {
                    created_at: timestamp(),
                    updated_at: timestamp(),
                    id: request.request_id.clone(),
                    project: request.project.clone(),
                    binding: request.binding,
                    agent: request.agent.clone(),
                    artifacts: request.artifacts.clone(),
                    title: request.title.clone(),
                    model: request.model.clone(),
                    state: SessionState::Preparing,
                    cursor: ReplayCursor {
                        generation: generation()?,
                        sequence: 1,
                    },
                };
                sessions.insert(
                    session.id.clone(),
                    StoredSession {
                        events: vec![Event {
                            sequence: 1,
                            turn_id: None,
                            data: EventData::State {
                                state: SessionState::Preparing,
                            },
                        }],
                        session: session.clone(),
                        request,
                        native_session: None,
                        workspace: None,
                        effective_agent: None,
                        prepared_fingerprint: None,
                        read_contexts: None,
                    },
                );
                Ok(session)
            },
        );
        // Also wake after an ambiguous write so observers re-read the real conditional journal.
        guard.changed.send_replace(());
        let (session, effect) = result?;
        guard
            .bind_generation(&session.cursor.generation)
            .map_err(|error| LiveError::after_effect(error, effect))?;
        Ok(session)
    }
    pub fn list(&self, access: &Access<'_>) -> Result<Vec<Session>, LiveError> {
        Ok(self
            .journal(access)?
            .list(access.original().snapshot(), || access.read())?)
    }
    pub fn observe(
        &self,
        access: &Access<'_>,
        id: &str,
        cursor: Option<&ReplayCursor>,
    ) -> Result<Observation, LiveError> {
        let slot = self.history_slot(access, id)?;
        let mut guard = slot
            .lock()
            .map_err(|_| io::Error::other("Live custody lock"))?;
        let changed = guard.changed.subscribe();
        let replay =
            self.journal(access)?
                .replay(id, cursor, access.original().snapshot(), || access.read())?;
        guard.bind_generation(&replay.session.cursor.generation)?;
        let terminal = guard.terminal.clone();
        drop(guard);
        Ok(Observation {
            replay,
            changed,
            terminal,
        })
    }
    pub(super) fn change<T>(
        &self,
        access: &Access<'_>,
        id: &str,
        change: impl FnOnce(&mut StoredSession) -> io::Result<T>,
    ) -> Result<T, LiveError> {
        access.write()?;
        let slot = self.slot(access, id)?;
        let guard = slot
            .lock()
            .map_err(|_| io::Error::other("Live custody lock"))?;
        let result = self.journal(access)?.change(
            access.original().snapshot(),
            || access.write(),
            |sessions| change(sessions.get_mut(id).ok_or(io::ErrorKind::NotFound)?),
        );
        guard.changed.send_replace(());
        result.map_err(Into::into)
    }
    /// Composition fixtures use the same admission, limits and journal path.
    #[cfg(test)]
    pub(crate) fn emit_fixture(
        &self,
        access: &Access<'_>,
        id: &str,
        turn: Option<&str>,
        data: EventData,
    ) -> Result<Event, LiveError> {
        if matches!(
            data,
            EventData::State {
                state: SessionState::Ready | SessionState::Running
            }
        ) {
            return self.change(access, id, |stored| {
                super::tests::prepared_history_fixture(stored);
                append(stored, turn, data)
            });
        }
        self.emit(access, id, turn, data)
    }
    pub(super) fn emit(
        &self,
        access: &Access<'_>,
        id: &str,
        turn: Option<&str>,
        data: EventData,
    ) -> Result<Event, LiveError> {
        // The persisted envelope and individual delivery have separate concrete IO ceilings.
        let encoded = serde_json::to_vec(&data).map_err(io::Error::other)?;
        if encoded.len() > self.limits.event_bytes.get() {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        self.change(access, id, |stored| append(stored, turn, data))
    }
    pub(super) fn cancel_with_effect(
        &self,
        access: &Access<'_>,
        id: &str,
    ) -> Result<(bool, NativeDocumentJournalEffect), LiveError> {
        if self.cancel_preparation(access, id)? {
            return Ok((true, NativeDocumentJournalEffect::Unchanged));
        }
        let effect = self.persist_terminal_with_effect(access, id)?;
        let requested = self
            .cancel_active(access, id)
            .map_err(|error| error.retain_prior_effect(effect))?;
        Ok((requested, effect))
    }
    fn cancel_active(&self, access: &Access<'_>, id: &str) -> Result<bool, LiveError> {
        access.write()?;
        // A slot is not proof that history exists for this currently admitted owner.
        self.observe(access, id, None)?;
        let slot = self.slot(access, id)?;
        let guard = slot
            .lock()
            .map_err(|_| io::Error::other("Live custody lock"))?;
        access.write()?;
        if let Some(signal) = &guard.active {
            signal.cancel();
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub(super) fn close_with_effect(
        &self,
        access: &Access<'_>,
        id: &str,
    ) -> Result<(Session, NativeDocumentJournalEffect), LiveError> {
        if self.cancel_preparation(access, id)? {
            return Err(LiveError::Busy);
        }
        let prior = self.persist_terminal_with_effect(access, id)?;
        let (session, effect) = self
            .close_after_terminal(access, id)
            .map_err(|error| error.retain_prior_effect(prior))?;
        Ok((
            session,
            if effect == NativeDocumentJournalEffect::Unchanged {
                prior
            } else {
                effect
            },
        ))
    }
    fn close_after_terminal(
        &self,
        access: &Access<'_>,
        id: &str,
    ) -> Result<(Session, NativeDocumentJournalEffect), LiveError> {
        access.write()?;
        let slot = self.slot(access, id)?;
        let mut guard = slot
            .lock()
            .map_err(|_| io::Error::other("Live custody lock"))?;
        if let Some(original) = &guard.preparing_original {
            original.cancellation().cancel();
            return Err(LiveError::Busy);
        }
        if let Some(active) = &guard.active {
            active.cancel();
            if let Some(process) = &guard.process {
                process.stop();
            }
            return Err(LiveError::Busy);
        }
        if guard.process.is_some() {
            return Err(LiveError::Busy);
        }
        guard.prepared_original = None;
        let result = self.journal(access)?.change_with_effect(
            access.original().snapshot(),
            || access.write(),
            |sessions| {
                let stored = sessions.get_mut(id).ok_or(io::ErrorKind::NotFound)?;
                if !stored.native_quiescent() {
                    return Err(io::ErrorKind::WouldBlock.into());
                }
                if stored.session.state != SessionState::Closed {
                    append(
                        stored,
                        None,
                        EventData::State {
                            state: SessionState::Closed,
                        },
                    )?;
                }
                Ok(stored.session.clone())
            },
        );
        guard.changed.send_replace(());
        result.map_err(Into::into)
    }
    pub(super) fn delete_with_effect(
        &self,
        access: &Access<'_>,
        id: &str,
    ) -> Result<NativeDocumentJournalEffect, LiveError> {
        let (_, close_effect) = self.close_with_effect(access, id)?;
        let result = (|| {
            let slot = self.slot(access, id)?;
            let mut guard = slot
                .lock()
                .map_err(|_| io::Error::other("Live custody lock"))?;
            if guard.active.is_some()
                || guard.process.is_some()
                || guard.preparing_original.is_some()
            {
                return Err(LiveError::Busy);
            }
            let stored = self
                .journal(access)?
                .stored(id, access.original().snapshot(), || access.read())?;
            if stored.session.state != SessionState::Closed {
                return Err(LiveError::NotReady);
            }
            if !stored.native_quiescent() {
                return Err(LiveError::Busy);
            }
            if guard.workspace.is_none()
                && let Some(record) = stored.workspace.clone()
            {
                match super::workspace::Workspace::restore(access, record, self.limits.native) {
                    Ok(workspace) => guard.workspace = Some(Arc::new(Mutex::new(workspace))),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            if let Some(workspace) = &guard.workspace {
                workspace
                    .lock()
                    .map_err(|_| io::Error::other("Live workspace lock"))?
                    .cleanup(access.original().snapshot())?;
            }
            guard.workspace = None;
            let result = self.journal(access)?.change_with_effect(
                access.original().snapshot(),
                || access.write(),
                |sessions| {
                    let stored = sessions.get(id).ok_or(io::ErrorKind::NotFound)?;
                    if stored.session.state != SessionState::Closed {
                        return Err(io::ErrorKind::WouldBlock.into());
                    }
                    sessions.remove(id);
                    Ok(())
                },
            );
            if result.is_ok() {
                guard.terminal = None;
                guard.preparation_failure = None;
                guard.native_session = None;
                guard.prepared_original = None;
                guard.generation = None;
                guard.abandoned = false;
            }
            guard.changed.send_replace(());
            result.map(|(_, effect)| effect).map_err(Into::into)
        })();
        result.map_err(|error: LiveError| error.retain_prior_effect(close_effect))
    }
}

pub(super) fn append(
    stored: &mut StoredSession,
    turn: Option<&str>,
    data: EventData,
) -> io::Result<Event> {
    if stored.session.state == SessionState::Closed {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let sequence = stored
        .session
        .cursor
        .sequence
        .checked_add(1)
        .ok_or(io::ErrorKind::FileTooLarge)?;
    stored
        .events
        .try_reserve(1)
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    if let EventData::State { state } = &data {
        stored.session.state = *state;
    }
    let event = Event {
        sequence,
        turn_id: turn.map(str::to_owned),
        data,
    };
    stored.events.push(event.clone());
    stored.session.cursor.sequence = sequence;
    stored.session.updated_at = timestamp();
    Ok(event)
}
fn timestamp() -> String {
    chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
        .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
}
pub(super) fn generation() -> io::Result<String> {
    use std::fmt::Write;
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(io::Error::other)?;
    let mut value = String::with_capacity(32);
    for byte in random {
        write!(&mut value, "{byte:02x}").map_err(io::Error::other)?;
    }
    Ok(value)
}
