//! Compose Entity Source with the existing request-scoped native graph owner.
use super::{Arguments, ArtifactId, ConcreteVersion, RequestContext, State};
use graphit_ast::{
    imported_graph::{
        EntitySourceError, ImportedGraphInput, open_imported_graph_local,
        open_imported_graph_runtime,
    },
    source_inspection::SourceInspection,
};

pub(super) async fn inspect(
    state: &State,
    id: &ArtifactId,
    version: &ConcreteVersion,
    arguments: &Arguments,
    request: &RequestContext<'_>,
    credential: &str,
    local: bool,
) -> Result<SourceInspection, EntitySourceError> {
    let input = ImportedGraphInput {
        registry: &state.policy.registry,
        id,
        version,
        installation: state.auth.installation(),
        file_policy: &state.policy.file,
        read_limits: state.policy.read,
        graph_policy: state.policy.native_install.graph,
        directory_limits: state.policy.index,
        request,
    };
    let graph = if local {
        open_imported_graph_local(state.auth.issuer(), credential, input)?
    } else {
        open_imported_graph_runtime(state.auth.issuer(), credential, input)?
    };
    let selectors = arguments.source.selection();
    if let Some(key) = arguments
        .source
        .native_key()
        .map_err(|_| EntitySourceError::InvalidRequest)?
    {
        graph
            .entity_source_by_key(
                &arguments.source.path,
                arguments
                    .source
                    .entity_type
                    .as_deref()
                    .ok_or(EntitySourceError::InvalidRequest)?,
                key,
                &selectors,
                state.policy.graph_query.max_rows.min(256),
            )
            .await
    } else {
        graph
            .entity_source(
                &tokio::runtime::Handle::current(),
                &arguments.source.path,
                arguments
                    .source
                    .entity
                    .as_deref()
                    .ok_or(EntitySourceError::InvalidRequest)?,
                arguments.source.entity_type.as_deref(),
                &selectors,
                state.policy.graph_query,
            )
            .await
    }
}
