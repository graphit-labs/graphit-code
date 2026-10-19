//! Authenticated PROJECT/imported export on the existing Source worker owner.
use super::*;
use crate::knowledge_export::{KnowledgeTextExportError, KnowledgeTextExportOutcome};
use crate::knowledge_export_entry::Format;
use graphit_auth::{
    AuthenticatedCaller, ExternalEffect, Obligation, Permission, ProjectReadModule,
    ProjectRecordAuthority,
};
use graphit_core::{
    artifact_store::{
        CopyPolicy, PayloadMode, RetainedBytesLimits, RetainedFileSet, SnapshotPolicy,
        SourceReadLimits,
    },
    native_path::NativePath,
};
use graphit_data::knowledge_index::{
    KnowledgeGeneration, KnowledgeIndexContentLimits, KnowledgeIndexReadLimits,
};
use std::num::{NonZeroU32, NonZeroUsize};

pub(super) const TOOL: &str = "graphit_knowledge_export";
pub(super) fn definition() -> Tool {
    Tool::new(
        TOOL,
        "Export authorized PROJECT or installed Knowledge as an installable native package or a Markdown, OKF or Obsidian directory. Requires explicit filesystem publication permission. Output must be an absolute NativePath outside the datasets; selectors grant no access.",
        json!({"type":"object","properties":{"output":{"type":"string"},"format":{"enum":["native","markdown","okf","obsidian"],"default":"native"},
            "source":{"type":"object","properties":{"kind":{"enum":["project_knowledge","imported_knowledge"]},
                "artifact":{"type":"object","properties":{"id":{"type":"string"},"version":{"type":"string"},"context_alias":{"type":"string"}},"required":["id","version"],"additionalProperties":false}},
                "required":["kind"],"additionalProperties":false},
            "project_dir":{"type":"string"},"project_version":{"type":"string"}},
            "required":["output"],"additionalProperties":false})
        .as_object()
        .expect("schema")
        .clone(),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    output: NativePath,
    #[serde(default)]
    format: Format,
    #[serde(default, deserialize_with = "present")]
    source: Option<crate::reference_explorer::SourceKey>,
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
        if !self.output.as_path().is_absolute() {
            return Err("invalid_arguments");
        }
        match &self.source {
            Some(crate::reference_explorer::SourceKey::ImportedKnowledge { artifact }) => {
                artifact.validate(state.policy.content)?;
                if self.project_dir.is_some() || self.project_version.is_some() {
                    return Err("imported_source_has_project_selection");
                }
                return Ok(None);
            }
            None | Some(crate::reference_explorer::SourceKey::ProjectKnowledge) => {}
            _ => return Err("invalid_knowledge_source"),
        }
        if self.project_version.as_deref().is_some_and(|version| {
            version.is_empty()
                || version.contains('\0')
                || version.len() > state.policy.content.max_scalar_bytes
        }) {
            return Err("invalid_arguments");
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

fn failure(code: &str) -> CallToolResult {
    crate::mcp_output::typed_error(json!({"status":"error","code":code}))
}
fn operation_failure(error: io::Error, format: Format) -> CallToolResult {
    if let Some(applied) = error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<TextCompletionFailure>())
    {
        return crate::mcp_output::typed_error(json!({"status":"error","format":format,
            "code":"knowledge_export_published_before_completion_failed",
            "published":{"output":applied.output,"generation":applied.generation,
                "fingerprint":fingerprint_text(&applied.fingerprint)},
            "cause":operation_code(&error),"cleanup":cleanup_status(&applied.cleanup)}));
    }
    if let Some(applied) = error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<ImportedNativePublishedFailure>())
    {
        return crate::mcp_output::typed_error(json!({"status":"error","format":format,
            "code":"knowledge_export_published_before_completion_failed",
            "output":applied.output,"published":applied.generation,
            "cause":operation_code(&applied.cause)}));
    }
    if let Some(cause) = error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<KnowledgeTextExportError>())
    {
        let mut payload =
            json!({"status":"error","code":"knowledge_export_failed","format":format});
        match cause {
            KnowledgeTextExportError::AfterPublication {
                output,
                generation,
                fingerprint,
                cleanup,
                ..
            } => {
                payload["code"] = json!("knowledge_export_published_before_completion_failed");
                payload["published"] = json!({"output":NativePath::new(output.clone()).expect("publisher preserves validated destination"),
                    "generation":generation,"fingerprint":fingerprint_text(fingerprint)});
                payload["cleanup"] = cleanup_status(cleanup);
                payload["cause"] = json!(operation_code(&error));
            }
            KnowledgeTextExportError::Publication {
                output,
                cause,
                cleanup,
            } => {
                payload["code"] = json!("knowledge_export_publication_failed");
                payload["output"] = json!(
                    NativePath::new(output.clone())
                        .expect("publisher preserves validated destination")
                );
                payload["publication"] = json!("unconfirmed");
                payload["io_error_kind"] = json!(format!("{:?}", cause.kind()));
                payload["cleanup"] = cleanup_status(cleanup);
            }
            _ => {
                payload["code"] = json!(match error.kind() {
                    io::ErrorKind::Interrupted | io::ErrorKind::TimedOut =>
                        "request_cancelled_or_expired",
                    io::ErrorKind::PermissionDenied => "authentication_or_permission_denied",
                    io::ErrorKind::InvalidInput => "invalid_arguments",
                    _ => "knowledge_export_failed",
                });
            }
        }
        return crate::mcp_output::typed_error(payload);
    }
    if let Some(applied) = error.get_ref().and_then(|cause| {
        cause.downcast_ref::<crate::knowledge_package_export::KnowledgePackagePublishedFailure>()
    }) {
        return crate::mcp_output::typed_error(json!({"status":"error","format":format,
            "code":"knowledge_export_published_before_completion_failed","published":applied.generation}));
    }
    crate::mcp_output::typed_error(
        json!({"status":"error","format":format,"code":operation_code(&error)}),
    )
}

fn operation_code(error: &io::Error) -> &'static str {
    let mut current: Option<&(dyn std::error::Error + 'static)> = error
        .get_ref()
        .map(|cause| cause as &(dyn std::error::Error + 'static));
    while let Some(cause) = current {
        if let Some(cause) = cause
            .downcast_ref::<graphit_knowledge::imported_knowledge::ImportedKnowledgeReadError>()
        {
            return crate::reference_explorer::Failure::imported_response(cause).1;
        }
        current = cause.source();
    }
    match error.kind() {
        io::ErrorKind::Interrupted | io::ErrorKind::TimedOut => "request_cancelled_or_expired",
        io::ErrorKind::PermissionDenied => "authentication_or_permission_denied",
        io::ErrorKind::InvalidInput => "invalid_arguments",
        _ => "knowledge_export_failed",
    }
}

fn text_format(
    format: Format,
    brand: &graphit_core::brand::Brand,
) -> Option<graphit_knowledge::okf_export::KnowledgeTextExportFormat> {
    use graphit_knowledge::okf_export::KnowledgeTextExportFormat;
    match format {
        Format::Native => None,
        Format::Markdown => Some(KnowledgeTextExportFormat::Markdown),
        Format::Okf => Some(KnowledgeTextExportFormat::Okf {
            brand: brand.clone(),
        }),
        Format::Obsidian => Some(KnowledgeTextExportFormat::Obsidian {
            brand: brand.clone(),
        }),
    }
}

fn fingerprint_text(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn cleanup_status<E: std::fmt::Display>(cleanup: &Result<(), E>) -> Value {
    match cleanup {
        Ok(()) => json!({"status":"complete"}),
        Err(cause) => json!({"status":"failed","cause":cause.to_string()}),
    }
}

fn export_error(
    cause: impl std::error::Error + Send + Sync + 'static,
    request: &RequestContext<'_>,
) -> io::Error {
    let kind = if let Err(error) = request.check() {
        error.kind()
    } else {
        let mut current: Option<&(dyn std::error::Error + 'static)> = Some(&cause);
        let mut kind = io::ErrorKind::Other;
        while let Some(error) = current {
            if let Some(error) = error.downcast_ref::<io::Error>() {
                kind = error.kind();
                break;
            }
            if error.is::<graphit_auth::AuthError>() {
                kind = io::ErrorKind::PermissionDenied;
                break;
            }
            current = error.source();
        }
        kind
    };
    io::Error::new(kind, cause)
}

#[derive(Debug)]
struct ImportedNativePublishedFailure {
    output: NativePath,
    generation: crate::knowledge_package_export::KnowledgePackageGeneration,
    cause: io::Error,
}

impl std::fmt::Display for ImportedNativePublishedFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Knowledge package published before imported capture recheck failed")
    }
}

impl std::error::Error for ImportedNativePublishedFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

fn check_external(
    caller: &AuthenticatedCaller<'_>,
    request: &RequestContext<'_>,
) -> io::Result<()> {
    request.check()?;
    if !std::ptr::eq(caller.request(), request)
        || caller
            .authorize(Permission::External(ExternalEffect::PublishedFilesystem))
            .map_err(|cause| export_error(cause, request))?
            != Obligation::Unconditional
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(())
}

fn read_limits(state: &State) -> io::Result<KnowledgeIndexReadLimits> {
    Ok(KnowledgeIndexReadLimits {
        native: state.policy.index,
        batch_rows: NonZeroU32::new(8192).expect("positive batch size"),
        content: KnowledgeIndexContentLimits {
            max_identifier_bytes: NonZeroUsize::new(state.policy.content.max_scalar_bytes)
                .ok_or(io::ErrorKind::InvalidInput)?,
        },
    })
}

fn text_policy(
    state: &State,
    index: KnowledgeIndexReadLimits,
) -> io::Result<crate::knowledge_export::KnowledgeTextExportPolicy> {
    use graphit_knowledge::{
        document::DocumentParseLimits,
        markdown_export::{MarkdownExportLimits, MarkdownExportModes},
        references::ReferenceLimits,
    };
    let content = state.policy.content;
    let positive = |value| NonZeroUsize::new(value).ok_or(io::ErrorKind::InvalidInput);
    let input = positive(content.max_input_bytes)?;
    let output = positive(content.max_output_bytes)?;
    let scalar = positive(content.max_scalar_bytes)?;
    let nodes = positive(content.max_nodes)?;
    let entries = positive(content.max_collection_entries)?;
    let directory = PayloadMode::private_directory();
    let file = PayloadMode::private_file();
    Ok(crate::knowledge_export::KnowledgeTextExportPolicy {
        index,
        text: MarkdownExportLimits {
            max_file_bytes: input,
            document: DocumentParseLimits {
                max_document_bytes: input,
                max_input_bytes: input,
                max_frontmatter_bytes: input,
                max_yaml_events: nodes,
                max_markdown_events: nodes,
                max_yaml_depth: positive(content.max_depth)?,
                max_scalar_bytes: scalar,
                max_links: entries,
                max_projection_bytes: output,
                max_summary_bytes: scalar,
            },
            references: ReferenceLimits {
                max_identifier_bytes: scalar,
            },
        },
        modes: MarkdownExportModes { directory, file },
    })
}

#[derive(Debug)]
struct TextCompletionFailure {
    output: NativePath,
    generation: KnowledgeGeneration,
    fingerprint: [u8; 32],
    guard: Option<io::Error>,
    cleanup: Result<(), KnowledgeTextExportError>,
}

impl std::fmt::Display for TextCompletionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Knowledge text published before Source completion failed")
    }
}

impl std::error::Error for TextCompletionFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.guard
            .as_ref()
            .map(|cause| cause as &(dyn std::error::Error + 'static))
            .or_else(|| {
                self.cleanup
                    .as_ref()
                    .err()
                    .map(|cause| cause as &(dyn std::error::Error + 'static))
            })
    }
}

fn text_result(
    result: KnowledgeTextExportOutcome,
    output: NativePath,
    current: io::Result<()>,
    request: &RequestContext<'_>,
    format: Format,
) -> io::Result<Value> {
    match result {
        KnowledgeTextExportOutcome::RegistryAbsent => {
            current?;
            Ok(json!({"status":"registry_absent","format":format}))
        }
        KnowledgeTextExportOutcome::NoCorpus => {
            current?;
            Ok(json!({"status":"no_corpus","format":format}))
        }
        KnowledgeTextExportOutcome::PublishedEmpty(generation) => {
            current?;
            Ok(json!({"status":"published_empty","format":format,"generation":generation}))
        }
        KnowledgeTextExportOutcome::Published(published) => {
            let guard = current.err();
            if guard.is_some() || published.cleanup.is_err() {
                return Err(export_error(
                    TextCompletionFailure {
                        output,
                        generation: published.generation,
                        fingerprint: published.fingerprint,
                        guard,
                        cleanup: published.cleanup,
                    },
                    request,
                ));
            }
            let groups: Vec<_> = published
                .communities
                .groups
                .iter()
                .map(|group| {
                    json!({"members":group.members,"label":group.label,"cohesion":group.cohesion})
                })
                .collect();
            Ok(json!({"status":"exported","format":format,"output":output,
                "generation":published.generation,"fingerprint":fingerprint_text(&published.fingerprint),
                "communities":{"groups":groups},"cleanup":{"status":"complete"}}))
        }
    }
}

async fn export_imported_native(
    state: &State,
    reader: &graphit_knowledge::imported_knowledge::CapturedImportedKnowledge<'_>,
    destination: NativePath,
    limits: KnowledgeIndexReadLimits,
    caller: &AuthenticatedCaller<'_>,
    request: &RequestContext<'_>,
) -> io::Result<Value> {
    use graphit_hub::native_package::{self, PackageKind};
    check_external(caller, request)?;
    let capture = reader
        .capture_semantic(limits)
        .await
        .map_err(|cause| export_error(cause, request))?;
    let copy = state.policy.native_install.hub.copy();
    let retention = RetainedBytesLimits::new(
        usize::try_from(copy.total_file_bytes_limit()).map_err(io::Error::other)?,
        copy.node_limit(),
        copy.total_relative_path_bytes_limit(),
    )?;
    let policy = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        copy,
        RetainedFileSet::new([], retention)?,
        retention,
    )?;
    let physical = capture
        .capture_physical_source(
            &policy,
            SourceReadLimits::new(64 * 1024, 64 * 1024, copy.total_file_bytes_limit())?,
        )
        .await
        .map_err(|cause| export_error(cause, request))?;
    let captured = physical
        .reader()
        .map_err(|cause| export_error(cause, request))?;
    let root = captured.source_path(request.snapshot())?;
    let name = destination
        .as_path()
        .file_name()
        .ok_or(io::ErrorKind::InvalidInput)?;
    let parent = destination
        .as_path()
        .parent()
        .ok_or(io::ErrorKind::InvalidInput)?;
    let destination = NativePath::new(std::fs::canonicalize(parent)?.join(name))?;
    if destination.as_path().starts_with(root) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "package destination is inside installed Knowledge payload",
        ));
    }
    physical
        .recheck()
        .await
        .map_err(|cause| export_error(cause, request))?;
    check_external(caller, request)?;
    native_package::export(
        captured,
        PackageKind::Knowledge,
        destination.as_path(),
        request.snapshot(),
    )
    .map_err(|cause| export_error(cause, request))?;
    // Publication is known before any asynchronous final guard can fail.
    let generation =
        crate::knowledge_package_export::KnowledgePackageGeneration::from(physical.generation());
    let current = async {
        physical
            .recheck()
            .await
            .map_err(|cause| export_error(cause, request))?;
        check_external(caller, request)
    }
    .await;
    if let Err(cause) = current {
        return Err(io::Error::new(
            cause.kind(),
            ImportedNativePublishedFailure {
                output: destination,
                generation,
                cause,
            },
        ));
    }
    Ok(
        json!({"status":"exported","format":Format::Native,"output":destination,"generation":generation}),
    )
}

async fn execute(
    state: &State,
    selection: Option<&(ProjectPaths, ProjectId, VersionContext)>,
    input: Arguments,
    credential: &str,
    local: bool,
    request: &RequestContext<'_>,
) -> io::Result<Value> {
    let original = request.snapshot();
    original.check()?;
    if let Some((paths, id, version)) = selection
        && !state.live_projects.is_active(paths, *id, version)?
    {
        return Err(io::ErrorKind::NotFound.into());
    }
    let issuer = state.auth.issuer();
    let caller = if local {
        issuer.authenticate_local(credential, request)
    } else {
        issuer.authenticate_runtime(credential, request)
    }
    .map_err(|cause| io::Error::new(io::ErrorKind::PermissionDenied, cause))?;
    if let Some(crate::reference_explorer::SourceKey::ImportedKnowledge { artifact }) =
        &input.source
    {
        use graphit_knowledge::imported_knowledge::{
            ImportedKnowledgeInput, ImportedKnowledgeOpen, ImportedKnowledgeReadLimits,
            open_imported_knowledge,
        };
        check_external(&caller, request)?;
        let id = ArtifactId::new(&artifact.id, state.policy.registry.limits().projection())
            .map_err(io::Error::other)?;
        let version = ConcreteVersion::new(
            &artifact.version,
            state.policy.registry.limits().projection(),
        )
        .map_err(io::Error::other)?;
        let limits = read_limits(state)?;
        if let Some(format) = text_format(input.format, state.policy.installation.brand()) {
            let result = crate::knowledge_export::export_imported_text_authenticated(
                issuer,
                &caller,
                crate::knowledge_export::ImportedKnowledgeTextSource {
                    installation: state.auth.installation(),
                    input: ImportedKnowledgeInput {
                        registry: &state.policy.registry,
                        id: &id,
                        version: &version,
                        context_alias: artifact.context_alias.as_deref(),
                    },
                    directory: state.policy.index,
                },
                request,
                crate::knowledge_export::KnowledgeTextExportRequest {
                    destination: input.output.as_path(),
                    format,
                },
                &text_policy(state, limits)?,
            )
            .await
            .map_err(|cause| export_error(cause, request))?;
            let current = check_external(&caller, request);
            return text_result(result, input.output, current, request, input.format);
        }
        let admission = issuer
            .admit_authenticated_imported_knowledge_read(&caller, state.auth.installation())
            .map_err(|cause| export_error(cause, request))?;
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
                index: limits,
            },
        )
        .await
        .map_err(|cause| export_error(cause, request))?;
        return match &opened {
            ImportedKnowledgeOpen::RegistryAbsent => {
                admission
                    .recheck()
                    .map_err(|cause| export_error(cause, request))?;
                check_external(&caller, request)?;
                Ok(json!({"status":"registry_absent","format":Format::Native}))
            }
            ImportedKnowledgeOpen::Published(reader) => {
                export_imported_native(state, reader, input.output, limits, &caller, request).await
            }
        };
    }
    let (paths, id, version) = selection.ok_or(io::ErrorKind::InvalidInput)?;
    let live = CapturedPublicationScope::project(paths, *id, state.policy.scope, original)?;
    let binding = live.binding()?;
    let scope = ScopeInput {
        installation: state.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &live,
                record: &binding,
            },
            version,
        }),
        providers: &[],
        binding: state.policy.binding,
        global_version: "",
    };
    let limits = read_limits(state)?;
    match input.format {
        Format::Native => {
            let authority = ProjectRecordAuthority::new(issuer);
            let admission = authority
                .admit_authenticated_read(&caller, request, scope, ProjectReadModule::Knowledge)
                .map_err(|cause| io::Error::new(io::ErrorKind::PermissionDenied, cause))?;
            use crate::knowledge_package_export::{
                KnowledgePackageExportOutcome, KnowledgePackagePublishedFailure,
            };
            match crate::knowledge_package_export::export_native_knowledge_package(
                request,
                &admission,
                &caller,
                input.output.as_path(),
                limits,
            )
            .await?
            {
                KnowledgePackageExportOutcome::NoCorpus => {
                    Ok(json!({"status":"no_corpus","format":Format::Native}))
                }
                KnowledgePackageExportOutcome::Exported { generation } => {
                    let current = original.check().and_then(|()| {
                        if state.live_projects.is_active(paths, *id, version)? {
                            Ok(())
                        } else {
                            Err(io::ErrorKind::NotFound.into())
                        }
                    });
                    if let Err(cause) = current {
                        return Err(io::Error::new(
                            cause.kind(),
                            KnowledgePackagePublishedFailure {
                                generation: *generation,
                                cause,
                            },
                        ));
                    }
                    Ok(
                        json!({"status":"exported","format":Format::Native,"output":input.output,"generation":generation}),
                    )
                }
            }
        }
        format => {
            let profile = text_format(format, state.policy.installation.brand())
                .ok_or(io::ErrorKind::InvalidInput)?;
            let result = crate::knowledge_export::export_text_authenticated(
                issuer,
                &caller,
                scope,
                request,
                crate::knowledge_export::KnowledgeTextExportRequest {
                    destination: input.output.as_path(),
                    format: profile,
                },
                &text_policy(state, limits)?,
            )
            .await
            .map_err(|cause| export_error(cause, request))?;
            let current = original.check().and_then(|()| {
                if state.live_projects.is_active(paths, *id, version)? {
                    Ok(())
                } else {
                    Err(io::ErrorKind::NotFound.into())
                }
            });
            text_result(result, input.output, current, request, input.format)
        }
    }
}

pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let input: Arguments =
        match serde_json::from_value(Value::Object(call.arguments.unwrap_or_default())) {
            Ok(input) => input,
            Err(_) => return failure("invalid_arguments"),
        };
    let format = input.format;
    let snapshot = match server.0.live_projects.snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => return failure("selection_unavailable"),
    };
    let selected = match input.select(&server.0, &snapshot) {
        Ok(selected) => selected,
        Err(code) => return failure(code),
    };
    let Some(parts) = context.extensions.get::<Parts>() else {
        return failure("incoming_credentials_required");
    };
    let local = match parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|v| v.to_str().ok())
    {
        Some("runtime") => false,
        Some("local") => true,
        _ => return failure("credential_origin_required"),
    };
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| !v.is_empty())
    else {
        return failure("credential_required");
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
            ()=context.ct.cancelled()=>return failure("request_cancelled_or_expired"),
            ()=state.stop.cancelled()=>return failure("stopped"),
            ()=tokio::time::sleep_until(deadline.into())=>return failure("request_cancelled_or_expired"),
        };
        jobs.reap();
        if state.stop.is_cancelled() {
            return failure("stopped");
        }
        let owner = Arc::clone(&state);
        jobs.tasks.spawn_blocking(move || {
            let result = handle.block_on(async {
                let request = RequestContext::new(owner.policy.content, &original_cancel, deadline)
                    .map_err(io::Error::other)?;
                execute(
                    &owner,
                    selected.map(|selected| &snapshot.projects[selected]),
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
        result=receive=>match result {
            Ok(Ok(payload))=>crate::mcp_output::typed(payload),
            Ok(Err(error))=>operation_failure(error, format),Err(_)=>failure("native_job_failed"),
        },
        ()=context.ct.cancelled()=>failure("request_cancelled_or_expired"),
        ()=state.stop.cancelled()=>failure("stopped"),
        ()=tokio::time::sleep_until(deadline.into())=>failure("request_cancelled_or_expired"),
    }
}
