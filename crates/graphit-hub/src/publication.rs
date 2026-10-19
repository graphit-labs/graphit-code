//! Ordered Hub → Core coordination. Journal data never grants payload ownership.
use crate::{
    ArtifactType, Projection, ProjectionError, ProjectionPlan, ProjectionPlanner,
    ProjectionRequest, SourceBinding,
    projection::{expected_parent, validate_runtime},
    registry::{
        ArtifactDisplayMetadata, DerivedPayload, ForgetPreview, HubDocument, HubRegistry,
        HubWriter, Installation, InstalledArtifact, InstalledBackend, InstalledPayload,
        IntentPhase, LogicalGoal, LogicalIntent, PendingDescriptor, PhysicalDescriptor,
        PublicationBindings, PublicationInput, consumer_for_artifact,
    },
};
use graphit_core::{
    artifact_store::{
        ArtifactChange, ArtifactError, ArtifactKey, ArtifactMode, ArtifactPending,
        ArtifactSnapshot, ArtifactStage, ArtifactStore, ArtifactWriter, PhysicalSource,
        PublicationCheck, PublicationSlot, ScopeContext, SnapshotContext, ValidatedPayloadPlan,
    },
    file_lock::Cancellation,
    native_path::NativePath,
};
use std::{fmt, fs, io, time::Instant};

#[cfg(test)]
use std::time::Duration;

#[derive(Debug)]
pub enum PublicationFailure {
    Local(io::Error),
    Projection(ProjectionError),
    Physical(ArtifactError),
}
impl From<io::Error> for PublicationFailure {
    fn from(value: io::Error) -> Self {
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
    pub hub_intent: Option<Box<LogicalIntent>>,
    pub core_pending: Option<ArtifactPending>,
    pub observation_failure: Option<ArtifactError>,
    pub committed: bool,
}
impl fmt::Display for PublicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.failure {
            PublicationFailure::Local(e) => write!(f, "Hub publication: {:?}", e.kind()),
            PublicationFailure::Projection(e) => e.fmt(f),
            PublicationFailure::Physical(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for PublicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match &self.failure {
            PublicationFailure::Local(e) => e,
            PublicationFailure::Projection(e) => e,
            PublicationFailure::Physical(e) => e,
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
    Recovered,
}
#[derive(Debug)]
pub struct PublicationOutcome {
    pub change: PublicationChange,
    /// Persistent layout infrastructure; possible residues, never ownership receipts.
    pub possible_parents: Vec<NativePath>,
    pub forget_preview: Option<Box<ForgetPreview>>,
    pub cleanup_pending: Option<PublicationFailure>,
}
struct PreparedPublication {
    display: Option<ArtifactDisplayMetadata>,
    projection: Projection,
    parents: Vec<NativePath>,
    target: crate::InstallationTarget,
    alias: Option<String>,
    entry_name: Option<String>,
}
type Result<T> = std::result::Result<T, PublicationFailure>;
type PublicResult<T> = std::result::Result<T, Box<PublicationError>>;

#[derive(Clone, Debug)]
pub struct ScopedCoordinator {
    registry: HubRegistry,
    destination_root: NativePath,
    store: ArtifactStore,
    consumer: PublicationSlot,
}
impl ScopedCoordinator {
    /// Reject incoherent captured contexts before either writer can create its lock.
    pub fn new(
        registry: HubRegistry,
        store: ArtifactStore,
        destination_root: NativePath,
    ) -> io::Result<Self> {
        if !registry.matches_store(&store)
            || !store
                .scope()
                .allowed_roots()
                .any(|root| root == destination_root.as_path())
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let consumer = store.publication_slot();
        if matches!(consumer, PublicationSlot::ProjectRoot(_))
            && destination_root.as_path() != store.scope().owner_root()
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
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
        let deadline = request
            .snapshot()
            .required_deadline()
            .map_err(|e| acquisition_error(e.into()))?;
        let cancellation = request.cancellation();
        let remaining = deadline.saturating_duration_since(Instant::now());
        let hub = self
            .registry
            .writer(remaining, cancellation)
            .map_err(|e| acquisition_error(e.into()))?;
        request.check().map_err(|e| {
            let mut error = acquisition_error(e.into());
            error.hub_intent = hub.snapshot().intent().cloned().map(Box::new);
            error
        })?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        let core = self.store.writer(remaining, cancellation).map_err(|e| {
            let mut error = acquisition_error(e.into());
            error.hub_intent = hub.snapshot().intent().cloned().map(Box::new);
            error
        })?;
        let context = std::sync::Arc::clone(request.snapshot());
        let core = core
            .with_io_context(std::sync::Arc::clone(&context))
            .map_err(|e| {
                let mut error = acquisition_error(e.into());
                error.hub_intent = hub.snapshot().intent().cloned().map(Box::new);
                error
            })?;
        Ok(PublicationWriter {
            coordinator: self.clone(),
            hub,
            core,
            cancellation: cancellation.clone(),
            context,
            deadline,
            #[cfg(test)]
            fault: None,
            #[cfg(test)]
            pause_at: None,
            #[cfg(test)]
            cancel_at: None,
            #[cfg(all(test, unix))]
            readonly_at: None,
        })
    }
}
fn acquisition_error(failure: PublicationFailure) -> Box<PublicationError> {
    Box::new(PublicationError {
        failure,
        hub_intent: None,
        core_pending: None,
        observation_failure: None,
        committed: false,
    })
}
pub struct PublicationWriter {
    coordinator: ScopedCoordinator,
    core: ArtifactWriter,
    hub: HubWriter,
    cancellation: Cancellation,
    context: std::sync::Arc<SnapshotContext>,
    deadline: Instant,
    #[cfg(test)]
    fault: Option<Boundary>,
    #[cfg(test)]
    pause_at: Option<(Boundary, std::path::PathBuf)>,
    #[cfg(test)]
    cancel_at: Option<Boundary>,
    #[cfg(all(test, unix))]
    readonly_at: Option<Boundary>,
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum Boundary {
    Intent,
    Parents,
    PhysicalStarted,
    Physical,
    PhysicalComplete,
    Logical,
    Clear,
    Forget,
    Cleared,
}
impl PublicationWriter {
    pub fn consumer(&self) -> &PublicationSlot {
        &self.coordinator.consumer
    }
    fn key_consumer(&self, key: &ArtifactKey) -> Result<PublicationSlot> {
        let kind = ArtifactType::ALL
            .into_iter()
            .find(|kind| kind.as_str() == key.artifact_type())
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        Ok(consumer_for_artifact(*self.consumer(), kind))
    }
    pub fn snapshot(&self) -> &HubDocument {
        self.hub.snapshot()
    }
    pub fn physical_snapshot(
        &self,
    ) -> std::result::Result<Option<ArtifactSnapshot>, ArtifactError> {
        self.core.load()
    }
    fn report(&self, result: Result<PublicationOutcome>) -> PublicResult<PublicationOutcome> {
        result.map_err(|failure| {
            let (core_pending, observation_failure) = match self.core.load() {
                Ok(snapshot) => (snapshot.and_then(|s| s.pending().map(|p| p.phase())), None),
                Err(e) => (None, Some(e)),
            };
            Box::new(PublicationError {
                failure,
                hub_intent: self.snapshot().intent().cloned().map(Box::new),
                core_pending,
                observation_failure,
                committed: self
                    .snapshot()
                    .intent()
                    .is_some_and(|i| i.phase == IntentPhase::LogicalCommitted),
            })
        })
    }
    fn cancelled(&self) -> Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(io::Error::from(io::ErrorKind::Interrupted).into());
        }
        if Instant::now() >= self.deadline {
            return Err(io::Error::from(io::ErrorKind::TimedOut).into());
        }
        Ok(())
    }
    fn planner(&self, mode: ArtifactMode) -> ProjectionPlanner {
        let limits = self.coordinator.registry.limits();
        ProjectionPlanner::new(
            self.coordinator.destination_root.clone(),
            NativePath::new(self.coordinator.store.runtime_root().to_owned())
                .expect("captured runtime"),
            limits.projection(),
            limits.artifacts(),
            mode,
        )
    }
    /// Observe the same selection as publication without creating destination parents or payloads.
    pub fn preview(
        &self,
        request: ProjectionRequest<'_>,
        mode: ArtifactMode,
    ) -> PublicResult<Projection> {
        let result = (|| {
            self.cancelled()?;
            self.context.check()?;
            let (projection, _) = self.planner(mode).prepare_publication(request)?;
            self.cancelled()?;
            self.context.check()?;
            Ok(projection)
        })();
        result.map_err(acquisition_error)
    }
    fn old(&self, key: &ArtifactKey) -> Option<InstalledArtifact> {
        let kind = ArtifactType::ALL
            .into_iter()
            .find(|t| t.as_str() == key.artifact_type())?;
        self.snapshot()
            .get_for_consumer(
                &consumer_for_artifact(*self.consumer(), kind),
                kind,
                key.id(),
            )
            .cloned()
    }
    fn desired(&self, installation: Installation) -> InstalledArtifact {
        let consumer = if matches!(installation.backend, InstalledBackend::Physical { .. }) {
            *self.consumer()
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
        if let Some(old) = entry
            .installations
            .iter_mut()
            .find(|v| v.target == installation.target)
        {
            *old = installation;
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
        let result = self.publish_inner(request, mode, display);
        self.report(result)
    }
    fn publish_inner(
        &mut self,
        request: ProjectionRequest<'_>,
        mode: ArtifactMode,
        display: Option<&ArtifactDisplayMetadata>,
    ) -> Result<PublicationOutcome> {
        self.cancelled()?;
        if let Some(display) = display {
            display.validate(self.coordinator.registry.limits())?;
        }
        if !self.preflight_publication(&request, mode, None)? {
            return Ok(outcome(PublicationChange::Unsupported, Vec::new()));
        }
        self.recover_inner()?;
        let prepared = match self.prepare(request, mode, display) {
            Ok(prepared) => prepared,
            Err(PublicationFailure::Projection(e))
                if e.stage() == crate::ProjectionStage::Unsupported =>
            {
                return Ok(outcome(PublicationChange::Unsupported, Vec::new()));
            }
            Err(error) => return Err(error),
        };
        self.publish_prepared(prepared, None)
    }
    // Reject pure input/capability errors before recovering any prior native effect.
    fn preflight_publication(
        &self,
        request: &ProjectionRequest<'_>,
        mode: ArtifactMode,
        derived: Option<&crate::materialization::DerivedArtifact>,
    ) -> Result<bool> {
        let target = match self.planner(mode).prepare(request) {
            Ok(target) => target,
            Err(e) if e.stage() == crate::ProjectionStage::Unsupported => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        let kind = request.source.artifact_type();
        if target.destination().is_none() && kind != ArtifactType::Rule {
            return Ok(false);
        }
        if let Some(derived) = derived {
            if !matches!(mode, ArtifactMode::Copy(_)) {
                return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
            }
            derived
                .lineage
                .validate(self.coordinator.registry.limits().materialization().lineage)?;
            if request.source != derived.original_source()
                || target.destination().is_none()
                || target.name() != derived.lineage.selected_name()
                || (kind == ArtifactType::Agent
                    && derived.lineage.host_schema() != request.binding.schema())
            {
                return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
            }
        } else if kind == ArtifactType::Agent {
            return Ok(false);
        }
        Ok(true)
    }
    fn prepare(
        &self,
        request: ProjectionRequest<'_>,
        mode: ArtifactMode,
        display: Option<&ArtifactDisplayMetadata>,
    ) -> Result<PreparedPublication> {
        if let Some(display) = display {
            display.validate(self.coordinator.registry.limits())?;
        }
        let (projection, parents) = self.planner(mode).prepare_publication(ProjectionRequest {
            binding: request.binding,
            source: request.source,
            alias: request.alias,
            entry_name: request.entry_name,
        })?;
        Ok(PreparedPublication {
            display: display.cloned(),
            projection,
            parents,
            target: request.target(),
            alias: request.alias.map(str::to_owned),
            entry_name: request.entry_name.map(str::to_owned),
        })
    }
    pub fn publish_derived(
        &mut self,
        request: ProjectionRequest<'_>,
        mode: ArtifactMode,
        derived: crate::materialization::DerivedArtifact,
        display: Option<&ArtifactDisplayMetadata>,
    ) -> PublicResult<PublicationOutcome> {
        let result = (|| {
            self.cancelled()?;
            if let Some(display) = display {
                display.validate(self.coordinator.registry.limits())?;
            }
            if !self.preflight_publication(&request, mode, Some(&derived))? {
                return Ok(outcome(PublicationChange::Unsupported, Vec::new()));
            }
            self.recover_inner()?;
            let prepared = match self.prepare(request, mode, display) {
                Ok(prepared) => prepared,
                Err(PublicationFailure::Projection(e))
                    if e.stage() == crate::ProjectionStage::Unsupported =>
                {
                    return Ok(outcome(PublicationChange::Unsupported, Vec::new()));
                }
                Err(error) => return Err(error),
            };
            self.publish_prepared(prepared, Some(&derived))
        })();
        self.report(result)
    }
    fn publish_prepared(
        &mut self,
        prepared: PreparedPublication,
        derived: Option<&crate::materialization::DerivedArtifact>,
    ) -> Result<PublicationOutcome> {
        self.cancelled()?;
        let PreparedPublication {
            display,
            projection,
            parents,
            target,
            alias,
            entry_name,
        } = prepared;
        let (backend, key) = match &projection {
            Projection::Physical(plan) => (
                InstalledBackend::Physical {
                    mode: plan.mode(),
                    node_kind: plan.selected().kind(),
                    destination: plan.destination().clone(),
                    name: plan.name().into(),
                },
                plan.key(),
            ),
            Projection::DynamicRule(rule) => {
                rule.revalidate_bindings()?;
                (InstalledBackend::DynamicContext, rule.key())
            }
            Projection::Native(_) => {
                return Ok(outcome(PublicationChange::Unsupported, Vec::new()));
            }
        };
        let source = match &projection {
            Projection::Physical(plan) => plan.source(),
            Projection::DynamicRule(rule) => rule.source(),
            Projection::Native(native) => native.source(),
        };
        let payload = if let Some(derived) = derived {
            let Projection::Physical(plan) = &projection else {
                return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
            };
            self.validate_derived(plan, derived)?;
            self.coordinator
                .store
                .scope()
                .revalidate(&ScopeContext::new(self.deadline, self.cancellation.clone()))?;
            derived.plan.revalidate_content(&self.context)?;
            InstalledPayload::Derived(Box::new(DerivedPayload {
                source_policy: derived.original_binding.link_policy(),
                source_fingerprint: derived.lineage.input_fingerprint().to_owned(),
                original_selection: derived.original_selection.clone(),
            }))
        } else {
            InstalledPayload::Original {
                selection_binding: match &projection {
                    Projection::Physical(plan) => Some(plan.selected().record()),
                    Projection::DynamicRule(rule) => Some(rule.selected().record()),
                    Projection::Native(_) => None,
                },
            }
        };
        let desired = self.desired(Installation {
            target,
            original_source: source.clone(),
            display,
            alias,
            entry_name,
            backend,
            payload: payload.clone(),
        });
        let old = self.old(key);
        let Projection::Physical(plan) = projection else {
            let changed = self.hub.put_metadata(desired)?;
            return Ok(outcome(
                if changed {
                    PublicationChange::MetadataOnly
                } else {
                    PublicationChange::VerifiedUnchanged
                },
                Vec::new(),
            ));
        };
        let key = plan.key().clone();
        let mut bindings =
            PublicationBindings::from_plan(&plan, &self.coordinator.destination_root)?;
        let generated = derived
            .map(|derived| derived.plan.selected(&derived.selection, &self.context))
            .transpose()?;
        if let Some(plan) = &generated {
            bindings.physical_input = PublicationInput::Generated {
                fingerprint: plan.fingerprint(),
            };
        }
        bindings.possible_parents = parents;
        self.validate_bindings(&bindings, true)?;
        if old.as_ref() == Some(&desired) && bindings.possible_parents.is_empty() {
            let check = if let Some(generated) = &generated {
                self.core.check_generated_publication(
                    &key,
                    generated,
                    plan.destination().as_path(),
                    plan.selected().kind(),
                    plan.mode(),
                )?
            } else {
                self.core.check_publication(
                    &key,
                    PhysicalSource::ExternalSource(bindings.selected.input.as_path()),
                    plan.destination().as_path(),
                    plan.selected().kind(),
                    plan.mode(),
                )?
            };
            if check == PublicationCheck::VerifiedUnchanged {
                return Ok(outcome(PublicationChange::VerifiedUnchanged, Vec::new()));
            }
        }
        self.hub.record(LogicalIntent {
            consumer: self.key_consumer(&key)?,
            key: key.clone(),
            phase: if bindings.possible_parents.is_empty() {
                IntentPhase::IntentReady
            } else {
                IntentPhase::PreparingParents
            },
            goal: LogicalGoal::Publish {
                bindings: Box::new(bindings.clone()),
            },
            old,
            desired: Some(desired),
        })?;
        self.boundary(Boundary::Intent)?;
        self.execute_publish(&key, &bindings, generated.as_ref())
    }
    fn validate_derived(
        &self,
        plan: &ProjectionPlan,
        derived: &crate::materialization::DerivedArtifact,
    ) -> Result<()> {
        self.cancelled()?;
        let limits = self.coordinator.registry.limits().materialization();
        derived.lineage.validate(limits.lineage)?;
        if let crate::InstallationTarget::NativeBinding { binding } =
            crate::InstallationTarget::from_key(plan.key())?
            && derived.lineage.host_schema() != binding.schema()
        {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        derived.original_binding.validate_data(limits.snapshots)?;
        crate::materialization::validate_original_selection(
            &derived.original_source,
            &derived.original_binding,
            &derived.original_selection,
        )?;
        derived.plan.validate_limits(limits.output, &self.context)?;
        derived.plan.validate_selection_limits(
            &derived.selection,
            self.coordinator.store.limits().copy_limits(),
            &self.context,
        )?;
        if derived.lineage.selected_name() != plan.name()
            || derived.selection.kind() != plan.selected().kind()
        {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        Ok(())
    }
    fn validate_bindings(&self, b: &PublicationBindings, allow_missing: bool) -> Result<()> {
        self.cancelled()?;
        let limits = self.coordinator.registry.limits().projection();
        if b.runtime.as_path() != self.coordinator.store.runtime_root()
            || !self
                .coordinator
                .store
                .scope()
                .allowed_roots()
                .any(|p| p == b.destination_root.as_path())
        {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        self.coordinator
            .store
            .scope()
            .revalidate(&ScopeContext::new(self.deadline, self.cancellation.clone()))?;
        match &b.physical_input {
            PublicationInput::External => {
                SourceBinding::restore(b.directory.clone(), limits)?.revalidate()?;
                SourceBinding::restore(b.selected.clone(), limits)?.revalidate()?;
            }
            // Generated bytes are admitted by Core from the current render plan.
            PublicationInput::Generated { .. } => {}
        }
        let (parent, missing) = expected_parent(b.input_parent.as_path(), limits, allow_missing)?;
        if parent != b.parent
            || !parent.as_path().starts_with(b.destination_root.as_path())
            || missing.iter().any(|p| !b.possible_parents.contains(p))
        {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        validate_runtime(b.runtime.as_path(), b.destination.as_path(), limits)?;
        Ok(())
    }
    fn provision(&mut self, b: &PublicationBindings) -> Result<()> {
        self.validate_bindings(b, true)?;
        for path in &b.possible_parents {
            self.cancelled()?;
            // Re-observe the ancestor before each separate, nonrecursive mkdir.
            self.validate_bindings(b, true)?;
            match fs::symlink_metadata(path.as_path()) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                Ok(_) => return Err(io::Error::from(io::ErrorKind::InvalidInput).into()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    let builder = fs::DirBuilder::new();
                    #[cfg(unix)]
                    let builder = {
                        use std::os::unix::fs::DirBuilderExt;
                        let mut builder = builder;
                        builder.mode(0o700);
                        builder
                    };
                    builder.create(path.as_path())?;
                }
                Err(e) => return Err(e.into()),
            }
            self.boundary(Boundary::Parents)?;
        }
        self.validate_bindings(b, false)?;
        self.hub.advance(IntentPhase::IntentReady)?;
        Ok(())
    }
    fn execute_publish(
        &mut self,
        key: &ArtifactKey,
        b: &PublicationBindings,
        generated: Option<&ValidatedPayloadPlan>,
    ) -> Result<PublicationOutcome> {
        if self
            .snapshot()
            .intent()
            .is_some_and(|i| i.phase == IntentPhase::PreparingParents)
        {
            self.provision(b)?;
        }
        self.validate_bindings(b, false)?;
        if self
            .snapshot()
            .intent()
            .is_some_and(|i| i.phase == IntentPhase::IntentReady)
        {
            self.hub.advance(IntentPhase::PhysicalStarted)?;
        }
        self.boundary(Boundary::PhysicalStarted)?;
        let change = match &b.physical_input {
            PublicationInput::External => self.core.publish(
                key,
                PhysicalSource::ExternalSource(b.selected.input.as_path()),
                b.destination.as_path(),
                b.selected.kind,
                b.mode,
            )?,
            PublicationInput::Generated { fingerprint } => {
                if let Some(plan) = generated {
                    if &plan.fingerprint() != fingerprint {
                        return Err(io::Error::from(io::ErrorKind::InvalidData).into());
                    }
                    self.core.publish_generated(
                        key,
                        plan,
                        b.destination.as_path(),
                        b.selected.kind,
                        b.mode,
                    )?
                } else {
                    // Core recovery uses its transient staged publication; it never needs
                    // a persistent second copy of generated content or the original source.
                    if self.core.check_generated_receipt(
                        key,
                        *fingerprint,
                        b.destination.as_path(),
                        b.selected.kind,
                        b.mode,
                    )? != PublicationCheck::VerifiedUnchanged
                    {
                        self.hub.discard_unpublished_generated()?;
                        return Ok(outcome(
                            PublicationChange::Recovered,
                            b.possible_parents.clone(),
                        ));
                    }
                    ArtifactChange::Unchanged
                }
            }
        };
        self.boundary(Boundary::Physical)?;
        let cleanup_pending = self.finish()?;
        let mut result = outcome(
            PublicationChange::Physical(change),
            b.possible_parents.clone(),
        );
        result.cleanup_pending = cleanup_pending;
        Ok(result)
    }
    fn finish(&mut self) -> Result<Option<PublicationFailure>> {
        if self
            .core
            .load()?
            .as_ref()
            .is_some_and(|s| s.pending().is_some())
        {
            return Err(io::Error::from(io::ErrorKind::WouldBlock).into());
        }
        if self
            .snapshot()
            .intent()
            .is_some_and(|i| i.phase == IntentPhase::PhysicalStarted)
        {
            self.hub.advance(IntentPhase::PhysicalComplete)?;
        }
        self.boundary(Boundary::PhysicalComplete)?;
        self.hub.commit_logical()?;
        self.boundary(Boundary::Logical)?;
        self.clear_committed()
    }
    fn clear_committed(&mut self) -> Result<Option<PublicationFailure>> {
        self.boundary(Boundary::Clear)?;
        self.hub.clear()?;
        Ok(self.boundary(Boundary::Cleared).err())
    }
    pub fn recover(&mut self) -> PublicResult<PublicationOutcome> {
        let result = self.recover_inner();
        self.report(result)
    }
    fn recover_inner(&mut self) -> Result<PublicationOutcome> {
        self.cancelled()?;
        let intent = self.snapshot().intent().cloned();
        if intent
            .as_ref()
            .is_some_and(|intent| self.key_consumer(&intent.key).ok() != Some(intent.consumer))
        {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        if let Some(LogicalIntent {
            goal: LogicalGoal::Forget { .. },
            ..
        }) = &intent
        {
            return self.execute_forget();
        }
        self.core.recover()?;
        let Some(intent) = intent else {
            return Ok(outcome(PublicationChange::Recovered, Vec::new()));
        };
        if matches!(
            intent.phase,
            IntentPhase::LogicalCommitted | IntentPhase::PhysicalComplete
        ) {
            let cleanup_pending = if intent.phase == IntentPhase::LogicalCommitted {
                self.clear_committed()?
            } else {
                self.finish()?
            };
            let mut result = outcome(PublicationChange::Recovered, parents(&intent));
            result.cleanup_pending = cleanup_pending;
            return Ok(result);
        }
        match intent.goal {
            LogicalGoal::Publish { bindings } => self.execute_publish(&intent.key, &bindings, None),
            LogicalGoal::Remove => self.execute_remove(&intent.key),
            LogicalGoal::Forget { .. } => unreachable!("Forget handled before physical recovery"),
        }
    }
    pub fn remove(&mut self, key: &ArtifactKey) -> PublicResult<PublicationOutcome> {
        let result = self.remove_inner(key);
        self.report(result)
    }
    fn remove_inner(&mut self, key: &ArtifactKey) -> Result<PublicationOutcome> {
        self.cancelled()?;
        self.coordinator.registry.limits().key(key)?;
        self.recover_inner()?;
        self.hub.record(LogicalIntent {
            consumer: self.key_consumer(key)?,
            key: key.clone(),
            phase: IntentPhase::IntentReady,
            goal: LogicalGoal::Remove,
            old: self.old(key),
            desired: None,
        })?;
        self.boundary(Boundary::Intent)?;
        self.execute_remove(key)
    }
    fn execute_remove(&mut self, key: &ArtifactKey) -> Result<PublicationOutcome> {
        if self
            .snapshot()
            .intent()
            .is_some_and(|i| i.phase == IntentPhase::IntentReady)
        {
            self.hub.advance(IntentPhase::PhysicalStarted)?;
        }
        self.boundary(Boundary::PhysicalStarted)?;
        let change = if self.key_consumer(key)? != *self.consumer() {
            PublicationChange::MetadataOnly
        } else {
            match self.core.remove(key) {
                Ok(change) => PublicationChange::Physical(change),
                Err(error)
                    if error.stage() == ArtifactStage::Validate
                        && error.kind() == io::ErrorKind::NotFound =>
                {
                    PublicationChange::MetadataOnly
                }
                Err(error) => return Err(error.into()),
            }
        };
        self.boundary(Boundary::Physical)?;
        let cleanup_pending = self.finish()?;
        let mut result = outcome(change, Vec::new());
        result.cleanup_pending = cleanup_pending;
        Ok(result)
    }
    pub fn forget(&mut self, key: &ArtifactKey) -> PublicResult<PublicationOutcome> {
        let result = self.forget_inner(key);
        self.report(result)
    }
    fn forget_inner(&mut self, key: &ArtifactKey) -> Result<PublicationOutcome> {
        self.cancelled()?;
        self.coordinator.registry.limits().key(key)?;
        if self
            .snapshot()
            .intent()
            .is_some_and(|i| &i.key != key || self.key_consumer(key).ok() != Some(i.consumer))
        {
            return Err(io::Error::from(io::ErrorKind::WouldBlock).into());
        }
        let snapshot = self.core.load()?;
        if snapshot
            .as_ref()
            .and_then(|s| s.pending())
            .is_some_and(|p| p.key() != Some(key) || p.publication_slot() != Some(*self.consumer()))
        {
            return Err(io::Error::from(io::ErrorKind::WouldBlock).into());
        }
        let preview = preview(
            snapshot.as_ref(),
            key,
            *self.consumer(),
            self.snapshot().intent().map(parents).unwrap_or_default(),
        )?;
        self.hub.forget(self.key_consumer(key)?, key, preview)?;
        self.boundary(Boundary::Intent)?;
        self.execute_forget()
    }
    fn execute_forget(&mut self) -> Result<PublicationOutcome> {
        let intent = self
            .snapshot()
            .intent()
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        let LogicalGoal::Forget { preview, .. } = intent.goal else {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        };
        let cleanup_pending = if intent.phase == IntentPhase::LogicalCommitted {
            self.clear_committed()?
        } else {
            if intent.phase == IntentPhase::IntentReady {
                self.hub.advance(IntentPhase::PhysicalStarted)?;
            }
            self.boundary(Boundary::PhysicalStarted)?;
            let snapshot = self.core.load()?;
            if snapshot
                .as_ref()
                .and_then(|s| s.pending())
                .is_some_and(|p| {
                    p.key() != Some(&intent.key) || p.publication_slot() != Some(*self.consumer())
                })
            {
                return Err(io::Error::from(io::ErrorKind::WouldBlock).into());
            }
            if snapshot.as_ref().is_some_and(|s| {
                s.entries()
                    .iter()
                    .any(|e| e.key() == &intent.key && e.publication_slot() == *self.consumer())
                    || s.pending().is_some_and(|p| {
                        p.key() == Some(&intent.key)
                            && p.publication_slot() == Some(*self.consumer())
                    })
            }) {
                self.core.forget(&intent.key)?;
            }
            self.boundary(Boundary::Forget)?;
            self.finish()?
        };
        let mut result = outcome(
            PublicationChange::Forgotten,
            preview.possible_parents.clone(),
        );
        result.forget_preview = Some(preview);
        result.cleanup_pending = cleanup_pending;
        Ok(result)
    }
    fn boundary(&mut self, point: Boundary) -> Result<()> {
        #[cfg(test)]
        {
            if let Some((boundary, marker)) = &self.pause_at
                && *boundary == point
            {
                fs::write(marker, b"durable boundary")?;
                loop {
                    std::thread::park_timeout(Duration::from_secs(1));
                }
            }
            if self.cancel_at == Some(point) {
                self.cancel_at = None;
                self.cancellation.cancel();
            }
            #[cfg(unix)]
            if self.readonly_at == Some(point) {
                use std::os::unix::fs::PermissionsExt;
                self.readonly_at = None;
                fs::set_permissions(
                    self.coordinator.registry.path(),
                    fs::Permissions::from_mode(0o400),
                )?;
            }
            if self.fault == Some(point) {
                self.fault = None;
                return Err(io::Error::other("publication test boundary").into());
            }
        }
        #[cfg(not(test))]
        let _ = point;
        Ok(())
    }
}
fn outcome(change: PublicationChange, possible_parents: Vec<NativePath>) -> PublicationOutcome {
    PublicationOutcome {
        change,
        possible_parents,
        forget_preview: None,
        cleanup_pending: None,
    }
}
fn parents(intent: &LogicalIntent) -> Vec<NativePath> {
    match &intent.goal {
        LogicalGoal::Publish { bindings } => bindings.possible_parents.clone(),
        LogicalGoal::Forget { preview, .. } => preview.possible_parents.clone(),
        LogicalGoal::Remove => Vec::new(),
    }
}
fn preview(
    snapshot: Option<&ArtifactSnapshot>,
    key: &ArtifactKey,
    consumer: PublicationSlot,
    possible_parents: Vec<NativePath>,
) -> io::Result<ForgetPreview> {
    let owned = snapshot
        .and_then(|s| {
            s.entries()
                .iter()
                .find(|e| e.key() == key && e.publication_slot() == consumer)
        })
        .map(|e| -> io::Result<_> {
            Ok(PhysicalDescriptor {
                source: NativePath::new(e.source().to_owned())?,
                destination: NativePath::new(e.destination().to_owned())?,
                node_kind: e.kind(),
                mode: e.mode(),
            })
        })
        .transpose()?;
    let pending = snapshot
        .and_then(|s| s.pending())
        .and_then(|p| p.publication())
        .filter(|p| p.key() == key && p.publication_slot() == consumer)
        .map(|p| -> io::Result<_> {
            Ok(PendingDescriptor {
                phase: p.phase(),
                candidate: PhysicalDescriptor {
                    source: NativePath::new(p.source().to_owned())?,
                    destination: NativePath::new(p.destination().to_owned())?,
                    node_kind: p.kind(),
                    mode: p.mode(),
                },
                staging_directory: p
                    .staging_directory()
                    .map(|v| NativePath::new(v.to_owned()))
                    .transpose()?,
                old_staging_directory: p
                    .old_staging_directory()
                    .map(|v| NativePath::new(v.to_owned()))
                    .transpose()?,
                old_destination: p
                    .old_destination()
                    .map(|v| NativePath::new(v.to_owned()))
                    .transpose()?,
                payload: p.payload().map(NativePath::new).transpose()?,
                backup: p.backup().map(NativePath::new).transpose()?,
                permissions_may_be_modified: p.permissions_may_be_modified(),
            })
        })
        .transpose()?;
    Ok(ForgetPreview {
        owned,
        pending,
        possible_parents,
    })
}

#[cfg(test)]
mod tests;
