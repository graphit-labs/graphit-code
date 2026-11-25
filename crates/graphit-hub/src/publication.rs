//! Hub installation metadata follows ordinary, repeatable content publication.
use crate::{
    ArtifactType, Projection, ProjectionError, ProjectionPlanner, ProjectionRequest,
    registry::{
        ArtifactDisplayMetadata, DerivedPayload, HubDocument, HubRegistry, HubWriter, Installation,
        InstalledArtifact, InstalledBackend, InstalledPayload, consumer_for_artifact,
    },
};
use graphit_core::{
    artifact_store::{
        ArtifactChange, ArtifactEffect, ArtifactEntry, ArtifactError, ArtifactKey, ArtifactMode,
        ArtifactSnapshot, ArtifactStore, ArtifactWriter, PublicationCheck, PublicationSlot,
    },
    native_path::NativePath,
};
use std::{fmt, fs};

#[derive(Debug)]
pub enum PublicationFailure {
    Local(std::io::Error),
    Projection(ProjectionError),
    Physical(ArtifactError),
}
impl From<std::io::Error> for PublicationFailure {
    fn from(value: std::io::Error) -> Self {
        Self::Local(value)
    }
}
impl From<ProjectionError> for PublicationFailure {
    fn from(value: ProjectionError) -> Self {
        Self::Projection(value)
    }
}
impl From<ArtifactError> for PublicationFailure {
    fn from(value: ArtifactError) -> Self {
        Self::Physical(value)
    }
}
#[derive(Debug)]
pub struct PublicationError {
    pub failure: PublicationFailure,
    pub effect: ArtifactEffect,
}
impl fmt::Display for PublicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.failure {
            PublicationFailure::Local(error) => write!(f, "Hub publication: {:?}", error.kind()),
            PublicationFailure::Projection(error) => error.fmt(f),
            PublicationFailure::Physical(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for PublicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match &self.failure {
            PublicationFailure::Local(error) => error,
            PublicationFailure::Projection(error) => error,
            PublicationFailure::Physical(error) => error,
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationChange {
    Physical(ArtifactChange),
    MetadataOnly,
    VerifiedUnchanged,
    Forgotten,
    Unsupported,
}
#[derive(Debug)]
pub struct PublicationOutcome {
    pub change: PublicationChange,
    pub forgotten: Option<ArtifactEntry>,
}
type PublicResult<T> = Result<T, Box<PublicationError>>;
fn failure(error: impl Into<PublicationFailure>) -> Box<PublicationError> {
    let failure = error.into();
    let effect = match &failure {
        PublicationFailure::Physical(error) => error.effect(),
        _ => ArtifactEffect::NotApplied,
    };
    Box::new(PublicationError { failure, effect })
}
fn after_physical(error: std::io::Error, change: ArtifactChange) -> Box<PublicationError> {
    Box::new(PublicationError {
        failure: PublicationFailure::Local(error),
        effect: if change == ArtifactChange::Unchanged {
            ArtifactEffect::NotApplied
        } else {
            ArtifactEffect::Applied
        },
    })
}
fn outcome(change: PublicationChange) -> PublicationOutcome {
    PublicationOutcome {
        change,
        forgotten: None,
    }
}

#[derive(Clone, Debug)]
pub struct ScopedCoordinator {
    registry: HubRegistry,
    destination_root: NativePath,
    store: ArtifactStore,
    consumer: PublicationSlot,
}
impl ScopedCoordinator {
    pub fn new(
        registry: HubRegistry,
        store: ArtifactStore,
        destination_root: NativePath,
    ) -> std::io::Result<Self> {
        if !registry.matches_store(&store) {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        let consumer = store.publication_slot();
        Ok(Self {
            registry,
            destination_root,
            store,
            consumer,
        })
    }
    pub fn writer(
        &self,
        request: &crate::policy::RequestContext<'_>,
    ) -> PublicResult<PublicationWriter> {
        request.check().map_err(failure)?;
        let hub = self
            .registry
            .writer_for_request(request.snapshot())
            .map_err(failure)?;
        let core = self
            .store
            .writer_with_context(request.snapshot())
            .map_err(failure)?;
        Ok(PublicationWriter {
            coordinator: self.clone(),
            core,
            hub,
            context: std::sync::Arc::clone(request.snapshot()),
        })
    }
}
pub struct PublicationWriter {
    coordinator: ScopedCoordinator,
    core: ArtifactWriter,
    hub: HubWriter,
    context: std::sync::Arc<graphit_core::artifact_store::SnapshotContext>,
}
impl PublicationWriter {
    pub fn consumer(&self) -> &PublicationSlot {
        &self.coordinator.consumer
    }
    pub fn snapshot(&self) -> &HubDocument {
        self.hub.snapshot()
    }
    pub fn physical_snapshot(&self) -> Result<Option<ArtifactSnapshot>, ArtifactError> {
        self.core.load()
    }
    fn planner(&self, mode: ArtifactMode) -> ProjectionPlanner {
        let limits = self.coordinator.registry.limits();
        ProjectionPlanner::new(
            self.coordinator.destination_root.clone(),
            limits.projection(),
            mode,
        )
    }
    pub fn preview(
        &self,
        request: ProjectionRequest<'_>,
        mode: ArtifactMode,
    ) -> PublicResult<Projection> {
        self.context.check().map_err(failure)?;
        self.planner(mode).plan(request).map_err(failure)
    }
    fn desired(&self, installation: Installation) -> InstalledArtifact {
        let consumer = if matches!(installation.backend, InstalledBackend::Physical { .. }) {
            self.consumer().clone()
        } else {
            PublicationSlot::Scope
        };
        let mut entry = self
            .snapshot()
            .get_for_consumer(
                &consumer,
                installation.original_source.artifact_type(),
                installation.original_source.id().as_str(),
            )
            .cloned()
            .unwrap_or_else(|| InstalledArtifact {
                consumer,
                artifact_type: installation.original_source.artifact_type(),
                id: installation.original_source.id().clone(),
                installations: Vec::new(),
            });
        if let Some(previous) = entry
            .installations
            .iter_mut()
            .find(|old| old.target == installation.target)
        {
            *previous = installation;
        } else {
            entry.installations.push(installation);
        }
        entry
    }
    pub fn publish(
        &mut self,
        request: ProjectionRequest<'_>,
        mode: ArtifactMode,
        display: Option<&ArtifactDisplayMetadata>,
    ) -> PublicResult<PublicationOutcome> {
        self.publish_inner(request, mode, display, None, false)
    }
    pub fn publish_derived(
        &mut self,
        request: ProjectionRequest<'_>,
        mode: ArtifactMode,
        derived: crate::materialization::DerivedArtifact,
        display: Option<&ArtifactDisplayMetadata>,
    ) -> PublicResult<PublicationOutcome> {
        self.publish_inner(request, mode, display, Some(&derived), false)
    }
    /// Use identical portable files in place without taking ownership of them.
    pub fn publish_portable_derived(
        &mut self,
        request: ProjectionRequest<'_>,
        mode: ArtifactMode,
        derived: crate::materialization::DerivedArtifact,
        display: Option<&ArtifactDisplayMetadata>,
    ) -> PublicResult<PublicationOutcome> {
        self.publish_inner(request, mode, display, Some(&derived), true)
    }
    fn publish_inner(
        &mut self,
        request: ProjectionRequest<'_>,
        mode: ArtifactMode,
        display: Option<&ArtifactDisplayMetadata>,
        derived: Option<&crate::materialization::DerivedArtifact>,
        portable: bool,
    ) -> PublicResult<PublicationOutcome> {
        self.context.check().map_err(failure)?;
        if let Some(display) = display {
            display
                .validate(self.coordinator.registry.limits())
                .map_err(failure)?;
        }
        if request.source.artifact_type() == ArtifactType::Agent && derived.is_none() {
            return Ok(outcome(PublicationChange::Unsupported));
        }
        if let Some(derived) = derived {
            if !matches!(mode, ArtifactMode::Copy(_)) || request.source != derived.original_source()
            {
                return Err(failure(std::io::Error::from(
                    std::io::ErrorKind::InvalidInput,
                )));
            }
            derived
                .lineage
                .validate(self.coordinator.registry.limits().materialization().lineage)
                .map_err(failure)?;
        }
        let planner = self.planner(mode);
        let planned = match derived {
            Some(derived) => planner.plan_derived(request, derived.original_selection()),
            None => planner.plan(request),
        };
        let projection = match planned {
            Ok(plan) => plan,
            Err(error) if error.stage() == crate::ProjectionStage::Unsupported => {
                return Ok(outcome(PublicationChange::Unsupported));
            }
            Err(error) => return Err(failure(error)),
        };
        let source = match &projection {
            Projection::Physical(plan) => plan.source(),
            Projection::DynamicRule(plan) => plan.source(),
            Projection::Native(_) => return Ok(outcome(PublicationChange::Unsupported)),
        };
        let backend = match &projection {
            Projection::Physical(plan) => InstalledBackend::Physical {
                mode: plan.mode(),
                node_kind: plan.kind(),
                destination: plan.destination().clone(),
                name: plan.name().into(),
            },
            Projection::DynamicRule(_) => InstalledBackend::DynamicContext,
            Projection::Native(_) => unreachable!(),
        };
        let payload = match derived {
            Some(derived) => InstalledPayload::Derived(Box::new(DerivedPayload {
                source_policy: match mode {
                    ArtifactMode::Copy(policy) => policy,
                    ArtifactMode::Link => unreachable!("derived mode checked"),
                },
                source_fingerprint: derived.lineage.input_fingerprint().to_owned(),
                original_selection: derived.original_selection.clone(),
            })),
            None => InstalledPayload::Original,
        };
        let desired = self.desired(Installation {
            target: request.target(),
            original_source: source.clone(),
            display: display.cloned(),
            alias: request.alias.map(str::to_owned),
            entry_name: request.entry_name.map(str::to_owned),
            backend,
            payload,
        });
        self.hub.preflight_metadata(&desired).map_err(failure)?;
        let Projection::Physical(plan) = &projection else {
            let changed = self.hub.put_metadata(desired).map_err(failure)?;
            return Ok(outcome(if changed {
                PublicationChange::MetadataOnly
            } else {
                PublicationChange::VerifiedUnchanged
            }));
        };
        let generated = derived
            .map(|derived| {
                if derived.lineage.selected_name() != plan.name()
                    || derived.selection.kind() != plan.kind()
                    || (source.artifact_type() == ArtifactType::Agent
                        && derived.lineage.host_schema() != request.binding.schema())
                {
                    return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
                }
                derived.plan.selected(&derived.selection, &self.context)
            })
            .transpose()
            .map_err(failure)?;
        self.context.check().map_err(failure)?;
        let check = match &generated {
            Some(generated) => self.core.check_generated_publication(
                plan.key(),
                generated,
                plan.destination().as_path(),
                plan.kind(),
                plan.mode(),
            ),
            None => self.core.check_publication(
                plan.key(),
                plan.selected(),
                plan.destination().as_path(),
                plan.kind(),
                plan.mode(),
            ),
        }
        .map_err(failure)?;
        if check == PublicationCheck::VerifiedUnchanged {
            let changed = self.hub.put_metadata(desired).map_err(failure)?;
            return Ok(outcome(if changed {
                PublicationChange::MetadataOnly
            } else {
                PublicationChange::VerifiedUnchanged
            }));
        }
        if portable
            && let Some(generated) = &generated
            && self
                .core
                .matches_unowned_generated(plan.key(), generated, plan.destination().as_path())
                .map_err(failure)?
        {
            return Ok(outcome(PublicationChange::VerifiedUnchanged));
        }
        if let Some(parent) = plan.destination().as_path().parent() {
            fs::create_dir_all(parent).map_err(failure)?;
        }
        let change = match &generated {
            Some(generated) => self.core.publish_generated(
                plan.key(),
                generated,
                plan.destination().as_path(),
                plan.kind(),
                plan.mode(),
            ),
            None => self.core.publish(
                plan.key(),
                plan.selected(),
                plan.destination().as_path(),
                plan.kind(),
                plan.mode(),
            ),
        }
        .map_err(failure)?;
        self.hub
            .put_metadata(desired)
            .map_err(|error| after_physical(error, change))?;
        Ok(outcome(PublicationChange::Physical(change)))
    }
    pub fn remove(&mut self, key: &ArtifactKey) -> PublicResult<PublicationOutcome> {
        self.context.check().map_err(failure)?;
        self.coordinator
            .registry
            .limits()
            .key(key)
            .map_err(failure)?;
        let kind = ArtifactType::ALL
            .into_iter()
            .find(|kind| kind.as_str() == key.artifact_type())
            .ok_or_else(|| failure(std::io::Error::from(std::io::ErrorKind::InvalidInput)))?;
        let consumer = consumer_for_artifact(self.consumer().clone(), kind);
        let change = if consumer == *self.consumer() {
            match self.core.remove(key) {
                Ok(change) => change,
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound
                        && error.effect() == ArtifactEffect::NotApplied =>
                {
                    ArtifactChange::Unchanged
                }
                Err(error) => return Err(failure(error)),
            }
        } else {
            ArtifactChange::Unchanged
        };
        self.hub
            .remove_target(&consumer, key)
            .map_err(|error| after_physical(error, change))?;
        Ok(outcome(if consumer == *self.consumer() {
            PublicationChange::Physical(change)
        } else {
            PublicationChange::MetadataOnly
        }))
    }
    pub fn forget(&mut self, key: &ArtifactKey) -> PublicResult<PublicationOutcome> {
        self.context.check().map_err(failure)?;
        self.coordinator
            .registry
            .limits()
            .key(key)
            .map_err(failure)?;
        let kind = ArtifactType::ALL
            .into_iter()
            .find(|kind| kind.as_str() == key.artifact_type())
            .ok_or_else(|| failure(std::io::Error::from(std::io::ErrorKind::InvalidInput)))?;
        let consumer = consumer_for_artifact(self.consumer().clone(), kind);
        let forgotten = if consumer == *self.consumer() {
            match self.core.forget(key) {
                Ok(entry) => Some(entry),
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound
                        && error.effect() == ArtifactEffect::NotApplied =>
                {
                    None
                }
                Err(error) => return Err(failure(error)),
            }
        } else {
            None
        };
        self.hub.remove_target(&consumer, key).map_err(failure)?;
        Ok(PublicationOutcome {
            change: PublicationChange::Forgotten,
            forgotten,
        })
    }
}

#[cfg(test)]
mod tests;
