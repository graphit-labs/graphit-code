use super::{
    Access, EventData, FailureCode, KnownTerminal, SendRequest, SessionState, TurnOutcome,
    service::{LiveError, LiveSessionService, SessionSlot, append},
    sink::LiveSink,
};
use crate::knowledge_ask_service::AskService;
use agent_client_protocol::schema::v1::{LoadSessionRequest, SessionId};
use graphit_ai::cli_kiro::{KiroEngine, KiroReason, KiroSessionStart};
use std::{
    io,
    sync::{Arc, Mutex},
};

pub(crate) struct TurnReceipt {
    pub persistence_error: Option<LiveError>,
}

struct TurnGuard {
    slot: Arc<Mutex<SessionSlot>>,
    finished: bool,
}
impl Drop for TurnGuard {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let mut slot = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        slot.abandoned = true;
        if let Some(cancel) = &slot.active {
            cancel.cancel();
        }
        if let Some(owner) = &slot.process {
            owner.stop();
        }
        slot.changed.send_replace(());
    }
}
impl LiveSessionService {
    /// The Source owner runs this future independently of every replay subscriber.
    /// `allowed` retains its actual PROJECT/installed-context admissions for this Original.
    pub async fn run_turn(
        &self,
        access: &Access<'_>,
        id: &str,
        input: &SendRequest,
        service: &AskService,
        allowed: &(dyn Fn() -> bool + Sync),
        launch: Option<&crate::live_agent_tools::LiveMcpLaunch<'_>>,
    ) -> Result<TurnReceipt, LiveError> {
        access.write()?;
        if self.stop.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.identifier(&input.turn_id)?;
        if input.text.is_empty() || input.text.len() > self.limits.prompt_bytes.get() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if !allowed() {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let slot = self.slot(access, id)?;
        let mut custody = TurnGuard {
            slot: Arc::clone(&slot),
            finished: true,
        };
        let (workspace, model, agent, native_session) = {
            let mut state = slot
                .lock()
                .map_err(|_| io::Error::other("Live custody lock"))?;
            super::preparation_guard::preparation_matches(&state, access)?;
            if let Some(terminal) = &state.terminal
                && terminal.turn_id == input.turn_id
            {
                let replay =
                    self.journal(access)?
                        .replay(id, None, access.original().snapshot(), || access.read())?;
                let previous = replay.events.iter().find(|e| {
                    e.turn_id.as_deref() == Some(&input.turn_id)
                        && matches!(e.data, EventData::Prompt { .. })
                });
                if previous.is_some_and(
                    |e| matches!(&e.data, EventData::Prompt { text } if text == &input.text),
                ) {
                    return Ok(TurnReceipt {
                        persistence_error: None,
                    });
                }
                return Err(LiveError::Conflict);
            }
            if state.active.is_some() {
                return Err(LiveError::Busy);
            }
            if !state
                .prepared_original
                .as_ref()
                .is_some_and(|original| Arc::ptr_eq(original, access.original().snapshot()))
            {
                return Err(LiveError::NotReady);
            }
            let workspace = Arc::clone(state.workspace.as_ref().ok_or(LiveError::NotReady)?);
            workspace
                .lock()
                .map_err(|_| io::Error::other("Live workspace lock"))?
                .revalidate(access.original().snapshot())?;
            // Journal validation and rejection precede any change to live custody.
            let journal = self.journal(access)?;
            let values = match journal.change(
                access.original().snapshot(),
                || access.write(),
                |sessions| {
                    let stored = sessions.get_mut(id).ok_or(io::ErrorKind::NotFound)?;
                    if stored
                        .events
                        .iter()
                        .any(|e| e.turn_id.as_deref() == Some(&input.turn_id))
                    {
                        return Err(io::ErrorKind::AlreadyExists.into());
                    }
                    if stored.session.state != SessionState::Ready {
                        return Err(io::ErrorKind::WouldBlock.into());
                    }
                    let agent = stored
                        .effective_agent
                        .clone()
                        .ok_or(io::ErrorKind::InvalidData)?;
                    let values = (
                        stored.session.model.clone(),
                        agent,
                        stored.native_session.clone(),
                    );
                    append(
                        stored,
                        Some(&input.turn_id),
                        EventData::State {
                            state: SessionState::Running,
                        },
                    )?;
                    append(
                        stored,
                        Some(&input.turn_id),
                        EventData::Prompt {
                            text: input.text.clone(),
                        },
                    )?;
                    Ok(values)
                },
            ) {
                Ok(values) => values,
                Err(failure) => {
                    if failure.effect
                        != graphit_core::native_document_file::NativeDocumentJournalEffect::Unchanged
                    {
                        // A possibly published Running attempt retains custody.
                        state.active = Some(access.original().cancellation().clone());
                        state.terminal = None;
                        state.abandoned = false;
                        custody.finished = false;
                        state.changed.send_replace(());
                    }
                    return Err(failure.into());
                }
            };
            state.active = Some(access.original().cancellation().clone());
            state.terminal = None;
            state.abandoned = false;
            custody.finished = false;
            state.changed.send_replace(());
            (workspace, values.0, values.1, values.2)
        };
        let cwd = workspace
            .lock()
            .map_err(|_| io::Error::other("Live workspace lock"))?
            .path()
            .to_owned();
        let mut private_output = zeroize::Zeroizing::new(vec![cwd.to_string_lossy().into_owned()]);
        if let Some(launch) = launch {
            private_output.extend(launch.private_output().iter().cloned());
        }
        let mut sink = LiveSink::new(self, access, id, &input.turn_id, allowed, &private_output);
        let process = service
            .kiro_process(&cwd, access.original().snapshot(), None, allowed)
            .await;
        let (execution, owner) = match process {
            Ok((mut transport, owner)) => {
                slot.lock()
                    .map_err(|_| io::Error::other("Live custody lock"))?
                    .process = Some(Arc::clone(&owner));
                if let Some(launch) = launch
                    && let Err(error) =
                        launch.apply_to(access.original().snapshot(), &mut transport)
                {
                    // No transport poll occurred: release its native slot before
                    // publishing the known NotSpawned result. No child is assumed joined.
                    drop(transport);
                    owner.stop();
                    let terminal = KnownTerminal {
                        turn_id: input.turn_id.clone(),
                        outcome: if access.original().cancellation().is_cancelled() {
                            TurnOutcome::Cancelled
                        } else {
                            TurnOutcome::Failed
                        },
                        error: Some(io_failure(error.kind())),
                        joined: true,
                        clean: true,
                        persisted: false,
                    };
                    return self.finish_turn(access, id, &slot, &mut custody, terminal, None);
                }
                let mut start = match native_session {
                    Some(id) => {
                        KiroSessionStart::Load(LoadSessionRequest::new(SessionId::new(id), &cwd))
                    }
                    None => service.kiro_new_session(&cwd, &model),
                };
                if service.kiro_engine() == Some(KiroEngine::V3) {
                    let meta = serde_json::json!({"kiro":{"modelId":model,"modeId":agent}})
                        .as_object()
                        .expect("object")
                        .clone();
                    match &mut start {
                        KiroSessionStart::New(r) => r.meta = Some(meta),
                        KiroSessionStart::Load(r) => r.meta = Some(meta),
                    }
                }
                let result = graphit_ai::cli_kiro::run_kiro(
                    access.original().snapshot(),
                    transport,
                    service.kiro_engine().ok_or(io::ErrorKind::Unsupported)?,
                    start,
                    service.kiro_prompt(input.text.clone(), &model),
                    service.kiro_limits(),
                    &mut sink,
                )
                .await;
                (Some(result), Some(owner))
            }
            Err(error) => {
                let code = io_failure(error.kind());
                let terminal = KnownTerminal {
                    turn_id: input.turn_id.clone(),
                    outcome: if access.original().cancellation().is_cancelled() {
                        TurnOutcome::Cancelled
                    } else {
                        TurnOutcome::Failed
                    },
                    error: Some(code),
                    joined: true,
                    clean: true,
                    persisted: false,
                };
                return self.finish_turn(access, id, &slot, &mut custody, terminal, None);
            }
        };
        let owner = owner.expect("successful launch owns transport");
        let spawned = owner.spawned();
        let joined = !spawned || service.join_kiro(&owner).await.unwrap_or(false);
        let clean = !spawned || (joined && owner.clean_joined().await);
        let execution = execution.expect("submitted execution");
        let receipt = execution.as_ref().map_or_else(|e| e.receipt(), |r| r);
        let native = ((receipt.new_response().is_some() || receipt.load_response().is_some())
            && receipt.session().is_some())
        .then(|| receipt.session().expect("observed session").to_string());
        let sink_error = if execution.is_ok() {
            sink.finish().err()
        } else {
            None
        };
        let error = if !joined || !clean {
            Some(FailureCode::CleanupIncomplete)
        } else if let Err(error) = &execution {
            Some(match error.reason() {
                KiroReason::Context(kind) => io_failure(kind),
                KiroReason::Sink(kind) => io_failure(kind),
                KiroReason::PromptLimit
                | KiroReason::ResponseLimit
                | KiroReason::NotificationLimit
                | KiroReason::IdLimit => FailureCode::Limit,
                _ => FailureCode::Native,
            })
        } else {
            sink_error.as_ref().map(|e| io_failure(e.kind()))
        };
        let cancelled = access.original().cancellation().is_cancelled();
        let terminal = KnownTerminal {
            turn_id: input.turn_id.clone(),
            outcome: if cancelled {
                TurnOutcome::Cancelled
            } else if error.is_some() {
                TurnOutcome::Failed
            } else {
                TurnOutcome::Completed
            },
            error,
            joined,
            clean,
            persisted: false,
        };
        self.finish_turn(access, id, &slot, &mut custody, terminal, native)
    }
    fn finish_turn(
        &self,
        access: &Access<'_>,
        id: &str,
        slot: &Arc<Mutex<SessionSlot>>,
        custody: &mut TurnGuard,
        terminal: KnownTerminal,
        native: Option<String>,
    ) -> Result<TurnReceipt, LiveError> {
        {
            let mut state = slot
                .lock()
                .map_err(|_| io::Error::other("Live custody lock"))?;
            state.native_session = native.map(zeroize::Zeroizing::new);
            state.terminal = Some(terminal.clone());
            if terminal.joined && terminal.clean {
                state.active = None;
                state.process = None;
            }
            state.changed.send_replace(());
        }
        custody.finished = true;
        let persistence_error = self.persist_terminal(access, id).err();
        Ok(TurnReceipt { persistence_error })
    }
    pub fn persist_terminal(&self, access: &Access<'_>, id: &str) -> Result<(), LiveError> {
        self.persist_terminal_with_effect(access, id).map(|_| ())
    }
    pub(super) fn persist_terminal_with_effect(
        &self,
        access: &Access<'_>,
        id: &str,
    ) -> Result<graphit_core::native_document_file::NativeDocumentJournalEffect, LiveError> {
        let prior = self.persist_preparation_failure(access, id)?;
        access
            .write()
            .map_err(|error| LiveError::from(error).retain_prior_effect(prior))?;
        let slot = self
            .history_slot(access, id)
            .map_err(|error| LiveError::from(error).retain_prior_effect(prior))?;
        let mut state = slot.lock().map_err(|_| {
            LiveError::from(io::Error::other("Live custody lock")).retain_prior_effect(prior)
        })?;
        let Some(terminal) = state.terminal.clone() else {
            return Ok(prior);
        };
        if terminal.persisted {
            return Ok(prior);
        }
        let result = self.journal(access).map_err(|error| LiveError::from(error).retain_prior_effect(prior))?.change_with_effect(
            access.original().snapshot(),
            || access.write(),
            |sessions| {
                let stored = sessions.get_mut(id).ok_or(io::ErrorKind::NotFound)?;
                if let Some(native) = &state.native_session {
                    stored.native_session = Some(native.to_string());
                }
                let previous = stored.events.iter().find(|event| event.turn_id.as_deref() == Some(&terminal.turn_id) && matches!(event.data, EventData::TurnDone { .. }));
                if let Some(previous) = previous {
                    if !matches!(previous.data, EventData::TurnDone { outcome, joined, clean } if outcome == terminal.outcome && joined == terminal.joined && clean == terminal.clean) {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    let previous_error = stored.events.iter().filter(|event| event.turn_id.as_deref() == Some(&terminal.turn_id)).find_map(|event| match event.data { EventData::Error { code } => Some(code), _ => None });
                    if previous_error != terminal.error { return Err(io::ErrorKind::InvalidData.into()); }
                }
                let exists = previous.is_some();
                if !exists {
                    if let Some(code) = terminal.error {
                        append(stored, Some(&terminal.turn_id), EventData::Error { code })?;
                    }
                    append(
                        stored,
                        Some(&terminal.turn_id),
                        EventData::TurnDone {
                            outcome: terminal.outcome,
                            joined: terminal.joined,
                            clean: terminal.clean,
                        },
                    )?;
                    let next = if terminal.joined && terminal.clean {
                        SessionState::Ready
                    } else {
                        SessionState::Failed
                    };
                    append(
                        stored,
                        Some(&terminal.turn_id),
                        EventData::State { state: next },
                    )?;
                }
                Ok(())
            },
        );
        state.changed.send_replace(());
        let (_, effect) =
            result.map_err(|error| LiveError::from(error).retain_prior_effect(prior))?;
        state
            .terminal
            .as_mut()
            .ok_or(io::ErrorKind::InvalidData)?
            .persisted = true;
        Ok(super::preparation_lifecycle::combined(prior, effect))
    }
}
pub(super) fn io_failure(kind: io::ErrorKind) -> FailureCode {
    match kind {
        io::ErrorKind::Interrupted => FailureCode::Cancelled,
        io::ErrorKind::TimedOut => FailureCode::Deadline,
        io::ErrorKind::PermissionDenied => FailureCode::PermissionDenied,
        io::ErrorKind::FileTooLarge | io::ErrorKind::OutOfMemory => FailureCode::Limit,
        io::ErrorKind::InvalidData => FailureCode::Corrupt,
        io::ErrorKind::NotFound | io::ErrorKind::Unsupported => FailureCode::Unavailable,
        _ => FailureCode::Storage,
    }
}
