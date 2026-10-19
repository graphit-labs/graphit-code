//! Cookie-authenticated AST reads with separate, fresh PROJECT authorization.
use super::*;
use crate::graph_server::{BrowserOperation, GraphServer};
use axum::body::Bytes;
use graphit_auth::broker::project_access::BrokerProjectAccess;
use graphit_hub::policy::RequestContext;

#[derive(Clone)]
pub(super) struct Routes {
    pub(super) sessions: BrowserAuth,
    pub(super) graphs: GraphServer,
    pub(super) sources: crate::source_server::SourceServer,
    pub(super) backend: Arc<BrokerProjectAccess>,
}
pub(super) fn router(
    sessions: BrowserAuth,
    sources: crate::source_server::SourceServer,
    backend: Arc<BrokerProjectAccess>,
) -> Router {
    let graphs = sources.graph_server();
    Router::new()
        .route("/api/ast/contexts", post(contexts))
        .route("/api/ast/imports", post(imports))
        .route("/api/ast/imported/schema", post(imported_schema))
        .route("/api/ast/imported/query", post(imported_query))
        .route("/api/ast/imported/sample", post(imported_sample))
        .route(
            "/api/ast/imported/neighborhood",
            post(imported_neighborhood),
        )
        .route("/api/ast/imported/source", post(imported_source))
        .route("/api/ast/source", post(source))
        .route("/api/ast/search", post(search))
        .route("/api/ast/imported/search", post(imported_search))
        .route("/api/ast/query", post(query))
        .route("/api/ast/schema", post(schema))
        .route("/api/ast/sample", post(sample))
        .route("/api/ast/neighborhood", post(neighborhood))
        .layer(DefaultBodyLimit::max(
            graphs.browser_policy().0.content.max_input_bytes,
        ))
        .layer(axum::middleware::map_response(private_response_async))
        .with_state(Routes {
            sessions,
            graphs,
            sources,
            backend,
        })
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
    body: Bytes,
    operation: BrowserOperation,
    cancellation: &Cancellation,
    deadline: Instant,
) -> Result<Response, Failure> {
    let shared = &routes.sessions.0;
    let original = RequestContext::new(
        routes.graphs.browser_policy().0.content,
        cancellation,
        deadline,
    )
    .map_err(|_| Failure::Invalid)?;
    let result = async {
        let credentials = shared.credentials(entry, original.snapshot()).await?;
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
        let output = routes
            .graphs
            .read_broker(
                body,
                operation,
                (&caller, verifier, provider),
                &routes.backend,
                &routes.sources,
                &original,
            )
            .await;
        original
            .snapshot()
            .check()
            .map_err(|_| Failure::Unavailable)?;
        if entry.cancelled.is_cancelled()
            || entry.expires <= Instant::now()
            || shared.stop.is_cancelled()
        {
            return Err(Failure::Unauthorized);
        }
        Ok(output)
    }
    .await;
    if let Err(error) = &result
        && session::invalidates(error)
    {
        shared.invalidate(id, entry).await;
    }
    result
}
pub(super) async fn read(
    routes: Routes,
    jar: CookieJar,
    headers: HeaderMap,
    body: Bytes,
    operation: BrowserOperation,
) -> Response {
    let shared = &routes.sessions.0;
    if let Err(error) = shared.origin(&headers) {
        return error.into_response();
    }
    let media = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next());
    if media != Some("application/json") {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }
    let (id, entry) = match shared.session(&jar).await {
        Ok(value) => value,
        Err(error) => return (shared.clear(jar, false), error).into_response(),
    };
    let (policy, graph_stop) = routes.graphs.browser_policy();
    let Some(deadline) = Instant::now().checked_add(policy.timeout) else {
        return Failure::Invalid.into_response();
    };
    let graph_stop = graph_stop.clone();
    let cancellation = Cancellation::default();
    let _cancel_on_drop = CancelOnDrop(cancellation.clone());
    let (send, receive) = oneshot::channel();
    {
        let mut jobs = shared.jobs.lock().await;
        while jobs.try_join_next().is_some() {}
        if shared.stop.is_cancelled() || graph_stop.is_cancelled() {
            return Failure::Unavailable.into_response();
        }
        let routes = routes.clone();
        let entry = entry.clone();
        let cancellation = cancellation.clone();
        let id = id.clone();
        jobs.spawn(async move {
            let result = execute(
                &routes,
                &entry,
                &id,
                body,
                operation,
                &cancellation,
                deadline,
            )
            .await;
            let _ = send.send(result);
        });
    }
    let result = tokio::select! {
        result = receive => result.unwrap_or(Err(Failure::Unavailable)),
        () = shared.stop.cancelled() => Err(Failure::Unavailable),
        () = graph_stop.cancelled() => Err(Failure::Unavailable),
        () = tokio::time::sleep_until(deadline.into()) => Err(Failure::Unavailable),
    };
    if entry.cancelled.is_cancelled() {
        shared.invalidate(&id, &entry).await;
        return (shared.clear(jar, false), result.into_response()).into_response();
    }
    result.into_response()
}
macro_rules! route {
    ($name:ident, $operation:expr) => {
        async fn $name(
            State(routes): State<Routes>,
            jar: CookieJar,
            headers: HeaderMap,
            body: Result<Bytes, axum::extract::rejection::BytesRejection>,
        ) -> Response {
            match body {
                Ok(body) => read(routes, jar, headers, body, $operation).await,
                Err(error) => error.into_response(),
            }
        }
    };
}
route!(contexts, BrowserOperation::Contexts);
route!(query, BrowserOperation::Query);
route!(schema, BrowserOperation::Schema);
route!(sample, BrowserOperation::Sample);
route!(neighborhood, BrowserOperation::Neighborhood);
route!(imports, BrowserOperation::Imports);
route!(
    imported_schema,
    BrowserOperation::Imported(crate::graph_server::Operation::Schema)
);
route!(
    imported_query,
    BrowserOperation::Imported(crate::graph_server::Operation::Query)
);
route!(
    imported_sample,
    BrowserOperation::Imported(crate::graph_server::Operation::Sample)
);
route!(
    imported_neighborhood,
    BrowserOperation::Imported(crate::graph_server::Operation::Neighborhood)
);
route!(imported_source, BrowserOperation::ImportedSource);

async fn source(
    State(routes): State<Routes>,
    jar: CookieJar,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    read(routes, jar, headers, body, BrowserOperation::Source).await
}

route!(search, BrowserOperation::Search);
route!(imported_search, BrowserOperation::ImportedSearch);
