//! FILE read ingress. Web proofs are retained through the existing SDK readers.
use super::{SourceServer, State};
use crate::remote_file_install::{FileSelection, FileTransport, client::Transport};
use graphit_auth::{
    InstalledHubReadAdmission, ProjectCapture, ProjectReadModule, ProjectRecordAuthority,
    ScopeCapture,
    broker::{
        BrokerVerifier,
        oauth::web::AuthenticatedWebCaller,
        project_access::{
            BrokerProjectAccess, BrokerProjectCaller, BrokerProjectScope, BrokerRecordRead,
        },
    },
};
use graphit_core::{
    artifact_store::{ArtifactStore, CapturedPublicationScope, StoreOwner},
    project_identity::ProjectIdentityStore,
};
use graphit_hub::{
    ArtifactId, ConcreteVersion, RemoteFileOrigin, SourceOrigin,
    policy::RequestContext,
    registry::{HubRegistry, HubScope},
};
use serde_json::{Value, json};
use std::io;
pub(crate) mod inputs;
mod installed;
mod listing;
use inputs::{Query, Scope};

fn denied(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, error)
}
impl SourceServer {
    pub(crate) async fn web_file_read<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        provider: &'a str,
        backend: &'a BrokerProjectAccess,
        original: &'a RequestContext<'a>,
        query: Query,
    ) -> io::Result<Vec<u8>> {
        let state = &self.0;
        let policy = &state.policy.native_install.hub;
        query.validate(policy)?;
        original.check()?;
        let issuer = state.auth.issuer();
        let read = issuer
            .admit_web_installed_hub_read(
                caller,
                verifier,
                original,
                provider,
                state.auth.installation(),
            )
            .map_err(denied)?;
        let projects = state.live_projects.snapshot()?;
        let selected = match query.scope() {
            Some(Scope::Project {
                project_id,
                project_version,
            }) => {
                let selection = crate::task_server::request::Selection {
                    project_dir: None,
                    project_id: Some(*project_id),
                    project_version: Some(project_version.clone()),
                };
                let index = selection.select(&projects.projects, 0).map_err(|code| {
                    io::Error::from(if code == "selection_ambiguous" {
                        io::ErrorKind::AlreadyExists
                    } else {
                        io::ErrorKind::NotFound
                    })
                })?;
                Some(
                    projects
                        .projects
                        .get(index)
                        .ok_or(io::ErrorKind::NotFound)?,
                )
            }
            _ => None,
        };
        let project = selected
            .map(|(paths, id, _)| {
                CapturedPublicationScope::project(
                    paths,
                    *id,
                    state.policy.scope,
                    original.snapshot(),
                )
            })
            .transpose()?;
        let binding = project
            .as_ref()
            .map(CapturedPublicationScope::binding)
            .transpose()?;
        let authority = ProjectRecordAuthority::new(issuer);
        let project_read = match (selected, project.as_ref(), binding.as_ref()) {
            (Some((_, _, version)), Some(project), Some(binding)) => Some(
                authority
                    .admit_broker_read(BrokerRecordRead {
                        caller: BrokerProjectCaller::Web(caller),
                        verifier,
                        backend,
                        original,
                        scope: BrokerProjectScope {
                            installation: state.auth.installation(),
                            project: ProjectCapture {
                                scope: ScopeCapture {
                                    live: project,
                                    record: binding,
                                },
                                version,
                            },
                            providers: &[],
                        },
                        provider,
                        module: ProjectReadModule::Hub,
                    })
                    .await
                    .map_err(denied)?,
            ),
            _ => None,
        };
        let live = read.installation().live;
        let StoreOwner::Installation { installation_id } = live.owner() else {
            return Err(io::ErrorKind::PermissionDenied.into());
        };
        let global_cache = matches!(query.scope(), Some(Scope::GlobalCache));
        let before = || -> io::Result<()> {
            original.check()?;
            if state.stop.is_cancelled() {
                return Err(io::ErrorKind::NotConnected.into());
            }
            read.recheck().map_err(denied)?;
            if live.owner_root() != state.policy.installation.root()
                || global_cache
                    && (state.policy.registry.path().parent() != Some(live.owner_root())
                        || !matches!(state.policy.registry.scope(), HubScope::Global { installation_id: id } if id == installation_id.as_str()))
            {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            if let Some(project_read) = &project_read {
                read.check_project_source(project_read).map_err(denied)?;
            }
            if let Some((paths, id, version)) = selected
                && !state.live_projects.is_active(paths, *id, version)?
            {
                return Err(io::ErrorKind::NotFound.into());
            }
            Ok(())
        };
        before()?;
        let registry = if matches!(query, Query::Remote(_)) {
            None
        } else {
            Some(if let Some((paths, id, _)) = selected {
                let identity = ProjectIdentityStore::at(paths)
                    .load()?
                    .ok_or(io::ErrorKind::NotFound)?;
                if identity.id() != *id {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                HubRegistry::project(paths, &identity, policy.hub())?
            } else {
                state.policy.registry.clone()
            })
        };
        before()?;
        let slot = project.as_ref().map_or(
            graphit_core::artifact_store::PublicationSlot::Scope,
            CapturedPublicationScope::publication_slot,
        );
        let result = match query {
            Query::Remote(input) => remote(state, &read, input, &before).await?,
            Query::Installed(input) => {
                let scope = CapturedPublicationScope::installation(
                    &state.policy.installation,
                    installation_id.clone(),
                    [state.policy.installation.root().to_owned()],
                    state.policy.scope,
                    original.snapshot(),
                )?;
                let cache = ArtifactStore::new(scope, policy.artifacts());
                installed::content(
                    state,
                    &read,
                    registry.as_ref().ok_or(io::ErrorKind::InvalidInput)?,
                    &cache,
                    (slot, input),
                    &before,
                )
                .await?
            }
            Query::List(input) => {
                listing::read(
                    state,
                    &read,
                    registry.as_ref().ok_or(io::ErrorKind::InvalidInput)?,
                    (slot, input),
                    &before,
                )
                .await?
            }
        };
        before()?;
        let bytes = serde_json::to_vec(&result).map_err(io::Error::other)?;
        if bytes.len() > policy.content().max_output_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        before()?;
        Ok(bytes)
    }
}
async fn remote(
    state: &State,
    read: &InstalledHubReadAdmission<'_>,
    input: crate::remote_file_install::client::RemoteContentArguments,
    before: &(dyn Fn() -> io::Result<()> + Sync),
) -> io::Result<Value> {
    let policy = &state.policy.native_install.hub;
    let issuer = state.auth.issuer();
    let mut service = match input.selection.transport {
        Transport::Broker => issuer.capture_installed_hub_service(read, &input.selection.provider),
        Transport::Standalone => issuer.capture_installed_hub_standalone_s3_service(
            read,
            &input.selection.provider,
            input.selection.profile.as_deref(),
        ),
    }
    .map_err(denied)?;
    let lease = service.credential().map_err(denied)?;
    let subject = lease
        .user_subject()
        .map_err(denied)?
        .ok_or(io::ErrorKind::PermissionDenied)?;
    let producer = input
        .selection
        .producer
        .parse()
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    before()?;
    let transport = FileTransport::open(
        &lease,
        Some(subject),
        producer,
        &state.native_http,
        state.native_allow_loopback_http,
    )
    .await?;
    let id = ArtifactId::new(&input.selection.id, policy.projection()).map_err(io::Error::other)?;
    let literal = ConcreteVersion::new(&input.selection.version, policy.projection())
        .map_err(io::Error::other)?;
    let resolved = transport
        .resolve(
            FileSelection {
                producer,
                kind: input.selection.kind.artifact_type(),
                id: &id,
                version: super::file_installation::version(&input.selection, &literal),
            },
            policy,
        )
        .await?;
    before()?;
    let stage = transport
        .download(&resolved, state.policy.installation.root(), policy)
        .await?;
    let content = transport.read_text_file(
        stage.path(),
        &input.path,
        state.policy.index,
        state.policy.content.max_output_bytes,
    );
    let cleaned = stage.close();
    let content = match (content, cleaned) {
        (Ok(content), Ok(())) => content,
        (Err(read), Err(cleanup)) => {
            return Err(io::Error::other(ReadCleanupFailure { read, cleanup }));
        }
        (Err(error), Ok(())) | (Ok(_), Err(error)) => return Err(error),
    };
    before()?;
    transport.check()?;
    lease.check().map_err(denied)?;
    Ok(
        json!({"status":"ok","source":"remote","producer":producer,"artifact_type":input.selection.kind.artifact_type(),"id":input.selection.id,"version":resolved.version,"content":content,"cache":"ephemeral"}),
    )
}

#[derive(Debug)]
struct ReadCleanupFailure {
    read: io::Error,
    cleanup: io::Error,
}
impl std::fmt::Display for ReadCleanupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FILE read and temporary stage cleanup failed ({:?}, {:?})",
            self.read.kind(),
            self.cleanup.kind()
        )
    }
}
impl std::error::Error for ReadCleanupFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.read)
    }
}
