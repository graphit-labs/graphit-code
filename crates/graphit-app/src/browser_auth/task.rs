use super::*;
use crate::task_server::{
    TaskServer,
    browser::{BrowserReadOutput, ReadFailure},
    request::Arguments,
};
use graphit_auth::broker::project_access::BrokerProjectAccess;
use graphit_hub::policy::RequestContext;

#[derive(Clone)]
struct Routes {
    sessions: BrowserAuth,
    tasks: TaskServer,
    backend: Arc<BrokerProjectAccess>,
}
pub(super) fn router(
    sessions: BrowserAuth,
    tasks: TaskServer,
    backend: Arc<BrokerProjectAccess>,
) -> Router {
    let input_bytes = tasks.browser_policy().0.max_input_bytes;
    let router = Router::new()
        .route("/api/tasks/context", post(context))
        .route("/api/tasks/contexts", post(contexts))
        .route("/api/tasks/catalogue", post(catalogue))
        .route("/api/tasks/search", post(search))
        .route("/api/tasks/get", post(get_task))
        .route("/api/tasks/session", post(get_session))
        .route("/api/tasks/view", post(view))
        .route("/api/tasks/session-heads", post(session_heads))
        .route("/api/tasks/references", post(references))
        .route("/api/memory/contexts", post(memory_contexts))
        .route("/api/knowledge/contexts", post(knowledge_contexts))
        .route("/api/memory/list", post(memory_list))
        .route("/api/memory/get", post(memory_get))
        .route("/api/memory/history", post(memory_history))
        .route("/api/memory/create", post(memory_create))
        .route("/api/memory/update", post(memory_update))
        .route("/api/memory/delete", post(memory_delete))
        .route("/api/memory/search", post(memory_search))
        .route("/api/knowledge/heads", post(knowledge_heads))
        .route("/api/knowledge/page", post(knowledge_page))
        .route("/api/user-memory/list", post(user_memory_list))
        .route("/api/user-memory/get", post(user_memory_get))
        .route("/api/user-memory/history", post(user_memory_history))
        .route("/api/user-memory/create", post(user_memory_create))
        .route("/api/user-memory/update", post(user_memory_update))
        .route("/api/user-memory/delete", post(user_memory_delete))
        .route("/api/user-memory/search", post(user_memory_search));
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    let router = router
        .route("/api/knowledge/search", post(knowledge_search))
        .route(
            "/api/knowledge/imported/search",
            post(imported_knowledge_search),
        )
        .route("/api/knowledge/ask", post(knowledge_ask))
        .route("/api/records/references", post(records_references))
        .route(
            "/api/knowledge/imported/contexts",
            post(imported_knowledge_contexts),
        )
        .route(
            "/api/knowledge/imported/page",
            post(imported_knowledge_page),
        )
        .route(
            "/api/knowledge/imported/heads",
            post(imported_knowledge_heads),
        );
    router
        .layer(DefaultBodyLimit::max(input_bytes))
        .layer(axum::middleware::map_response(private_response_async))
        .with_state(Routes {
            sessions,
            tasks,
            backend,
        })
}
enum ReadError {
    Session(Failure),
    Task(ReadFailure),
}
impl From<Failure> for ReadError {
    fn from(value: Failure) -> Self {
        Self::Session(value)
    }
}
impl IntoResponse for ReadError {
    fn into_response(self) -> Response {
        match self {
            Self::Session(error) => error.into_response(),
            Self::Task(error) => error.into_response(),
        }
    }
}
struct CancelOnDrop(Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
async fn execute(
    routes: &Routes,
    entry: &Session,
    id: &str,
    arguments: Arguments,
    operation: &'static str,
    (cancellation, deadline): (&Cancellation, Instant),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))] ask: Option<
        crate::foreground_knowledge::ask::Transport,
    >,
) -> Result<BrowserReadOutput, ReadError> {
    let shared = &routes.sessions.0;
    let original = RequestContext::new(routes.tasks.browser_policy().0, cancellation, deadline)
        .map_err(|_| Failure::Invalid)?;
    let work = async {
        let credentials = shared.credentials(entry, original.snapshot()).await?;
        // The verified caller borrows its bearer; retain only that token during the
        // request, rather than locking credentials throughout a provider process.
        let access = zeroize::Zeroizing::new(credentials.access_token().to_owned());
        drop(credentials);
        let flow = shared.flow(entry.provider)?;
        let (provider, verifier) = &shared.providers[entry.provider];
        let context = BrokerRequestContext::new(original.snapshot());
        let caller = flow
            .verify_access(&entry.registered, &access, context)
            .await
            .map_err(Failure::from)?;
        caller.check(verifier, context).map_err(Failure::from)?;
        let mut value = routes
            .tasks
            .read_broker(
                arguments,
                operation,
                (&caller, verifier, provider),
                &routes.backend,
                &original,
                #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                ask,
            )
            .await
            .map_err(ReadError::Task)?;
        if let Err(error) = original.snapshot().check() {
            match &mut value {
                BrowserReadOutput::MemoryWrite(output) => output.completion_failure(
                    graphit_records::memory_store::MemoryStoreError::Request(error),
                ),
                #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                BrowserReadOutput::KnowledgeAsk(output) => {
                    *output = crate::source_server::knowledge_ask::delivery_denied(
                        std::mem::take(output),
                        "RequestDeliveryDenied",
                    )
                }
                _ => return Err(Failure::Unavailable.into()),
            }
        }
        if entry.cancelled.is_cancelled()
            || entry.expires <= Instant::now()
            || shared.stop.is_cancelled()
        {
            match &mut value {
                BrowserReadOutput::MemoryWrite(output) => output.completion_failure(
                    graphit_records::memory_store::MemoryStoreError::Auth(
                        graphit_auth::AuthError::Denied,
                    ),
                ),
                #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                BrowserReadOutput::KnowledgeAsk(output) => {
                    *output = crate::source_server::knowledge_ask::delivery_denied(
                        std::mem::take(output),
                        "SessionDeliveryDenied",
                    )
                }
                _ => return Err(Failure::Unauthorized.into()),
            }
        }
        Ok(value)
    }
    .await;
    if let Err(ReadError::Session(error)) = &work
        && session::invalidates(error)
    {
        shared.invalidate(id, entry).await;
    }
    work
}
async fn read(
    routes: Routes,
    jar: CookieJar,
    headers: HeaderMap,
    arguments: Arguments,
    operation: &'static str,
) -> Response {
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    if operation == "knowledge_ask" {
        return read_ask(routes, jar, headers, arguments).await;
    }
    let shared = &routes.sessions.0;
    if let Err(error) = shared.origin(&headers) {
        return error.into_response();
    }
    let (id, entry) = match shared.session(&jar).await {
        Ok(value) => value,
        Err(error) => return (shared.clear(jar, false), error).into_response(),
    };
    let (_, timeout, task_stop) = routes.tasks.browser_policy();
    let Some(deadline) = Instant::now().checked_add(timeout) else {
        return Failure::Invalid.into_response();
    };
    let cancellation = Cancellation::default();
    let _cancel_on_drop = CancelOnDrop(cancellation.clone());
    let (send, receive) = oneshot::channel();
    {
        let mut jobs = shared.jobs.lock().await;
        while jobs.try_join_next().is_some() {}
        if shared.stop.is_cancelled() || task_stop.is_cancelled() {
            return Failure::Unavailable.into_response();
        }
        let routes = routes.clone();
        let entry = entry.clone();
        let original_cancel = cancellation.clone();
        let id = id.clone();
        jobs.spawn(async move {
            let result = Box::pin(execute(
                &routes,
                &entry,
                &id,
                arguments,
                operation,
                (&original_cancel, deadline),
                #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                None,
            ))
            .await;
            let _ = send.send(result);
        });
    }
    let result = tokio::select! {
        result = receive => result.unwrap_or_else(|_| Err(Failure::Unavailable.into())),
        () = shared.stop.cancelled() => Err(Failure::Unavailable.into()),
        () = task_stop.cancelled() => Err(Failure::Unavailable.into()),
        () = tokio::time::sleep_until(deadline.into()) => Err(Failure::Unavailable.into()),
    };
    if entry.cancelled.is_cancelled() {
        shared.invalidate(&id, &entry).await;
        return (shared.clear(jar, false), result.into_response()).into_response();
    }
    result.into_response()
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
async fn read_ask(
    routes: Routes,
    jar: CookieJar,
    headers: HeaderMap,
    arguments: Arguments,
) -> Response {
    use axum::response::{Sse, sse::Event};
    let shared = &routes.sessions.0;
    if let Err(error) = shared.origin(&headers) {
        return error.into_response();
    }
    let (id, entry) = match shared.session(&jar).await {
        Ok(value) => value,
        Err(error) => return (shared.clear(jar, false), error).into_response(),
    };
    let (limits, timeout, task_stop) = routes.tasks.browser_policy();
    let Some(deadline) = Instant::now().checked_add(timeout) else {
        return Failure::Invalid.into_response();
    };
    let deadline = deadline.min(entry.expires);
    let cancellation = Cancellation::default();
    let cancel_on_drop = CancelOnDrop(cancellation.clone());
    let (send, receive) = oneshot::channel();
    let (ready_send, ready_receive) = oneshot::channel();
    let (progress_send, progress_receive) = tokio::sync::mpsc::channel(1);
    let work_routes = routes.clone();
    let work_entry = Arc::clone(&entry);
    let work_cancel = cancellation.clone();
    let work_stop = task_stop.clone();
    let live_entry = Arc::clone(&entry);
    let live_shared = Arc::clone(shared);
    let transport = crate::foreground_knowledge::ask::Transport {
        ready: ready_send,
        progress: progress_send,
        session_live: Box::new(move || {
            !live_entry.cancelled.is_cancelled()
                && live_entry.expires > Instant::now()
                && !live_shared.stop.is_cancelled()
        }),
    };
    if routes.tasks.retain_browser_ask(async move {
        let execution = execute(&work_routes, &work_entry, &id, arguments, "knowledge_ask", (&work_cancel, deadline), Some(transport));
        tokio::pin!(execution);
        let result = tokio::select! {
            result = &mut execution => result,
            () = work_routes.sessions.0.stop.cancelled() => { work_cancel.cancel(); execution.await },
            () = work_stop.cancelled() => { work_cancel.cancel(); execution.await },
            () = tokio::time::sleep_until(deadline.into()) => { work_cancel.cancel(); execution.await },
        };
        let _ = send.send(result);
    }).await.is_err() { return Failure::Unavailable.into_response(); }
    // Denied grants and unavailable startup bindings remain ordinary HTTP failures.
    let ready = tokio::select! {
        ready = ready_receive => ready.is_ok(),
        () = shared.stop.cancelled() => return Failure::Unavailable.into_response(),
        () = task_stop.cancelled() => return Failure::Unavailable.into_response(),
        () = tokio::time::sleep_until(deadline.into()) => return Failure::Unavailable.into_response(),
    };
    if !ready {
        return receive
            .await
            .unwrap_or_else(|_| Err(Failure::Unavailable.into()))
            .into_response();
    }
    let events = futures_util::stream::unfold(
        Some((progress_receive, receive, cancel_on_drop)),
        move |state| async move {
            let (mut progress, terminal, guard) = state?;
            if let Some(event) = progress.recv().await {
                return Some((
                    Ok::<Event, std::convert::Infallible>(event),
                    Some((progress, terminal, guard)),
                ));
            }
            let result = terminal
                .await
                .unwrap_or_else(|_| Err(Failure::Unavailable.into()));
            let event = match result {
                Ok(BrowserReadOutput::KnowledgeAsk(result)) => {
                    let mut payload = result.structured_content.unwrap_or_else(
                        || serde_json::json!({"status":"failed","error_content":result.content}),
                    );
                    let mut bytes = crate::task_server::browser::codec::encode_response(
                        &payload,
                        limits.max_output_bytes,
                    );
                    if bytes.is_err() {
                        let denied = crate::source_server::knowledge_ask::delivery_denied(
                            rmcp::model::CallToolResult::structured(payload),
                            "ResponseLimit",
                        );
                        payload = denied.structured_content.expect("structured denial");
                        bytes = crate::task_server::browser::codec::encode_response(
                            &payload,
                            limits.max_output_bytes,
                        );
                    }
                    match bytes {
                        Ok(bytes) => Event::default()
                            .event("final")
                            .data(String::from_utf8(bytes).expect("JSON is UTF-8")),
                        Err(_) => Event::default()
                            .event("error")
                            .data("{\"error\":{\"code\":\"response_limit\"}}"),
                    }
                }
                result => {
                    let response = result.into_response();
                    match axum::body::to_bytes(response.into_body(), limits.max_output_bytes).await
                    {
                        Ok(bytes) => Event::default()
                            .event("error")
                            .data(String::from_utf8_lossy(&bytes)),
                        Err(_) => Event::default()
                            .event("error")
                            .data("{\"error\":{\"code\":\"response_limit\"}}"),
                    }
                }
            };
            drop(guard);
            Some((Ok(event), None))
        },
    );
    Sse::new(events).into_response()
}

macro_rules! route {
    ($function:ident, $operation:literal) => {
        async fn $function(
            State(routes): State<Routes>,
            jar: CookieJar,
            headers: HeaderMap,
            body: Result<Json<Arguments>, axum::extract::rejection::JsonRejection>,
        ) -> Response {
            let arguments = match body {
                Ok(Json(arguments)) => arguments,
                Err(error) => {
                    let status=if error.status()==StatusCode::PAYLOAD_TOO_LARGE { StatusCode::PAYLOAD_TOO_LARGE } else { StatusCode::BAD_REQUEST };
                    return (status,Json(serde_json::json!({"error":{"code":"invalid_arguments"}}))).into_response();
                }
            };
            read(routes, jar, headers, arguments, $operation).await
        }
    };
}
route!(context, "context");
route!(contexts, "contexts");
route!(catalogue, "catalogue");
route!(search, "search");
route!(get_task, "get_task");
route!(get_session, "get_session");
route!(view, "read_view");
route!(session_heads, "session_heads");

async fn references(
    State(routes): State<Routes>,
    jar: CookieJar,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Response {
    let media = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    let json_media = media.is_some_and(|media| {
        media.eq_ignore_ascii_case("application/json")
            || (media.to_ascii_lowercase().starts_with("application/")
                && media.to_ascii_lowercase().ends_with("+json"))
    });
    if !json_media {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":{"code":"invalid_arguments"}})),
        )
            .into_response();
    }
    let bytes = match body {
        Ok(bytes) => bytes,
        Err(error) => {
            let status = if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
                StatusCode::PAYLOAD_TOO_LARGE
            } else {
                StatusCode::BAD_REQUEST
            };
            return (
                status,
                Json(serde_json::json!({"error":{"code":"invalid_arguments"}})),
            )
                .into_response();
        }
    };
    let arguments =
        match crate::foreground_task_references::parse(&bytes, routes.tasks.browser_policy().0) {
            Ok(arguments) => arguments,
            Err(code) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error":{"code":code}})),
                )
                    .into_response();
            }
        };
    read(routes, jar, headers, arguments, "references").await
}

macro_rules! native_route {
    ($function:ident,$operation:literal,$parser:path) => {
        async fn $function(State(routes):State<Routes>, jar:CookieJar, headers:HeaderMap,
            body:Result<axum::body::Bytes,axum::extract::rejection::BytesRejection>) -> Response {
            let media=headers.get(axum::http::header::CONTENT_TYPE).and_then(|h|h.to_str().ok()).and_then(|s|s.split(';').next()).map(str::trim);
            if !media.is_some_and(|s|s.eq_ignore_ascii_case("application/json") || (s.to_ascii_lowercase().starts_with("application/") && s.to_ascii_lowercase().ends_with("+json"))) {
                return (StatusCode::BAD_REQUEST,Json(serde_json::json!({"error":{"code":"invalid_arguments"}}))).into_response();
            }
            let bytes=match body { Ok(bytes)=>bytes,Err(error)=>return (if error.status()==StatusCode::PAYLOAD_TOO_LARGE {StatusCode::PAYLOAD_TOO_LARGE} else {StatusCode::BAD_REQUEST},Json(serde_json::json!({"error":{"code":"invalid_arguments"}}))).into_response() };
            let arguments=match $parser(&bytes,routes.tasks.browser_policy().0,$operation) { Ok(arguments)=>arguments,Err(code)=>return (StatusCode::BAD_REQUEST,Json(serde_json::json!({"error":{"code":code}}))).into_response() };
            read(routes,jar,headers,arguments,$operation).await
        }
    };
}
native_route!(memory_list, "memory_list", crate::foreground_memory::parse);
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
native_route!(
    records_references,
    "records_references",
    crate::foreground_record_references::parse
);
native_route!(
    memory_contexts,
    "memory_contexts",
    crate::reference_explorer::parse
);
native_route!(
    knowledge_contexts,
    "knowledge_contexts",
    crate::reference_explorer::parse
);
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
native_route!(
    imported_knowledge_contexts,
    "imported_knowledge_contexts",
    crate::reference_explorer::parse
);
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
native_route!(
    imported_knowledge_page,
    "imported_knowledge_page",
    crate::reference_explorer::parse
);
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
native_route!(
    imported_knowledge_heads,
    "imported_knowledge_heads",
    crate::reference_explorer::parse
);
native_route!(memory_get, "memory_get", crate::foreground_memory::parse);
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
native_route!(
    imported_knowledge_search,
    "imported_knowledge_search",
    crate::reference_explorer::parse
);
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
native_route!(
    knowledge_search,
    "knowledge_search",
    crate::foreground_knowledge::parse
);
native_route!(
    memory_search,
    "memory_search",
    crate::foreground_memory::parse
);
native_route!(
    memory_create,
    "memory_create",
    crate::foreground_memory::parse
);
native_route!(
    memory_update,
    "memory_update",
    crate::foreground_memory::parse
);
native_route!(
    memory_delete,
    "memory_delete",
    crate::foreground_memory::parse
);
native_route!(
    memory_history,
    "memory_history",
    crate::foreground_memory::parse
);

native_route!(
    knowledge_heads,
    "knowledge_heads",
    crate::foreground_knowledge::parse
);
native_route!(
    knowledge_page,
    "knowledge_page",
    crate::foreground_knowledge::parse
);

native_route!(
    user_memory_list,
    "user_memory_list",
    crate::foreground_user_memory::parse
);
native_route!(
    user_memory_get,
    "user_memory_get",
    crate::foreground_user_memory::parse
);
native_route!(
    user_memory_history,
    "user_memory_history",
    crate::foreground_user_memory::parse
);
native_route!(
    user_memory_create,
    "user_memory_create",
    crate::foreground_user_memory::parse
);
native_route!(
    user_memory_search,
    "user_memory_search",
    crate::foreground_user_memory::parse
);
native_route!(
    user_memory_update,
    "user_memory_update",
    crate::foreground_user_memory::parse
);
native_route!(
    user_memory_delete,
    "user_memory_delete",
    crate::foreground_user_memory::parse
);

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
native_route!(
    knowledge_ask,
    "knowledge_ask",
    crate::foreground_knowledge::parse
);
