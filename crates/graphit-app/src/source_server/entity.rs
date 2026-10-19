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
            &arguments.source.selection(),
            state.policy.graph_query,
        )
        .await
}
