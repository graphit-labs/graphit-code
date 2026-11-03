//! PROJECT semantic retrieval using the foreground's one authenticated model child.
use super::*;
use axum::response::IntoResponse;
use graphit_auth::{
    AuthenticatedCaller, ProjectReadAdmission, ProjectReadModule,
    broker::oauth::web::AuthenticatedWebCaller,
};
use graphit_core::{
    artifact_store::StoreOwner, module_settings::Module, project_identity::ProjectId,
    project_paths::ProjectPaths, version_context::VersionContext,
};

pub(super) const TOOL: &str = "graphit_ast_search";

pub(super) fn definition() -> Tool {
    Tool::new(
        TOOL,
        "Search indexed code with FTS, semantic, vector or hybrid retrieval. Selectors grant no access. Search reads available corpus rows independently of graph and Source updates; missing, empty and disabled indexes are distinct.",
        json!({"type":"object","properties":{
            "query":{"type":"string"}, "mode":{"enum":["fts","hybrid","semantic","vector"]},
            "k":{"type":"integer","minimum":1}, "metric":{"enum":["l2","cosine","dot"]},
            "rerank":{"type":"boolean"},
            "context":{"type":"string"},"project_dir":{"type":"string"},"project_version":{"type":"string"}},
            "required":["query"],"additionalProperties":false})
        .as_object()
        .expect("object schema")
        .clone(),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Mode {
    Semantic,
    Vector,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Metric {
    L2,
    #[default]
    Cosine,
    Dot,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    query: String,
    #[serde(default, deserialize_with = "present")]
    mode: Option<Mode>,
    #[serde(default, deserialize_with = "present")]
    k: Option<usize>,
    #[serde(default)]
    metric: Metric,
    #[serde(default, deserialize_with = "present")]
    rerank: Option<bool>,
}
impl Arguments {
    fn validate(&self, state: &State) -> Result<(), &'static str> {
        if self.query.trim().is_empty()
            || self.query.contains('\0')
            || self.query.len() > state.policy.content.max_scalar_bytes
            || self
                .k
                .is_some_and(|k| k == 0 || k > state.policy.content.max_collection_entries)
        {
            return Err("semantic query or selection invalid");
        }
        Ok(())
    }
}

#[derive(Debug)]
enum SemanticError {
    Authentication(graphit_auth::AuthError),
    Operation(io::Error),
    Json(serde_json::Error),
}
impl From<io::Error> for SemanticError {
    fn from(error: io::Error) -> Self {
        Self::Operation(error)
    }
}
impl From<io::ErrorKind> for SemanticError {
    fn from(error: io::ErrorKind) -> Self {
        Self::Operation(error.into())
    }
}
impl std::fmt::Display for SemanticError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authentication(error) => error.fmt(formatter),
            Self::Operation(error) => error.fmt(formatter),
            Self::Json(error) => error.fmt(formatter),
        }
    }
}
impl std::error::Error for SemanticError {}
impl SemanticError {
    fn kind(&self) -> io::ErrorKind {
        match self {
            Self::Operation(error) => error.kind(),
            Self::Authentication(_) => io::ErrorKind::PermissionDenied,
            Self::Json(_) => io::ErrorKind::InvalidData,
        }
    }
}

#[derive(Clone, Copy)]
enum EmbeddingCaller<'a> {
    Native(&'a AuthenticatedCaller<'a>),
    Web(&'a AuthenticatedWebCaller<'a>),
}

pub(super) async fn execute_web<'a>(
    state: &'a State,
    selection: &(ProjectPaths, ProjectId, VersionContext),
    input: Value,
    caller: &'a AuthenticatedWebCaller<'a>,
    read: &'a ProjectReadAdmission<'a>,
    request: &'a RequestContext<'a>,
) -> io::Result<Value> {
    execute_borrowed(
        state,
        selection,
        input,
        EmbeddingCaller::Web(caller),
        read,
        request,
    )
    .await
}

pub(super) async fn execute_native<'a>(
    state: &'a State,
    selection: &(ProjectPaths, ProjectId, VersionContext),
    input: Value,
    caller: &'a AuthenticatedCaller<'a>,
    read: &'a ProjectReadAdmission<'a>,
    request: &'a RequestContext<'a>,
) -> io::Result<Value> {
    execute_borrowed(
        state,
        selection,
        input,
        EmbeddingCaller::Native(caller),
        read,
        request,
    )
    .await
}

async fn execute_borrowed<'a>(
    state: &'a State,
    selection: &(ProjectPaths, ProjectId, VersionContext),
    input: Value,
    caller: EmbeddingCaller<'a>,
    read: &'a ProjectReadAdmission<'a>,
    request: &'a RequestContext<'a>,
) -> io::Result<Value> {
    request.check()?;
    let input: Arguments = serde_json::from_value(input)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    input
        .validate(state)
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    execute_admitted(state, selection, input, request, read, caller)
        .await
        .map_err(|error| io::Error::new(error.kind(), error))
}

async fn execute_admitted<'a>(
    state: &'a State,
    selection: &(ProjectPaths, ProjectId, VersionContext),
    input: Arguments,
    request: &'a RequestContext<'a>,
    read: &'a ProjectReadAdmission<'a>,
    caller: EmbeddingCaller<'a>,
) -> Result<Value, SemanticError> {
    let original = request.snapshot();
    let (paths, id, version) = selection;
    if read.module() != ProjectReadModule::Ast
        || !Arc::ptr_eq(
            read.request()
                .map_err(SemanticError::Authentication)?
                .snapshot(),
            original,
        )
    {
        return Err(SemanticError::Authentication(
            graphit_auth::AuthError::Denied,
        ));
    }
    let project = *read.project().map_err(SemanticError::Authentication)?;
    if project.scope.live.owner_root() != paths.root()
        || !matches!(project.scope.live.owner(), StoreOwner::Project { project_id } if project_id == id)
        || project.version.version() != version.version()
        || project.version.scope() != version.scope()
    {
        return Err(SemanticError::Authentication(
            graphit_auth::AuthError::Denied,
        ));
    }
    let modules = state
        .hook
        .as_ref()
        .ok_or(io::ErrorKind::NotConnected)?
        .selected_modules(&state.policy.installation, paths, original)?;
    if !modules.enabled(Module::Ast) || !modules.enabled(Module::Embedding) {
        read.recheck().map_err(SemanticError::Authentication)?;
        return Ok(json!({"status":"model_disabled","results":[]}));
    }
    let Some((capture, search)) = state
        .graph
        .open_project_search(selection, read, request)
        .await
        .map_err(published_error)?
    else {
        read.recheck().map_err(SemanticError::Authentication)?;
        return Ok(json!({"status":"index_absent","results":[]}));
    };
    let authority = match caller {
        EmbeddingCaller::Native(caller) => {
            crate::graph_server::search::Authority::Native(caller, read)
        }
        EmbeddingCaller::Web(caller) => crate::graph_server::search::Authority::Web(caller, read),
    };
    let mode = match input.mode.unwrap_or(Mode::Semantic) {
        Mode::Semantic => crate::graph_server::search::Mode::Semantic,
        Mode::Vector => crate::graph_server::search::Mode::Vector,
    };
    let metric = match input.metric {
        Metric::L2 => crate::graph_server::search::Metric::L2,
        Metric::Cosine => crate::graph_server::search::Metric::Cosine,
        Metric::Dot => crate::graph_server::search::Metric::Dot,
    };
    let response = crate::graph_server::search::copied(
        crate::graph_server::search::Context {
            policy: &state.policy,
            auth: &state.auth,
            config: state.graph.project_config()?,
            local_models: &state.local_models,
            verifiers: state.native_verifiers.as_deref(),
            models: &state.semantic,
        },
        &search,
        authority,
        request,
        crate::graph_server::search::Input {
            query: input.query,
            mode,
            metric,
            k: input.k,
            rerank: input.rerank,
        },
    )
    .await
    .map_err(|error| match error {
        crate::graph_server::ReadFailure::Query(error)
        | crate::graph_server::ReadFailure::Output(error) => SemanticError::Operation(error),
        crate::graph_server::ReadFailure::Project(
            crate::project_graph::ReadError::Authentication(error),
        ) => SemanticError::Authentication(error),
        other => {
            let status = other.into_response().status();
            let kind = match status.as_u16() {
                400 => io::ErrorKind::InvalidInput,
                401 | 403 => io::ErrorKind::PermissionDenied,
                404 => io::ErrorKind::NotFound,
                408 | 504 => io::ErrorKind::TimedOut,
                409 => io::ErrorKind::InvalidData,
                503 => io::ErrorKind::NotConnected,
                _ => io::ErrorKind::Other,
            };
            SemanticError::Operation(io::Error::new(kind, status.to_string()))
        }
    })?;
    state
        .graph
        .recheck_project_search(&capture, read, request)?;
    if serde_json::to_vec(&response)
        .map_err(SemanticError::Json)?
        .len()
        > state.policy.content.max_output_bytes
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "semantic response exceeds output limit",
        )
        .into());
    }
    Ok(response)
}

fn published_error(error: io::Error) -> SemanticError {
    SemanticError::Operation(error)
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
