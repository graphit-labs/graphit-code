use super::{
    EffectContext, Project, denied,
    inputs::{Install, Operation},
};
use crate::remote_file_install::{
    FileSelection, FileTransport,
    client::{InstallArguments, Transport},
};
use graphit_core::{
    artifact_store::{ArtifactStore, CapturedPublicationScope, ScopeContext, StoreOwner},
    native_path::NativePath,
    project_identity::ProjectIdentityStore,
};
use graphit_hub::{
    ArtifactId, ConcreteVersion, InstallationTarget, SourceRef,
    publication::{PublicationChange, PublicationError, ScopedCoordinator},
    registry::{
        ArtifactDisplayMetadata, HubRegistry, Installation, InstalledArtifact, InstalledBackend,
        InstalledPayload,
    },
};
use serde_json::{Value, json};
use std::{io, time::Instant};

pub(super) async fn apply(
    ctx: &EffectContext<'_>,
    input: Install,
    operation: Operation,
    selected: Option<&Project>,
    project_scope: Option<&CapturedPublicationScope>,
    outcome: &mut Value,
) -> io::Result<()> {
    let state = ctx.state;
    let request = ctx.original;
    let policy = &state.policy.native_install.hub;
    let installation = &state.policy.installation;
    let issuer = state.auth.issuer();
    let read = issuer
        .admit_web_installed_hub_read(
            ctx.caller,
            ctx.verifier,
            request,
            ctx.provider,
            state.auth.installation(),
        )
        .map_err(denied)?;
    let mut service = match input.selection.transport {
        Transport::Broker => issuer.capture_installed_hub_service(&read, &input.selection.provider),
        Transport::Standalone => issuer.capture_installed_hub_standalone_s3_service(
            &read,
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
    (ctx.current)()?;
    // A failed transport can report a retired lease as opaque I/O. Recheck
    // the actual admission before classifying that pre-effect failure, while
    // keeping the transport error as its causal source.
    let network_failure = |cause: io::Error| match (ctx.current)() {
        Ok(()) => cause,
        Err(current) => io::Error::new(current.kind(), cause),
    };
    let transport = FileTransport::open(
        &lease,
        Some(subject),
        producer,
        &state.native_http,
        state.native_allow_loopback_http,
    )
    .await
    .map_err(&network_failure)?;
    let current = || -> io::Result<()> {
        (ctx.current)()?;
        read.recheck().map_err(denied)?;
        lease.check().map_err(denied)?;
        transport.check()
    };
    current()?;
    let kind = input.selection.kind.artifact_type();
    let id = ArtifactId::new(&input.selection.id, policy.projection()).map_err(io::Error::other)?;
    let literal = ConcreteVersion::new(&input.selection.version, policy.projection())
        .map_err(io::Error::other)?;
    let resolved = transport
        .resolve(
            FileSelection {
                producer,
                kind,
                id: &id,
                version: super::super::file_installation::version(&input.selection, &literal),
            },
            policy,
        )
        .await
        .map_err(&network_failure)?;
    let source = SourceRef::hub_cache(
        installation,
        producer,
        kind,
        id,
        None,
        resolved.version.clone(),
        policy.projection(),
    )
    .map_err(io::Error::other)?
    .with_remote_file_origin(
        super::super::file_installation::remote_origin(&input.selection),
        resolved.archive_sha256,
        policy.projection(),
    )
    .map_err(io::Error::other)?;
    let StoreOwner::Installation { installation_id } = ctx.admission.installation().live.owner()
    else {
        return Err(io::ErrorKind::PermissionDenied.into());
    };
    let cache_scope = CapturedPublicationScope::installation(
        installation,
        installation_id.clone(),
        [installation.root().to_owned()],
        state.policy.scope,
        request.snapshot(),
    )?;
    let cache = ArtifactStore::new(cache_scope.clone(), policy.artifacts());
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
        state.policy.registry.clone()
    };
    let mut native = InstallArguments {
        selection: input.selection,
        global,
        project_dir: None,
        project_version: None,
        binding: input.binding,
        alias: input.alias,
        entry_name: input.entry_name,
        mode: input.mode,
    };
    let target = if global {
        InstallationTarget::GlobalCache
    } else {
        InstallationTarget::for_artifact(native.binding.ok_or(io::ErrorKind::InvalidInput)?, kind)
    };
    let previous = {
        let document = registry.load()?;
        let exact = if global {
            document.get_version(kind, source.id().as_str(), source.version())
        } else {
            document.get(kind, source.id().as_str())
        }
        .and_then(|artifact| {
            artifact
                .installations
                .iter()
                .find(|entry| entry.target == target)
        });
        let old = if exact.is_none() && global && matches!(operation, Operation::Update) {
            document
                .entries()
                .filter(|(k, id, _)| *k == kind && *id == source.id().as_str())
                .flat_map(|(_, _, artifact)| artifact.installations.iter())
                .find(|entry| {
                    entry.target == target
                        && entry.original_source.producer() == producer
                        && entry.original_source.remote_file_origin() == source.remote_file_origin()
                })
        } else {
            exact
        };
        if let Some(old) = old {
            if old.original_source.producer() != producer
                || old.original_source.remote_file_origin() != source.remote_file_origin()
            {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            if matches!(operation, Operation::Update) {
                if native.alias.is_none() {
                    native.alias = old.alias.clone();
                }
                if native.entry_name.is_none() {
                    native.entry_name = old.entry_name.clone();
                }
            }
        } else if matches!(operation, Operation::Update) {
            return Err(io::ErrorKind::NotFound.into());
        }
        old.cloned()
    };
    outcome["producer"] = producer.to_string().into();
    outcome["artifact_type"] = serde_json::to_value(kind).map_err(io::Error::other)?;
    outcome["id"] = source.id().as_str().into();
    outcome["version"] = source.version().as_str().into();
    current()?;
    let cached = transport.verify_cache(&source, &cache, source.directory().as_path())?;
    if cached {
        ctx.admission.recheck_for_effect().await.map_err(denied)?;
        current()?;
        outcome["cache"] = "Unchanged".into();
    } else {
        let stage = transport
            .download(&resolved, installation.root(), policy)
            .await
            .map_err(&network_failure)?;
        let fresh = ctx.admission.recheck_for_effect().await.map_err(denied);
        let committed = fresh.and_then(|()| {
            current()?;
            let parent = crate::native_install::parent(
                installation,
                policy,
                source.directory().as_path(),
                request,
            )?;
            outcome["cache"] = "in_progress".into();
            let change =
                transport.commit_cache_stage(&resolved, &cache, &source, &stage, &current)?;
            outcome["cache"] = format!("{change:?}").into();
            current()?;
            parent.revalidate(request.snapshot())?;
            Ok(())
        });
        let cleanup = stage.close();
        if cleanup.is_err() {
            outcome["stage_cleanup_pending"] = true.into();
        }
        match (committed, cleanup) {
            (Err(error), _) => return Err(error),
            (Ok(()), Err(error)) => return Err(error),
            (Ok(()), Ok(())) => {}
        }
    }
    #[cfg(test)]
    if global {
        super::cache_commit_pause::observed(&source, request)?;
    }
    current()?;
    let display = ArtifactDisplayMetadata::new(
        resolved.manifest.name(),
        resolved.manifest.description(),
        resolved.manifest.tags(),
        policy.hub(),
    )?;
    if global {
        let mut writer = registry.writer(
            request
                .snapshot()
                .required_deadline()?
                .saturating_duration_since(Instant::now()),
            request.cancellation(),
        )?;
        current()?;
        if writer
            .snapshot()
            .get_version(kind, source.id().as_str(), source.version())
            .is_some_and(|old| {
                old.installations.iter().any(|entry| {
                    entry.original_source.producer() != producer
                        || entry.original_source.remote_file_origin() != source.remote_file_origin()
                })
            })
        {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        if matches!(operation, Operation::Update)
            && let Some(old) = &previous
            && writer
                .snapshot()
                .get_version(kind, source.id().as_str(), old.original_source.version())
                .and_then(|artifact| {
                    artifact
                        .installations
                        .iter()
                        .find(|entry| entry.target == target)
                })
                .is_none_or(|entry| !entry.same_source(old))
        {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        current()?;
        outcome["publication"] = json!({"change":"Unknown","committed":null});
        let effect = writer.put_metadata(InstalledArtifact {
            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
            artifact_type: kind,
            id: source.id().clone(),
            installations: vec![Installation {
                target: InstallationTarget::GlobalCache,
                original_source: source,
                display: Some(display),
                alias: None,
                entry_name: None,
                backend: InstalledBackend::GlobalCache,
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        });
        match effect {
            Ok(changed) => {
                outcome["publication"] = if changed {
                    "MetadataOnly"
                } else {
                    "VerifiedUnchanged"
                }
                .into()
            }
            Err(error) => {
                outcome["observation_failed"] = true.into();
                return Err(error);
            }
        }
        current()?;
        return Ok(());
    }
    let (paths, id, version) = selected.ok_or(io::ErrorKind::InvalidInput)?;
    outcome["project"] = id.to_string().into();
    outcome["project_version"] = version.version().into();
    outcome["binding"] = serde_json::to_value(native.binding).map_err(io::Error::other)?;
    let scope = project_scope.ok_or(io::ErrorKind::InvalidInput)?;
    scope.revalidate_bounded(&ScopeContext::from_snapshot(request.snapshot()))?;
    let core = ArtifactStore::new(scope.clone(), policy.artifacts());
    let coordinator =
        ScopedCoordinator::new(registry, core, NativePath::new(paths.root().to_owned())?)?;
    let expected = if matches!(operation, Operation::Update) {
        previous.as_ref()
    } else {
        None
    };
    let result = super::super::file_installation::publish(
        state,
        super::super::file_installation::ProjectionOptions {
            binding: native.binding,
            alias: native.alias.as_deref(),
            entry_name: native.entry_name.as_deref(),
            mode: native.mode,
        },
        &source,
        &coordinator,
        (Some(&display), expected),
        request,
        &current,
    );
    match result {
        Ok(publication) => {
            outcome["publication"] = format!("{:?}", publication.change).into();
            outcome["possible_parents"] =
                serde_json::to_value(publication.possible_parents).map_err(io::Error::other)?;
            if publication.change == PublicationChange::Unsupported {
                return Err(io::ErrorKind::Unsupported.into());
            }
            if let Some(cleanup) = publication.cleanup_pending {
                outcome["cleanup_pending"] = true.into();
                return Err(super::super::file_installation::cleanup_failure(cleanup));
            }
        }
        Err(error) => {
            if let Some(publication) = error
                .get_ref()
                .and_then(|source| source.downcast_ref::<PublicationError>())
            {
                outcome["publication"] = json!({"committed":publication.committed,"hub_pending":publication.hub_intent.is_some(),"core_pending":publication.core_pending.is_some(),"observation_failed":publication.observation_failure.is_some()});
            }
            return Err(error);
        }
    }
    current()
}
