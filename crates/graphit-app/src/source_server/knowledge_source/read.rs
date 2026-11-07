//! Compose ordinary authenticated readers and the existing Markdown renderer.
use super::*;
use crate::{
    foreground_knowledge::{Generation, reference_binding::ReferenceBinding},
    knowledge_source::{
        KnowledgeSourceError, KnowledgeSourcePolicy, KnowledgeSourceRead, KnowledgeSourceRequest,
        read_knowledge_source_authenticated,
    },
    task_server::browser::codec::{AdmittedSelection, encode_response},
};
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority};
use graphit_data::{
    knowledge_index::{KnowledgeGeneration, KnowledgeIndexContentLimits, KnowledgeIndexReadLimits},
    knowledge_reference_query::KnowledgeReferenceSource,
    knowledge_snapshot::KnowledgeSnapshotPage,
};
use graphit_knowledge::{
    document_source::{DocumentSourceError, DocumentSourceLimits, render_document_source},
    imported_knowledge::{
        ImportedKnowledgeInput, ImportedKnowledgeOpen, ImportedKnowledgeReadLimits,
        open_imported_knowledge,
    },
};
use serde::Serialize;
use std::num::{NonZeroU32, NonZeroUsize};

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum View<'a> {
    NoCorpus,
    RegistryAbsent,
    PublishedEmpty {
        generation: Generation<'a>,
    },
    PageAbsent {
        generation: Generation<'a>,
    },
    Found {
        generation: Generation<'a>,
        slug: &'a str,
        selection: &'a text_slice::Result,
    },
}
fn encode(
    state: &State,
    source: &SourceKey,
    selected: Option<&AdmittedSelection>,
    focused: Option<&KnowledgeReferenceSource>,
    result: &KnowledgeSourceRead,
) -> Result<Value, Value> {
    let native = match result {
        KnowledgeSourceRead::NoCorpus => View::NoCorpus,
        KnowledgeSourceRead::PublishedEmpty { generation } => View::PublishedEmpty {
            generation: generation.into(),
        },
        KnowledgeSourceRead::PageAbsent { generation } => View::PageAbsent {
            generation: generation.into(),
        },
        KnowledgeSourceRead::Found {
            generation,
            slug,
            selection,
            ..
        } => View::Found {
            generation: generation.into(),
            slug,
            selection,
        },
    };
    encode_view(state, source, selected, focused, native)
}
fn encode_view(
    state: &State,
    source: &SourceKey,
    selected: Option<&AdmittedSelection>,
    focused: Option<&KnowledgeReferenceSource>,
    native: View<'_>,
) -> Result<Value, Value> {
    let bytes = encode_response(
        &ReferenceBinding {
            selection: selected,
            source,
            focused_source: focused,
            native,
        },
        state.policy.content.max_output_bytes,
    )
    .map_err(|error| {
        if error.io_error_kind() == Some(io::ErrorKind::FileTooLarge) {
            code("response_limit")
        } else {
            code("response_encoding_failed")
        }
    })?;
    serde_json::from_slice(&bytes).map_err(|_| code("response_encoding_failed"))
}
fn policy(state: &State) -> Result<KnowledgeSourcePolicy, Value> {
    let c = state.policy.content;
    let positive = |n| NonZeroUsize::new(n).ok_or_else(|| code("invalid_policy"));
    Ok(KnowledgeSourcePolicy {
        index: KnowledgeIndexReadLimits {
            native: state.policy.index,
            batch_rows: NonZeroU32::new(8192).expect("SDK scan batch"),
            content: KnowledgeIndexContentLimits {
                max_identifier_bytes: positive(c.max_scalar_bytes)?,
            },
        },
        source: DocumentSourceLimits {
            max_input_bytes: positive(c.max_input_bytes)?,
            max_targets: positive(c.max_collection_entries)?,
            max_virtual_bytes: positive(c.max_input_bytes)?,
            max_virtual_lines: positive(c.max_nodes)?,
            max_output_bytes: positive(c.max_output_bytes)?,
            max_matches: positive(c.max_collection_entries)?,
        },
    })
}
fn document_error(error: &DocumentSourceError) -> Value {
    match error {
        DocumentSourceError::Request(_) => code("request_cancelled_or_expired"),
        DocumentSourceError::Selection(_) => code("invalid_source_selection"),
        DocumentSourceError::Limit(_) | DocumentSourceError::Overflow(_) => {
            code("knowledge_source_limit")
        }
        _ => code("knowledge_source_render_failed"),
    }
}
fn source_error(error: KnowledgeSourceError) -> Value {
    match error {
        KnowledgeSourceError::Request(error) => io_failure(error),
        KnowledgeSourceError::Auth(_) => code("authentication_or_permission_denied"),
        KnowledgeSourceError::Data(error) => {
            code(crate::reference_explorer::Failure::knowledge_response(&error).1)
        }
        KnowledgeSourceError::Source(error) => document_error(&error),
        KnowledgeSourceError::InvalidSlug => code("invalid_slug"),
        KnowledgeSourceError::SlugLimit => code("input_limit"),
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
    let policy = policy(state)?;
    if let Some(SourceKey::ImportedKnowledge { artifact }) = &input.source {
        let source = input.source.as_ref().expect("matched import");
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
            state.policy.index,
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
                .page(&lease, &input.slug, policy.index)
                .await
                .map_err(|error| code(error.code()))?;
            let result = render(request, &input, &page.generation, &page.page, policy.source)?;
            let payload = encode(state, source, None, page.focused_source.as_ref(), &result)?;
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
            ImportedKnowledgeReadLimits {
                directory: state.policy.index,
                index: policy.index,
            },
        )
        .await
        .map_err(imported_error)?;
        let payload = match &opened {
            ImportedKnowledgeOpen::RegistryAbsent => {
                encode_view(state, source, None, None, View::RegistryAbsent)?
            }
            ImportedKnowledgeOpen::Published(reader) => {
                let page = reader
                    .page(&input.slug, policy.index)
                    .await
                    .map_err(imported_error)?;
                let focused = match &page {
                    KnowledgeSnapshotPage::Found { focused_source, .. } => {
                        focused_source.as_deref()
                    }
                    _ => None,
                };
                let result = render(
                    request,
                    &input,
                    &reader.observation().generation,
                    &page,
                    policy.source,
                )?;
                let payload = encode(state, source, None, focused, &result)?;
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
    let live =
        CapturedPublicationScope::project(paths, *id, state.policy.scope, request.snapshot())
            .map_err(io_failure)?;
    let record = live.binding().map_err(io_failure)?;
    let scope = || ScopeInput {
        installation: state.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &live,
                record: &record,
            },
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
        scope(),
        KnowledgeSourceRequest {
            slug: &input.slug,
            selection: &input.selectors,
        },
        policy,
        request,
    )
    .await
    .map_err(source_error)?;
    let admission = authority
        .admit_authenticated_read(&caller, request, scope(), ProjectReadModule::Knowledge)
        .map_err(|_| code("authentication_or_permission_denied"))?;
    let focused = match &result {
        KnowledgeSourceRead::Found { focused_source, .. } => focused_source.as_deref(),
        _ => None,
    };
    let selection = AdmittedSelection {
        project_dir: NativePath::new(paths.root().to_owned()).map_err(io_failure)?,
        project_id: *id,
        project_version: version.version().to_owned(),
    };
    let payload = encode(
        state,
        &SourceKey::ProjectKnowledge,
        Some(&selection),
        focused,
        &result,
    )?;
    admission
        .recheck()
        .map_err(|_| code("authentication_or_permission_denied"))?;
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

fn render(
    request: &RequestContext<'_>,
    input: &Arguments,
    generation: &KnowledgeGeneration,
    page: &KnowledgeSnapshotPage,
    limits: DocumentSourceLimits,
) -> Result<KnowledgeSourceRead, Value> {
    Ok(match page {
        KnowledgeSnapshotPage::PublishedEmpty => KnowledgeSourceRead::PublishedEmpty {
            generation: generation.clone(),
        },
        KnowledgeSnapshotPage::PageAbsent => KnowledgeSourceRead::PageAbsent {
            generation: generation.clone(),
        },
        KnowledgeSnapshotPage::Found {
            document,
            focused_source,
        } => KnowledgeSourceRead::Found {
            generation: generation.clone(),
            slug: document.slug.clone(),
            focused_source: focused_source.clone(),
            selection: render_document_source(
                request.snapshot(),
                document,
                &input.selectors,
                limits,
            )
            .map_err(|error| document_error(&error))?,
        },
    })
}
