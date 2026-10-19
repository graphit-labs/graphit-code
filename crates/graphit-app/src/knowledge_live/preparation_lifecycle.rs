//! Preparation failures keep their real workspace custody and their primary cause.
use super::{
    Access, EventData, FailureCode, Session, SessionState,
    runner::io_failure,
    service::{LiveError, LiveSessionService, SessionSlot, append},
};
use graphit_core::native_document_file::NativeDocumentJournalEffect;
use std::{
    io,
    sync::{Arc, Mutex},
};

pub(super) fn combined(
    left: NativeDocumentJournalEffect,
    right: NativeDocumentJournalEffect,
) -> NativeDocumentJournalEffect {
    use NativeDocumentJournalEffect::{Applied, Unchanged, Unknown};
    match (left, right) {
        (Unknown, _) | (_, Unknown) => Unknown,
        (Applied, _) | (_, Applied) => Applied,
        _ => Unchanged,
    }
}
impl LiveSessionService {
    /// Root may report resolver/renderer failure without replacing its primary cause.
    pub fn fail_preparation(
        &self,
        access: &Access<'_>,
        session: &Session,
        cause: LiveError,
    ) -> LiveError {
        match self.history_slot(access, &session.id) {
            Ok(slot) => self.fail_preparation_in_slot(
                access,
                &session.id,
                &session.cursor.generation,
                &slot,
                cause,
            ),
            Err(error) => {
                let effect = cause.effect();
                LiveError::Preparation {
                    cause: Box::new(cause),
                    persistence: Some(Box::new(error.into())),
                    effect,
                }
            }
        }
    }
    pub(super) fn fail_preparation_in_slot(
        &self,
        access: &Access<'_>,
        id: &str,
        generation: &str,
        slot: &Arc<Mutex<SessionSlot>>,
        cause: LiveError,
    ) -> LiveError {
        // A rejected active/closed operation is not a failed preparation attempt.
        if matches!(cause.primary_cause(), LiveError::Busy | LiveError::Closed) {
            return cause;
        }
        let mut state = match slot.lock() {
            Ok(state) => state,
            Err(_) => {
                let effect = cause.effect();
                return LiveError::Preparation {
                    cause: Box::new(cause),
                    persistence: Some(Box::new(io::Error::other("Live custody lock").into())),
                    effect,
                };
            }
        };
        if state.generation.as_deref() != Some(generation)
            || state.active.is_some()
            || state.process.is_some()
        {
            return cause;
        }
        state.prepared_original = None;
        let code = io_failure(cause.kind());
        state.preparation_failure = Some(code);
        let result = self.write_preparation_failure(access, id, &mut state, code);
        state.changed.send_replace(());
        let (observed, persistence) = match result {
            Ok(effect) => (effect, None),
            Err(error) => (error.effect(), Some(Box::new(error))),
        };
        let effect = combined(cause.effect(), observed);
        LiveError::Preparation {
            cause: Box::new(cause),
            persistence,
            effect,
        }
    }
    pub(super) fn persist_preparation_failure(
        &self,
        access: &Access<'_>,
        id: &str,
    ) -> Result<NativeDocumentJournalEffect, LiveError> {
        access.write()?;
        let slot = self.history_slot(access, id)?;
        let mut state = slot
            .lock()
            .map_err(|_| io::Error::other("Live custody lock"))?;
        let Some(code) = state.preparation_failure else {
            return Ok(NativeDocumentJournalEffect::Unchanged);
        };
        let result = self.write_preparation_failure(access, id, &mut state, code);
        state.changed.send_replace(());
        result
    }
    fn write_preparation_failure(
        &self,
        access: &Access<'_>,
        id: &str,
        state: &mut SessionSlot,
        code: FailureCode,
    ) -> Result<NativeDocumentJournalEffect, LiveError> {
        access.write()?;
        if state.active.is_some() || state.process.is_some() {
            return Err(LiveError::Busy);
        }
        let journal = self.journal(access)?;
        let stored = journal.stored(id, access.original().snapshot(), || access.read())?;
        if state.generation.as_deref() != Some(stored.session.cursor.generation.as_str()) {
            return Err(LiveError::Conflict);
        }
        if stored.session.state == SessionState::Closed {
            state.preparation_failure = None;
            return Ok(NativeDocumentJournalEffect::Unchanged);
        }
        let record = if let Some(workspace) = &state.workspace {
            let mut workspace = workspace
                .lock()
                .map_err(|_| io::Error::other("Live workspace lock"))?;
            workspace.rebind(access)?;
            Some(workspace.record(self.limits.native, access.original().snapshot())?)
        } else {
            None
        };
        let result = journal.change_with_effect(access.original().snapshot(), || access.write(), |sessions| {
            let stored = sessions.get_mut(id).ok_or(io::ErrorKind::NotFound)?;
            if stored.session.state == SessionState::Closed { return Ok(()); }
            if !stored.native_quiescent() { return Err(io::ErrorKind::WouldBlock.into()); }
            if state.generation.as_deref() != Some(stored.session.cursor.generation.as_str()) { return Err(io::ErrorKind::AlreadyExists.into()); }
            if let Some(record) = record {
                if stored.workspace.as_ref().is_some_and(|previous| previous != &record) { return Err(io::ErrorKind::AlreadyExists.into()); }
                stored.workspace = Some(record);
            }
            let already_recorded = stored.events.last().is_some_and(|event| event.turn_id.is_none() && matches!(event.data, EventData::State { state: SessionState::Failed }))
                && stored.events.iter().rev().nth(1).is_some_and(|event| event.turn_id.is_none() && matches!(event.data, EventData::Error { code: previous } if previous == code));
            if !already_recorded {
                append(stored, None, EventData::Error { code })?;
                append(stored, None, EventData::State { state: SessionState::Failed })?;
            }
            Ok(())
        });
        if result.is_err() {
            state.preparation_failure = Some(code);
        }
        let (_, effect) = result?;
        state.preparation_failure = None;
        Ok(effect)
    }
}
