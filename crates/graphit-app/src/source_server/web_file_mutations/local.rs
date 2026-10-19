//! LocalLink to one authored Resources tree under independent Web source/read and target mutation admissions.
use super::{EffectContext, Project, denied, local_input::LocalInstall};
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture,
    broker::project_access::{BrokerProjectCaller, BrokerProjectScope, BrokerRecordRead},
};
use graphit_core::{
    artifact_store::{
        ArtifactStore, CapturedPublicationScope, CapturedSourceReader, CopyPolicy, SourceReadLimits,
    },
    native_path::NativePath,
    project_identity::ProjectIdentityStore,
};
use graphit_hub::{
    ArtifactId, ConcreteVersion, InstallationTarget, SourceOrigin, SourceRef,
    publication::{PublicationChange, PublicationError, ScopedCoordinator},
    registry::HubRegistry,
};
use serde_json::{Value, json};
use std::io;

pub(super) async fn apply(
    ctx: &EffectContext<'_>,
    mut input: LocalInstall,
    consumer: &Project,
    consumer_scope: &CapturedPublicationScope,
    outcome: &mut Value,
) -> io::Result<()> {
    let state = ctx.state;
    let original = ctx.original;
    let policy = &state.policy.native_install.hub;
    outcome["source"] = "local".into();
    (ctx.current)()?;
    let projects = state.live_projects.snapshot()?;
    let selection = crate::task_server::request::Selection {
        project_dir: None,
        project_id: Some(input.source.project_id),
        project_version: Some(input.source.project_version.clone()),
    };
    let index = selection.select(&projects.projects, 0).map_err(|code| {
        io::Error::from(if code == "selection_ambiguous" {
            io::ErrorKind::AlreadyExists
        } else {
            io::ErrorKind::NotFound
        })
    })?;
    let (source_paths, source_id, source_version) = projects
        .projects
        .get(index)
        .ok_or(io::ErrorKind::NotFound)?;
    let source_scope = CapturedPublicationScope::project(
        source_paths,
        *source_id,
        state.policy.scope,
        original.snapshot(),
    )?;
    let source_binding = source_scope.binding()?;
    let authority = ProjectRecordAuthority::new(state.auth.issuer());
    let source_read = authority
        .admit_broker_read(BrokerRecordRead {
            caller: BrokerProjectCaller::Web(ctx.caller),
            verifier: ctx.verifier,
            backend: ctx.backend,
            original,
            provider: ctx.provider,
            scope: BrokerProjectScope {
                installation: state.auth.installation(),
                project: ProjectCapture {
                    scope: ScopeCapture {
                        live: &source_scope,
                        record: &source_binding,
                    },
                    version: source_version,
                },
                providers: &[],
            },
            module: ProjectReadModule::Hub,
        })
        .await
        .map_err(denied)?;
    let before_capture = || -> io::Result<()> {
        (ctx.current)()?;
        source_read.recheck().map_err(denied)?;
        if !state
            .live_projects
            .is_active(source_paths, *source_id, source_version)?
        {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(())
    };
    before_capture()?;
    let producer = ProjectIdentityStore::at(source_paths)
        .load()?
        .ok_or(io::ErrorKind::NotFound)?;
    if producer.id() != *source_id {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let kind = input.source.kind.artifact_type();
    let authored = SourceRef::resources(
        source_paths,
        producer.id(),
        kind,
        ArtifactId::new(&input.source.id, policy.projection()).map_err(io::Error::other)?,
        ConcreteVersion::new(&input.source.version, policy.projection())
            .map_err(io::Error::other)?,
        policy.projection(),
    )
    .map_err(io::Error::other)?;
    let relative = authored
        .directory()
        .as_path()
        .strip_prefix(source_paths.root())
        .map_err(io::Error::other)?;
    let reader = CapturedSourceReader::capture_project(
        &source_scope,
        relative,
        graphit_fs::NodeKind::Directory,
        &policy.snapshot_policy(CopyPolicy::RejectFileLinks, [])?,
        SourceReadLimits::new(64 * 1024, 64 * 1024, policy.copy().total_file_bytes_limit())?,
        original.snapshot(),
    )?;
    let current = || -> io::Result<()> {
        before_capture()?;
        reader.revalidate_bindings()?;
        if ProjectIdentityStore::at(source_paths)
            .load()?
            .as_ref()
            .map(|identity| identity.id())
            != Some(producer.id())
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(())
    };
    current()?;
    let source = SourceRef::local_link(
        NativePath::new(reader.source_path(original.snapshot())?.to_owned())?,
        producer.id(),
        kind,
        authored.id().clone(),
        authored.version().clone(),
        policy.projection(),
    )
    .map_err(io::Error::other)?;
    let (paths, project_id, version) = consumer;
    let identity = ProjectIdentityStore::at(paths)
        .load()?
        .ok_or(io::ErrorKind::NotFound)?;
    if identity.id() != *project_id {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let registry = HubRegistry::project(paths, &identity, policy.hub())?;
    let target = InstallationTarget::for_artifact(input.binding, kind);
    let previous = registry
        .load()?
        .get(kind, source.id().as_str())
        .and_then(|artifact| {
            artifact
                .installations
                .iter()
                .find(|installation| installation.target == target)
        })
        .cloned();
    if let Some(previous) = &previous {
        if previous.original_source.origin() != SourceOrigin::LocalLink
            || previous.original_source.producer() != producer.id()
            || previous.original_source.directory() != source.directory()
        {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        if input.alias.is_none() {
            input.alias = previous.alias.clone();
        }
        if input.entry_name.is_none() {
            input.entry_name = previous.entry_name.clone();
        }
    }
    outcome["producer"] = producer.id().to_string().into();
    outcome["artifact_type"] = serde_json::to_value(kind).map_err(io::Error::other)?;
    outcome["id"] = source.id().as_str().into();
    outcome["version"] = source.version().as_str().into();
    outcome["project"] = project_id.to_string().into();
    outcome["project_version"] = version.version().into();
    outcome["binding"] = serde_json::to_value(input.binding).map_err(io::Error::other)?;
    ctx.admission.recheck_for_effect().await.map_err(denied)?;
    current()?;
    let core = ArtifactStore::new(consumer_scope.clone(), policy.artifacts());
    let coordinator =
        ScopedCoordinator::new(registry, core, NativePath::new(paths.root().to_owned())?)?;
    let result = super::super::file_installation::publish(
        state,
        super::super::file_installation::ProjectionOptions {
            binding: Some(input.binding),
            alias: input.alias.as_deref(),
            entry_name: input.entry_name.as_deref(),
            mode: input.mode,
        },
        &source,
        &coordinator,
        (None, previous.as_ref()),
        original,
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
                .and_then(|error| error.downcast_ref::<PublicationError>())
            {
                outcome["publication"] = json!({"committed":publication.committed,
                    "hub_pending":publication.hub_intent.is_some(),"core_pending":publication.core_pending.is_some(),
                    "observation_failed":publication.observation_failure.is_some()});
            }
            return Err(error);
        }
    }
    current()
}
