use super::{
    tree::{self, Access, NodeReceipt, TreeReceipt},
    *,
};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Phase {
    Building,
    Ready,
    Cleanup,
}
impl From<Phase> for ArtifactPending {
    fn from(phase: Phase) -> Self {
        match phase {
            Phase::Building => Self::Building,
            Phase::Ready => Self::Ready,
            Phase::Cleanup => Self::Cleanup,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Goal {
    Create,
    Update,
    Recreate,
    Remove,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Location {
    Payload,
    OldDestination,
    NewDestination,
    Backup,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Movement {
    Publish,
    Detach,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Stage {
    pub path: NativePath,
    pub receipt: Option<NodeReceipt>,
}
impl Stage {
    fn new(destination: &Path) -> io::Result<Self> {
        Ok(Self {
            path: NativePath(
                destination
                    .parent()
                    .ok_or(io::ErrorKind::InvalidInput)?
                    .join(format!(".graphit-artifact-{}", ulid::Ulid::generate())),
            ),
            receipt: None,
        })
    }
    fn validate(&self, destination: &Path, building: bool) -> io::Result<()> {
        valid_path(&self.path.0)?;
        let name = self
            .path
            .0
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or(io::ErrorKind::InvalidData)?;
        if name
            .strip_prefix(".graphit-artifact-")
            .is_none_or(|s| s.parse::<ulid::Ulid>().is_err())
            || self.path.0.parent() != destination.parent()
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if let Some(receipt) = &self.receipt {
            receipt.id.validate()?;
            receipt.mode.validate()?;
            if !receipt.relative.0.as_os_str().is_empty()
                || receipt.content != tree::Content::Directory
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
        } else if !building {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(())
    }
    fn verify(&self, payload: bool, backup: bool) -> io::Result<()> {
        tree::checked_node(
            &self.path.0,
            self.receipt.as_ref().ok_or(io::ErrorKind::AlreadyExists)?,
            None,
            false,
        )?;
        for item in fs::read_dir(&self.path.0)? {
            let name = item?.file_name();
            if !(payload && name == "payload" || backup && name == "backup") {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Operation {
    pub candidate: Candidate,
    pub mode: ArtifactMode,
    pub stage: Stage,
    // Equal parents share the primary wrapper and its one physical authority.
    pub old_stage: Option<Stage>,
    pub prepared: Option<Proof>,
    pub old: Option<ArtifactEntry>,
    pub phase: Phase,
    pub goal: Goal,
    pub aborting: bool,
    pub read_access: Option<Location>,
    pub movement: Option<Movement>,
}
fn overlap(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
impl Operation {
    pub(super) fn new(
        candidate: Candidate,
        mode: ArtifactMode,
        old: Option<ArtifactEntry>,
        goal: Goal,
    ) -> io::Result<Self> {
        let stage = Stage::new(&candidate.destination.0)?;
        let old_stage = old
            .as_ref()
            .filter(|entry| entry.destination().parent() != candidate.destination.0.parent())
            .map(|entry| Stage::new(entry.destination()))
            .transpose()?;
        Ok(Self {
            candidate,
            mode,
            stage,
            old_stage,
            prepared: None,
            old,
            phase: Phase::Building,
            goal,
            aborting: false,
            read_access: None,
            movement: None,
        })
    }
    pub(super) fn stages(&self) -> impl Iterator<Item = &Stage> {
        std::iter::once(&self.stage).chain(self.old_stage.iter())
    }
    pub(super) fn payload(&self) -> PathBuf {
        self.stage.path.0.join("payload")
    }
    pub(super) fn backup(&self) -> PathBuf {
        self.old_stage
            .as_ref()
            .unwrap_or(&self.stage)
            .path
            .0
            .join("backup")
    }
    fn new_entry(&self) -> io::Result<ArtifactEntry> {
        Ok(ArtifactEntry {
            candidate: self.candidate.clone(),
            receipt: self.prepared.clone().ok_or(io::ErrorKind::InvalidData)?,
        })
    }
    fn location(&self, location: Location) -> io::Result<(PathBuf, &Proof, &Candidate)> {
        match location {
            Location::Payload | Location::NewDestination => Ok((
                if location == Location::Payload {
                    self.payload()
                } else {
                    self.candidate.destination.0.clone()
                },
                self.prepared.as_ref().ok_or(io::ErrorKind::InvalidData)?,
                &self.candidate,
            )),
            Location::OldDestination | Location::Backup => {
                let old = self.old.as_ref().ok_or(io::ErrorKind::InvalidData)?;
                Ok((
                    if location == Location::Backup {
                        self.backup()
                    } else {
                        old.destination().to_owned()
                    },
                    &old.receipt,
                    &old.candidate,
                ))
            }
        }
    }
    fn moved_proof(
        &self,
        movement: Movement,
    ) -> io::Result<(PathBuf, PathBuf, &Proof, &Candidate)> {
        let (from_location, to_location) = match movement {
            Movement::Publish => (Location::Payload, Location::NewDestination),
            Movement::Detach => (Location::OldDestination, Location::Backup),
        };
        let (from, proof, candidate) = self.location(from_location)?;
        let (to, _, _) = self.location(to_location)?;
        Ok((from, to, proof, candidate))
    }
    pub fn validate(&self, document: &Document, store: &ArtifactStore) -> io::Result<()> {
        self.validate_at_entries(&document.entries, store)
    }
    pub(super) fn validate_destinations(&self, scope: &ScopeBindingRecord) -> io::Result<()> {
        scope.destination(&self.candidate.destination.0)?;
        if let Some(old) = &self.old {
            scope.destination(old.destination())?;
        }
        for stage in self.stages() {
            scope.destination(&stage.path.0)?;
        }
        Ok(())
    }
    fn authorize_destinations(&self, store: &ArtifactStore) -> io::Result<()> {
        store.require_publication_slot(&self.candidate)?;
        store.destination_scope(&self.candidate.destination.0)?;
        if let Some(old) = &self.old {
            store.destination_scope(old.destination())?;
        }
        for stage in self.stages() {
            store.destination_scope(&stage.path.0)?;
        }
        Ok(())
    }
    fn validate_at_entries(
        &self,
        entries: &[ArtifactEntry],
        store: &ArtifactStore,
    ) -> io::Result<()> {
        let invalid = || io::Error::from(io::ErrorKind::InvalidData);
        codec::validate_candidate_data(&self.candidate, store)?;
        self.stage
            .validate(&self.candidate.destination.0, self.phase == Phase::Building)?;
        if let Some(old) = &self.old {
            codec::validate_candidate_data(&old.candidate, store)?;
            old.receipt.validate(&old.candidate, store)?;
            if self.goal == Goal::Create
                || !old.matches_publication(self.candidate.slot, &self.candidate.key)
            {
                return Err(invalid());
            }
            if self.goal == Goal::Recreate && !same_owned_file_copy(old, &self.candidate, self.mode)
            {
                return Err(invalid());
            }
            let different_parent =
                old.destination().parent() != self.candidate.destination.0.parent();
            if different_parent != self.old_stage.is_some() {
                return Err(invalid());
            }
            if let Some(stage) = &self.old_stage {
                stage.validate(old.destination(), self.phase == Phase::Building)?;
            }
            if old.destination() != self.candidate.destination.0
                && overlap(old.destination(), &self.candidate.destination.0)
            {
                return Err(invalid());
            }
            if matches!(old.receipt, Proof::Copy { .. })
                && self.candidate.source.0.starts_with(old.destination())
            {
                return Err(invalid());
            }
            if self.goal == Goal::Remove
                && (old.candidate != self.candidate || old.mode() != self.mode)
            {
                return Err(invalid());
            }
        } else if self.goal != Goal::Create || self.old_stage.is_some() {
            return Err(invalid());
        }
        if self.aborting && (self.phase != Phase::Cleanup || self.goal == Goal::Remove)
            || self.goal == Goal::Remove && self.prepared.is_some()
            || !self.aborting
                && self.goal != Goal::Remove
                && self.phase != Phase::Building
                && self.prepared.is_none()
            || self.phase == Phase::Building
                && (self.prepared.is_some()
                    || self.read_access.is_some()
                    || self.movement.is_some())
        {
            return Err(invalid());
        }
        if let Some(proof) = &self.prepared {
            proof.validate(&self.candidate, store)?;
            if proof.mode() != self.mode {
                return Err(invalid());
            }
        }
        let expected = if self.phase == Phase::Cleanup && !self.aborting {
            if self.goal == Goal::Remove {
                None
            } else {
                Some(self.new_entry()?)
            }
        } else {
            self.old.clone()
        };
        if entries
            .iter()
            .find(|e| e.matches_publication(self.candidate.slot, &self.candidate.key))
            != expected.as_ref()
        {
            return Err(invalid());
        }
        let mut ids = std::collections::HashSet::new();
        for entry in entries {
            if !entry.matches_publication(self.candidate.slot, &self.candidate.key) {
                entry.receipt.register_ids(&mut ids)?;
                if overlap(entry.destination(), &self.candidate.destination.0)
                    || self
                        .old
                        .as_ref()
                        .is_some_and(|old| overlap(entry.destination(), old.destination()))
                {
                    return Err(invalid());
                }
            }
        }
        if self.goal != Goal::Recreate
            && let Some(old) = &self.old
        {
            old.receipt.register_ids(&mut ids)?;
        }
        if let Some(proof) = &self.prepared {
            proof.register_ids(&mut ids)?;
        }
        for stage in self.stages() {
            if overlap(&stage.path.0, &self.candidate.source.0)
                || overlap(&stage.path.0, &self.candidate.destination.0)
                || self.old.as_ref().is_some_and(|old| {
                    overlap(&stage.path.0, old.destination())
                        || overlap(&stage.path.0, old.source())
                })
                || entries
                    .iter()
                    .any(|e| overlap(&stage.path.0, e.destination()))
            {
                return Err(invalid());
            }
            if let Some(receipt) = &stage.receipt
                && !ids.insert(receipt.id.clone())
            {
                return Err(invalid());
            }
        }
        if self
            .old_stage
            .as_ref()
            .is_some_and(|old| overlap(&old.path.0, &self.stage.path.0))
        {
            return Err(invalid());
        }
        if let Some(location) = self.read_access {
            let (_, proof, _) = self.location(location)?;
            if self.phase != Phase::Ready
                || !matches!(proof, Proof::Copy { .. })
                || self.movement.is_some()
                || self.goal == Goal::Recreate
                    && matches!(location, Location::OldDestination | Location::Backup)
            {
                return Err(invalid());
            }
        }
        if let Some(movement) = self.movement {
            let (_, _, proof, _) = self.moved_proof(movement)?;
            if self.phase != Phase::Ready
                || self.read_access.is_some()
                || movement == Movement::Detach
                    && matches!(self.goal, Goal::Create | Goal::Recreate)
                || movement == Movement::Publish && self.goal == Goal::Remove
            {
                return Err(invalid());
            }
            if let Proof::Copy { tree, .. } = proof {
                let root = tree.nodes.first().ok_or_else(invalid)?;
                if !root.content.directory()
                    || !matches!(root.mode, tree::Mode::Unix(mode) if mode & 0o200 == 0)
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
    fn verify_wrappers(&self) -> io::Result<()> {
        self.stage.verify(
            self.goal != Goal::Remove,
            self.old.is_some() && self.old_stage.is_none() && self.goal != Goal::Recreate,
        )?;
        if let Some(stage) = &self.old_stage {
            stage.verify(false, true)?;
        }
        Ok(())
    }
    pub(super) fn permissions_may_be_modified(&self) -> bool {
        self.read_access.is_some()
            || self.movement.is_some_and(|movement| {
                self.moved_proof(movement)
                    .is_ok_and(|(_, _, proof, _)| matches!(proof, Proof::Copy { .. }))
            })
            || self.phase == Phase::Cleanup
                && (if self.aborting {
                    self.prepared
                        .as_ref()
                        .is_some_and(|proof| matches!(proof, Proof::Copy { .. }))
                } else {
                    self.old
                        .as_ref()
                        .is_some_and(|entry| matches!(entry.receipt, Proof::Copy { .. }))
                })
    }
    fn same_parents(&self) -> io::Result<()> {
        for path in std::iter::once(self.candidate.destination.0.as_path())
            .chain(self.old.iter().map(ArtifactEntry::destination))
        {
            let parent = path.parent().ok_or(io::ErrorKind::InvalidData)?;
            if fs::canonicalize(parent)? != parent {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
        }
        Ok(())
    }
}
/// Durable intent for an incomplete publication, including wrappers without a
/// recorded birth receipt. It grants no ownership or cleanup authority.
#[derive(Clone, Serialize)]
pub(super) struct AbandonedPublication {
    pub scope: ScopeBindingRecord,
    pub operation: Operation,
}
impl AbandonedPublication {
    pub(super) fn validate(&self, store: &ArtifactStore) -> io::Result<()> {
        if self.operation.phase != Phase::Building
            || self.operation.goal == Goal::Remove
            || !self.scope.matches_catalog(&store.scope)
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        // The old entry is historical after a later successful publication. It
        // must remain a valid intent, without demanding that it is still current.
        self.operation.validate_destinations(&self.scope)?;
        self.operation
            .validate_at_entries(self.operation.old.as_slice(), store)
    }
}

// Only a recorded copied file at its unchanged slot can be recreated. Missing
// directories, links, moved destinations and existing foreign entries stay errors.
fn same_owned_file_copy(entry: &ArtifactEntry, candidate: &Candidate, mode: ArtifactMode) -> bool {
    entry.destination() == candidate.destination.0
        && entry.candidate.kind == NodeKind::File.into()
        && candidate.kind == NodeKind::File.into()
        && matches!(entry.receipt, Proof::Copy { .. })
        && matches!(mode, ArtifactMode::Copy(_))
}
fn missing_owned_file(
    entry: &ArtifactEntry,
    candidate: &Candidate,
    mode: ArtifactMode,
) -> io::Result<bool> {
    if !same_owned_file_copy(entry, candidate, mode) {
        return Ok(false);
    }
    match fs::symlink_metadata(entry.destination()) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = entry
                .destination()
                .parent()
                .ok_or(io::ErrorKind::InvalidInput)?;
            let metadata = fs::symlink_metadata(parent)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            Ok(true)
        }
        Err(error) => Err(error),
    }
}
fn present(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}
fn native_error(stage: ArtifactStage, failure: LinkFailure) -> ArtifactError {
    ArtifactError {
        stage,
        kind: failure.kind(),
        native: Some(failure),
        secondary: None,
    }
}
fn reconcile_error(
    primary: io::Result<()>,
    restoration: ArtifactResult<()>,
    stage: ArtifactStage,
) -> ArtifactResult<()> {
    match (primary, restoration) {
        (Err(primary), Err(secondary)) => Err(ArtifactError {
            stage,
            kind: primary.kind(),
            native: None,
            secondary: Some(secondary.kind()),
        }),
        (Err(primary), Ok(())) => Err(at(stage, primary)),
        (Ok(()), restoration) => restoration,
    }
}
impl ArtifactWriter {
    pub(super) fn io_fault(&self, stage: ArtifactStage) -> io::Result<()> {
        #[cfg(test)]
        {
            self.inject(stage).map_err(|e| io::Error::from(e.kind()))?;
        }
        if stage == ArtifactStage::CopyChunk {
            self.scope_context.check()?;
        }
        self.store.scope.check(None)?;
        Ok(())
    }
    fn set_operation(
        &self,
        document: &mut Document,
        operation: &Operation,
        stage: ArtifactStage,
    ) -> ArtifactResult<()> {
        if stage == ArtifactStage::Prepare {
            #[cfg(test)]
            if let Some(before_prepare) = self.before_prepare.borrow_mut().take() {
                before_prepare();
            }
            self.scope_context.check().map_err(|e| at(stage, e))?;
        }
        document.pending = Some(Pending::Publication(Box::new(operation.clone())));
        self.save(document, stage)
    }
    fn create_wrappers(
        &self,
        document: &mut Document,
        operation: &mut Operation,
    ) -> ArtifactResult<()> {
        for index in 0..1 + usize::from(operation.old_stage.is_some()) {
            self.scope_context
                .check()
                .map_err(|e| at(ArtifactStage::Create, e))?;
            self.io_fault(ArtifactStage::Create)
                .map_err(|e| at(ArtifactStage::Create, e))?;
            let stage = if index == 0 {
                &mut operation.stage
            } else {
                operation.old_stage.as_mut().ok_or_else(conflict)?
            };
            let born = graphit_fs::create_private_directory(&stage.path.0)
                .map_err(|e| at(ArtifactStage::Create, e))?;
            self.io_fault(ArtifactStage::Observe)
                .map_err(|e| at(ArtifactStage::Observe, e))?;
            let observed =
                TreeReceipt::directory(&stage.path.0).map_err(|e| at(ArtifactStage::Observe, e))?;
            if tree::handle_id(&born).map_err(|e| at(ArtifactStage::Observe, e))? != observed.id {
                return Err(conflict());
            }
            graphit_fs::verify_private_directory(&born)
                .map_err(|e| at(ArtifactStage::Observe, e))?;
            stage.receipt = Some(observed);
            if operation.goal == Goal::Remove {
                operation.phase = Phase::Ready;
                self.set_operation(document, operation, ArtifactStage::RecordProof)?;
            } else {
                self.set_operation(document, operation, ArtifactStage::RecordWrapper)?;
            }
        }
        Ok(())
    }
    fn restore_movement(
        &self,
        document: &mut Document,
        operation: &mut Operation,
    ) -> ArtifactResult<()> {
        let Some(movement) = operation.movement else {
            return Ok(());
        };
        let (from, to, proof, candidate) = operation
            .moved_proof(movement)
            .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        let from_present = present(&from).map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        let to_present = present(&to).map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        let path = match (from_present, to_present) {
            (true, false) => &from,
            (false, true) => &to,
            _ => {
                return Err(error(
                    ArtifactStage::RecoveryConflict,
                    io::ErrorKind::AlreadyExists,
                ));
            }
        };
        match proof {
            Proof::Link { receipt: expected } => {
                let observed = receipt::observe_at(candidate, path)
                    .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?
                    .ok_or_else(conflict)?;
                if if from_present {
                    &observed != expected
                } else {
                    !observed.same_identity(expected)
                } {
                    return Err(error(
                        ArtifactStage::RecoveryConflict,
                        io::ErrorKind::AlreadyExists,
                    ));
                }
                if !from_present {
                    match movement {
                        Movement::Publish => {
                            operation.prepared = Some(Proof::Link { receipt: observed })
                        }
                        Movement::Detach => {
                            let old = operation.old.as_mut().ok_or_else(conflict)?;
                            old.receipt = Proof::Link { receipt: observed };
                            let entry = document
                                .entries
                                .iter_mut()
                                .find(|e| e.matches_publication(old.publication_slot(), old.key()))
                                .ok_or_else(conflict)?;
                            *entry = old.clone();
                        }
                    }
                    // Keep movement authority until its refreshed full receipt is durable.
                    self.set_operation(document, operation, ArtifactStage::RefreshProof)?;
                }
            }
            Proof::Copy { tree, .. } => {
                let root = tree.nodes.first().ok_or_else(conflict)?;
                tree::checked_node(path, root, Some(Access::Move), false)
                    .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
                root.mode
                    .set(path)
                    .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
                self.io_fault(ArtifactStage::RestoreMove)
                    .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
            }
        }
        operation.movement = None;
        self.set_operation(document, operation, ArtifactStage::ClearMove)
    }
    fn rename_owned(
        &self,
        document: &mut Document,
        operation: &mut Operation,
        movement: Movement,
        stage: ArtifactStage,
        cancellable: bool,
    ) -> ArtifactResult<()> {
        let (from, to, proof, _) = operation.moved_proof(movement).map_err(|e| at(stage, e))?;
        let root = match proof {
            Proof::Copy { tree, .. } => tree.nodes.first().cloned(),
            _ => None,
        };
        let grant = root.as_ref().is_some_and(|root| {
            root.content.directory()
                && matches!(root.mode, tree::Mode::Unix(mode) if mode & 0o200 == 0)
        });
        let recorded = grant || matches!(proof, Proof::Link { .. });
        if cancellable {
            self.scope_context.check().map_err(|e| at(stage, e))?;
        }
        if recorded {
            operation.movement = Some(movement);
            self.set_operation(document, operation, ArtifactStage::RecordMove)?;
        }
        if grant {
            let root = root.as_ref().ok_or_else(conflict)?;
            tree::checked_node(&from, root, Some(Access::Move), false).map_err(|e| at(stage, e))?;
            root.mode
                .granted(true, Access::Move)
                .set(&from)
                .map_err(|e| at(stage, e))?;
            self.io_fault(ArtifactStage::GrantMove)
                .map_err(|e| at(stage, e))?;
        }
        self.io_fault(ArtifactStage::BeforeRename)
            .map_err(|e| at(ArtifactStage::BeforeRename, e))?;
        if cancellable && let Err(e) = self.scope_context.check() {
            let restoration = if recorded {
                self.restore_movement(document, operation)
            } else {
                Ok(())
            };
            return reconcile_error(Err(e), restoration, stage);
        }
        graphit_fs::rename_exclusive(&from, &to).map_err(|e| at(stage, e))?;
        self.io_fault(stage).map_err(|e| at(stage, e))?;
        if recorded {
            self.restore_movement(document, operation)?;
        }
        Ok(())
    }
    /// Publish generated package content directly to its effective installation.
    /// The only intermediate copy is the recoverable, same-parent publication stage.
    pub fn publish_generated(
        &mut self,
        key: &ArtifactKey,
        plan: &ValidatedPayloadPlan,
        destination: &Path,
        kind: NodeKind,
        mode: ArtifactMode,
    ) -> ArtifactResult<ArtifactChange> {
        let ArtifactMode::Copy(policy) = mode else {
            return Err(error(
                ArtifactStage::InvalidPolicy,
                io::ErrorKind::InvalidInput,
            ));
        };
        let context = self
            .io_context
            .clone()
            .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::InvalidInput))?;
        context
            .check()
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        if plan.kind() != kind {
            return Err(error(ArtifactStage::Validate, io::ErrorKind::InvalidInput));
        }
        plan.validate_limits(self.store.limits.copy, &context)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        plan.revalidate_content(&context)
            .map_err(|e| at(ArtifactStage::SourceChanged, e))?;
        let mut document = self.document()?;
        self.ready(&document)?;
        // Generated bytes originate in the installed package, not a retained cache.
        let source = std::env::current_exe().map_err(|e| at(ArtifactStage::Validate, e))?;
        let (mut candidate, old) =
            self.publication_candidate(&document, key, source, destination, kind, mode)?;
        candidate.generated_origin = Some(GeneratedOrigin {
            package: "graphit".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        });
        let mut old = old.cloned();
        if let Some(entry) = &mut old {
            self.refresh_generated_receipt(&mut document, entry)?;
        }
        let recreate = old
            .as_ref()
            .map(|entry| missing_owned_file(entry, &candidate, mode))
            .transpose()
            .map_err(|e| at(ArtifactStage::Conflict, e))?
            .unwrap_or(false);
        if let Some(entry) = &old
            && !recreate
            && entry.destination() == candidate.destination.0
            && entry.kind() == kind
            && entry.mode() == mode
            && entry.content_fingerprint() == Some(plan.fingerprint())
        {
            self.verify_owned(&mut document, entry, ArtifactStage::Conflict)?;
            return Ok(ArtifactChange::Unchanged);
        }
        if let Some(entry) = &old
            && !recreate
        {
            self.verify_owned(&mut document, entry, ArtifactStage::Conflict)?;
        }
        let goal = if recreate {
            Goal::Recreate
        } else if old.is_some() {
            Goal::Update
        } else {
            Goal::Create
        };
        let mut operation = Operation::new(candidate, mode, old, goal)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        operation
            .validate(&document, &self.store)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        self.set_operation(&mut document, &operation, ArtifactStage::Prepare)?;
        self.create_wrappers(&mut document, &mut operation)?;
        let mut created = TreeReceipt { nodes: Vec::new() };
        use tree::BuildTree;
        let built = plan.build_tree(
            &operation.payload(),
            &self.cancellation,
            Some(&context),
            &mut created,
            &mut || self.io_fault(ArtifactStage::CopyChunk),
        );
        if let Err(primary) = built {
            operation.phase = Phase::Cleanup;
            operation.aborting = true;
            operation.prepared = (!created.nodes.is_empty()).then_some(Proof::Copy {
                policy,
                tree: created,
            });
            let cleanup = self
                .set_operation(&mut document, &operation, ArtifactStage::Cleanup)
                .and_then(|()| self.finish_cleanup(&mut document, &operation).map(|_| ()));
            return Err(ArtifactError {
                stage: ArtifactStage::SourceChanged,
                kind: primary.kind(),
                native: None,
                secondary: cleanup.err().map(|e| e.kind()),
            });
        }
        operation.prepared = Some(Proof::Copy {
            policy,
            tree: created,
        });
        operation.phase = Phase::Ready;
        self.set_operation(&mut document, &operation, ArtifactStage::RecordProof)?;
        match self.recover_publication(document, operation)? {
            ArtifactRecovery::CompletedCreate => Ok(ArtifactChange::Created),
            ArtifactRecovery::CompletedUpdate => Ok(ArtifactChange::Updated),
            _ => Err(error(
                ArtifactStage::RecoveryConflict,
                io::ErrorKind::InvalidData,
            )),
        }
    }

    /// Verify installed content using its recorded hash; no source copy is required.
    pub fn check_generated_receipt(
        &self,
        key: &ArtifactKey,
        fingerprint: [u8; 32],
        destination: &Path,
        kind: NodeKind,
        mode: ArtifactMode,
    ) -> ArtifactResult<PublicationCheck> {
        let ArtifactMode::Copy(_) = mode else {
            return Err(error(
                ArtifactStage::InvalidPolicy,
                io::ErrorKind::InvalidInput,
            ));
        };
        key.validate(self.store.limits.max_key_bytes)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        self.store
            .destination_scope(destination)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        let document = self.document()?;
        self.ready(&document)?;
        let Some(entry) = document
            .entries
            .iter()
            .find(|entry| entry.matches_publication(self.publication_slot(), key))
        else {
            return Ok(PublicationCheck::NeedsPublication);
        };
        if entry.destination() != destination
            || entry.kind() != kind
            || entry.mode() != mode
            || entry.content_fingerprint() != Some(fingerprint)
        {
            return Ok(PublicationCheck::NeedsPublication);
        }
        if missing_owned_file(entry, &entry.candidate, mode)
            .map_err(|e| at(ArtifactStage::Conflict, e))?
        {
            return Ok(PublicationCheck::NeedsPublication);
        }
        self.verify(entry, ArtifactStage::Conflict)?;
        Ok(PublicationCheck::VerifiedUnchanged)
    }

    pub fn check_generated_publication(
        &self,
        key: &ArtifactKey,
        plan: &ValidatedPayloadPlan,
        destination: &Path,
        kind: NodeKind,
        mode: ArtifactMode,
    ) -> ArtifactResult<PublicationCheck> {
        let context = self
            .io_context
            .as_ref()
            .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::InvalidInput))?;
        if plan.kind() != kind {
            return Err(error(ArtifactStage::Validate, io::ErrorKind::InvalidInput));
        }
        plan.validate_limits(self.store.limits.copy, context)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        plan.revalidate_content(context)
            .map_err(|e| at(ArtifactStage::SourceChanged, e))?;
        self.check_generated_receipt(key, plan.fingerprint(), destination, kind, mode)
    }

    fn refresh_generated_receipt(
        &self,
        document: &mut Document,
        entry: &mut ArtifactEntry,
    ) -> ArtifactResult<()> {
        if entry.generated_origin().is_none() {
            return Ok(());
        }
        let Proof::Copy { tree, .. } = &mut entry.receipt else {
            return Err(conflict());
        };
        if !present(&entry.candidate.destination.0).map_err(|e| at(ArtifactStage::Conflict, e))? {
            return Ok(());
        }
        let refreshed = tree.recapture_content(
            &entry.candidate.destination.0,
            self.store.limits.copy,
            &self.cancellation,
            self.io_context.as_deref(),
        );
        *tree = match refreshed {
            Ok(tree) => tree,
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && cfg!(unix) => {
                self.verify_owned(document, entry, ArtifactStage::Conflict)?;
                return Ok(());
            }
            Err(e) => return Err(at(ArtifactStage::Conflict, e)),
        };
        if let Some(current) = document
            .entries
            .iter_mut()
            .find(|current| current.matches_publication(entry.publication_slot(), entry.key()))
        {
            *current = entry.clone();
        }
        Ok(())
    }

    fn publication_source_plan(
        &self,
        candidate: &Candidate,
        policy: CopyPolicy,
    ) -> ArtifactResult<tree::SourcePlan> {
        match &self.io_context {
            Some(context) => tree::SourcePlan::capture_with_context(
                &candidate.source.0,
                candidate.kind,
                policy,
                self.store.limits.copy,
                context,
            ),
            None => tree::SourcePlan::capture(
                &candidate.source.0,
                candidate.kind,
                policy,
                self.store.limits.copy,
                &self.cancellation,
            ),
        }
        .map_err(|e| at(ArtifactStage::SourceChanged, e))
    }
    fn publication_input<'a>(
        &self,
        document: &'a Document,
        key: &ArtifactKey,
        source: PhysicalSource<'_>,
        destination: &Path,
        kind: NodeKind,
        mode: ArtifactMode,
    ) -> ArtifactResult<(Candidate, Option<&'a ArtifactEntry>)> {
        self.ready(document)?;
        key.validate(self.store.limits.max_key_bytes)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        let PhysicalSource::ExternalSource(source) = source;
        absolute_input(source).map_err(|e| at(ArtifactStage::Validate, e))?;
        absolute_input(destination).map_err(|e| at(ArtifactStage::Validate, e))?;
        if let ArtifactMode::Copy(policy) = mode {
            let original =
                fs::symlink_metadata(source).map_err(|e| at(ArtifactStage::Validate, e))?;
            if original.file_type().is_symlink()
                && (policy == CopyPolicy::RejectFileLinks
                    || !fs::metadata(source)
                        .map_err(|e| at(ArtifactStage::Validate, e))?
                        .is_file())
            {
                return Err(error(ArtifactStage::Validate, io::ErrorKind::InvalidInput));
            }
        }
        let canonical_source =
            fs::canonicalize(source).map_err(|e| at(ArtifactStage::Validate, e))?;
        let metadata =
            fs::metadata(&canonical_source).map_err(|e| at(ArtifactStage::Validate, e))?;
        if !match kind {
            NodeKind::File => metadata.is_file(),
            NodeKind::Directory => metadata.is_dir(),
        } {
            return Err(error(ArtifactStage::Validate, io::ErrorKind::InvalidInput));
        }
        self.publication_candidate(document, key, canonical_source, destination, kind, mode)
    }
    fn publication_candidate<'a>(
        &self,
        document: &'a Document,
        key: &ArtifactKey,
        resolved_source: PathBuf,
        destination: &Path,
        kind: NodeKind,
        mode: ArtifactMode,
    ) -> ArtifactResult<(Candidate, Option<&'a ArtifactEntry>)> {
        key.validate(self.store.limits.max_key_bytes)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        absolute_input(destination).map_err(|e| at(ArtifactStage::Validate, e))?;
        self.store
            .destination_scope(destination)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        let candidate = Candidate {
            key: key.clone(),
            slot: self.publication_slot(),
            source: NativePath(resolved_source),
            generated_origin: None,
            destination: NativePath(
                captured_destination(destination).map_err(|e| at(ArtifactStage::Validate, e))?,
            ),
            kind: kind.into(),
        };
        codec::validate_candidate(&candidate, &self.store)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        if matches!(mode, ArtifactMode::Copy(_))
            && overlap(&candidate.source.0, &candidate.destination.0)
        {
            return Err(error(ArtifactStage::Validate, io::ErrorKind::InvalidInput));
        }
        let old = document
            .entries
            .iter()
            .find(|e| e.matches_publication(candidate.slot, key));
        if let Some(entry) = old {
            self.store
                .destination_scope(entry.destination())
                .map_err(|e| at(ArtifactStage::Validate, e))?;
        }
        if document.entries.iter().any(|e| {
            !e.matches_publication(candidate.slot, key)
                && overlap(e.destination(), &candidate.destination.0)
        }) {
            return Err(conflict());
        }
        if let Some(entry) = &old
            && (entry.destination() != candidate.destination.0
                && overlap(entry.destination(), &candidate.destination.0)
                || matches!(entry.receipt, Proof::Copy { .. })
                    && candidate.source.0.starts_with(entry.destination()))
        {
            return Err(conflict());
        }
        if old
            .as_ref()
            .is_none_or(|entry| entry.destination() != candidate.destination.0)
            && present(&candidate.destination.0).map_err(|e| at(ArtifactStage::Validate, e))?
        {
            return Err(conflict());
        }
        Ok((candidate, old))
    }

    pub(super) fn check_owned_publication(
        &self,
        key: &ArtifactKey,
        source: PhysicalSource<'_>,
        destination: &Path,
        kind: NodeKind,
        mode: ArtifactMode,
    ) -> ArtifactResult<PublicationCheck> {
        let document = self.document()?;
        let (candidate, old) =
            self.publication_input(&document, key, source, destination, kind, mode)?;
        let mut result = PublicationCheck::NeedsPublication;
        if let Some(entry) = &old
            && !missing_owned_file(entry, &candidate, mode)
                .map_err(|e| at(ArtifactStage::Conflict, e))?
        {
            let readable = match self.verify(entry, ArtifactStage::Conflict) {
                Ok(()) => true,
                #[cfg(unix)]
                Err(e)
                    if matches!(entry.receipt, Proof::Copy { .. })
                        && e.kind() == io::ErrorKind::PermissionDenied =>
                {
                    false
                }
                Err(e) => return Err(e),
            };
            if entry.candidate == candidate && entry.mode() == mode {
                let equivalent = match mode {
                    ArtifactMode::Link => true,
                    ArtifactMode::Copy(policy) => {
                        // Source errors are never hidden by an unreadable owned destination.
                        let plan = self.publication_source_plan(&candidate, policy)?;
                        plan.recheck(self.store.limits.copy, &self.cancellation)
                            .map_err(|e| at(ArtifactStage::SourceChanged, e))?;
                        entry.equivalent_copy(&candidate, &plan)
                    }
                };
                if readable && equivalent {
                    #[cfg(test)]
                    if let Some(external_change) = self.before_check_rebind.borrow_mut().take() {
                        external_change();
                    }
                    // Re-observe input bindings after the proof; the result is no reservation.
                    let (current, _) =
                        self.publication_input(&document, key, source, destination, kind, mode)?;
                    if current != candidate {
                        return Err(error(
                            ArtifactStage::SourceChanged,
                            io::ErrorKind::AlreadyExists,
                        ));
                    }
                    result = PublicationCheck::VerifiedUnchanged;
                }
            }
        }
        #[cfg(test)]
        self.inject(ArtifactStage::Observe)?;
        self.scope_context
            .check()
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        Ok(result)
    }

    pub(super) fn publish_owned(
        &mut self,
        key: &ArtifactKey,
        source: PhysicalSource<'_>,
        destination: &Path,
        kind: NodeKind,
        mode: ArtifactMode,
    ) -> ArtifactResult<ArtifactChange> {
        let mut document = self.document()?;
        let (candidate, old) =
            self.publication_input(&document, key, source, destination, kind, mode)?;
        let old = old.cloned();
        let recreate = old
            .as_ref()
            .map(|entry| missing_owned_file(entry, &candidate, mode))
            .transpose()
            .map_err(|e| at(ArtifactStage::Conflict, e))?
            .unwrap_or(false);
        if let Some(entry) = &old
            && entry.candidate == candidate
            && mode == ArtifactMode::Link
            && entry.mode() == mode
        {
            self.verify(entry, ArtifactStage::Conflict)?;
            return Ok(ArtifactChange::Unchanged);
        }
        let plan = match mode {
            ArtifactMode::Copy(policy) => Some(self.publication_source_plan(&candidate, policy)?),
            ArtifactMode::Link => None,
        };
        if !recreate
            && let Some(entry) = &old
            && let Proof::Copy {
                tree,
                policy: old_policy,
            } = &entry.receipt
            && let ArtifactMode::Copy(policy) = mode
            && let Some(plan) = &plan
            && entry.equivalent_copy(&candidate, plan)
        {
            plan.recheck(self.store.limits.copy, &self.cancellation)
                .map_err(|e| at(ArtifactStage::SourceChanged, e))?;
            if entry.candidate == candidate && old_policy == &policy {
                self.verify_owned(&mut document, entry, ArtifactStage::Conflict)?;
                return Ok(ArtifactChange::Unchanged);
            }
            let rebound = ArtifactEntry {
                candidate,
                receipt: Proof::Copy {
                    policy,
                    tree: tree.clone(),
                },
            };
            self.verify_owned(&mut document, entry, ArtifactStage::Conflict)?;
            document
                .entries
                .retain(|e| !e.matches_publication(self.publication_slot(), key));
            document.entries.push(rebound);
            self.save(&document, ArtifactStage::Commit)?;
            return Ok(ArtifactChange::Updated);
        }
        let goal = if recreate {
            Goal::Recreate
        } else if old.is_some() {
            Goal::Update
        } else {
            Goal::Create
        };
        let mut operation = Operation::new(candidate, mode, old, goal)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        // Validate the current operation before any physical publication effect.
        operation
            .validate(&document, &self.store)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        if let Some(entry) = &operation.old {
            if recreate {
                if !missing_owned_file(entry, &operation.candidate, operation.mode)
                    .map_err(|e| at(ArtifactStage::Conflict, e))?
                {
                    return Err(conflict());
                }
            } else {
                self.verify_owned(&mut document, entry, ArtifactStage::Conflict)?;
            }
        }
        self.set_operation(&mut document, &operation, ArtifactStage::Prepare)?;
        self.create_wrappers(&mut document, &mut operation)?;
        match mode {
            ArtifactMode::Link => {
                let link =
                    LinkPlan::capture(&operation.candidate.source.0, &operation.payload(), kind)
                        .map_err(|e| native_error(ArtifactStage::Validate, e))?;
                self.scope_context
                    .check()
                    .map_err(|e| at(ArtifactStage::Create, e))?;
                link.create()
                    .map_err(|e| native_error(ArtifactStage::Create, e))?;
                self.io_fault(ArtifactStage::ObservePayload)
                    .map_err(|e| at(ArtifactStage::ObservePayload, e))?;
                operation.prepared = Some(Proof::Link {
                    receipt: receipt::observe_at(&operation.candidate, &operation.payload())
                        .map_err(|e| at(ArtifactStage::ObservePayload, e))?
                        .ok_or_else(conflict)?,
                });
            }
            ArtifactMode::Copy(policy) => {
                let plan = plan.as_ref().ok_or_else(conflict)?;
                let mut created = TreeReceipt { nodes: Vec::new() };
                let built = plan
                    .build(
                        &operation.payload(),
                        &self.cancellation,
                        self.io_context.as_deref(),
                        &mut created,
                        || self.io_fault(ArtifactStage::CopyChunk),
                    )
                    .and_then(|()| match &self.io_context {
                        Some(context) => plan.recheck_with_context(self.store.limits.copy, context),
                        None => plan.recheck(self.store.limits.copy, &self.cancellation),
                    });
                if let Err(primary) = built {
                    operation.phase = Phase::Cleanup;
                    operation.aborting = true;
                    operation.prepared = (!created.nodes.is_empty()).then_some(Proof::Copy {
                        policy,
                        tree: created,
                    });
                    let cleanup = self
                        .set_operation(&mut document, &operation, ArtifactStage::Cleanup)
                        .and_then(|()| self.finish_cleanup(&mut document, &operation).map(|_| ()));
                    return Err(ArtifactError {
                        stage: ArtifactStage::SourceChanged,
                        kind: primary.kind(),
                        native: None,
                        secondary: cleanup.err().map(|e| e.kind()),
                    });
                }
                operation.prepared = Some(Proof::Copy {
                    policy,
                    tree: created,
                });
            }
        }
        operation.phase = Phase::Ready;
        self.set_operation(&mut document, &operation, ArtifactStage::RecordProof)?;
        match self.recover_publication(document, operation)? {
            ArtifactRecovery::CompletedCreate => Ok(ArtifactChange::Created),
            ArtifactRecovery::CompletedUpdate => Ok(ArtifactChange::Updated),
            _ => Err(error(
                ArtifactStage::RecoveryConflict,
                io::ErrorKind::InvalidData,
            )),
        }
    }
    fn verify_owned(
        &self,
        document: &mut Document,
        entry: &ArtifactEntry,
        stage: ArtifactStage,
    ) -> ArtifactResult<()> {
        self.store
            .require_publication_slot(&entry.candidate)
            .map_err(|e| at(stage, e))?;
        self.store
            .destination_scope(entry.destination())
            .map_err(|e| at(stage, e))?;
        let Proof::Copy { tree, .. } = &entry.receipt else {
            return self.verify(entry, stage);
        };
        match self.verify_tree(tree, entry.destination(), None, false, &self.cancellation) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && cfg!(unix) => {
                self.scope_context
                    .check()
                    .map_err(|e| at(ArtifactStage::GrantAccess, e))?;
                document.pending = Some(Pending::Inspect {
                    entry: Box::new(entry.clone()),
                });
                self.save(document, ArtifactStage::GrantAccess)?;
                let primary = self
                    .grant_tree(tree, entry.destination(), Access::Read, false, || {
                        self.io_fault(ArtifactStage::GrantAccess)
                    })
                    .and_then(|()| {
                        self.verify_tree(
                            tree,
                            entry.destination(),
                            Some(Access::Read),
                            false,
                            &self.cancellation,
                        )
                    });
                let restoration = self.restore_inspect(entry).and_then(|()| {
                    document.pending = None;
                    self.save(document, ArtifactStage::RestoreAccess)
                });
                reconcile_error(primary, restoration, stage)
            }
            Err(e) => Err(at(stage, e)),
        }
    }
    pub(super) fn restore_inspect(&self, entry: &ArtifactEntry) -> ArtifactResult<()> {
        self.store
            .require_publication_slot(&entry.candidate)
            .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        self.store
            .destination_scope(entry.destination())
            .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        let Proof::Copy { tree, .. } = &entry.receipt else {
            return Err(conflict());
        };
        tree.restore(entry.destination(), || {
            self.io_fault(ArtifactStage::RestoreAccess)
        })
        .map_err(|e| at(ArtifactStage::RecoveryConflict, e))
    }
    fn prove_at(
        &self,
        document: &mut Document,
        operation: &mut Operation,
        location: Location,
    ) -> ArtifactResult<()> {
        let (path, proof, candidate) = operation
            .location(location)
            .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        let tree = match proof {
            Proof::Link { receipt: expected } => {
                if receipt::observe_at(candidate, &path)
                    .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?
                    .as_ref()
                    != Some(expected)
                {
                    return Err(error(
                        ArtifactStage::RecoveryConflict,
                        io::ErrorKind::AlreadyExists,
                    ));
                }
                return Ok(());
            }
            Proof::Copy { tree, .. } => tree.clone(),
        };
        match self.verify_tree(&tree, &path, None, false, &Cancellation::default()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && cfg!(unix) => {
                operation.read_access = Some(location);
                self.set_operation(document, operation, ArtifactStage::GrantAccess)?;
                let primary = self
                    .grant_tree(&tree, &path, Access::Read, false, || {
                        self.io_fault(ArtifactStage::GrantAccess)
                    })
                    .and_then(|()| {
                        self.verify_tree(
                            &tree,
                            &path,
                            Some(Access::Read),
                            false,
                            &Cancellation::default(),
                        )
                    });
                let restoration = tree
                    .restore(&path, || self.io_fault(ArtifactStage::RestoreAccess))
                    .map_err(|e| at(ArtifactStage::RecoveryConflict, e))
                    .and_then(|()| {
                        operation.read_access = None;
                        self.set_operation(document, operation, ArtifactStage::RestoreAccess)
                    });
                reconcile_error(primary, restoration, ArtifactStage::RecoveryConflict)
            }
            Err(e) => Err(at(ArtifactStage::RecoveryConflict, e)),
        }
    }
    pub(super) fn remove_owned(
        &self,
        mut document: Document,
        mut entry: ArtifactEntry,
    ) -> ArtifactResult<ArtifactChange> {
        self.refresh_generated_receipt(&mut document, &mut entry)?;
        let mut operation = Operation::new(
            entry.candidate.clone(),
            entry.mode(),
            Some(entry),
            Goal::Remove,
        )
        .map_err(|e| at(ArtifactStage::Validate, e))?;
        operation
            .authorize_destinations(&self.store)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        operation
            .validate(&document, &self.store)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        self.verify_owned(
            &mut document,
            operation.old.as_ref().ok_or_else(conflict)?,
            ArtifactStage::Conflict,
        )?;
        self.set_operation(&mut document, &operation, ArtifactStage::Prepare)?;
        self.create_wrappers(&mut document, &mut operation)?;
        self.recover_publication(document, operation)?;
        Ok(ArtifactChange::Removed)
    }
    pub(super) fn recover_publication(
        &self,
        mut document: Document,
        mut operation: Operation,
    ) -> ArtifactResult<ArtifactRecovery> {
        operation
            .authorize_destinations(&self.store)
            .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        operation
            .same_parents()
            .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        if operation.phase == Phase::Building {
            let mut any_present = false;
            for stage in operation.stages() {
                any_present |=
                    present(&stage.path.0).map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
            }
            // Absence can abort only before a proved payload, with ALL wrappers absent.
            if !any_present {
                document.pending = None;
                self.save(&document, ArtifactStage::Commit)?;
                return Ok(ArtifactRecovery::AbortedPublication);
            }
            if operation.goal == Goal::Remove {
                return Err(error(
                    ArtifactStage::IncompleteStage,
                    io::ErrorKind::AlreadyExists,
                ));
            }
            // A birth receipt proves only the private wrapper, never every
            // partial child. Validate recorded wrappers and preserve the complete
            // intent. Unreceipted namespaces remain unknown and untouched.
            self.scope_context
                .check()
                .map_err(|e| at(ArtifactStage::Validate, e))?;
            let mut held_stages = Vec::new();
            for stage in operation.stages() {
                let Some(receipt) = stage.receipt.as_ref() else {
                    continue;
                };
                if present(&stage.path.0).map_err(|e| at(ArtifactStage::RecoveryConflict, e))? {
                    tree::checked_node(&stage.path.0, receipt, None, false)
                        .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
                    let held = crate::native_document_file::open_access_directory(&stage.path.0)
                        .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
                    graphit_fs::verify_private_directory(&held)
                        .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
                    if tree::handle_id(&held).map_err(|e| at(ArtifactStage::RecoveryConflict, e))?
                        != receipt.id
                    {
                        return Err(conflict());
                    }
                    stage
                        .verify(stage.path == operation.stage.path, false)
                        .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
                    held_stages.push((stage.clone(), held));
                }
            }
            for (stage, held) in &held_stages {
                graphit_fs::verify_private_directory(held)
                    .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
                tree::checked_node(
                    &stage.path.0,
                    stage.receipt.as_ref().ok_or_else(conflict)?,
                    None,
                    false,
                )
                .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
            }
            self.scope_context
                .check()
                .map_err(|e| at(ArtifactStage::Validate, e))?;
            document.abandoned_publications.push(AbandonedPublication {
                scope: document.scope.clone(),
                operation,
            });
            document.pending = None;
            self.save(&document, ArtifactStage::Commit)?;
            return Ok(ArtifactRecovery::AbortedPublication);
        }
        if operation.phase == Phase::Cleanup {
            return self.finish_cleanup(&mut document, &operation);
        }
        operation
            .verify_wrappers()
            .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        if operation.movement.is_some() {
            self.restore_movement(&mut document, &mut operation)?;
        }
        if let Some(location) = operation.read_access {
            let (path, proof, _) = operation
                .location(location)
                .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
            let Proof::Copy { tree, .. } = proof else {
                return Err(conflict());
            };
            tree.restore(&path, || self.io_fault(ArtifactStage::RestoreAccess))
                .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
            operation.read_access = None;
            self.set_operation(&mut document, &operation, ArtifactStage::RestoreAccess)?;
        }
        let has_payload =
            present(&operation.payload()).map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        let has_backup =
            present(&operation.backup()).map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        let has_new = present(&operation.candidate.destination.0)
            .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        if matches!(operation.goal, Goal::Create | Goal::Recreate) {
            if has_backup {
                return Err(error(
                    ArtifactStage::RecoveryConflict,
                    io::ErrorKind::AlreadyExists,
                ));
            }
            match (has_payload, has_new) {
                (true, false) => {
                    self.prove_at(&mut document, &mut operation, Location::Payload)?;
                    self.rename_owned(
                        &mut document,
                        &mut operation,
                        Movement::Publish,
                        ArtifactStage::Publish,
                        true,
                    )?;
                }
                (false, true) => {
                    self.prove_at(&mut document, &mut operation, Location::NewDestination)?
                }
                _ => {
                    return Err(error(
                        ArtifactStage::RecoveryConflict,
                        io::ErrorKind::AlreadyExists,
                    ));
                }
            }
        } else {
            let old = operation.old.as_ref().ok_or_else(conflict)?;
            let same_destination = old.destination() == operation.candidate.destination.0;
            let has_old =
                present(old.destination()).map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
            if !has_backup {
                if !has_old
                    || (operation.goal == Goal::Update) != has_payload
                    || !same_destination && has_new
                {
                    return Err(error(
                        ArtifactStage::RecoveryConflict,
                        io::ErrorKind::AlreadyExists,
                    ));
                }
                self.prove_at(&mut document, &mut operation, Location::OldDestination)?;
                if operation.goal == Goal::Update {
                    self.prove_at(&mut document, &mut operation, Location::Payload)?;
                }
                self.rename_owned(
                    &mut document,
                    &mut operation,
                    Movement::Detach,
                    ArtifactStage::Detach,
                    true,
                )?;
            } else if !same_destination && has_old {
                return Err(error(
                    ArtifactStage::RecoveryConflict,
                    io::ErrorKind::AlreadyExists,
                ));
            }
            self.prove_at(&mut document, &mut operation, Location::Backup)?;
            if operation.goal == Goal::Update {
                match (
                    present(&operation.payload())
                        .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?,
                    present(&operation.candidate.destination.0)
                        .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?,
                ) {
                    (true, false) => {
                        self.prove_at(&mut document, &mut operation, Location::Payload)?;
                        self.rename_owned(
                            &mut document,
                            &mut operation,
                            Movement::Publish,
                            ArtifactStage::Publish,
                            false,
                        )?;
                    }
                    (false, true) => {
                        self.prove_at(&mut document, &mut operation, Location::NewDestination)?
                    }
                    _ => {
                        return Err(error(
                            ArtifactStage::RecoveryConflict,
                            io::ErrorKind::AlreadyExists,
                        ));
                    }
                }
            } else if has_payload
                || present(&operation.candidate.destination.0)
                    .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?
            {
                return Err(error(
                    ArtifactStage::RecoveryConflict,
                    io::ErrorKind::AlreadyExists,
                ));
            }
        }
        document
            .entries
            .retain(|e| !e.matches_publication(operation.candidate.slot, &operation.candidate.key));
        if operation.goal != Goal::Remove {
            if let Some(Proof::Copy { tree, .. }) = &mut operation.prepared {
                // The destination effect is already observed. Finish the
                // cooperative journal despite pre-effect cancellation, as in
                // prove_at. A native Original remains enforced when present;
                // its failure leaves the durable operation recoverable.
                tree.record_verified(
                    &operation.candidate.destination.0,
                    self.store.limits.copy,
                    &Cancellation::default(),
                    self.io_context.as_deref(),
                )
                .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
            }
            document.entries.push(
                operation
                    .new_entry()
                    .map_err(|e| at(ArtifactStage::Commit, e))?,
            );
        }
        operation.phase = Phase::Cleanup;
        self.set_operation(&mut document, &operation, ArtifactStage::Commit)?;
        self.finish_cleanup(&mut document, &operation)
    }
    fn cleanup_proof(&self, path: &Path, proof: &Proof, candidate: &Candidate) -> io::Result<()> {
        match proof {
            Proof::Link { receipt: expected } => {
                if !present(path)? {
                    return Ok(());
                }
                if receipt::observe_at(candidate, path)?.as_ref() != Some(expected) {
                    return Err(io::ErrorKind::AlreadyExists.into());
                }
                receipt::remove_at(candidate.kind, path)?;
                self.io_fault(ArtifactStage::Cleanup)
            }
            Proof::Copy { tree, .. } => match &self.io_context {
                Some(context) => {
                    tree.cleanup_with_context(path, self.store.limits.copy, context, || {
                        self.io_fault(ArtifactStage::Cleanup)
                    })
                }
                None => tree.cleanup(path, self.store.limits.copy, || {
                    self.io_fault(ArtifactStage::Cleanup)
                }),
            },
        }
    }
    fn finish_cleanup(
        &self,
        document: &mut Document,
        operation: &Operation,
    ) -> ArtifactResult<ArtifactRecovery> {
        let cleanup = || -> io::Result<()> {
            // Inspect each surviving wrapper independently; one cleaned wrapper cannot hide another.
            if present(&operation.stage.path.0)? {
                operation.stage.verify(
                    operation.goal != Goal::Remove,
                    operation.old.is_some()
                        && operation.old_stage.is_none()
                        && operation.goal != Goal::Recreate,
                )?;
                if operation.aborting {
                    if present(&operation.backup())? {
                        return Err(io::ErrorKind::AlreadyExists.into());
                    }
                    match &operation.prepared {
                        Some(proof) => {
                            self.cleanup_proof(&operation.payload(), proof, &operation.candidate)?
                        }
                        None if present(&operation.payload())? => {
                            return Err(io::ErrorKind::AlreadyExists.into());
                        }
                        None => {}
                    }
                } else if present(&operation.payload())? {
                    return Err(io::ErrorKind::AlreadyExists.into());
                }
            }
            if let Some(stage) = &operation.old_stage
                && present(&stage.path.0)?
            {
                stage.verify(false, true)?;
            }
            if !operation.aborting
                && operation.goal != Goal::Recreate
                && let Some(old) = &operation.old
            {
                self.cleanup_proof(&operation.backup(), &old.receipt, &old.candidate)?;
            } else if present(&operation.backup())? {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            for stage in operation.stages() {
                if present(&stage.path.0)? {
                    tree::checked_node(
                        &stage.path.0,
                        stage.receipt.as_ref().ok_or(io::ErrorKind::InvalidData)?,
                        None,
                        false,
                    )?;
                    fs::remove_dir(&stage.path.0)?;
                    self.io_fault(ArtifactStage::Cleanup)?;
                }
            }
            Ok(())
        };
        cleanup().map_err(|e| at(ArtifactStage::CleanupRequired, e))?;
        document.pending = None;
        self.save(document, ArtifactStage::Commit)?;
        Ok(if operation.aborting {
            ArtifactRecovery::AbortedPublication
        } else {
            match operation.goal {
                Goal::Create => ArtifactRecovery::CompletedCreate,
                Goal::Update | Goal::Recreate => ArtifactRecovery::CompletedUpdate,
                Goal::Remove => ArtifactRecovery::CompletedRemove,
            }
        })
    }
}

impl ArtifactEntry {
    fn equivalent_copy(&self, candidate: &Candidate, plan: &tree::SourcePlan) -> bool {
        self.destination() == candidate.destination.0
            && self.candidate.kind == candidate.kind
            && matches!(&self.receipt, Proof::Copy { tree, .. } if tree.equivalent(plan))
    }
}
