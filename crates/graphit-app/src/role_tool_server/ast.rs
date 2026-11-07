//! Readonly PROJECT graph tools under the retained Role admission.
use super::{SelectedProject, State, ToolResult, catalog::Operation, error, parse};
use crate::graph_server;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, VerifiedRoleGrant};
use graphit_hub::policy::RequestContext;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io, sync::Arc};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

enum Read {
    Schema,
    Query(graph_server::QueryData),
    Search(graph_server::search::Input),
}
pub(super) async fn execute<'a>(
    state: &State,
    authority: &'a ProjectRecordAuthority<'_>,
    grant: &'a VerifiedRoleGrant<'a>,
    selected: &SelectedProject,
    operation: Operation,
    input: Option<Value>,
    original: &'a RequestContext<'a>,
) -> ToolResult {
    let admission = authority
        .admit_read(grant, original, ProjectReadModule::Ast)
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let command = match operation {
        Operation::AstSchema => {
            let _: Empty = parse(input)?;
            Read::Schema
        }
        Operation::AstQuery => Read::Query(parse(input)?),
        Operation::AstSearch => {
            let input: graph_server::search::Input = parse(input)?;
            input
                .validate(&state.policy.source)
                .map_err(|_| error("invalid_arguments"))?;
            Read::Search(input)
        }
        _ => return Err(error("unknown_tool")),
    };
    admission
        .recheck()
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let (capture, pin) = if matches!(&command, Read::Search(_)) {
        let Some((capture, pin)) = state
            .graph
            .open_project_search(selected, &admission, original)
            .await
            .map_err(io_failure)?
        else {
            return Err(error("ast_graph_absent"));
        };
        (capture, Some(pin))
    } else {
        (
            state
                .graph
                .capture_project_read(selected, &admission, original)
                .map_err(io_failure)?,
            None,
        )
    };
    let result = match command {
        Read::Search(input) => {
            let pin = pin.ok_or_else(|| error("ast_graph_absent"))?;
            let value = graph_server::search::copied(
                graph_server::search::Context {
                    policy: &state.policy.source,
                    auth: &state.auth,
                    config: &state.policy.config,
                    local_models: &state.local_models,
                    verifiers: state.hub_verifiers.as_deref(),
                    models: &state.semantic,
                },
                &pin,
                graph_server::search::Authority::Role(grant, &admission),
                original,
                input,
            )
            .await
            .map_err(query_failure)?;
            pin.revalidate().map_err(io_failure)?;
            value
        }
        command => {
            let registry = state.graph.live_ast();
            let _fence = capture
                .open_live(
                    state.auth.installation(),
                    &registry,
                    &state.policy.source,
                    &state.policy.config,
                    &state.live_projects,
                )
                .await
                .map_err(failure)?;
            let request = Arc::clone(original.snapshot());
            let policy = state.policy.source.clone();
            let runtime = tokio::runtime::Handle::current();
            let index = capture
                .live_index()
                .ok_or_else(|| error("ast_graph_absent"))?;
            let graph = Arc::clone(&index.graph);
            let catalog = state.graph.query_catalog();
            let scope = graph_server::NeighborhoodScope::Project {
                project: selected.1.to_string(),
                version: selected.2.version().into(),
            };
            tokio::task::spawn_blocking(move || {
                let _runtime = runtime.enter();
                let (operation, input) = match command {
                    Read::Schema => (graph_server::Operation::Schema, None),
                    Read::Query(input) => (graph_server::Operation::Query, Some(input)),
                    Read::Search(_) => unreachable!(),
                };
                let bytes = graph_server::live::render_live(
                    &graph,
                    &catalog,
                    &request,
                    &scope,
                    operation,
                    input,
                    None,
                    policy.graph_query,
                    None,
                    None,
                    policy.content.max_output_bytes,
                )
                .map_err(query_failure)?;
                serde_json::from_slice(&bytes).map_err(|cause| io_failure(io::Error::other(cause)))
            })
            .await
            .map_err(|_| error("native_job_failed"))??
        }
    };
    state
        .graph
        .recheck_project_search(&capture, &admission, original)
        .map_err(io_failure)?;
    original
        .check()
        .map_err(|_| error("request_cancelled_or_expired"))?;
    Ok(json!({"kind":"graph","result":result}))
}

fn query_failure(cause: graph_server::ReadFailure) -> Value {
    match cause {
        graph_server::ReadFailure::Query(cause) | graph_server::ReadFailure::Output(cause) => {
            io_failure(cause)
        }
        graph_server::ReadFailure::Context => error("request_cancelled_or_expired"),
        graph_server::ReadFailure::BadInput => error("invalid_arguments"),
        _ => error("ast_read_failed"),
    }
}

fn io_failure(cause: io::Error) -> Value {
    json!({"code":"ast_read_failed","io_kind":format!("{:?}",cause.kind())})
}
fn failure(cause: crate::project_graph::ReadError) -> Value {
    use crate::project_graph::ReadError;
    match cause {
        ReadError::Authentication(_) => error("authentication_or_permission_denied"),
        ReadError::Inactive => error("selection_unavailable"),
        ReadError::Disabled => error("ast_disabled"),
        ReadError::Absent => error("ast_graph_absent"),
        ReadError::Operation(cause) => io_failure(cause),
        ReadError::Graph(_) => error("ast_graph_failed"),
        ReadError::Source(_) | ReadError::Entity(_) => error("ast_read_failed"),
    }
}
