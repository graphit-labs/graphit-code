//! Reproducible installations: prepare output, check user edits, replace, then record the result.
//! No durable intent, permission grants, backup, rollback or recovery state machine.
use super::*;

fn present(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}
fn remove_path(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;
        // Directory links use RemoveDirectory on Windows; leave their target intact.
        if metadata.file_type().is_symlink_dir() {
            return fs::remove_dir(path);
        }
    }
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}
impl ArtifactWriter {
    fn content_hash(
        &self,
        path: &Path,
        kind: NodeKind,
        policy: CopyPolicy,
    ) -> ArtifactResult<[u8; 32]> {
        tree::SourcePlan::capture_with_context(
            path,
            kind.into(),
            policy,
            self.store.limits.copy,
            &self.context,
        )
        .map(|p| p.fingerprint())
        .map_err(|e| at(ArtifactStage::Read, e))
    }
    pub(super) fn verify(&self, entry: &ArtifactEntry) -> ArtifactResult<()> {
        self.store
            .destination_scope(entry.destination())
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        match entry.mode() {
            ArtifactMode::Link => {
                if fs::read_link(entry.destination()).map_err(|e| at(ArtifactStage::Conflict, e))?
                    != entry.source()
                {
                    return Err(conflict());
                }
            }
            ArtifactMode::Copy(policy) => {
                if self.content_hash(entry.destination(), entry.kind(), policy)?
                    != entry.content_hash.ok_or_else(conflict)?
                {
                    return Err(conflict());
                }
            }
        }
        Ok(())
    }
    fn validate_destination(
        &self,
        key: &ArtifactKey,
        source: &Path,
        destination: &Path,
        kind: NodeKind,
    ) -> ArtifactResult<()> {
        self.context
            .check()
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        key.validate(self.store.limits.max_key_bytes)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        crate::native_path::absolute_input(source)
            .and_then(|()| valid_path(destination))
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        self.store
            .destination_scope(destination)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        if source == destination
            || destination.starts_with(self.store.runtime_root())
            || self.store.runtime_root().starts_with(destination)
            || (kind == NodeKind::Directory && destination.starts_with(source))
        {
            return Err(error(ArtifactStage::Validate, io::ErrorKind::InvalidInput));
        }
        Ok(())
    }
    fn commit_installation(
        &self,
        entry: ArtifactEntry,
        prepared: &Path,
    ) -> ArtifactResult<ArtifactChange> {
        // Preparation and hashing of the producer take place before the writer lock.
        let _lock = self.lock()?;
        let mut document = self.document()?;
        let old = self.entry(&document, &entry.key).cloned();
        if let Some(old) = &old {
            if present(old.destination()).map_err(|e| at(ArtifactStage::Conflict, e))?
                && let Err(error) = self.verify(old)
            {
                // A previous replacement may have succeeded before catalogue persistence failed.
                // Completing only metadata is safe when the current content already equals the requested output.
                if error.kind() == io::ErrorKind::AlreadyExists
                    && old.destination() == entry.destination()
                    && old.kind() == entry.kind()
                    && let ArtifactMode::Copy(policy) = entry.mode()
                    && Some(self.content_hash(entry.destination(), entry.kind(), policy)?)
                        == entry.content_hash
                {
                    document.entries.retain(|e| {
                        !(e.publication_slot() == entry.publication_slot()
                            && e.key() == entry.key())
                    });
                    document.entries.push(entry);
                    self.save(&document, false)?;
                    return Ok(ArtifactChange::Updated);
                }
                return Err(error);
            }
            if old.destination() == entry.destination()
                && old.mode() == entry.mode()
                && old.kind() == entry.kind()
                && old.source() == entry.source()
                && old.content_hash == entry.content_hash
                && present(entry.destination()).map_err(|e| at(ArtifactStage::Read, e))?
            {
                return Ok(ArtifactChange::Unchanged);
            }
        }
        if present(entry.destination()).map_err(|e| at(ArtifactStage::Read, e))?
            && old
                .as_ref()
                .is_none_or(|e| e.destination() != entry.destination())
        {
            return Err(conflict());
        }
        self.context
            .check()
            .map_err(|e| at(ArtifactStage::Publish, e))?;
        // Files can be atomically replaced. Directory installations are reproducible independent operations;
        // an interrupted replacement can be explicitly rerun without rolling back user files.
        let destination_exists =
            present(entry.destination()).map_err(|e| at(ArtifactStage::Read, e))?;
        let mut applied = false;
        if destination_exists
            && (entry.kind() == NodeKind::Directory
                || fs::symlink_metadata(entry.destination())
                    .map_err(|e| at(ArtifactStage::Read, e))?
                    .is_dir())
        {
            remove_path(entry.destination()).map_err(|e| ArtifactError {
                effect: ArtifactEffect::MayHaveApplied,
                ..at(ArtifactStage::Detach, e)
            })?;
            applied = true;
        }
        if let Err(e) = fs::rename(prepared, entry.destination()) {
            return Err(ArtifactError {
                effect: if applied {
                    ArtifactEffect::MayHaveApplied
                } else {
                    ArtifactEffect::NotApplied
                },
                ..at(ArtifactStage::Publish, e)
            });
        }
        if let Some(old) = &old
            && old.destination() != entry.destination()
            && present(old.destination()).map_err(|e| ArtifactError {
                effect: ArtifactEffect::Applied,
                ..at(ArtifactStage::Read, e)
            })?
        {
            remove_path(old.destination()).map_err(|e| ArtifactError {
                effect: ArtifactEffect::Applied,
                ..at(ArtifactStage::Detach, e)
            })?;
        }
        let change = if old.is_some() {
            ArtifactChange::Updated
        } else {
            ArtifactChange::Created
        };
        document.entries.retain(|e| {
            !(e.publication_slot() == entry.publication_slot() && e.key() == entry.key())
        });
        document.entries.push(entry);
        self.save(&document, true)?;
        Ok(change)
    }
    fn prepare_parent(&self, destination: &Path) -> ArtifactResult<tempfile::TempDir> {
        let parent = destination
            .parent()
            .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::InvalidInput))?;
        fs::create_dir_all(parent).map_err(|e| at(ArtifactStage::Create, e))?;
        tempfile::Builder::new()
            .prefix(".graphit-artifact-")
            .tempdir_in(parent)
            .map_err(|e| at(ArtifactStage::Prepare, e))
    }
    pub fn publish(
        &mut self,
        key: &ArtifactKey,
        source: &Path,
        destination: &Path,
        kind: NodeKind,
        mode: ArtifactMode,
    ) -> ArtifactResult<ArtifactChange> {
        self.validate_destination(key, source, destination, kind)?;
        let metadata = fs::metadata(source).map_err(|e| at(ArtifactStage::Read, e))?;
        if metadata.is_dir() != (kind == NodeKind::Directory) {
            return Err(error(ArtifactStage::Validate, io::ErrorKind::InvalidInput));
        }
        let stage = self.prepare_parent(destination)?;
        let payload = stage.path().join("payload");
        let content_hash = match mode {
            ArtifactMode::Link => {
                create_link(source, &payload)
                    .map_err(|e| error(ArtifactStage::Create, e.kind()))?;
                None
            }
            ArtifactMode::Copy(policy) => {
                let plan = tree::SourcePlan::capture_with_context(
                    source,
                    kind.into(),
                    policy,
                    self.store.limits.copy,
                    &self.context,
                )
                .map_err(|e| at(ArtifactStage::Read, e))?;
                plan.write_to(&payload, &self.context)
                    .map_err(|e| at(ArtifactStage::SourceChanged, e))?;
                Some(plan.fingerprint())
            }
        };
        self.commit_installation(
            ArtifactEntry {
                key: key.clone(),
                slot: self.publication_slot(),
                source: if mode == ArtifactMode::Link {
                    fs::read_link(&payload).map_err(|e| at(ArtifactStage::Read, e))?
                } else {
                    source.to_owned()
                },
                generated_origin: None,
                destination: destination.to_owned(),
                kind: kind.into(),
                mode,
                content_hash,
            },
            &payload,
        )
    }
    pub fn check_publication(
        &self,
        key: &ArtifactKey,
        source: &Path,
        destination: &Path,
        kind: NodeKind,
        mode: ArtifactMode,
    ) -> ArtifactResult<PublicationCheck> {
        self.validate_destination(key, source, destination, kind)?;
        let document = self.document()?;
        let Some(entry) = self.entry(&document, key) else {
            return Ok(PublicationCheck::NeedsPublication);
        };
        if !present(entry.destination()).map_err(|e| at(ArtifactStage::Read, e))? {
            return Ok(PublicationCheck::NeedsPublication);
        }
        self.verify(entry)?;
        if entry.source() != source
            || entry.destination() != destination
            || entry.kind() != kind
            || entry.mode() != mode
        {
            return Ok(PublicationCheck::NeedsPublication);
        }
        if let ArtifactMode::Copy(policy) = mode
            && Some(self.content_hash(source, kind, policy)?) != entry.content_hash
        {
            return Ok(PublicationCheck::NeedsPublication);
        }
        Ok(PublicationCheck::VerifiedUnchanged)
    }
    pub fn publish_generated(
        &mut self,
        key: &ArtifactKey,
        plan: &ValidatedPayloadPlan,
        destination: &Path,
        kind: NodeKind,
        mode: ArtifactMode,
    ) -> ArtifactResult<ArtifactChange> {
        if !matches!(mode, ArtifactMode::Copy(_)) || plan.kind() != kind {
            return Err(error(
                ArtifactStage::InvalidPolicy,
                io::ErrorKind::InvalidInput,
            ));
        }
        let source = std::env::current_exe().map_err(|e| at(ArtifactStage::Read, e))?;
        self.validate_destination(key, &source, destination, kind)?;
        plan.validate_limits(self.store.limits.copy, &self.context)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        let stage = self.prepare_parent(destination)?;
        let payload = stage.path().join("payload");
        plan.write_to(&payload, &self.context)
            .map_err(|e| at(ArtifactStage::SourceChanged, e))?;
        self.commit_installation(
            ArtifactEntry {
                key: key.clone(),
                slot: self.publication_slot(),
                source,
                generated_origin: Some(GeneratedOrigin {
                    package: "graphit".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                }),
                destination: destination.to_owned(),
                kind: kind.into(),
                mode,
                content_hash: Some(plan.fingerprint()),
            },
            &payload,
        )
    }
    pub fn check_generated_publication(
        &self,
        key: &ArtifactKey,
        plan: &ValidatedPayloadPlan,
        destination: &Path,
        kind: NodeKind,
        mode: ArtifactMode,
    ) -> ArtifactResult<PublicationCheck> {
        if !matches!(mode, ArtifactMode::Copy(_)) {
            return Err(error(
                ArtifactStage::InvalidPolicy,
                io::ErrorKind::InvalidInput,
            ));
        }
        let document = self.document()?;
        let Some(entry) = self.entry(&document, key) else {
            return Ok(PublicationCheck::NeedsPublication);
        };
        if !present(entry.destination()).map_err(|e| at(ArtifactStage::Read, e))? {
            return Ok(PublicationCheck::NeedsPublication);
        }
        self.verify(entry)?;
        Ok(
            if entry.destination() == destination
                && entry.kind() == kind
                && entry.mode() == mode
                && entry.content_hash == Some(plan.fingerprint())
            {
                PublicationCheck::VerifiedUnchanged
            } else {
                PublicationCheck::NeedsPublication
            },
        )
    }

    /// Recognize a portable generated payload already present in a checkout.
    /// The files stay unowned. A later update or removal still requires a receipt.
    pub fn matches_unowned_generated(
        &self,
        key: &ArtifactKey,
        plan: &ValidatedPayloadPlan,
        destination: &Path,
    ) -> ArtifactResult<bool> {
        let source = std::env::current_exe().map_err(|e| at(ArtifactStage::Read, e))?;
        self.validate_destination(key, &source, destination, plan.kind())?;
        let _lock = self.lock()?;
        let document = self.document()?;
        if self.entry(&document, key).is_some()
            || document
                .entries
                .iter()
                .any(|entry| entry.destination() == destination)
            || !present(destination).map_err(|e| at(ArtifactStage::Read, e))?
        {
            return Ok(false);
        }
        let captured = (|| {
            let limits = RetainedBytesLimits::new(1, 1)?;
            let policy = SnapshotPolicy::new(
                CopyPolicy::RejectEntryLinks,
                self.store.limits.copy,
                RetainedFileSet::new([], limits)?,
                limits,
            )?;
            SourceSnapshot::capture(destination, plan.kind(), &policy, &self.context)
        })()
        .map_err(|e| at(ArtifactStage::Read, e))?;
        // Git preserves file content but does not preserve native permission modes.
        let expected = plan.nodes().map(|node| {
            (
                node.relative().to_owned(),
                node.kind(),
                node.bytes(),
                node.digest(),
            )
        });
        let observed = captured.nodes().map(|node| {
            (
                node.relative().to_owned(),
                node.kind(),
                node.bytes(),
                node.digest(),
            )
        });
        Ok(expected.eq(observed))
    }
    pub fn remove(&mut self, key: &ArtifactKey) -> ArtifactResult<ArtifactChange> {
        let _lock = self.lock()?;
        let mut document = self.document()?;
        let index = document
            .entries
            .iter()
            .position(|e| e.publication_slot() == self.publication_slot() && e.key() == key)
            .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::NotFound))?;
        let entry = &document.entries[index];
        let exists = present(entry.destination()).map_err(|e| at(ArtifactStage::Read, e))?;
        if exists {
            self.verify(entry)?;
            self.context
                .check()
                .map_err(|e| at(ArtifactStage::Detach, e))?;
            remove_path(entry.destination()).map_err(|e| ArtifactError {
                effect: ArtifactEffect::MayHaveApplied,
                ..at(ArtifactStage::Detach, e)
            })?;
        }
        document.entries.remove(index);
        self.save(&document, exists)?;
        Ok(ArtifactChange::Removed)
    }
}
