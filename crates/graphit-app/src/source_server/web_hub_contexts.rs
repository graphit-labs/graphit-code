//! Local PROJECT discovery uses the Web caller's Hub authority.
use super::SourceServer;
use graphit_auth::{
    AuthError, ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture,
    broker::{
        BrokerVerifier,
        oauth::web::AuthenticatedWebCaller,
        project_access::{
            BrokerProjectAccess, BrokerProjectCaller, BrokerProjectError, BrokerProjectScope,
            BrokerRecordRead,
        },
    },
};
use graphit_core::{artifact_store::CapturedPublicationScope, native_path::NativePath};
use graphit_hub::policy::RequestContext;
use serde_json::json;
use std::io;

impl SourceServer {
    pub(crate) async fn web_hub_contexts<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        provider: &'a str,
        backend: &'a BrokerProjectAccess,
        original: &'a RequestContext<'a>,
    ) -> io::Result<Vec<u8>> {
        let state = &self.0;
        let check = || -> io::Result<()> {
            original.check()?;
            if state.stop.is_cancelled() {
                return Err(io::ErrorKind::NotConnected.into());
            }
            Ok(())
        };
        check()?;
        let snapshot = state.live_projects.snapshot()?;
        let mut captures = Vec::new();
        for (index, (paths, id, version)) in snapshot.projects.iter().enumerate() {
            check()?;
            if state.live_projects.is_active(paths, *id, version)? {
                captures.push((
                    index,
                    CapturedPublicationScope::project(
                        paths,
                        *id,
                        state.policy.scope,
                        original.snapshot(),
                    )?,
                ));
            }
        }
        let bindings = captures
            .iter()
            .map(|(_, scope)| scope.binding())
            .collect::<io::Result<Vec<_>>>()?;
        let authority = ProjectRecordAuthority::new(state.auth.issuer());
        let mut admissions = Vec::new();
        let mut contexts = Vec::new();
        let mut current = None;
        for ((index, scope), binding) in captures.iter().zip(&bindings) {
            check()?;
            let (paths, id, version) = &snapshot.projects[*index];
            let admission = match authority
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
                                live: scope,
                                record: binding,
                            },
                            version,
                        },
                        providers: &[],
                    },
                    module: ProjectReadModule::Hub,
                })
                .await
            {
                Ok(admission) => admission,
                Err(
                    BrokerProjectError::Denied | BrokerProjectError::Authority(AuthError::Denied),
                ) => continue,
                Err(error) => {
                    check()?;
                    return Err(io::Error::other(error));
                }
            };
            if contexts.len()
                == state
                    .policy
                    .native_install
                    .hub
                    .content()
                    .max_collection_entries
            {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            if snapshot.current == Some(*index) {
                current = Some(contexts.len());
            }
            contexts.push(json!({
                "project_dir": NativePath::new(paths.root().to_owned())?,
                "project_id": id,
                "project_version": version.version(),
            }));
            admissions.push((admission, *index));
        }
        let before = || -> io::Result<()> {
            check()?;
            for (admission, index) in &admissions {
                admission
                    .recheck()
                    .map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error))?;
                let (paths, id, version) = &snapshot.projects[*index];
                if !state.live_projects.is_active(paths, *id, version)? {
                    return Err(io::ErrorKind::NotFound.into());
                }
            }
            Ok(())
        };
        before()?;
        #[derive(serde::Serialize)]
        struct Output<'a> {
            contexts: &'a [serde_json::Value],
            current: Option<&'a serde_json::Value>,
            projects: Vec<crate::live_project_selections::ProjectName>,
        }
        let bytes = serde_json::to_vec(&Output {
            contexts: &contexts,
            current: current.map(|index| &contexts[index]),
            projects: crate::live_project_selections::project_names(
                admissions
                    .iter()
                    .map(|(_, index)| &snapshot.projects[*index]),
                original.snapshot(),
            )?,
        })
        .map_err(io::Error::other)?;
        if bytes.len() > state.policy.native_install.hub.content().max_output_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        before()?;
        Ok(bytes)
    }
}
