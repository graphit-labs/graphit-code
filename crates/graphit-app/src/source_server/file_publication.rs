//! PROJECT FILE selection and HubWrite remain independent admissions.
use super::*;
use crate::file_publication::{self, Metadata};
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ServiceMode};
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, CapturedSourceReader, CopyPolicy, SourceReadLimits,
    },
    native_path::NativePath,
};
use graphit_hub::SourceRef;

pub(super) const TOOL: &str = "graphit_hub_publish_file";
pub(super) fn definition() -> Tool {
    Tool::new(
        TOOL,
        "Publish a captured PROJECT FILE artifact through the configured HubWrite provider. Selects the maintained resource tree by type, full ID and concrete version.",
        json!({
            "type":"object","properties":{
                "publication":{"type":"object","properties":{
                    "provider":{"type":"string"},
                    "artifact_type":{"type":"string","enum":["skill","command","workflow","agent","rule","mcp","language","power"]},
                    "id":{"type":"string"},"version":{"type":"string"},"name":{"type":"string"},
                    "description":{"type":["string","null"]},"author":{"type":["string","null"]},
                    "tags":{"type":"array","items":{"type":"string"}},
                    "dependencies":{"type":["array","null"],"items":{"type":"object","properties":{
                        "artifact_type":{"type":"string","enum":["skill","command","workflow","agent","rule","mcp","ast","knowledge","language","power"]},
                        "id":{"type":"string"},"selector":{"type":"string"}
                    },"required":["artifact_type","id","selector"],"additionalProperties":false}},
                    "expected_sha256":{"type":["string","null"],"description":"Current archive SHA256 required for a changed digest at the same concrete version."},
                    "set_latest":{"type":"boolean","description":"Explicitly set latest to the published concrete version; omission preserves the current pointer."}
                },"required":["provider","artifact_type","id","version","name"],"additionalProperties":false},
                "project_dir":{"type":"string","description":"PROJECT path as a literal Unicode string."},
                "project_version":{"type":"string"}
            },"required":["publication","project_dir"],"additionalProperties":false
        }).as_object().unwrap().clone(),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Arguments {
    pub(super) publication: Metadata,
    pub(super) project_dir: NativePath,
    #[serde(default, deserialize_with = "present")]
    pub(super) project_version: Option<String>,
}

pub(super) async fn execute(
    state: &State,
    selected: &(ProjectPaths, ProjectId, VersionContext),
    input: Arguments,
    credential: &str,
    local: bool,
    request: &RequestContext<'_>,
) -> io::Result<Value> {
    let (paths, id, version) = selected;
    if !state.live_projects.is_active(paths, *id, version)? {
        return Err(io::ErrorKind::NotFound.into());
    }
    let policy = &state.policy.native_install.hub;
    let (artifact_id, artifact_version) = input.publication.validate(policy)?;
    let issuer = state.auth.issuer();
    let caller = if local {
        issuer.authenticate_local(credential, request)
    } else {
        issuer.authenticate_runtime(credential, request)
    }
    .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
    let scope =
        CapturedPublicationScope::project(paths, *id, state.policy.scope, request.snapshot())?;
    let binding = scope.binding()?;
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = authority
        .admit_authenticated_read(
            &caller,
            request,
            ScopeInput {
                installation: state.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &scope,
                        record: &binding,
                    },
                    version,
                }),
                providers: &[],
                binding: state.policy.binding,
                global_version: "",
            },
            ProjectReadModule::Hub,
        )
        .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
    let mut selection = issuer
        .capture_hub_write_service(&caller, &input.publication.provider)
        .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
    let verifier = if selection.mode().map_err(io::Error::other)? == ServiceMode::BrokerNative {
        Some(
            state
                .native_verifiers
                .as_ref()
                .ok_or(io::ErrorKind::NotConnected)?
                .get(&mut selection)
                .map_err(io::Error::other)?,
        )
    } else {
        None
    };
    let lease = match verifier.as_deref() {
        Some(verifier) => selection.native_credential(verifier).await,
        None => selection.credential(),
    }
    .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
    let before = || -> io::Result<()> {
        request.check()?;
        admission.recheck().map_err(io::Error::other)?;
        lease.check().map_err(io::Error::other)?;
        if !state.live_projects.is_active(paths, *id, version)? {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(())
    };
    before()?;
    let source = SourceRef::resources(
        paths,
        *id,
        input.publication.artifact_type.artifact_type(),
        artifact_id,
        artifact_version,
        policy.projection(),
    )
    .map_err(io::Error::other)?;
    let relative = source
        .directory()
        .as_path()
        .strip_prefix(paths.root())
        .map_err(io::Error::other)?;
    let reader = CapturedSourceReader::capture_project(
        &scope,
        relative,
        graphit_fs::NodeKind::Directory,
        &policy.snapshot_policy(CopyPolicy::RejectFileLinks, [])?,
        SourceReadLimits::new(64 * 1024, 64 * 1024, policy.copy().total_file_bytes_limit())?,
        request.snapshot(),
    )?;
    let prepared =
        file_publication::prepare(&input.publication, *id, &reader, policy, request, &before)?;
    before()?;
    let storage = crate::native_storage_credentials::broker_credentials(
        &lease,
        *id,
        &state.native_http,
        state.native_allow_loopback_http,
    )
    .await
    .map_err(io::Error::from)?;
    let project_metadata = crate::hub_project_metadata::publish(
        storage.store.as_ref(),
        &storage.prefix,
        (paths, *id),
        policy.content().max_input_bytes,
        request,
        &|| {
            before()?;
            storage.check().map_err(io::Error::from)
        },
    )
    .await?;
    let receipt = file_publication::publish(
        file_publication::Input {
            store: storage.store.as_ref(),
            storage_prefix: &storage.prefix,
            prepared,
        },
        request,
        || {
            before()?;
            storage.check().map_err(io::Error::from)
        },
    )
    .await
    .map_err(|cause| crate::hub_project_metadata::artifact_failure(project_metadata, cause))?;
    Ok(json!({"status":"published","publication":receipt,"project_metadata":project_metadata}))
}

pub(super) fn operation_failure(error: io::Error) -> CallToolResult {
    let metadata = error
        .get_ref()
        .and_then(|e| e.downcast_ref::<crate::hub_project_metadata::Failure>());
    let error = metadata.map_or(&error, |effect| &effect.cause);
    let code = match (crate::native_publication::cause_code(error), error.kind()) {
        ("conditional_conflict", _) => "file_publication_conflict",
        (_, io::ErrorKind::PermissionDenied) => "authentication_or_permission_denied",
        (_, io::ErrorKind::Interrupted | io::ErrorKind::TimedOut) => "request_cancelled_or_expired",
        (_, io::ErrorKind::InvalidInput) => "invalid_arguments",
        (_, io::ErrorKind::AlreadyExists) => "file_publication_conflict",
        _ => "file_publication_failed",
    };
    let mut body = if let Some(effect) = error
        .get_ref()
        .and_then(|e| e.downcast_ref::<file_publication::Failure>())
    {
        let cause = match crate::native_publication::cause_code(&effect.cause) {
            "native_operation_failed" => "file_operation_failed",
            cause => cause,
        };
        json!({"status":"error","code":code,"stage":effect.stage,"publication":effect.receipt,"cause":cause,"owned_temporary_residue":crate::native_publication::cleanup_residue(&effect.cause)})
    } else {
        json!({"status":"error","code":code,"cause":crate::native_publication::cause_code(error)})
    };
    if let Some(effect) = metadata {
        body["project_metadata"] = json!(effect.effect);
        if body.get("stage").is_none() {
            body["stage"] = json!(effect.stage);
            body["cause"] = json!(crate::native_publication::cause_code(error));
        }
    }
    crate::mcp_output::typed_error(body)
}
