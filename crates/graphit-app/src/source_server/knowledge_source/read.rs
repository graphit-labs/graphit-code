//! Compose ordinary authenticated readers and the existing Markdown renderer.
use super::*;
use crate::{
    knowledge_source::{
        KnowledgeSourceError, KnowledgeSourceRead, KnowledgeSourceRequest,
        read_knowledge_source_authenticated,
    },
    task_server::browser::codec::encode_response,
};
use graphit_auth::ProjectRecordAuthority;
use graphit_data::knowledge_index::{KnowledgeIndexContentLimits, KnowledgeIndexReadLimits};
use graphit_knowledge::imported_knowledge::{
    ImportedKnowledgeInput, ImportedKnowledgeOpen, ImportedKnowledgeReadLimits,
    open_imported_knowledge,
};
use std::num::{NonZeroU32, NonZeroUsize};

fn encode(state: &State, result: KnowledgeSourceRead) -> Result<Value, Value> {
    let bytes =
        encode_response(&result, state.policy.content.max_output_bytes).map_err(|error| {
            if error.io_error_kind() == Some(io::ErrorKind::FileTooLarge) {
                code("response_limit")
            } else {
                code("response_encoding_failed")
            }
        })?;
    serde_json::from_slice(&bytes).map_err(|_| code("response_encoding_failed"))
}

fn source_error(error: KnowledgeSourceError) -> Value {
    match error {
        KnowledgeSourceError::Request(error) => io_failure(error),
        KnowledgeSourceError::Auth(_) => code("authentication_or_permission_denied"),
        KnowledgeSourceError::Data(error) => {
            code(crate::reference_explorer::Failure::knowledge_response(&error).1)
        }
        KnowledgeSourceError::Selection(_) => code("invalid_source_selection"),
    }
}
fn imported_error(
    error: graphit_knowledge::imported_knowledge::ImportedKnowledgeReadError,
) -> Value {
    code(crate::reference_explorer::Failure::imported_response(&error).1)
}

pub(super) async fn execute(
    state: &State,
    selected: Option<&(ProjectPaths, ProjectId, VersionContext)>,
    input: Arguments,
    credential: &str,
    local: bool,
    request: &RequestContext<'_>,
) -> Result<Value, Value> {
    request.check().map_err(io_failure)?;
    let issuer = state.auth.issuer();
    let caller = if local {
        issuer.authenticate_local(credential, request)
    } else {
        issuer.authenticate_runtime(credential, request)
    }
    .map_err(|_| code("authentication_or_permission_denied"))?;
    let index = KnowledgeIndexReadLimits {
        content: KnowledgeIndexContentLimits {
            max_identifier_bytes: NonZeroUsize::new(state.policy.content.max_scalar_bytes)
                .ok_or_else(|| code("invalid_policy"))?,
        },
        batch_rows: NonZeroU32::new(8192).expect("positive SDK batch"),
    };
    if let Some(SourceKey::ImportedKnowledge { artifact }) = &input.source {
        let admission = issuer
            .admit_authenticated_imported_knowledge_read(&caller, state.auth.installation())
            .map_err(|_| code("authentication_or_permission_denied"))?;
        let id = ArtifactId::new(&artifact.id, state.policy.registry.limits().projection())
            .map_err(|_| code("invalid_import_selection"))?;
        let version = ConcreteVersion::new(
            &artifact.version,
            state.policy.registry.limits().projection(),
        )
        .map_err(|_| code("invalid_import_selection"))?;
        if let Some(selection) = crate::remote_native_mount::Selection::capture(
            crate::remote_native_mount::ReadAdmission::Knowledge(&admission),
            &state.policy.registry,
            &id,
            &version,
        )
        .map_err(|error| code(error.code()))?
        {
            let mut service = issuer
                .capture_imported_knowledge_hub_service(
                    &admission,
                    selection.record().provider_name(),
                )
                .map_err(|_| code("authentication_or_permission_denied"))?;
            let verifier = if service
                .mode()
                .map_err(|_| code("authentication_or_permission_denied"))?
                == graphit_auth::ServiceMode::BrokerNative
            {
                Some(
                    state
                        .native_verifiers
                        .as_ref()
                        .ok_or_else(|| code("native_verifier_unavailable"))?
                        .get(&mut service)
                        .map_err(|_| code("native_verifier_unavailable"))?,
                )
            } else {
                None
            };
            let lease = match verifier.as_deref() {
                Some(verifier) => service.native_credential(verifier).await,
                None => service.credential(),
            }
            .map_err(|_| code("authentication_or_permission_denied"))?;
            let opened = super::super::remote_native::open(
                selection,
                &lease,
                &state.native_http,
                state.native_allow_loopback_http,
                crate::native_publication::metadata_limits(
                    state.policy.content,
                    state.policy.registry.limits().projection(),
                ),
            )
            .await
            .map_err(|error| code(error.code()))?;
            let page = opened
                .page(&lease, &input.slug, index)
                .await
                .map_err(|error| code(error.code()))?;
            let result =
                KnowledgeSourceRead::from_page(page, &input.selectors).map_err(source_error)?;
            let payload = encode(state, result)?;
            opened.check(&lease).map_err(|error| code(error.code()))?;
            admission
                .recheck()
                .map_err(|_| code("authentication_or_permission_denied"))?;
            request.check().map_err(io_failure)?;
            return Ok(payload);
        }
        let opened = open_imported_knowledge(
            &admission,
            ImportedKnowledgeInput {
                registry: &state.policy.registry,
                id: &id,
                version: &version,
                context_alias: artifact.context_alias.as_deref(),
            },
            ImportedKnowledgeReadLimits { index },
        )
        .await
        .map_err(imported_error)?;
        let payload = match &opened {
            ImportedKnowledgeOpen::RegistryAbsent => {
                encode(state, KnowledgeSourceRead::RegistryAbsent)?
            }
            ImportedKnowledgeOpen::Published(reader) => {
                let page = reader
                    .page(&input.slug, index)
                    .await
                    .map_err(imported_error)?;
                let result =
                    KnowledgeSourceRead::from_page(page, &input.selectors).map_err(source_error)?;
                let payload = encode(state, result)?;
                reader.recheck().map_err(imported_error)?;
                payload
            }
        };
        admission
            .recheck()
            .map_err(|_| code("authentication_or_permission_denied"))?;
        request.check().map_err(io_failure)?;
        return Ok(payload);
    }
    let (paths, id, version) = selected.ok_or_else(|| code("selection_unavailable"))?;
    if !state
        .live_projects
        .is_active(paths, *id, version)
        .map_err(io_failure)?
    {
        return Err(code("selection_unavailable"));
    }
    let live = PublicationScope::project(paths, *id).map_err(io_failure)?;

    let scope = ScopeInput {
        installation: state.auth.installation(),
        project: Some(ProjectSelection {
            scope: &live,
            version,
        }),
        providers: &[],
        binding: state.policy.binding,
        global_version: "",
    };
    let authority = ProjectRecordAuthority::new(issuer);
    let result = read_knowledge_source_authenticated(
        &authority,
        &caller,
        scope,
        KnowledgeSourceRequest {
            slug: &input.slug,
            selection: &input.selectors,
        },
        index,
        request,
    )
    .await
    .map_err(source_error)?;
    let payload = encode(state, result)?;
    request.check().map_err(io_failure)?;
    if !state
        .live_projects
        .is_active(paths, *id, version)
        .map_err(io_failure)?
    {
        return Err(code("selection_unavailable"));
    }
    Ok(payload)
}
