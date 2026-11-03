//! Local FILE effects retain their actual Web administrator and physical capture.
use super::{SourceServer, State};
use graphit_auth::{
    ProjectCapture, ScopeCapture, WebHubMutationAdmission, WebHubProjectMutationInput,
    broker::{
        BrokerVerifier,
        oauth::web::AuthenticatedWebCaller,
        project_access::{BrokerProjectAccess, BrokerProjectScope},
    },
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, StoreOwner},
    project_identity::ProjectId,
    project_paths::ProjectPaths,
    version_context::VersionContext,
};
use graphit_hub::{policy::RequestContext, registry::HubScope};
use serde_json::{Value, json};
use std::io;
#[cfg(all(test, target_os = "linux"))]
pub(crate) mod cache_commit_pause;
pub(crate) mod inputs;
mod install;
mod local;
mod local_input;
mod update;
pub(crate) use super::file_installation::FileEffectFailure;
use super::web_file_reads::inputs::Scope;
use inputs::Query;
type Project = (ProjectPaths, ProjectId, VersionContext);
fn denied(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, error)
}
struct EffectContext<'a> {
    state: &'a State,
    caller: &'a AuthenticatedWebCaller<'a>,
    verifier: &'a BrokerVerifier,
    provider: &'a str,
    backend: &'a BrokerProjectAccess,
    original: &'a RequestContext<'a>,
    admission: &'a WebHubMutationAdmission<'a>,
    current: &'a dyn Fn() -> io::Result<()>,
}
impl SourceServer {
    pub(crate) async fn web_file_mutation<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        provider: &'a str,
        backend: &'a BrokerProjectAccess,
        original: &'a RequestContext<'a>,
        query: Query,
    ) -> io::Result<Value> {
        let state = &self.0;
        query.validate(&state.policy.native_install.hub)?;
        original.check()?;
        let operation = query.operation();
        let mut outcome = json!({"status":"error","operation":operation.name(),"scope":query.scope(),"cache":"not_attempted","publication":"not_attempted"});
        let projects = state.live_projects.snapshot()?;
        let selected = match query.scope() {
            Scope::GlobalCache => None,
            Scope::Project {
                project_id,
                project_version,
            } => {
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
        };
        let scope = selected
            .map(|(paths, id, _)| {
                CapturedPublicationScope::project(
                    paths,
                    *id,
                    state.policy.scope,
                    original.snapshot(),
                )
            })
            .transpose()?;
        let binding = scope
            .as_ref()
            .map(CapturedPublicationScope::binding)
            .transpose()?;
        let issuer = state.auth.issuer();
        let admission = match (selected, scope.as_ref(), binding.as_ref()) {
            (Some((_, _, version)), Some(scope), Some(binding)) => issuer
                .admit_web_hub_project_mutation(WebHubProjectMutationInput {
                    caller,
                    verifier,
                    original,
                    provider,
                    backend,
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
                })
                .await
                .map_err(denied)?,
            (None, None, None) => issuer
                .admit_web_hub_global_mutation(
                    caller,
                    verifier,
                    original,
                    provider,
                    state.auth.installation(),
                )
                .map_err(denied)?,
            _ => return Err(io::ErrorKind::InvalidData.into()),
        };
        let global = selected.is_none();
        let current = || -> io::Result<()> {
            original.check()?;
            if state.stop.is_cancelled() {
                return Err(io::ErrorKind::NotConnected.into());
            }
            admission.recheck().map_err(denied)?;
            let live = admission.installation().live;
            let StoreOwner::Installation { installation_id } = live.owner() else {
                return Err(io::ErrorKind::PermissionDenied.into());
            };
            if live.owner_root() != state.policy.installation.root()
                || global
                    && (state.policy.registry.path().parent() != Some(live.owner_root())
                        || !matches!(state.policy.registry.scope(),HubScope::Global{installation_id:id} if id==installation_id.as_str()))
            {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            if let Some((paths, id, version)) = selected
                && !state.live_projects.is_active(paths, *id, version)?
            {
                return Err(io::ErrorKind::NotFound.into());
            }
            Ok(())
        };
        let context = EffectContext {
            state,
            caller,
            verifier,
            provider,
            backend,
            original,
            admission: &admission,
            current: &current,
        };
        let result = async {
            current()?;
            match query {
                Query::UpdateAll(input) => {
                    update::apply(&context, input, selected, scope.as_ref(), &mut outcome).await
                }
                Query::Local(input) => {
                    local::apply(
                        &context,
                        input,
                        selected.ok_or(io::ErrorKind::InvalidInput)?,
                        scope.as_ref().ok_or(io::ErrorKind::InvalidInput)?,
                        &mut outcome,
                    )
                    .await
                }
                Query::Install(op, input) => {
                    install::apply(&context, input, op, selected, scope.as_ref(), &mut outcome)
                        .await
                }
                Query::Lifecycle(op, input) => {
                    let native = crate::remote_file_install::lifecycle::Arguments {
                        kind: input.kind.artifact_type(),
                        scope: if global {
                            crate::remote_file_install::lifecycle::Scope::GlobalCache
                        } else {
                            crate::remote_file_install::lifecycle::Scope::Project
                        },
                        id: input.id,
                        version: input.version,
                        project_dir: None,
                        project_version: None,
                        binding: input.binding,
                    };
                    outcome["artifact_type"] =
                        serde_json::to_value(native.kind).map_err(io::Error::other)?;
                    outcome["id"] = native.id.clone().into();
                    if let Some(selected) = selected {
                        super::file_installation::lifecycle::apply_selected(
                            state,
                            op.lifecycle()?,
                            &native,
                            selected,
                            original,
                            &mut outcome,
                            &current,
                        )
                    } else {
                        super::file_installation::lifecycle::global::apply(
                            state,
                            &native,
                            original,
                            &mut outcome,
                            &current,
                        )
                    }
                }
            }
        }
        .await;
        match result {
            Ok(()) => {
                current().map_err(|error| FileEffectFailure::observed(outcome.clone(), error))?;
                outcome["status"] = "ok".into();
                Ok(outcome)
            }
            Err(error) => {
                if outcome["cache"] == "in_progress" {
                    outcome["cache"] = "not_confirmed".into();
                }
                Err(FileEffectFailure::observed(outcome, error))
            }
        }
    }
}
