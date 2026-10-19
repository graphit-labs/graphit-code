//! Read authored Resources through one live Web PROJECT Hub/read admission.
use super::SourceServer;
use crate::authored_resources::inputs::Query;
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture,
    broker::{
        BrokerVerifier,
        oauth::web::AuthenticatedWebCaller,
        project_access::{
            BrokerProjectAccess, BrokerProjectCaller, BrokerProjectScope, BrokerRecordRead,
        },
    },
};
use graphit_core::artifact_store::CapturedPublicationScope;
use graphit_hub::policy::RequestContext;
use std::io;
fn denied(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, error)
}

impl SourceServer {
    pub(crate) async fn web_authored_resource<'a>(
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
        let projects = state.live_projects.snapshot()?;
        let (project_id, project_version) = query.selection();
        let selection = crate::task_server::request::Selection {
            project_dir: None,
            project_id: Some(project_id),
            project_version: Some(project_version.into()),
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
        let authority = ProjectRecordAuthority::new(state.auth.issuer());
        let read = authority
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
        let mut before = || -> io::Result<()> {
            original.check()?;
            if state.stop.is_cancelled() {
                return Err(io::ErrorKind::NotConnected.into());
            }
            read.recheck().map_err(denied)?;
            if !state.live_projects.is_active(paths, *id, version)? {
                return Err(io::ErrorKind::NotFound.into());
            }
            Ok(())
        };
        before()?;
        crate::authored_resources::read((paths, *id), &scope, query, policy, original, &mut before)
    }
}
