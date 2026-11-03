//! Fresh USER Knowledge admissions for the shared Live service.
use super::*;
use crate::knowledge_live::{
    self, Access, HistoryOperation, LiveError, ReplayCursor, Session, TailDelivery,
};
use std::io;

pub(crate) enum Input {
    #[cfg(all(test, target_os = "linux"))]
    Seed(Box<crate::knowledge_live::CreateRequest>),
    History(HistoryOperation),
    Stream {
        id: String,
        cursor: Option<ReplayCursor>,
        delivery: TailDelivery,
    },
}
pub(crate) enum Output {
    History(Vec<u8>),
    Closed(Box<Session>),
}
pub(crate) enum Error {
    Admission(ReadFailure),
    Live(LiveError),
}
impl From<LiveError> for Error {
    fn from(value: LiveError) -> Self {
        Self::Live(value)
    }
}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Live(value.into())
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            Self::Admission(error) => error.into_response(),
            Self::Live(error) => {
                let (status, code) = response(&error);
                (status, Json(json!({"error":{"code":code,"journal_effect":format!("{:?}",error.effect()),"diagnostic":error.diagnostic()}}))).into_response()
            }
        }
    }
}
pub(crate) fn response(error: &LiveError) -> (StatusCode, &'static str) {
    let primary = error.primary_cause();
    let status = match primary.kind() {
        io::ErrorKind::Interrupted | io::ErrorKind::TimedOut => StatusCode::REQUEST_TIMEOUT,
        io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
        io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
        io::ErrorKind::AlreadyExists | io::ErrorKind::WouldBlock => StatusCode::CONFLICT,
        io::ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
        io::ErrorKind::FileTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let code = match primary {
        LiveError::Busy => "busy",
        LiveError::Conflict => "context_changed",
        LiveError::Closed => "closed",
        LiveError::NotReady => "not_ready",
        _ => match primary.kind() {
            io::ErrorKind::Interrupted => "cancelled",
            io::ErrorKind::TimedOut => "deadline",
            io::ErrorKind::PermissionDenied => "permission_denied",
            io::ErrorKind::NotFound => "not_found",
            io::ErrorKind::AlreadyExists => "conflict",
            io::ErrorKind::InvalidInput => "invalid_context",
            io::ErrorKind::InvalidData => "corrupt",
            io::ErrorKind::WouldBlock => "busy",
            io::ErrorKind::FileTooLarge | io::ErrorKind::OutOfMemory => "limit",
            _ => "storage",
        },
    };
    (status, code)
}
impl TaskServer {
    /// Drives real USER history for the HTTP stream fixture; this does not simulate native execution.
    #[cfg(all(test, target_os = "linux"))]
    pub(crate) fn record_live_browser_test_turn<'a>(
        &'a self,
        identity: (&'a AuthenticatedWebCaller<'a>, &'a BrokerVerifier, &'a str),
        original: &'a RequestContext<'a>,
        id: &str,
        turn: &str,
    ) -> Result<(), Error> {
        use crate::knowledge_live::{EventData, SessionState, TurnOutcome};
        let live = self
            .0
            .live_sessions
            .as_ref()
            .ok_or_else(|| io::Error::from(io::ErrorKind::Unsupported))?;
        let (caller, verifier, provider) = identity;
        let issuer = self.0.auth.issuer();
        let scope = crate::foreground_user_memory::capture(
            issuer,
            self.0.auth.installation(),
            caller,
            verifier,
            original,
            &self.0.policy,
        )
        .map_err(|error| Error::Admission(ReadFailure(Kind::UserCapture(Box::new(error)))))?;
        let binding = scope.binding()?;
        let capture = || ScopeCapture {
            live: &scope,
            record: &binding,
        };
        let read = issuer
            .admit_web_user_knowledge_read(caller, verifier, original, provider, capture())
            .map_err(|error| Error::Admission(ReadFailure::task(Failure::auth(error))))?;
        let write = issuer
            .admit_web_user_knowledge_write(caller, verifier, original, provider, capture())
            .map_err(|error| Error::Admission(ReadFailure::task(Failure::auth(error))))?;
        let access = Access::user(&read, Some(&write))?;
        for data in [
            EventData::State {
                state: SessionState::Running,
            },
            EventData::Prompt {
                text: format!("question {turn}"),
            },
            EventData::Text {
                text: format!("answer {turn}"),
            },
            EventData::TurnDone {
                outcome: TurnOutcome::Completed,
                joined: true,
                clean: true,
            },
            EventData::State {
                state: SessionState::Ready,
            },
        ] {
            live.emit_fixture(&access, id, Some(turn), data)?;
        }
        Ok(())
    }

    pub(crate) async fn live_broker<'a>(
        &'a self,
        identity: (&'a AuthenticatedWebCaller<'a>, &'a BrokerVerifier, &'a str),
        original: &'a RequestContext<'a>,
        input: Input,
        session_live: &(dyn Fn() -> bool + Sync),
    ) -> Result<Output, Error> {
        if self.0.stop.is_cancelled() || !session_live() {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied).into());
        }
        let live = self
            .0
            .live_sessions
            .as_ref()
            .ok_or_else(|| io::Error::from(io::ErrorKind::Unsupported))?;
        let (caller, verifier, provider) = identity;
        let issuer = self.0.auth.issuer();
        let scope = crate::foreground_user_memory::capture(
            issuer,
            self.0.auth.installation(),
            caller,
            verifier,
            original,
            &self.0.policy,
        )
        .map_err(|e| Error::Admission(ReadFailure(Kind::UserCapture(Box::new(e)))))?;
        let binding = scope.binding()?;
        let capture = || ScopeCapture {
            live: &scope,
            record: &binding,
        };
        let read = issuer
            .admit_web_user_knowledge_read(caller, verifier, original, provider, capture())
            .map_err(|e| Error::Admission(ReadFailure::task(Failure::auth(e))))?;
        let mutates = match &input {
            Input::History(operation) => operation.mutates(),
            Input::Stream { .. } => false,
            #[cfg(all(test, target_os = "linux"))]
            Input::Seed(_) => true,
        };
        let write = if mutates {
            Some(
                issuer
                    .admit_web_user_knowledge_write(caller, verifier, original, provider, capture())
                    .map_err(|e| Error::Admission(ReadFailure::task(Failure::auth(e))))?,
            )
        } else {
            None
        };
        let access = Access::user(&read, write.as_ref())?;
        let mut effect = graphit_core::native_document_file::NativeDocumentJournalEffect::Unchanged;
        let output = match input {
            #[cfg(all(test, target_os = "linux"))]
            Input::Seed(request) => {
                access.write()?;
                let store = graphit_core::artifact_store::ArtifactStore::new(
                    access.scope().clone(),
                    self.0.policy.write.artifacts,
                );
                let writer = store
                    .writer_with_context(original.snapshot())
                    .map_err(|error| io::Error::new(error.kind(), error))?;
                drop(writer);
                access.write()?;
                let session = live.create(&access, *request)?;
                Output::History(
                    super::codec::encode_response(&session, self.0.policy.content.max_output_bytes)
                        .map_err(io::Error::other)?,
                )
            }
            Input::History(operation) => {
                let response = knowledge_live::dispatch(live, &access, operation)?;
                effect = response.effect;
                let bytes = super::codec::encode_response(
                    &response.value,
                    self.0.policy.content.max_output_bytes,
                )
                .map_err(|cause| {
                    let kind = cause.io_error_kind().unwrap_or(io::ErrorKind::InvalidData);
                    LiveError::after_effect(io::Error::new(kind, cause), effect)
                })?;
                response.recheck(&access)?;
                Output::History(bytes)
            }
            Input::Stream {
                id,
                cursor,
                delivery,
            } => Output::Closed(Box::new(
                live.forward_events(&access, &id, cursor, delivery, session_live)
                    .await?,
            )),
        };
        access
            .read()
            .map_err(|cause| LiveError::after_effect(cause, effect))?;
        if self.0.stop.is_cancelled() || !session_live() {
            return Err(
                LiveError::after_effect(io::ErrorKind::PermissionDenied.into(), effect).into(),
            );
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::native_document_file::NativeDocumentJournalEffect;

    #[tokio::test]
    async fn wrapped_preparation_errors_keep_primary_http_code_and_outer_journal_effect() {
        for (primary, expected_status, expected_code) in [
            (LiveError::Conflict, StatusCode::CONFLICT, "context_changed"),
            (LiveError::NotReady, StatusCode::BAD_REQUEST, "not_ready"),
            (LiveError::Busy, StatusCode::CONFLICT, "busy"),
            (LiveError::Closed, StatusCode::BAD_REQUEST, "closed"),
        ] {
            let history = LiveError::after_effect(
                io::Error::new(primary.kind(), primary),
                NativeDocumentJournalEffect::Applied,
            );
            let error = LiveError::Preparation {
                cause: Box::new(history),
                persistence: Some(Box::new(LiveError::after_effect(
                    io::Error::new(io::ErrorKind::Interrupted, "private-native-path-and-token"),
                    NativeDocumentJournalEffect::Unknown,
                ))),
                effect: NativeDocumentJournalEffect::Unknown,
            };
            assert_eq!(response(&error), (expected_status, expected_code));
            assert_eq!(
                error.preparation_persistence_error().unwrap().kind(),
                io::ErrorKind::Interrupted
            );
            let response = Error::Live(error).into_response();
            assert_eq!(response.status(), expected_status);
            let body = axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(body["error"]["code"], expected_code);
            assert_eq!(body["error"]["journal_effect"], "Unknown");
            assert_eq!(
                body["error"]["diagnostic"]["persistence"]["kind"],
                "Interrupted"
            );
            assert_eq!(
                body["error"]["diagnostic"]["persistence"]["journal_effect"],
                "Unknown"
            );
            assert!(!body.to_string().contains("private-native-path-and-token"));
        }
    }
}
