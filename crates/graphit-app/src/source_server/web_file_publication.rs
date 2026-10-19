//! Web publication reads one captured PROJECT; HubWrite retains its own authority.
use super::SourceServer;
use crate::file_publication::{self, Metadata};
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture,
    WebHubPublicationInput,
    broker::{
        BrokerVerifier,
        oauth::web::AuthenticatedWebCaller,
        project_access::{
            BrokerProjectAccess, BrokerProjectCaller, BrokerProjectScope, BrokerRecordRead,
        },
    },
};
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, CapturedSourceReader, CopyPolicy, SourceReadLimits, StoreOwner,
    },
    project_identity::ProjectId,
};
use graphit_hub::{
    SourceRef,
    policy::{HubResourcePolicy, RequestContext},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Query {
    pub project_id: ProjectId,
    pub project_version: String,
    pub publication: Metadata,
}
impl Query {
    pub(crate) fn decode(bytes: &[u8]) -> io::Result<Self> {
        serde_json::from_slice(bytes).map_err(|_| io::ErrorKind::InvalidInput.into())
    }
    pub(crate) fn validate(&self, policy: &HubResourcePolicy) -> io::Result<()> {
        if self.project_version.is_empty()
            || self.project_version.contains('\0')
            || self.project_version.len() > policy.content().max_scalar_bytes
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.publication.validate(policy)?;
        Ok(())
    }
}
fn denied(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, error)
}
impl SourceServer {
    pub(crate) async fn web_file_publication<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        provider: &'a str,
        backend: &'a BrokerProjectAccess,
        original: &'a RequestContext<'a>,
        input: Query,
    ) -> io::Result<Value> {
        let state = &self.0;
        let policy = &state.policy.native_install.hub;
        input.validate(policy)?;
        original.check()?;
        if input.publication.provider != provider {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let projects = state.live_projects.snapshot()?;
        let selection = crate::task_server::request::Selection {
            project_dir: None,
            project_id: Some(input.project_id),
            project_version: Some(input.project_version.clone()),
        };
        let index = selection.select(&projects.projects, 0).map_err(|code| {
            io::Error::from(if code == "selection_ambiguous" {
                io::ErrorKind::AlreadyExists
            } else {
                io::ErrorKind::NotFound
            })
        })?;
        let (paths, id, version) = projects
            .projects
            .get(index)
            .ok_or(io::ErrorKind::NotFound)?;
        let scope =
            CapturedPublicationScope::project(paths, *id, state.policy.scope, original.snapshot())?;
        let binding = scope.binding()?;
        let issuer = state.auth.issuer();
        let authority = ProjectRecordAuthority::new(issuer);
        let source_read = authority
            .admit_broker_read(BrokerRecordRead {
                caller: BrokerProjectCaller::Web(caller),
                verifier,
                backend,
                original,
                provider,
                scope: BrokerProjectScope {
                    installation: state.auth.installation(),
                    project: ProjectCapture {
                        scope: ScopeCapture {
                            live: &scope,
                            record: &binding,
                        },
                        version,
                    },
                    providers: &[],
                },
                module: ProjectReadModule::Hub,
            })
            .await
            .map_err(denied)?;
        let admission = issuer
            .admit_web_hub_publication(WebHubPublicationInput {
                caller,
                verifier,
                original,
                provider,
                installation: state.auth.installation(),
                source: &source_read,
            })
            .map_err(denied)?;
        let mut selection = issuer
            .capture_web_hub_write_service(&admission, provider)
            .map_err(denied)?;
        let lease = selection.credential().map_err(denied)?;
        let before = || -> io::Result<()> {
            original.check()?;
            if state.stop.is_cancelled() {
                return Err(io::ErrorKind::NotConnected.into());
            }
            admission.recheck().map_err(denied)?;
            lease.check().map_err(denied)?;
            let installation = admission.installation().live;
            if !matches!(installation.owner(), StoreOwner::Installation { .. })
                || installation.owner_root() != state.policy.installation.root()
            {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            if !state.live_projects.is_active(paths, *id, version)? {
                return Err(io::ErrorKind::NotFound.into());
            }
            Ok(())
        };
        before()?;
        let (artifact_id, artifact_version) = input.publication.validate(policy)?;
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
            original.snapshot(),
        )?;
        let prepared =
            file_publication::prepare(&input.publication, *id, &reader, policy, original, &before)?;
        before()?;
        let storage = crate::native_storage_credentials::broker_credentials(
            &lease,
            *id,
            &state.native_http,
            state.native_allow_loopback_http,
        )
        .await
        .map_err(io::Error::from)?;
        let current = || -> io::Result<()> {
            before()?;
            storage.check().map_err(io::Error::from)
        };
        let project_metadata = crate::hub_project_metadata::publish(
            storage.store.as_ref(),
            &storage.prefix,
            (paths, *id),
            policy.content().max_input_bytes,
            original,
            &current,
        )
        .await?;
        let publication = file_publication::publish(
            file_publication::Input {
                store: storage.store.as_ref(),
                storage_prefix: &storage.prefix,
                prepared,
            },
            original,
            current,
        )
        .await
        .map_err(|cause| crate::hub_project_metadata::artifact_failure(project_metadata, cause))?;
        Ok(
            json!({"status":"published","publication":publication,"project_metadata":project_metadata}),
        )
    }
}

/// Preserve typed kernel effects while exposing only established public cause codes.
pub(crate) fn failure_body(error: &io::Error) -> Value {
    let metadata = error
        .get_ref()
        .and_then(|e| e.downcast_ref::<crate::hub_project_metadata::Failure>());
    let cause = metadata.map_or(error, |effect| &effect.cause);
    let code = match (crate::native_publication::cause_code(cause), cause.kind()) {
        ("conditional_conflict", _) => "file_publication_conflict",
        (_, io::ErrorKind::PermissionDenied) => "authentication_or_permission_denied",
        (_, io::ErrorKind::Interrupted | io::ErrorKind::TimedOut) => "request_cancelled_or_expired",
        (_, io::ErrorKind::InvalidInput) => "invalid_arguments",
        (_, io::ErrorKind::AlreadyExists | io::ErrorKind::WouldBlock) => {
            "file_publication_conflict"
        }
        _ => "file_publication_failed",
    };
    let mut body = if let Some(effect) = cause
        .get_ref()
        .and_then(|e| e.downcast_ref::<file_publication::Failure>())
    {
        let public_cause = match crate::native_publication::cause_code(&effect.cause) {
            "native_operation_failed" => "file_operation_failed",
            value => value,
        };
        json!({"status":"error","code":code,"stage":effect.stage,"publication":effect.receipt,
            "cause":public_cause,"owned_temporary_residue":crate::native_publication::cleanup_residue(&effect.cause)})
    } else if let Some(effect) = cause
        .get_ref()
        .and_then(|error| error.downcast_ref::<crate::native_publication::PublicationFailure>())
    {
        json!({"status":"error","code":code,"stage":effect.stage,"publication":effect.receipt,
            "cause":crate::native_publication::cause_code(&effect.cause),
            "owned_temporary_residue":crate::native_publication::cleanup_residue(&effect.cause)})
    } else {
        json!({"status":"error","code":code,"cause":crate::native_publication::cause_code(cause)})
    };
    if let Some(effect) = metadata {
        body["project_metadata"] = json!(effect.effect);
        if body.get("stage").is_none() {
            body["stage"] = json!(effect.stage);
        }
    }
    body
}
