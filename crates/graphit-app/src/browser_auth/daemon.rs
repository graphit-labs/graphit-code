//! Signed Web-session read of the same daemon state used by native control.
use super::*;
use crate::source_server::SourceServer;

#[derive(Clone)]
struct Routes {
    sessions: BrowserAuth,
    source: SourceServer,
}

pub(super) fn router(sessions: BrowserAuth, source: SourceServer) -> Router {
    Router::new()
        .route("/api/daemon/status", get(status))
        .layer(axum::middleware::map_response(private_response_async))
        .with_state(Routes { sessions, source })
}

async fn status(State(routes): State<Routes>, jar: CookieJar, headers: HeaderMap) -> Response {
    let shared = &routes.sessions.0;
    if headers.contains_key(header::ORIGIN)
        && let Err(error) = shared.origin(&headers)
    {
        return error.into_response();
    }
    let (id, entry) = match shared.session(&jar).await {
        Ok(entry) => entry,
        Err(error) => return (shared.clear(jar, false), error).into_response(),
    };
    let original = match shared.context() {
        Ok(original) => original,
        Err(error) => return error.into_response(),
    };
    let work = async {
        let credentials = shared.credentials(&entry, &original).await?;
        let flow = shared.flow(entry.provider)?;
        let context = BrokerRequestContext::new(&original);
        let caller = flow
            .verify_access(&entry.registered, credentials.access_token(), context)
            .await?;
        caller.check(&shared.providers[entry.provider].1, context)?;
        original.check().map_err(|_| Failure::Unavailable)?;
        if entry.cancelled.is_cancelled()
            || entry.expires <= Instant::now()
            || shared.stop.is_cancelled()
        {
            return Err(Failure::Unauthorized);
        }
        Ok(routes.source.daemon_status())
    };
    let result = tokio::select! {
        _ = shared.stop.cancelled() => {
            original.cancellation().cancel();
            Err(Failure::Unavailable)
        },
        result = work => result,
    };
    match result {
        Ok(reply) => Json(reply).into_response(),
        Err(error) => {
            if session::invalidates(&error) {
                shared.invalidate(&id, &entry).await;
                (shared.clear(jar, false), error).into_response()
            } else {
                error.into_response()
            }
        }
    }
}
