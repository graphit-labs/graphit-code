//! One reconciliation producer shared by the CLI and Runtime MCP.
use super::file_installation::FileEffectFailure;
use super::*;
use crate::{remote_file_install::client::UpdateAllArguments, sync_entry::Arguments};

pub(super) fn definition() -> Tool {
    Tool::new(
        crate::sync_entry::TOOL,
        "Reconcile installed Hub artifacts at their recorded versions, native Graphit skills/agents, Coordinator, Git/agent hooks and MCP. Administrative Runtime access required; user modifications are preserved as explicit conflicts.",
        json!({"type":"object","properties":{
            "global":{"type":"boolean","default":false},
            "project_dir":{"type":["string","null"]},"project_version":{"type":["string","null"]},
            "agent":{"type":["string","null"]},"coordinator_source":{"type":["string","null"]}
        },"additionalProperties":false})
        .as_object()
        .expect("static sync schema")
        .clone(),
    )
}
pub(super) async fn execute(
    state: &State,
    value: Value,
    credential: &str,
    request: &RequestContext<'_>,
) -> io::Result<Value> {
    let mut input: Arguments = serde_json::from_value(value)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    if input.global
        && (input.project_dir.is_some()
            || input.project_version.is_some()
            || input.coordinator_source.is_some())
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    if input
        .coordinator_source
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    // All reads and effects use the original administrative caller, never a replacement runtime key.
    let caller = state
        .auth
        .issuer()
        .authenticate_runtime(credential, request)
        .map_err(|cause| io::Error::new(io::ErrorKind::PermissionDenied, cause))?;
    let session = state
        .auth
        .issuer()
        .bootstrap_admin(&caller)
        .map_err(|cause| io::Error::new(io::ErrorKind::PermissionDenied, cause))?;
    let parent = state
        .auth
        .issuer()
        .admin_parent(&caller, &session)
        .map_err(|cause| io::Error::new(io::ErrorKind::PermissionDenied, cause))?;
    for permission in [
        graphit_auth::Permission::Configure,
        graphit_auth::Permission::ManageAccessGrants,
        graphit_auth::Permission::ManageCache,
        graphit_auth::Permission::External(graphit_auth::ExternalEffect::PublishedFilesystem),
    ] {
        parent
            .authorize(permission)
            .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
    }
    let projects = state.live_projects.snapshot()?;
    if !input.global {
        let index = match input.project_dir.as_deref() {
            Some(path) if path.is_absolute() => select_project(
                &projects.projects,
                &std::fs::canonicalize(path)?,
                input.project_version.as_deref(),
            )
            .map_err(io::Error::other)?,
            Some(_) => return Err(io::ErrorKind::InvalidInput.into()),
            None if input.project_version.is_none() => {
                projects.current.ok_or(io::ErrorKind::NotFound)?
            }
            None => return Err(io::ErrorKind::InvalidInput.into()),
        };
        let (paths, _, version) = projects
            .projects
            .get(index)
            .ok_or(io::ErrorKind::NotFound)?;
        input.project_dir = Some(paths.root().to_owned());
        input.project_version = Some(version.version().to_owned());
    }
    input.agent = Some(match input.agent {
        Some(binding) => binding,
        None => state
            .roles
            .configured_binding(input.project_dir.as_deref(), request)?,
    });
    let mut result = json!({"status":"ok","operation":"sync","steps":{}});
    for global in [true, false] {
        if !global && input.global {
            continue;
        }
        let step = if global { "hub_global" } else { "hub_project" };
        let hub = UpdateAllArguments {
            global,
            project_dir: if global {
                None
            } else {
                input.project_dir.clone()
            },
            project_version: if global {
                None
            } else {
                input.project_version.clone()
            },
            binding: None,
            identity_provider: None,
        };
        match super::file_installation::update::sync(state, hub, credential, request).await {
            Ok(value) => result["steps"][step] = value,
            Err(cause) => {
                result["status"] = "error".into();
                result["failed_step"] = step.into();
                if let Some(failure) = cause
                    .get_ref()
                    .and_then(|e| e.downcast_ref::<FileEffectFailure>())
                {
                    result["steps"][step] = failure.outcome.clone();
                }
                return Err(FileEffectFailure::observed(result, cause));
            }
        }
    }
    let roles = state.roles.clone();
    let credential = zeroize::Zeroizing::new(credential.to_owned());
    let cancel = request.cancellation().clone();
    let deadline = request.snapshot().required_deadline()?;
    let content = state.policy.content;
    let native = tokio::task::spawn_blocking(move || {
        let original = RequestContext::new(content, &cancel, deadline).map_err(io::Error::other)?;
        roles.sync_native(&input, &credential, &original)
    })
    .await
    .unwrap_or_else(|cause| Err(io::Error::other(cause)));
    match native {
        Ok(value) => {
            result["steps"]["native"] = value;
            Ok(result)
        }
        Err(cause) => {
            result["status"] = "error".into();
            result["failed_step"] = "native".into();
            if let Some(failure) = cause
                .get_ref()
                .and_then(|e| e.downcast_ref::<FileEffectFailure>())
            {
                result["steps"]["native"] = failure.outcome.clone();
            }
            Err(FileEffectFailure::observed(result, cause))
        }
    }
}
