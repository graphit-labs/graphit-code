//! Fixed, shipped Vite assets for the opt-in native record browser on the Foreground listener.
use axum::{
    Router,
    http::header,
    response::{IntoResponse, Redirect, Response},
    routing::get,
};

const INDEX: &[u8] = include_bytes!("../assets/ui/index.html");
const LOGO: &[u8] = include_bytes!("../assets/ui/logo.svg");
const FAVICON: &[u8] = include_bytes!("../assets/ui/favicon.svg");

pub(crate) fn router() -> Router {
    Router::new()
        .route("/", get(entry))
        .route("/task", get(entry))
        .route("/task/", get(entry))
        .route("/workspace", get(index))
        .route("/task/gantt", get(index))
        .route("/task/explorer", get(index))
        .route("/task/explorer/{task_id}", get(index))
        .route("/task/sessions", get(index))
        .route("/task/sessions/{session_id}", get(index))
        .route("/memory/explorer", get(index))
        .route("/memory/explorer/project", get(index))
        .route("/memory/explorer/project/{memory_id}", get(index))
        .route("/memory/explorer/user", get(index))
        .route("/memory/explorer/user/{memory_id}", get(index))
        .route("/live", get(index))
        .route("/hub/installed", get(index))
        .route("/hub/registry", get(index))
        .route("/hub/local", get(index))
        .route("/hub/upload", get(index))
        .route("/knowledge/explorer", get(index))
        .route("/knowledge/contexts", get(index))
        .route("/ast/contexts", get(index))
        .route("/ast/explorer", get(index))
        .route("/ast/explorer/{context_id}", get(index))
        .route("/logo.svg", get(logo))
        .route("/favicon.svg", get(favicon))
}

async fn entry() -> Redirect {
    Redirect::temporary("/task/gantt")
}
fn asset(bytes: &'static [u8], mime: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "no-cache"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    )
        .into_response()
}
async fn index() -> Response {
    asset(INDEX, "text/html; charset=utf-8")
}
async fn logo() -> Response {
    asset(LOGO, "image/svg+xml")
}
async fn favicon() -> Response {
    asset(FAVICON, "image/svg+xml")
}

#[cfg(test)]
mod tests;
