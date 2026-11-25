//! Same-origin desktop UI. Runtime credentials stay inside the service owner.
use super::*;
use axum::{
    Json,
    body::Body,
    extract::{Request, State as AxumState},
    http::{Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use graphit_auth::profile::{ProfileError, ProviderKind};
use tower::ServiceExt;

#[derive(Clone)]
pub(super) struct LocalUi {
    source: SourceServer,
    address: std::net::SocketAddr,
    origin: String,
    broker_configured: bool,
}
impl LocalUi {
    pub(super) fn new(
        source: SourceServer,
        address: std::net::SocketAddr,
        broker_configured: bool,
    ) -> Self {
        Self {
            source,
            address,
            origin: format!("http://{address}"),
            broker_configured,
        }
    }
    fn mode(&self, original: &RequestContext<'_>) -> io::Result<Mode> {
        let installation = &self.source.0.policy.installation;
        let config = match &self.source.0.hook {
            Some(hook) => hook.selected_configuration(installation, None, original.snapshot())?,
            None => graphit_core::config::ConfigSnapshot::new(
                installation.brand(),
                [graphit_core::config::ConfigStore::global(installation)?.load()?],
            )?,
        };
        let snapshot = crate::foreground_source::auth_snapshot(installation, &config, original)?;
        match snapshot.document().active(original) {
            Ok((provider, profile)) => Ok(match provider.config().kind {
                ProviderKind::Broker(_) => Mode::Broker,
                ProviderKind::Local { .. } => Mode::Local {
                    provider: provider.name().to_owned(),
                    profile: Some((profile.name().to_owned(), profile.revision())),
                    username: profile.data().and_then(|data| match &data.credentials {
                        graphit_auth::profile::ProfileCredentials::Local(credentials)
                            if !credentials.anonymous =>
                        {
                            Some(data.username.clone())
                        }
                        _ => None,
                    }),
                },
            }),
            Err(ProfileError::NoActive)
                if self.broker_configured
                    || config
                        .get::<serde_json::Value>(&graphit_core::config::ConfigKey::new(
                            "task.browser",
                        )?)?
                        .is_some() =>
            {
                Ok(Mode::Broker)
            }
            Err(ProfileError::NoActive) => Ok(Mode::Local {
                provider: "local".into(),
                profile: None,
                username: None,
            }),
            Err(error) => Err(io::Error::other(error)),
        }
    }
    fn allowed(&self, request: &Request) -> bool {
        let headers = request.headers();
        self.address.ip().is_loopback()
            && headers.get_all(header::HOST).iter().count() == 1
            && headers.get(header::HOST).and_then(|v| v.to_str().ok())
                == Some(self.address.to_string().as_str())
            && headers.get_all(header::ORIGIN).iter().count() <= 1
            && headers
                .get(header::ORIGIN)
                .is_none_or(|v| v.to_str().ok() == Some(self.origin.as_str()))
            && headers
                .get("sec-fetch-site")
                .is_none_or(|v| matches!(v.to_str(), Ok("same-origin" | "none")))
            && (request.method() == Method::GET
                || (headers
                    .get("x-graphit-request")
                    .and_then(|v| v.to_str().ok())
                    == Some("ui")
                    && headers.get(header::ORIGIN).and_then(|v| v.to_str().ok())
                        == Some(self.origin.as_str())))
    }
}
#[derive(PartialEq, Eq)]
enum Mode {
    Local {
        provider: String,
        profile: Option<(String, u64)>,
        username: Option<String>,
    },
    Broker,
}
fn authorization(raw: &str) -> header::HeaderValue {
    let text = Zeroizing::new(format!("Bearer {raw}"));
    let mut value = header::HeaderValue::from_str(&text).expect("runtime key header");
    value.set_sensitive(true);
    value
}
fn failure(status: StatusCode, code: &'static str) -> Response {
    (status, Json(json!({"error":{"code":code}}))).into_response()
}
fn private(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}
fn ui_path(path: &str) -> bool {
    path.starts_with("/auth/")
        || path == "/api/daemon/status"
        || path == "/api/dream/local/status"
        || path == "/api/projects/local/list"
        || path == "/api/projects/local/labels"
        || path == "/api/records/references"
        || [
            "/api/tasks/",
            "/api/memory/",
            "/api/user-memory/",
            "/api/knowledge/",
            "/api/ast/",
            "/api/hub/",
            "/api/now/web/",
        ]
        .iter()
        .any(|prefix| path.starts_with(prefix))
            && !path.starts_with("/api/tasks/native/")
}
pub(super) async fn dispatch(
    AxumState(ui): AxumState<LocalUi>,
    request: Request,
    next: Next,
) -> Response {
    if !ui_path(request.uri().path()) {
        return next.run(request).await;
    }
    let cancel = Cancellation::default();
    let guard = CancelOnDrop(cancel.clone());
    let original = match RequestContext::new(
        ui.source.0.policy.content,
        &cancel,
        Instant::now() + ui.source.0.policy.timeout,
    ) {
        Ok(v) => v,
        Err(_) => {
            return private(failure(
                StatusCode::REQUEST_TIMEOUT,
                "request_cancelled_or_expired",
            ));
        }
    };
    let mode = match ui.mode(&original) {
        Ok(v) => v,
        Err(_) => {
            return private(if original.check().is_err() {
                failure(StatusCode::REQUEST_TIMEOUT, "request_cancelled_or_expired")
            } else {
                failure(StatusCode::SERVICE_UNAVAILABLE, "provider_unavailable")
            });
        }
    };
    if mode == Mode::Broker {
        return next.run(request).await;
    }
    if !ui.allowed(&request) {
        return private(failure(StatusCode::FORBIDDEN, "local_origin_required"));
    }
    let owned_write = request.method() == Method::POST
        && matches!(
            request.uri().path(),
            "/api/user-memory/create"
                | "/api/user-memory/update"
                | "/api/user-memory/delete"
                | "/api/projects/local/labels"
        );
    let source = ui.source.clone();
    let stop = source.0.stop.clone();
    let deadline = original
        .snapshot()
        .required_deadline()
        .expect("finite local UI request");
    let (send, mut receive) = oneshot::channel();
    let worker_ui = ui.clone();
    let worker_cancel = cancel.clone();
    {
        let mut jobs = source.0.jobs.lock().await;
        jobs.reap();
        if stop.is_cancelled() {
            return private(failure(StatusCode::SERVICE_UNAVAILABLE, "stopped"));
        }
        jobs.tasks.spawn(async move {
            let original = match RequestContext::new(
                worker_ui.source.0.policy.content,
                &worker_cancel,
                deadline,
            ) {
                Ok(v) => v,
                Err(_) => {
                    let _ = send.send(private(failure(
                        StatusCode::REQUEST_TIMEOUT,
                        "request_cancelled_or_expired",
                    )));
                    return;
                }
            };
            let response = if worker_ui.mode(&original).ok().as_ref() == Some(&mode) {
                execute(&worker_ui, request, &mode, &original).await
            } else {
                failure(StatusCode::CONFLICT, "provider_changed")
            };
            let (parts, body) = response.into_parts();
            let response = match axum::body::to_bytes(
                body,
                worker_ui.source.0.policy.content.max_output_bytes,
            )
            .await
            {
                Ok(bytes) => Response::from_parts(parts, Body::from(bytes)),
                Err(_) => failure(StatusCode::INTERNAL_SERVER_ERROR, "response_limit"),
            };
            let response = if owned_write {
                // USER mutations return safe receipts, including recovery data after a commit.
                response
            } else if worker_ui.source.0.stop.is_cancelled() {
                failure(StatusCode::SERVICE_UNAVAILABLE, "stopped")
            } else if original.check().is_err() {
                failure(StatusCode::REQUEST_TIMEOUT, "request_cancelled_or_expired")
            } else if worker_ui.mode(&original).ok().as_ref() != Some(&mode) {
                failure(StatusCode::CONFLICT, "provider_changed")
            } else {
                response
            };
            let _ = send.send(private(response));
        });
    }
    let interrupted = tokio::select! {
        response = &mut receive => {
            return response.unwrap_or_else(|_| private(failure(StatusCode::SERVICE_UNAVAILABLE, "unavailable")));
        }
        () = stop.cancelled() => failure(StatusCode::SERVICE_UNAVAILABLE, "stopped"),
        () = tokio::time::sleep_until(deadline.into()) => failure(StatusCode::REQUEST_TIMEOUT, "request_cancelled_or_expired"),
    };
    drop(guard);
    if owned_write {
        return receive.await.unwrap_or_else(|_| private(interrupted));
    }
    private(interrupted)
}
async fn execute(
    ui: &LocalUi,
    request: Request,
    mode: &Mode,
    original: &RequestContext<'_>,
) -> Response {
    let path = request.uri().path().to_owned();
    if request.method() == Method::GET {
        return match path.as_str() {
            "/auth/session" => {
                let Mode::Local {
                    provider, username, ..
                } = mode
                else {
                    unreachable!()
                };
                Json(json!({"mode":"local","provider":provider,"username":username,"subject":null,"anonymous":username.is_none(),"expires_at":null})).into_response()
            }
            "/auth/providers" => Json(json!({"providers":[]})).into_response(),
            "/api/daemon/status" => Json(ui.source.daemon_status()).into_response(),
            _ => failure(StatusCode::NOT_FOUND, "unknown_operation"),
        };
    }
    if request.method() != Method::POST {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    if path == "/auth/logout" {
        return StatusCode::NO_CONTENT.into_response();
    }
    let maximum = ui.source.0.policy.content.max_input_bytes;
    if request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(';').next())
        != Some("application/json")
    {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }
    let bytes = tokio::select! {
        result = axum::body::to_bytes(request.into_body(), maximum) => match result {
            Ok(value) => value,
            Err(_) => return failure(StatusCode::PAYLOAD_TOO_LARGE, "invalid_arguments"),
        },
        () = original.snapshot().cancellation().cancelled() => {
            return failure(StatusCode::REQUEST_TIMEOUT, "request_cancelled_or_expired");
        }
    };
    let key = match ui.source.0.auth.runtime().check(original.snapshot()) {
        Ok(key) => key,
        Err(_) => return failure(StatusCode::UNAUTHORIZED, "runtime_unavailable"),
    };
    let raw = match key
        .secret(original.snapshot())
        .ok()
        .and_then(|v| std::str::from_utf8(v).ok())
    {
        Some(v) => v,
        None => return failure(StatusCode::UNAUTHORIZED, "runtime_unavailable"),
    };
    let caller = match ui
        .source
        .0
        .auth
        .issuer()
        .authenticate_runtime(raw, original)
    {
        Ok(v) => v,
        Err(_) => return failure(StatusCode::UNAUTHORIZED, "runtime_unavailable"),
    };
    if path == "/api/dream/local/status" {
        return dream_status(&ui.source, &bytes, &caller, original);
    }
    if matches!(
        path.as_str(),
        "/api/projects/local/list" | "/api/projects/local/labels"
    ) {
        let result = if path.ends_with("/list") {
            super::projects::browser_list(&ui.source.0, &bytes, &caller, original)
        } else {
            super::projects::browser_labels(&ui.source.0, &bytes, &caller, original)
        };
        return match result {
            Ok(value) => Json(value).into_response(),
            Err(code) => failure(
                match code {
                    "authentication_or_permission_denied" | "checkout_not_active_or_denied" => {
                        StatusCode::FORBIDDEN
                    }
                    "invalid_arguments" | "invalid_cursor" | "invalid_label_metadata" => {
                        StatusCode::BAD_REQUEST
                    }
                    "cursor_expired"
                    | "checkout_not_registered"
                    | "checkout_selection_ambiguous"
                    | "project_version_required" => StatusCode::CONFLICT,
                    "request_cancelled_or_expired" => StatusCode::REQUEST_TIMEOUT,
                    _ => StatusCode::SERVICE_UNAVAILABLE,
                },
                code,
            ),
        };
    }
    if path == "/api/ast/source" {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            selection: crate::task_server::request::Selection,
            source: crate::ast_source::SourceParameters,
        }
        let input: Input = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => return failure(StatusCode::BAD_REQUEST, "invalid_arguments"),
        };
        if input.source.validate().is_err() {
            return failure(StatusCode::BAD_REQUEST, "invalid_arguments");
        }
        let snapshot = match ui.source.0.live_projects.snapshot() {
            Ok(v) => v,
            Err(_) => return failure(StatusCode::SERVICE_UNAVAILABLE, "selection_unavailable"),
        };
        let selected = match input
            .selection
            .select_read(&snapshot.projects, snapshot.current.unwrap_or(0))
            .ok()
            .and_then(|i| snapshot.projects.get(i))
        {
            Some(v) => v,
            None => return failure(StatusCode::NOT_FOUND, "selection_unavailable"),
        };
        return match ui
            .source
            .graph_server()
            .inspect_source_native(
                (&selected.0, selected.1, &selected.2),
                &input.source,
                raw,
                false,
                original,
            )
            .await
        {
            Ok(value) => Json(value).into_response(),
            Err(error) => crate::graph_server::ReadFailure::Project(error).into_response(),
        };
    }
    if path == "/api/ast/search" {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            selection: crate::task_server::request::Selection,
            search: Value,
        }
        let input: Input = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => return failure(StatusCode::BAD_REQUEST, "invalid_arguments"),
        };
        let Value::Object(mut arguments) = input.search else {
            return failure(StatusCode::BAD_REQUEST, "invalid_arguments");
        };
        if let Some(dir) = input.selection.project_dir {
            arguments.insert("project_dir".into(), json!(dir));
        }
        if let Some(version) = input.selection.project_version {
            arguments.insert("project_version".into(), json!(version));
        }
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-graphit-auth-origin",
            header::HeaderValue::from_static("runtime"),
        );
        headers.insert(header::AUTHORIZATION, authorization(raw));
        let transport = CancellationToken::new();
        let graphs = ui.source.graph_server();
        let work = graphs.call_search_mcp(
            &ui.source,
            CallToolRequestParams::new("graphit_ast_search").with_arguments(arguments),
            headers,
            transport.clone(),
        );
        tokio::pin!(work);
        let result = tokio::select! {
            result = &mut work => result,
            () = original.snapshot().cancellation().cancelled() => { transport.cancel(); work.await },
            () = tokio::time::sleep_until(original.snapshot().required_deadline().expect("finite local UI request").into()) => { transport.cancel(); work.await },
        };
        return if result.is_error == Some(true) {
            failure(StatusCode::BAD_REQUEST, "ast_search_failed")
        } else {
            match result.structured_content {
                Some(value) => Json(value).into_response(),
                None => failure(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "response_encoding_failed",
                ),
            }
        };
    }
    if let Some(op) = path.strip_prefix("/api/ast/")
        && matches!(
            op,
            "query" | "schema" | "sample" | "neighborhood" | "imports"
        )
    {
        let mut request = Request::builder()
            .method(Method::POST)
            .uri(if op == "imports" {
                "/api/graphs/imports".to_owned()
            } else {
                format!("/api/graphs/project/{op}")
            })
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-graphit-auth-origin", "runtime")
            .header(header::AUTHORIZATION, authorization(raw))
            .body(Body::from(bytes))
            .expect("fixed native graph request");
        request.headers_mut().remove(header::ORIGIN);
        return tokio::select! {
            response = ui.source.graph_server().router().oneshot(request) => response.unwrap_or_else(|never| match never {}),
            () = original.snapshot().cancellation().cancelled() => failure(StatusCode::REQUEST_TIMEOUT, "request_cancelled_or_expired"),
            () = tokio::time::sleep_until(original.snapshot().required_deadline().expect("finite local UI request").into()) => failure(StatusCode::REQUEST_TIMEOUT, "request_cancelled_or_expired"),
        };
    }
    if path == "/api/records/references" {
        let arguments = match crate::foreground_record_references::parse(
            &bytes,
            ui.source.0.policy.content,
            "records_references",
        ) {
            Ok(arguments) => arguments,
            Err(code) => return failure(StatusCode::BAD_REQUEST, code),
        };
        return match ui.source.task_server() {
            Ok(tasks) => tasks
                .local_record_references(arguments, &caller, original)
                .await
                .into_response(),
            Err(_) => failure(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        };
    }
    let (domain, operation) = match path.strip_prefix("/api/").and_then(|v| v.split_once('/')) {
        Some(v) => v,
        None => return failure(StatusCode::NOT_FOUND, "unknown_operation"),
    };
    if domain == "user-memory" {
        let operation = match operation {
            "list" => "user_memory_list",
            "get" => "user_memory_get",
            "history" => "user_memory_history",
            "search" => "user_memory_search",
            "create" => "user_memory_create",
            "update" => "user_memory_update",
            "delete" => "user_memory_delete",
            _ => return failure(StatusCode::NOT_FOUND, "unknown_operation"),
        };
        let arguments = match crate::foreground_user_memory::parse(
            &bytes,
            ui.source.0.policy.content,
            operation,
        ) {
            Ok(arguments) => arguments,
            Err(code) => return failure(StatusCode::BAD_REQUEST, code),
        };
        return match ui.source.task_server() {
            Ok(tasks) => tasks
                .execute_user_memory(arguments, &caller, original)
                .await
                .into_response(),
            Err(_) => failure(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        };
    }
    let operation = match (domain, operation) {
        ("tasks", "contexts") => "contexts",
        ("tasks", "context") => "context",
        ("tasks", "catalogue") => "catalogue",
        ("tasks", "get") => "get_task",
        ("tasks", "session") => "get_session",
        ("tasks", "session-heads") => "session_heads",
        ("tasks", "search") => "search",
        ("tasks", "view") => "read_view",
        ("tasks", "references") => "references",
        ("memory", "contexts") => "memory_contexts",
        ("knowledge", "contexts") => "knowledge_contexts",
        ("ast", "contexts") => "ast_contexts",
        ("hub", "contexts") => "hub_contexts",
        ("now", "web/contexts") => "now_contexts",
        ("now", "web/snapshot") => "now_snapshot",
        ("memory", "list") => "memory_list",
        ("memory", "get") => "memory_get",
        ("memory", "history") => "memory_history",
        ("memory", "search") => "memory_search",
        ("memory", "create") => "memory_create",
        ("memory", "update") => "memory_update",
        ("memory", "delete") => "memory_delete",
        ("knowledge", "heads") => "knowledge_heads",
        ("knowledge", "page") => "knowledge_page",
        ("knowledge", "search") => "knowledge_search",
        _ => return failure(StatusCode::NOT_FOUND, "unknown_operation"),
    };
    let limits = ui.source.0.policy.content;
    let parsed = if matches!(operation, "memory_contexts" | "knowledge_contexts") {
        crate::reference_explorer::parse(&bytes, limits, operation)
    } else if operation.starts_with("memory_") {
        crate::foreground_memory::parse(&bytes, limits, operation)
    } else if operation.starts_with("knowledge_") {
        crate::foreground_knowledge::parse(&bytes, limits, operation)
    } else if operation == "references" {
        crate::foreground_task_references::parse(&bytes, limits)
    } else if matches!(operation, "ast_contexts" | "hub_contexts" | "now_contexts") {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Empty {}
        serde_json::from_slice::<Empty>(&bytes)
            .map(|_| crate::task_server::request::Arguments {
                selection: Default::default(),
                input: None,
            })
            .map_err(|_| "invalid_arguments")
    } else {
        serde_json::from_slice(&bytes).map_err(|_| "invalid_arguments")
    };
    let arguments = match parsed {
        Ok(v) => v,
        Err(code) => return failure(StatusCode::BAD_REQUEST, code),
    };
    match ui.source.task_server() {
        Ok(tasks) => tasks
            .read_local(arguments, operation, &caller, original)
            .await
            .into_response(),
        Err(_) => failure(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}

fn dream_status<'a>(
    source: &'a SourceServer,
    bytes: &[u8],
    caller: &'a graphit_auth::AuthenticatedCaller<'a>,
    original: &'a RequestContext<'a>,
) -> Response {
    use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectSelection, ScopeInput};
    use graphit_core::{artifact_store::PublicationScope, config::ConfigKey};

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        selection: crate::task_server::request::Selection,
    }
    let Ok(Input { selection }) = serde_json::from_slice(bytes) else {
        return failure(StatusCode::BAD_REQUEST, "invalid_arguments");
    };
    let read = || -> Result<Value, crate::reference_explorer::Failure> {
        use crate::reference_explorer::Failure;
        let tasks = source.task_server().map_err(|_| Failure::Unavailable)?;
        let snapshot = tasks.record_snapshot().map_err(|_| Failure::Unavailable)?;
        let resources = tasks.record_resources(&snapshot);
        let target = resources.select_exact(&selection)?;
        resources.require_active(target)?;
        let (paths, id, version) = target;
        let project = PublicationScope::project(paths, *id).map_err(Failure::Physical)?;
        let authority = ProjectRecordAuthority::new(source.0.auth.issuer());
        let admission = authority.admit_authenticated_read(
            caller,
            original,
            ScopeInput {
                installation: source.0.auth.installation(),
                project: Some(ProjectSelection {
                    scope: &project,
                    version,
                }),
                providers: &[],
                binding: source.0.policy.binding,
                global_version: "",
            },
            ProjectReadModule::Memory,
        )?;
        let installation = &source.0.policy.installation;
        let config = match &source.0.hook {
            Some(hook) => {
                hook.selected_configuration(installation, Some(paths), original.snapshot())
            }
            None => {
                use graphit_core::config::{ConfigSnapshot, ConfigStore};
                (|| {
                    ConfigSnapshot::new(
                        installation.brand(),
                        [
                            ConfigStore::global(installation)?.load()?,
                            ConfigStore::project(paths)?.load()?,
                        ],
                    )
                })()
            }
        }
        .map_err(Failure::Physical)?;
        let configured = crate::foreground_dream::enabled(&config).map_err(Failure::Physical)?;
        let seconds = |key: &str, default| -> Result<u64, Failure> {
            let key = ConfigKey::new(key).map_err(Failure::Physical)?;
            config
                .get::<u64>(&key)
                .map(|value| value.unwrap_or(default))
                .map_err(Failure::Physical)
        };
        let worker_attached = source
            .0
            .human_activity
            .as_ref()
            .map(|activity| activity.read())
            .transpose()
            .map_err(|_| Failure::Unavailable)?
            .as_deref()
            .and_then(Option::as_ref)
            .is_some_and(|(context, activity)| {
                context.id == *id
                    && context.root() == paths.root()
                    && &context.version == version
                    && activity.receiver_count() > 0
            });
        admission.recheck()?;
        resources.require_active(target)?;
        original.check().map_err(Failure::Physical)?;
        Ok(json!({
            "enabled": configured, "worker_attached": worker_attached,
            "idle_timeout_seconds": seconds("dream.idle_timeout", 7200)?,
            "max_duration_seconds": seconds("dream.max_duration", 600)?,
        }))
    };
    match read() {
        Ok(value) => Json(value).into_response(),
        Err(error) => {
            let (status, code) = error.response();
            failure(status, code)
        }
    }
}
struct CancelOnDrop(Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[cfg(test)]
mod tests;
