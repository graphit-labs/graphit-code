//! Complete Knowledge virtual Markdown through the existing native Source owner.
use super::*;
use crate::reference_explorer::SourceKey;
use graphit_core::{native_path::NativePath, text_slice};

mod read;
#[cfg(all(test, target_os = "linux"))]
mod tests;

pub(super) const TOOL: &str = "graphit_wiki_source";
pub(super) fn definition() -> Tool {
    Tool::new(TOOL,
        "Read a complete authorized Knowledge page as native metadata and literal Markdown, or an explicit Core slice. Slug is the exact page identity, never a filesystem path. PROJECT/version and installed-artifact selections grant no access; output limits fail instead of truncating.",
        json!({"type":"object","properties":{
            "slug":{"type":"string","minLength":1},
            "selectors":{"type":"object","properties":{
                "head":{"type":"integer","minimum":0},"tail":{"type":"integer","minimum":0},
                "start_line":{"type":"integer","minimum":0},"end_line":{"type":"integer","minimum":0},
                "pattern":{"type":"string"},"is_regex":{"type":"boolean"},
                "before":{"type":"integer","minimum":0},"after":{"type":"integer","minimum":0},
                "line_numbers":{"type":"boolean"}},"additionalProperties":false},
            "source":{"type":"object","properties":{"kind":{"enum":["project_knowledge","imported_knowledge"]},
                "artifact":{"type":"object","properties":{"id":{"type":"string"},"version":{"type":"string"},"context_alias":{"type":"string"}},"required":["id","version"],"additionalProperties":false}},
                "required":["kind"],"additionalProperties":false},
            "project_dir":{"type":"string"},"project_version":{"type":"string"}
        },"required":["slug"],"additionalProperties":false}).as_object().expect("source schema").clone())
        .with_annotations(rmcp::model::ToolAnnotations::new().read_only(true))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    slug: String,
    #[serde(default)]
    selectors: text_slice::Request,
    #[serde(default, deserialize_with = "present")]
    source: Option<SourceKey>,
    #[serde(default, deserialize_with = "present")]
    project_dir: Option<NativePath>,
    #[serde(default, deserialize_with = "present")]
    project_version: Option<String>,
}
impl Arguments {
    fn select(
        &self,
        state: &State,
        snapshot: &crate::live_project_selections::LiveProjectSnapshot,
    ) -> Result<Option<usize>, &'static str> {
        let maximum = state.policy.content.max_scalar_bytes;
        if self.slug.len() > maximum || self.selectors.pattern.len() > maximum {
            return Err("input_limit");
        }
        graphit_data::knowledge_index::validate_slug(&self.slug).map_err(|_| "invalid_slug")?;
        // Use the same Core validation/regex engine as the renderer, before an absent-page result.
        text_slice::apply("", &self.selectors).map_err(|_| "invalid_source_selection")?;
        match &self.source {
            Some(SourceKey::ImportedKnowledge { artifact }) => {
                artifact.validate(state.policy.content)?;
                if self.project_dir.is_some() || self.project_version.is_some() {
                    return Err("imported_source_has_project_selection");
                }
                return Ok(None);
            }
            None | Some(SourceKey::ProjectKnowledge) => (),
            _ => return Err("invalid_knowledge_source"),
        }
        if self
            .project_version
            .as_deref()
            .is_some_and(|v| v.is_empty() || v.contains('\0') || v.len() > maximum)
        {
            return Err("invalid_project_version");
        }
        let current = snapshot.current.ok_or("selection_unavailable")?;
        let root = self
            .project_dir
            .as_ref()
            .map(NativePath::as_path)
            .unwrap_or(snapshot.projects[current].0.root());
        if self.project_version.is_none() && root == snapshot.projects[current].0.root() {
            Ok(Some(current))
        } else {
            select_read_project(&snapshot.projects, root, self.project_version.as_deref()).map(Some)
        }
    }
}
fn code(value: &'static str) -> Value {
    json!({"code":value})
}
fn failure(mut value: Value) -> CallToolResult {
    value["status"] = "error".into();
    crate::mcp_output::typed_error(value)
}
fn io_failure(cause: io::Error) -> Value {
    match cause.kind() {
        io::ErrorKind::Interrupted | io::ErrorKind::TimedOut => {
            code("request_cancelled_or_expired")
        }
        io::ErrorKind::PermissionDenied => code("authentication_or_permission_denied"),
        _ => code("knowledge_source_capture_failed"),
    }
}

pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let arguments = call.arguments.unwrap_or_default();
    if let Some(selectors) = arguments.get("selectors") {
        let fields =
            serde_json::to_value(text_slice::Request::default()).expect("Core selectors serialize");
        if selectors
            .as_object()
            .is_none_or(|selectors| selectors.keys().any(|key| fields.get(key).is_none()))
        {
            return failure(code("invalid_arguments"));
        }
    }
    if serde_json::to_vec(&arguments).map_or(true, |encoded| {
        encoded.len() > server.0.policy.content.max_input_bytes
    }) {
        return failure(code("input_limit"));
    }
    let input: Arguments = match serde_json::from_value(Value::Object(arguments)) {
        Ok(value) => value,
        Err(_) => return failure(code("invalid_arguments")),
    };
    let snapshot = match server.0.live_projects.snapshot() {
        Ok(value) => value,
        Err(_) => return failure(code("selection_unavailable")),
    };
    let selected = match input.select(&server.0, &snapshot) {
        Ok(value) => value,
        Err(value) => return failure(code(value)),
    };
    let Some(parts) = context.extensions.get::<Parts>() else {
        return failure(code("incoming_credentials_required"));
    };
    let local = match parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|v| v.to_str().ok())
    {
        Some("runtime") => false,
        Some("local") => true,
        _ => return failure(code("credential_origin_required")),
    };
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| !v.is_empty())
    else {
        return failure(code("credential_required"));
    };
    let credential = Zeroizing::new(raw.to_owned());
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let original_cancel = cancel.clone();
    let state = Arc::clone(&server.0);
    let deadline = Instant::now() + state.policy.timeout;
    let handle = Handle::current();
    let (send, receive) = oneshot::channel();
    {
        let mut jobs = tokio::select! {
            jobs=state.jobs.lock()=>jobs,
            ()=context.ct.cancelled()=>return failure(code("request_cancelled_or_expired")),
            ()=state.stop.cancelled()=>return failure(code("stopped")),
            ()=tokio::time::sleep_until(deadline.into())=>return failure(code("request_cancelled_or_expired")),
        };
        jobs.reap();
        if state.stop.is_cancelled() {
            return failure(code("stopped"));
        }
        let owner = Arc::clone(&state);
        jobs.tasks.spawn_blocking(move || {
            let result = handle.block_on(async {
                let request = RequestContext::new(owner.policy.content, &original_cancel, deadline)
                    .map_err(|cause| {
                        use graphit_hub::{native::ContentError, policy::RequestError};
                        let kind = match &cause {
                            RequestError::Content(ContentError::Cancelled) => {
                                io::ErrorKind::Interrupted
                            }
                            RequestError::Content(ContentError::TimedOut) => {
                                io::ErrorKind::TimedOut
                            }
                            _ => io::ErrorKind::InvalidInput,
                        };
                        io_failure(io::Error::new(kind, cause))
                    })?;
                read::execute(
                    &owner,
                    selected.map(|index| &snapshot.projects[index]),
                    input,
                    &credential,
                    local,
                    &request,
                )
                .await
            });
            let _ = send.send(result);
        });
    }
    tokio::select! {
        result=receive=>match result{
            Ok(_)if context.ct.is_cancelled()||cancel.is_cancelled()||state.stop.is_cancelled()=>failure(code("request_cancelled_or_expired")),
            Ok(_)if Instant::now()>=deadline=>failure(code("request_cancelled_or_expired")),
            Ok(Ok(value))=>crate::mcp_output::typed(value),Ok(Err(value))=>failure(value),Err(_)=>failure(code("native_job_failed")),
        },
        ()=context.ct.cancelled()=>failure(code("request_cancelled_or_expired")),
        ()=state.stop.cancelled()=>failure(code("stopped")),
        ()=tokio::time::sleep_until(deadline.into())=>failure(code("request_cancelled_or_expired")),
    }
}
