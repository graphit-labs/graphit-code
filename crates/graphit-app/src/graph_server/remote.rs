//! Native installed graph entry over explicit authenticated remote storage.
use super::*;
use crate::{
    remote_native_mount::{ReadAdmission, Selection as RemoteSelection},
    source_server::remote_native,
};

pub(super) async fn search_admitted(
    state: &Shared,
    source: &crate::source_server::SourceServer,
    read: &graphit_auth::ImportedAstReadAdmission<'_>,
    id: &ArtifactId,
    version: &ConcreteVersion,
    input: &super::search::Input,
) -> Result<Option<serde_json::Value>, ReadFailure> {
    input.validate(&state.policy)?;
    let Some(selection) = RemoteSelection::capture(
        ReadAdmission::Ast(read),
        &state.policy.registry,
        id,
        version,
    )
    .map_err(|error| ReadFailure::Remote(Box::new(error)))?
    else {
        return Ok(None);
    };
    let mut service = state
        .auth
        .issuer()
        .capture_imported_ast_hub_service(read, selection.record().provider_name())
        .map_err(|error| ReadFailure::Remote(Box::new(error.into())))?;
    let verifier = if service
        .mode()
        .map_err(|error| ReadFailure::Remote(Box::new(error.into())))?
        == graphit_auth::ServiceMode::BrokerNative
    {
        Some(
            state
                .native_verifiers
                .as_ref()
                .ok_or(ReadFailure::Context)?
                .get(&mut service)
                .map_err(|error| ReadFailure::Query(io::Error::other(error)))?,
        )
    } else {
        None
    };
    let lease = match verifier.as_deref() {
        Some(verifier) => service.native_credential(verifier).await,
        None => service.credential(),
    }
    .map_err(|error| ReadFailure::Remote(Box::new(error.into())))?;
    let metadata = crate::native_publication::metadata_limits(
        state.policy.content,
        state.policy.registry.limits().projection(),
    );
    let opened = remote_native::open(
        selection,
        &lease,
        &state.native_http,
        state.native_allow_loopback_http,
        metadata,
    )
    .await
    .map_err(|error| ReadFailure::Remote(Box::new(error)))?;
    let Some(lexical) = opened
        .mount
        .entity_search(&opened.selection, &lease)
        .await
        .map_err(|error| ReadFailure::Remote(Box::new(error)))?
    else {
        return Ok(Some(
            serde_json::json!({"status":"index_absent","results":[]}),
        ));
    };
    let pin = graphit_ast::entity_search::EntitySearch::from_corpus(
        lexical,
        Arc::clone(read.request().snapshot()),
    )
    .map_err(ReadFailure::Query)?;
    let value = super::search::copied(
        source.ast_search_context().map_err(ReadFailure::Query)?,
        &pin,
        super::search::Authority::Imported(read),
        read.request(),
        input.clone(),
    )
    .await?;
    opened
        .check(&lease)
        .map_err(|error| ReadFailure::Remote(Box::new(error)))?;
    read.recheck().map_err(|error| {
        ReadFailure::Remote(Box::new(crate::remote_native_mount::ErrorKind::Auth(error)))
    })?;
    Ok(Some(value))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn read_admitted<'a>(
    state: &Arc<Shared>,
    read: &'a graphit_auth::ImportedAstReadAdmission<'a>,
    id: &ArtifactId,
    version: &ConcreteVersion,
    operation: BrowserOperation,
    query: Option<QueryData>,
    neighborhood: Option<NeighborhoodData>,
    source: Option<&crate::ast_source::SourceParameters>,
) -> Result<Option<Vec<u8>>, ReadFailure> {
    let Some(selection) = RemoteSelection::capture(
        ReadAdmission::Ast(read),
        &state.policy.registry,
        id,
        version,
    )
    .map_err(|error| ReadFailure::Remote(Box::new(error)))?
    else {
        return Ok(None);
    };
    let mut service = state
        .auth
        .issuer()
        .capture_imported_ast_hub_service(read, selection.record().provider_name())
        .map_err(|error| ReadFailure::Remote(Box::new(error.into())))?;
    let verifier = if service
        .mode()
        .map_err(|error| ReadFailure::Remote(Box::new(error.into())))?
        == graphit_auth::ServiceMode::BrokerNative
    {
        Some(
            state
                .native_verifiers
                .as_ref()
                .ok_or(ReadFailure::Context)?
                .get(&mut service)
                .map_err(|error| ReadFailure::Query(io::Error::other(error)))?,
        )
    } else {
        None
    };
    let lease = match verifier.as_deref() {
        Some(verifier) => service.native_credential(verifier).await,
        None => service.credential(),
    }
    .map_err(|error| ReadFailure::Remote(Box::new(error.into())))?;
    let limits = crate::native_publication::metadata_limits(
        state.policy.content,
        state.policy.registry.limits().projection(),
    );
    let opened = remote_native::open(
        selection,
        &lease,
        &state.native_http,
        state.native_allow_loopback_http,
        limits,
    )
    .await
    .map_err(|error| ReadFailure::Remote(Box::new(error)))?;
    let bytes = if let Some(source) = source {
        let selectors = source.selection();
        let inspected = if let Some(entity) = &source.entity {
            let graph = opened
                .mount
                .graph(&opened.selection, &lease, state.policy.native_install.graph)
                .await
                .map_err(|error| ReadFailure::Remote(Box::new(error)))?;
            let path = source.path.clone();
            let entity = entity.clone();
            let kind = source.entity_type.clone();
            let original = Arc::clone(read.request().snapshot());
            let options = state.policy.graph_query;
            let runtime = tokio::runtime::Handle::current();
            let (send, receive) = oneshot::channel();
            {
                let mut jobs = state.jobs.lock().await;
                while jobs.try_join_next().is_some() {}
                jobs.spawn_blocking(move || {
                    let window = graphit_ast::imported_graph::resolve_remote_entity_window(
                        &graph,
                        &original,
                        &runtime,
                        &path,
                        &entity,
                        kind.as_deref(),
                        options,
                    );
                    let _ = send.send(window);
                });
            }
            let window = receive
                .await
                .map_err(|_| ReadFailure::Context)?
                .map_err(ReadFailure::Entity)?;
            opened
                .entity_at(&lease, &source.path, &selectors, window)
                .await
        } else {
            opened
                .source(&lease, &source.path, &selectors)
                .await
                .and_then(|source| {
                    source.ok_or(crate::remote_native_mount::ErrorKind::Entity(
                        graphit_ast::imported_graph::EntitySourceError::Unavailable,
                    ))
                })
        }
        .map_err(|error| ReadFailure::Remote(Box::new(error)))?;
        encode(&inspected, state.policy.content.max_output_bytes)?
    } else {
        let BrowserOperation::Imported(operation) = operation else {
            return Err(ReadFailure::BadInput);
        };
        let graph = opened
            .mount
            .graph(&opened.selection, &lease, state.policy.native_install.graph)
            .await
            .map_err(|error| ReadFailure::Remote(Box::new(error)))?;
        let owner = Arc::clone(state);
        let original = Arc::clone(read.request().snapshot());
        let (send, receive) = oneshot::channel();
        {
            let mut jobs = state.jobs.lock().await;
            while jobs.try_join_next().is_some() {}
            jobs.spawn_blocking(move || {
                #[cfg(test)]
                {
                    let gate = owner.gate.lock().unwrap().take();
                    if let Some((started, release)) = gate {
                        started.send(original.cancellation().clone()).unwrap();
                        release.recv().unwrap();
                    }
                }
                let bytes = render_operation(
                    &owner,
                    &GraphView::Remote(graph, original),
                    operation,
                    query,
                    neighborhood,
                    owner.policy.native_install.graph,
                );
                let _ = send.send(bytes);
            });
        }
        // The existing job set retains ownership if this subscriber disappears.
        // The shared Original interrupts native query/range work and shutdown joins it.
        receive.await.map_err(|_| ReadFailure::Context)??
    };
    opened
        .check(&lease)
        .map_err(|error| ReadFailure::Remote(Box::new(error)))?;
    read.recheck().map_err(|error| {
        ReadFailure::Remote(Box::new(crate::remote_native_mount::ErrorKind::Auth(error)))
    })?;
    if state.stop.is_cancelled() {
        return Err(ReadFailure::Context);
    }
    Ok(Some(bytes))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn read_with_auth(
    state: &Shared,
    credential: &str,
    local: bool,
    id: &ArtifactId,
    version: &ConcreteVersion,
    operation: Operation,
    query: Option<QueryData>,
    neighborhood: Option<NeighborhoodData>,
    request: &RequestContext<'_>,
) -> Result<Option<Vec<u8>>, ReadFailure> {
    let issuer = state.auth.issuer();
    let caller = if local {
        issuer.authenticate_local(credential, request)
    } else {
        issuer.authenticate_runtime(credential, request)
    }
    .map_err(|error| {
        ReadFailure::Remote(Box::new(crate::remote_native_mount::ErrorKind::Auth(error)))
    })?;
    let read = issuer
        .admit_authenticated_imported_ast_read(&caller, state.auth.installation())
        .map_err(|error| {
            ReadFailure::Remote(Box::new(crate::remote_native_mount::ErrorKind::Auth(error)))
        })?;
    let Some(selection) = RemoteSelection::capture(
        ReadAdmission::Ast(&read),
        &state.policy.registry,
        id,
        version,
    )
    .map_err(|error| ReadFailure::Remote(Box::new(error)))?
    else {
        return Ok(None);
    };
    let mut service = issuer
        .capture_hub_service(&caller, selection.record().provider_name())
        .map_err(|error| ReadFailure::Remote(Box::new(error.into())))?;
    let runtime = tokio::runtime::Handle::current();
    let verifier = if service
        .mode()
        .map_err(|error| ReadFailure::Remote(Box::new(error.into())))?
        == graphit_auth::ServiceMode::BrokerNative
    {
        Some(
            state
                .native_verifiers
                .as_ref()
                .ok_or(ReadFailure::Context)?
                .get(&mut service)
                .map_err(|error| ReadFailure::Query(io::Error::other(error)))?,
        )
    } else {
        None
    };
    let lease = runtime
        .block_on(async {
            match verifier.as_deref() {
                Some(verifier) => service.native_credential(verifier).await,
                None => service.credential(),
            }
        })
        .map_err(|error| ReadFailure::Remote(Box::new(error.into())))?;
    let limits = crate::native_publication::metadata_limits(
        state.policy.content,
        state.policy.registry.limits().projection(),
    );
    let opened = runtime
        .block_on(remote_native::open(
            selection,
            &lease,
            &state.native_http,
            state.native_allow_loopback_http,
            limits,
        ))
        .map_err(|error| ReadFailure::Remote(Box::new(error)))?;
    let graph = runtime
        .block_on(
            opened
                .mount
                .graph(&opened.selection, &lease, state.policy.native_install.graph),
        )
        .map_err(|error| ReadFailure::Remote(Box::new(error)))?;
    let bytes = render_operation(
        state,
        &GraphView::Remote(graph, Arc::clone(request.snapshot())),
        operation,
        query,
        neighborhood,
        state.policy.native_install.graph,
    )?;
    opened
        .check(&lease)
        .map_err(|error| ReadFailure::Remote(Box::new(error)))?;
    read.recheck().map_err(|error| {
        ReadFailure::Remote(Box::new(crate::remote_native_mount::ErrorKind::Auth(error)))
    })?;
    Ok(Some(bytes))
}
