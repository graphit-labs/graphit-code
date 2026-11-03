//! Collective FILE updates reuse each ingress's existing admitted publisher.
use super::{
    EffectContext, Project,
    inputs::{Install, Operation, UpdateAll},
    install,
};
use crate::remote_file_install::client::{RemoteArguments, Transport};
use graphit_core::{
    artifact_store::CapturedPublicationScope, project_identity::ProjectIdentityStore,
};
use graphit_hub::{InstallationTarget, RemoteFileOrigin, registry::HubRegistry};
use serde_json::{Value, json};
use std::io;

pub(super) async fn apply(
    ctx: &EffectContext<'_>,
    input: UpdateAll,
    selected: Option<&Project>,
    project_scope: Option<&CapturedPublicationScope>,
    outcome: &mut Value,
) -> io::Result<()> {
    (ctx.current)()?;
    let policy = &ctx.state.policy.native_install.hub;
    let global = selected.is_none();
    let registry = if let Some((paths, id, _)) = selected {
        let identity = ProjectIdentityStore::at(paths)
            .load()?
            .ok_or(io::ErrorKind::NotFound)?;
        if identity.id() != *id {
            return Err(io::ErrorKind::InvalidData.into());
        }
        HubRegistry::project(paths, &identity, policy.hub())?
    } else {
        ctx.state.policy.registry.clone()
    };
    let document = registry.load()?;
    let slot = project_scope.map_or(
        graphit_core::artifact_store::PublicationSlot::Scope,
        CapturedPublicationScope::publication_slot,
    );
    let candidates: Vec<_> = super::super::file_installation::update::remote_installations(
        &document,
        global,
        input.binding.unwrap_or(ctx.state.policy.binding),
        slot,
    )
    .filter(|(_, _, installed)| {
        installed.original_source.remote_file_origin().is_some()
            && (!global || installed.target == InstallationTarget::GlobalCache)
    })
    .collect();
    *outcome = json!({"status":"error","operation":"update_all","scope":input.scope,"global":global,
        "items":candidates.iter().map(|(kind,id,installed)| json!({"artifact_type":kind,"id":id,
            "producer":installed.original_source.producer(),"previous_version":installed.original_source.version(),
            "target":installed.target,"status":"not_attempted"})).collect::<Vec<_>>()});
    if serde_json::to_vec(outcome).map_err(io::Error::other)?.len()
        > policy.content().max_output_bytes
    {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    for (index, (kind, id, installed)) in candidates.into_iter().enumerate() {
        let mut effects =
            json!({"status":"error","cache":"not_attempted","publication":"not_attempted"});
        let result = async {
            (ctx.current)()?;
            let observed = registry.load()?;
            if !observed
                .get_version_for_consumer(
                    &graphit_hub::registry::consumer_for_artifact(slot, kind),
                    kind,
                    id,
                    installed.original_source.version(),
                )
                .is_some_and(|artifact| {
                    artifact
                        .installations
                        .iter()
                        .any(|entry| entry.same_source(installed))
                })
            {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            let (transport, provider, profile) = match installed
                .original_source
                .remote_file_origin()
                .ok_or(io::ErrorKind::InvalidData)?
            {
                RemoteFileOrigin::Broker { provider } => {
                    (Transport::Broker, provider.clone(), None)
                }
                RemoteFileOrigin::Standalone { provider, profile } => {
                    (Transport::Standalone, provider.clone(), profile.clone())
                }
            };
            let kind = serde_json::from_value(json!(kind)).map_err(io::Error::other)?;
            let scope = match selected {
                Some((_, project_id, version)) => super::Scope::Project {
                    project_id: *project_id,
                    project_version: version.version().to_owned(),
                },
                None => super::Scope::GlobalCache,
            };
            install::apply(
                ctx,
                Install {
                    selection: RemoteArguments {
                        transport,
                        provider,
                        profile,
                        identity_provider: None,
                        producer: installed.original_source.producer().to_string(),
                        kind,
                        id: id.to_owned(),
                        version: "latest".into(),
                        exact: false,
                    },
                    scope,
                    binding: input.binding,
                    alias: installed.alias.clone(),
                    entry_name: installed.entry_name.clone(),
                    mode: None,
                },
                Operation::Update,
                selected,
                project_scope,
                &mut effects,
            )
            .await
        }
        .await;
        if effects["cache"] == "in_progress" {
            effects["cache"] = "not_confirmed".into();
        }
        outcome["items"][index]["outcome"] = effects;
        match result {
            Ok(()) => {
                outcome["items"][index]["status"] = "ok".into();
                outcome["items"][index]["outcome"]["status"] = "ok".into();
            }
            Err(cause) => {
                outcome["items"][index]["status"] = "error".into();
                outcome["items"][index]["code"] =
                    super::super::file_installation::update::error_code(&cause).into();
                return Err(cause);
            }
        }
    }
    (ctx.current)()?;
    Ok(())
}
