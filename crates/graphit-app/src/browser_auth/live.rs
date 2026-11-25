//! Cookie-authenticated Live history and replay. Execution has independent custody.
use super::*;
use crate::{
    knowledge_live::{HistoryOperation, ReplayCursor, TailDelivery, TailMessage},
    task_server::{
        TaskServer,
        browser::{
            codec,
            live::{self, Input, Output},
        },
    },
};
use axum::{
    extract::{Path, Query},
    response::{Sse, sse::Event},
};
use graphit_hub::policy::RequestContext;

#[derive(Clone)]
struct Routes {
    sessions: BrowserAuth,
    tasks: TaskServer,
}
pub(super) fn router(sessions: BrowserAuth, tasks: TaskServer) -> Router {
    let maximum = tasks.browser_policy().0.max_input_bytes;
    Router::new()
        .route("/api/live/sessions", get(list))
        .route("/api/live/sessions/{id}", get(read).delete(remove))
        .route("/api/live/sessions/{id}/cancel", post(cancel))
        .route("/api/live/sessions/{id}/close", post(close))
        .route("/api/live/sessions/{id}/events", get(events))
        .layer(DefaultBodyLimit::max(maximum))
        .layer(axum::middleware::map_response(private_response_async))
        .with_state(Routes { sessions, tasks })
}
pub(super) enum Error {
    Session(Failure),
    Live(live::Error),
}
impl From<Failure> for Error {
    fn from(value: Failure) -> Self {
        Self::Session(value)
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            Self::Session(error) => error.into_response(),
            Self::Live(error) => error.into_response(),
        }
    }
}
pub(super) struct CancelOnDrop(pub(super) Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
#[derive(Clone)]
pub(super) struct DeliveryGate {
    pub(super) entry: Arc<Session>,
    pub(super) shared: Arc<Shared>,
    pub(super) stop: CancellationToken,
    pub(super) cancellation: Cancellation,
    pub(super) deadline: Instant,
}
impl DeliveryGate {
    fn check(&self) -> io::Result<()> {
        // A deadline wake cancels the owned job for drain. Preserve that known cause.
        if Instant::now() >= self.deadline || Instant::now() >= self.entry.expires {
            return Err(io::ErrorKind::TimedOut.into());
        }
        if self.entry.cancelled.is_cancelled() {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        if self.cancellation.is_cancelled()
            || self.stop.is_cancelled()
            || self.shared.stop.is_cancelled()
        {
            return Err(io::ErrorKind::Interrupted.into());
        }
        Ok(())
    }
}
pub(super) struct Subscriber {
    pub(super) first: Option<Event>,
    pub(super) progress: tokio::sync::mpsc::Receiver<TailMessage>,
    pub(super) terminal: oneshot::Receiver<Result<Output, Error>>,
    pub(super) guard: CancelOnDrop,
    pub(super) generation: String,
    pub(super) gate: DeliveryGate,
    pub(super) maximum: usize,
}
fn refused(error: io::Error, maximum: usize) -> Result<Event, io::Error> {
    let code = match error.kind() {
        io::ErrorKind::TimedOut => "deadline",
        io::ErrorKind::Interrupted => "cancelled",
        _ => "permission_denied",
    };
    frame(
        "error",
        &serde_json::json!({"error":{"code":code}}),
        maximum,
    )
}
pub(super) fn subscriber_stream(
    state: Subscriber,
) -> impl futures_util::Stream<Item = Result<Event, io::Error>> {
    futures_util::stream::unfold(Some(state), |state| async move {
        let mut state = state?;
        if let Err(error) = state.gate.check() {
            return Some((refused(error, state.maximum), None));
        }
        if let Some(event) = state.first.take() {
            return Some((Ok(event), Some(state)));
        }
        if let Some(message) = state.progress.recv().await {
            if let Err(error) = state.gate.check() {
                return Some((refused(error, state.maximum), None));
            }
            let event = match message {
                TailMessage::Event(event) => frame("event", &event, state.maximum)
                    .map(|frame| frame.id(format!("{}:{}", state.generation, event.sequence))),
                TailMessage::Terminal(terminal) => frame("terminal", &terminal, state.maximum),
            };
            return match event {
                Ok(event) => Some((Ok(event), Some(state))),
                Err(error) => Some((Err(error), None)),
            };
        }
        let result = state
            .terminal
            .await
            .unwrap_or(Err(Error::Session(Failure::Unavailable)));
        if let Err(error) = state.gate.check() {
            return Some((refused(error, state.maximum), None));
        }
        let event = match result {
            Ok(Output::Closed(session)) => frame("closed", &session, state.maximum),
            Ok(Output::History(_)) => Err(io::ErrorKind::InvalidData.into()),
            Err(error) => {
                let response = error.into_response();
                match axum::body::to_bytes(response.into_body(), state.maximum).await {
                    Ok(bytes) => {
                        if let Err(error) = state.gate.check() {
                            return Some((refused(error, state.maximum), None));
                        }
                        Ok(Event::default()
                            .event("error")
                            .data(String::from_utf8_lossy(&bytes)))
                    }
                    Err(error) => Err(io::Error::other(error)),
                }
            }
        };
        drop(state.guard);
        Some((event, None))
    })
}
async fn execute(
    routes: &Routes,
    entry: &Session,
    id: &str,
    input: Input,
    cancellation: &Cancellation,
    deadline: Instant,
    stream_deadline: Option<oneshot::Sender<Instant>>,
) -> Result<Output, Error> {
    let shared = &routes.sessions.0;
    let original = RequestContext::new(routes.tasks.browser_policy().0, cancellation, deadline)
        .map_err(|_| Failure::Invalid)?;
    let work = async {
        let credentials = shared.credentials(entry, original.snapshot()).await?;
        let natural_deadline = if stream_deadline.is_some() {
            // Sample monotonic time first: the conversion cannot extend the verified UTC bound.
            let anchor = Instant::now();
            let remaining = credentials
                .expires_at()
                .duration_since(std::time::SystemTime::now())
                .map_err(|_| Error::Live(io::Error::from(io::ErrorKind::TimedOut).into()))?;
            anchor
                .checked_add(remaining)
                .ok_or(Failure::Invalid)?
                .min(deadline)
        } else {
            deadline
        };
        let streaming = stream_deadline.is_some();
        let access = zeroize::Zeroizing::new(credentials.access_token().to_owned());
        drop(credentials);
        if let Some(send) = stream_deadline {
            send.send(natural_deadline)
                .map_err(|_| Error::Live(io::Error::from(io::ErrorKind::BrokenPipe).into()))?;
        }
        let admitted = async {
            let flow = shared.flow(entry.provider)?;
            let (provider, verifier) = &shared.providers[entry.provider];
            let context = BrokerRequestContext::new(original.snapshot());
            let caller = flow
                .verify_access(&entry.registered, &access, context)
                .await
                .map_err(Failure::from)?;
            caller.check(verifier, context).map_err(Failure::from)?;
            let allowed = || {
                !entry.cancelled.is_cancelled()
                    && entry.expires > Instant::now()
                    && !shared.stop.is_cancelled()
            };
            routes
                .tasks
                .live_broker((&caller, verifier, provider), &original, input, &allowed)
                .await
                .map_err(Error::Live)
        };
        tokio::pin!(admitted);
        if streaming {
            tokio::select! {
                result = &mut admitted => result,
                () = tokio::time::sleep_until(natural_deadline.into()) => {
                    cancellation.cancel();
                    let drained = admitted.await;
                    if let Err(Error::Session(error)) = &drained
                        && session::invalidates(error)
                    {
                        shared.invalidate(id, entry).await;
                    }
                    Err(Error::Live(io::Error::from(io::ErrorKind::TimedOut).into()))
                }
            }
        } else {
            admitted.await
        }
    }
    .await;
    if let Err(Error::Session(error)) = &work
        && session::invalidates(error)
    {
        shared.invalidate(id, entry).await;
    }
    work
}
fn origin(shared: &Shared, headers: &HeaderMap, readonly: bool) -> Result<(), Failure> {
    if readonly
        && !headers.contains_key(header::ORIGIN)
        && headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) == Some("same-origin")
        && matches!(
            headers.get("sec-fetch-mode").and_then(|v| v.to_str().ok()),
            Some("cors" | "same-origin")
        )
    {
        if shared.stop.is_cancelled() {
            return Err(Failure::Unavailable);
        }
        return Ok(());
    }
    shared.origin(headers)
}
async fn admitted(
    routes: &Routes,
    jar: CookieJar,
    headers: &HeaderMap,
    readonly: bool,
) -> Result<(String, Arc<Session>), Box<Response>> {
    let shared = &routes.sessions.0;
    origin(shared, headers, readonly).map_err(|error| Box::new(error.into_response()))?;
    let (id, entry) = shared
        .session(&jar)
        .await
        .map_err(|error| Box::new((shared.clear(jar, false), error).into_response()))?;
    if entry.expires <= Instant::now() {
        return Err(Box::new(Failure::Unavailable.into_response()));
    }
    Ok((id, entry))
}
async fn history(
    routes: Routes,
    jar: CookieJar,
    headers: HeaderMap,
    operation: HistoryOperation,
) -> Response {
    let arrival = Instant::now();
    let (id, entry) = match admitted(&routes, jar, &headers, !operation.mutates()).await {
        Ok(v) => v,
        Err(e) => return *e,
    };
    let deadline = match arrival.checked_add(routes.tasks.browser_policy().1) {
        Some(value) => value.min(entry.expires),
        None => return Failure::Invalid.into_response(),
    };
    if deadline <= Instant::now() {
        return Failure::Unavailable.into_response();
    }
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let (send, receive) = oneshot::channel();
    let shared = &routes.sessions.0;
    let stop = routes.tasks.browser_policy().2;
    {
        let mut jobs = tokio::select! {
            jobs = shared.jobs.lock() => jobs,
            () = shared.stop.cancelled() => return Failure::Unavailable.into_response(),
            () = stop.cancelled() => return Failure::Unavailable.into_response(),
            () = tokio::time::sleep_until(deadline.into()) => return Failure::Unavailable.into_response(),
        };
        while jobs.try_join_next().is_some() {}
        let routes = routes.clone();
        jobs.spawn(async move {
            let _ = send.send(
                execute(
                    &routes,
                    &entry,
                    &id,
                    Input::History(operation),
                    &cancel,
                    deadline,
                    None,
                )
                .await,
            );
        });
    }
    let result = tokio::select! {
        result = receive => result.unwrap_or(Err(Error::Session(Failure::Unavailable))),
        () = shared.stop.cancelled() => Err(Error::Session(Failure::Unavailable)),
        () = stop.cancelled() => Err(Error::Session(Failure::Unavailable)),
        () = tokio::time::sleep_until(deadline.into()) => Err(Error::Session(Failure::Unavailable)),
    };
    match result {
        Ok(Output::History(bytes)) => {
            ([(header::CONTENT_TYPE, "application/json")], bytes).into_response()
        }
        Ok(Output::Closed(_)) => Failure::Unavailable.into_response(),
        Err(error) => error.into_response(),
    }
}
async fn list(State(routes): State<Routes>, jar: CookieJar, headers: HeaderMap) -> Response {
    history(routes, jar, headers, HistoryOperation::List).await
}
macro_rules! history_route {
    ($name:ident, $operation:ident) => {
        async fn $name(
            State(routes): State<Routes>,
            Path(id): Path<String>,
            jar: CookieJar,
            headers: HeaderMap,
        ) -> Response {
            history(routes, jar, headers, HistoryOperation::$operation { id }).await
        }
    };
}
history_route!(read, Get);
history_route!(remove, Delete);
history_route!(cancel, Cancel);
history_route!(close, Close);
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorQuery {
    generation: Option<String>,
    sequence: Option<String>,
}
fn cursor(
    query: CursorQuery,
    headers: &HeaderMap,
    maximum: usize,
) -> Result<Option<ReplayCursor>, Failure> {
    let raw = headers
        .get("last-event-id")
        .map(|h| h.to_str().map_err(|_| Failure::Invalid))
        .transpose()?;
    let pair = match (raw, query.generation, query.sequence) {
        (None, None, None) => return Ok(None),
        (None, Some(generation), Some(sequence)) => (generation, sequence),
        (Some(raw), None, None) => {
            let (generation, sequence) = raw.split_once(':').ok_or(Failure::Invalid)?;
            (generation.to_owned(), sequence.to_owned())
        }
        _ => return Err(Failure::Invalid),
    };
    if pair.0.is_empty()
        || pair.0.len() > maximum
        || !pair.0.bytes().all(|b| b.is_ascii_hexdigit())
        || pair.1.is_empty()
        || (pair.1.len() > 1 && pair.1.starts_with('0'))
        || !pair.1.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(Failure::Invalid);
    }
    Ok(Some(ReplayCursor {
        generation: pair.0,
        sequence: pair.1.parse().map_err(|_| Failure::Invalid)?,
    }))
}
fn frame<T: Serialize>(name: &str, value: &T, maximum: usize) -> Result<Event, std::io::Error> {
    let bytes = codec::encode_response(value, maximum).map_err(std::io::Error::other)?;
    Ok(Event::default()
        .event(name)
        .data(String::from_utf8(bytes).expect("JSON is UTF-8")))
}
async fn events(
    State(routes): State<Routes>,
    Path(session_id): Path<String>,
    jar: CookieJar,
    headers: HeaderMap,
    query: Result<Query<CursorQuery>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let limits = routes.tasks.browser_policy().0;
    let after = match query
        .map_err(|_| Failure::Invalid)
        .and_then(|Query(query)| cursor(query, &headers, limits.max_scalar_bytes))
    {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let (id, entry) = match admitted(&routes, jar, &headers, true).await {
        Ok(v) => v,
        Err(e) => return *e,
    };
    let deadline = entry.expires;
    let cancellation = Cancellation::default();
    let guard = CancelOnDrop(cancellation.clone());
    let (opened, opening) = oneshot::channel();
    let (events, progress) = tokio::sync::mpsc::channel(1);
    let (send, terminal) = oneshot::channel();
    let stop = routes.tasks.browser_policy().2;
    let gate_entry = Arc::clone(&entry);
    let gate_stop = stop.clone();
    let gate_cancellation = cancellation.clone();
    let (bound, natural_bound) = oneshot::channel();
    {
        let mut jobs = tokio::select! {
            jobs = routes.sessions.0.jobs.lock() => jobs,
            () = routes.sessions.0.stop.cancelled() => return Failure::Unavailable.into_response(),
            () = stop.cancelled() => return Failure::Unavailable.into_response(),
            () = tokio::time::sleep_until(deadline.into()) => return Failure::Unavailable.into_response(),
        };
        while jobs.try_join_next().is_some() {}
        let work_routes = routes.clone();
        jobs.spawn(async move {
            let session_cancel = entry.cancelled.clone();
            let execute = execute(&work_routes, &entry, &id, Input::Stream { id: session_id, cursor: after, delivery: TailDelivery { opened, events } }, &cancellation, deadline, Some(bound));
            tokio::pin!(execute);
            let result = tokio::select! {
                result = &mut execute => result,
                () = work_routes.sessions.0.stop.cancelled() => { cancellation.cancel(); execute.await },
                () = stop.cancelled() => { cancellation.cancel(); execute.await },
                () = session_cancel.cancelled() => { cancellation.cancel(); execute.await },
                () = tokio::time::sleep_until(deadline.into()) => { cancellation.cancel(); execute.await },
            };
            let _ = send.send(result);
        });
    }
    let natural_deadline = match natural_bound.await {
        Ok(deadline) => deadline,
        Err(_) => {
            return terminal
                .await
                .unwrap_or(Err(Error::Session(Failure::Unavailable)))
                .err()
                .map_or_else(
                    || Failure::Unavailable.into_response(),
                    IntoResponse::into_response,
                );
        }
    };
    let gate = DeliveryGate {
        entry: gate_entry,
        shared: Arc::clone(&routes.sessions.0),
        stop: gate_stop,
        cancellation: gate_cancellation,
        deadline: natural_deadline,
    };
    let opening = match opening.await {
        Ok(opening) => opening,
        Err(_) => {
            return terminal
                .await
                .unwrap_or(Err(Error::Session(Failure::Unavailable)))
                .err()
                .map_or_else(
                    || Failure::Unavailable.into_response(),
                    IntoResponse::into_response,
                );
        }
    };
    let generation = opening.session.cursor.generation.clone();
    let first = match frame("opened", &opening, limits.max_output_bytes) {
        Ok(v) => v,
        Err(_) => return Failure::Unavailable.into_response(),
    };
    let stream = subscriber_stream(Subscriber {
        first: Some(first),
        progress,
        terminal,
        guard,
        generation,
        gate,
        maximum: limits.max_output_bytes,
    });
    Sse::new(stream).into_response()
}
