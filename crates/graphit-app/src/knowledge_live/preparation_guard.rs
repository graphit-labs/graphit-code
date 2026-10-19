//! One request owns preparation before create/resolution; captures remain authority-bearing SDK values.
use super::{
    Access, EventData, FailureCode, Session, SessionState,
    journal::StoredSession,
    service::{LiveError, LiveSessionService, SessionSlot, append},
    workspace::Workspace,
};
use graphit_core::{artifact_store::StoreOwner, file_io::SnapshotContext};
use std::{
    cell::Cell,
    io,
    sync::{Arc, Mutex},
};

pub(crate) struct PreparationGuard<'s> {
    service: &'s LiveSessionService,
    slot: Arc<Mutex<SessionSlot>>,
    original: Arc<SnapshotContext>,
    owner: StoreOwner,
    id: String,
    previous: Option<Session>,
    recovered_effect: Cell<graphit_core::native_document_file::NativeDocumentJournalEffect>,
}
impl LiveSessionService {
    /// Source retains this guard for the entire job, before create/observe and every resolver await.
    pub(crate) fn begin_preparation<'s>(
        &'s self,
        access: &Access<'_>,
        id: &str,
    ) -> Result<PreparationGuard<'s>, LiveError> {
        access.write()?;
        if self.stop.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let slot = self.slot(access, id)?;
        let mut state = slot
            .lock()
            .map_err(|_| io::Error::other("Live custody lock"))?;
        if state.preparing_original.is_some() || state.active.is_some() || state.process.is_some() {
            return Err(LiveError::Busy);
        }
        let previous = self
            .journal(access)?
            .existing(id, access.original().snapshot(), || access.read())?
            .map(|stored| stored.session);
        if let Some(session) = &previous {
            state.bind_generation(&session.cursor.generation)?;
        } else if state.generation.is_some() || state.workspace.is_some() {
            return Err(LiveError::Conflict);
        }
        access.write()?;
        state.preparing_original = Some(Arc::clone(access.original().snapshot()));
        state.changed.send_replace(());
        drop(state);
        Ok(PreparationGuard {
            service: self,
            slot,
            original: Arc::clone(access.original().snapshot()),
            owner: access.scope().owner().clone(),
            id: id.to_owned(),
            previous,
            recovered_effect: Cell::new(
                graphit_core::native_document_file::NativeDocumentJournalEffect::Unchanged,
            ),
        })
    }
    pub(super) fn existing_slot(
        &self,
        access: &Access<'_>,
        id: &str,
    ) -> Result<Option<Arc<Mutex<SessionSlot>>>, LiveError> {
        access.read()?;
        self.identifier(id)?;
        let owner = serde_json::to_string(access.scope().owner()).map_err(io::Error::other)?;
        Ok(self
            .slots
            .lock()
            .map_err(|_| io::Error::other("Live custody lock"))?
            .get(&(owner, id.to_owned()))
            .cloned())
    }
    pub(super) fn cancel_preparation(
        &self,
        access: &Access<'_>,
        id: &str,
    ) -> Result<bool, LiveError> {
        access.write()?;
        let Some(slot) = self.existing_slot(access, id)? else {
            return Ok(false);
        };
        let state = slot
            .lock()
            .map_err(|_| io::Error::other("Live custody lock"))?;
        if let Some(original) = &state.preparing_original {
            original.cancellation().cancel();
            return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn require_preparation_access(
        &self,
        access: &Access<'_>,
        id: &str,
    ) -> Result<(), LiveError> {
        access.write()?;
        if let Some(slot) = self.existing_slot(access, id)? {
            let state = slot
                .lock()
                .map_err(|_| io::Error::other("Live custody lock"))?;
            preparation_matches(&state, access)?;
        }
        Ok(())
    }
}
pub(super) fn preparation_matches(
    state: &SessionSlot,
    access: &Access<'_>,
) -> Result<(), LiveError> {
    if state
        .preparing_original
        .as_ref()
        .is_some_and(|original| !Arc::ptr_eq(original, access.original().snapshot()))
    {
        return Err(LiveError::Busy);
    }
    Ok(())
}
impl PreparationGuard<'_> {
    pub(crate) fn recovered_effect(
        &self,
    ) -> graphit_core::native_document_file::NativeDocumentJournalEffect {
        self.recovered_effect.get()
    }
    pub(crate) fn previous_session(&self) -> Option<&Session> {
        self.previous.as_ref()
    }
    fn check(&self, access: &Access<'_>, state: &SessionSlot) -> Result<(), LiveError> {
        access.write()?;
        if !Arc::ptr_eq(&self.original, access.original().snapshot())
            || &self.owner != access.scope().owner()
            || !state
                .preparing_original
                .as_ref()
                .is_some_and(|original| Arc::ptr_eq(original, &self.original))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if let Some(previous) = &self.previous
            && state.generation.as_deref() != Some(&previous.cursor.generation)
        {
            return Err(LiveError::Conflict);
        }
        Ok(())
    }
    /// Call only after create(request)/observe succeeded, preserving request equality before recovery.
    pub(crate) fn recover_interrupted(
        &self,
        access: &Access<'_>,
    ) -> Result<Option<Session>, LiveError> {
        {
            let state = self
                .slot
                .lock()
                .map_err(|_| io::Error::other("Live custody lock"))?;
            self.check(access, &state)?;
        }
        let prior = self
            .service
            .persist_terminal_with_effect(access, &self.id)
            .map_err(|error| error.retain_prior_effect(self.recovered_effect.get()))?;
        self.recovered_effect
            .set(super::preparation_lifecycle::combined(
                self.recovered_effect.get(),
                prior,
            ));
        self.recover_current(
            access,
            prior != graphit_core::native_document_file::NativeDocumentJournalEffect::Unchanged,
        )
        .map_err(|error| error.retain_prior_effect(self.recovered_effect.get()))
    }
    fn recover_current(
        &self,
        access: &Access<'_>,
        reconciled: bool,
    ) -> Result<Option<Session>, LiveError> {
        let mut state = self
            .slot
            .lock()
            .map_err(|_| io::Error::other("Live custody lock"))?;
        self.check(access, &state)?;
        let stored =
            self.service
                .journal(access)?
                .stored(&self.id, access.original().snapshot(), || access.read())?;
        if state.generation.as_deref() != Some(&stored.session.cursor.generation) {
            return Err(LiveError::Conflict);
        }
        if stored.session.state == SessionState::Closed {
            return Ok(None);
        }
        if !stored.native_quiescent() || state.active.is_some() || state.process.is_some() {
            return Err(LiveError::Busy);
        }
        if reconciled && stored.session.state == SessionState::Failed {
            return Ok(Some(stored.session));
        }
        if self.previous.is_none()
            || !matches!(
                stored.session.state,
                SessionState::Preparing | SessionState::Running
            )
        {
            return Ok(None);
        }
        let Some(record) = stored.workspace.clone() else {
            return if stored.session.state == SessionState::Preparing {
                Ok(None)
            } else {
                Err(LiveError::Busy)
            };
        };
        if let Some(workspace) = &state.workspace {
            let mut workspace = workspace
                .lock()
                .map_err(|_| io::Error::other("Live workspace lock"))?;
            workspace.rebind(access)?;
            if workspace.record(self.service.limits.native, access.original().snapshot())? != record
            {
                return Err(LiveError::Conflict);
            }
        } else {
            let workspace = Workspace::restore(access, record, self.service.limits.native)?;
            state.workspace = Some(Arc::new(Mutex::new(workspace)));
        }
        let result = self.service.journal(access)?.change_with_effect(
            access.original().snapshot(),
            || access.write(),
            |sessions| {
                let actual = sessions.get_mut(&self.id).ok_or(io::ErrorKind::NotFound)?;
                if actual.request != stored.request
                    || actual.session.cursor != stored.session.cursor
                {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        LiveError::Conflict,
                    ));
                }
                if !recoverable(actual) {
                    return Err(io::Error::new(io::ErrorKind::WouldBlock, LiveError::Busy));
                }
                append(
                    actual,
                    None,
                    EventData::Error {
                        code: FailureCode::Interrupted,
                    },
                )?;
                append(
                    actual,
                    None,
                    EventData::State {
                        state: SessionState::Failed,
                    },
                )?;
                Ok(actual.session.clone())
            },
        );
        state.prepared_original = None;
        state.changed.send_replace(());
        let (session, effect) = result?;
        self.recovered_effect
            .set(super::preparation_lifecycle::combined(
                self.recovered_effect.get(),
                effect,
            ));
        Ok(Some(session))
    }
}
fn recoverable(stored: &StoredSession) -> bool {
    matches!(
        stored.session.state,
        SessionState::Preparing | SessionState::Running
    ) && stored.workspace.is_some()
        && stored.native_quiescent()
}
impl Drop for PreparationGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        if state
            .preparing_original
            .as_ref()
            .is_some_and(|original| Arc::ptr_eq(original, &self.original))
        {
            state.preparing_original = None;
            state.changed.send_replace(());
        }
    }
}
