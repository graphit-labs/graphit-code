//! Selection membership/Auth stays with the Source owner; captured bytes stay with Core.
use super::{
    Access, ArtifactSelection, EventData, ReadContext, ReadContexts, Session, SessionState,
    service::{LiveError, LiveSessionService, append},
    workspace::Workspace,
};
use graphit_core::{artifact_store::CapturedSourceReader, file_io::SnapshotContext};
use graphit_fs::NodeKind;
use graphit_hub::native::RenderedContent;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, btree_map::Entry},
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub(crate) struct CapturedArtifact<'a> {
    pub selection: &'a ArtifactSelection,
    pub reader: &'a CapturedSourceReader,
}
/// Data stamps bind selected read contexts; SDK captures and authority stay with the caller.
pub(crate) enum PreparedSelection<'a> {
    NativeText {
        artifact: CapturedArtifact<'a>,
        source_entry: &'a Path,
        rendered: &'a RenderedContent,
    },
    InlineRule(CapturedArtifact<'a>),
    ReadContext {
        selection: &'a ArtifactSelection,
        original: &'a Arc<SnapshotContext>,
    },
}
impl PreparedSelection<'_> {
    fn selection(&self) -> &ArtifactSelection {
        match self {
            Self::NativeText { artifact, .. } | Self::InlineRule(artifact) => artifact.selection,
            Self::ReadContext { selection, .. } => selection,
        }
    }
}
/// Native content must come from the real Hub descriptor/renderer and selected binding.
/// `allowed` retains every actual Source PROJECT/installed-context admission until completion.
pub(crate) struct Preparation<'a> {
    pub agent: CapturedArtifact<'a>,
    pub agent_source_entry: &'a Path,
    pub rendered: &'a RenderedContent,
    pub artifacts: &'a [PreparedSelection<'a>],
    pub allowed: &'a (dyn Fn() -> bool + Sync),
}
impl LiveSessionService {
    pub fn prepare(
        &self,
        access: &Access<'_>,
        id: &str,
        input: Preparation<'_>,
    ) -> Result<Session, LiveError> {
        self.require_preparation_access(access, id)?;
        let effect = self.persist_terminal_with_effect(access, id)?;
        let slot = self
            .history_slot(access, id)
            .map_err(|error| LiveError::from(error).retain_prior_effect(effect))?;
        let mut generation = slot
            .lock()
            .map_err(|_| {
                LiveError::from(io::Error::other("Live custody lock")).retain_prior_effect(effect)
            })?
            .generation
            .clone();
        self.prepare_current(access, id, input, &mut generation)
            .map_err(|error| {
                let cause = error.retain_prior_effect(effect);
                match generation.as_deref() {
                    Some(generation) => {
                        self.fail_preparation_in_slot(access, id, generation, &slot, cause)
                    }
                    None => cause,
                }
            })
    }
    fn prepare_current(
        &self,
        access: &Access<'_>,
        id: &str,
        input: Preparation<'_>,
        generation: &mut Option<String>,
    ) -> Result<Session, LiveError> {
        access.write()?;
        if self.stop.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if !(input.allowed)() {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let stored = self
            .journal(access)?
            .stored(id, access.original().snapshot(), || access.read())?;
        *generation = Some(stored.session.cursor.generation.clone());
        if stored.session.state == SessionState::Closed {
            return Err(LiveError::Closed);
        }
        if !stored.native_quiescent() {
            return Err(LiveError::Busy);
        }
        if input.agent.selection != &stored.request.agent
            || input.artifacts.len() != stored.request.artifacts.len()
            || input
                .artifacts
                .iter()
                .zip(&stored.request.artifacts)
                .any(|(actual, wanted)| actual.selection() != wanted)
            || input.rendered.provenance.binding != stored.request.binding
        {
            return Err(LiveError::Conflict);
        }
        let current = access.original().snapshot();
        if input.agent.selection.artifact_type != "agent" {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Workspace::relative(Path::new(&input.rendered.relative_entry))?;
        input.agent.reader.check_request(current)?;
        input.agent.reader.revalidate_content()?;
        Workspace::source_entry(input.agent.reader, input.agent_source_entry)?;
        let mut contexts = ReadContexts {
            selected: Vec::new(),
        };
        let mut fingerprint = Sha256::new();
        hash_part(&mut fingerprint, b"graphit-live-preparation-v3")?;
        hash_part(
            &mut fingerprint,
            &serde_json::to_vec(input.agent.selection).map_err(io::Error::other)?,
        )?;
        hash_part(&mut fingerprint, &input.agent.reader.fingerprint())?;
        hash_rendered(&mut fingerprint, input.rendered)?;
        hash_part(
            &mut fingerprint,
            input.agent_source_entry.as_os_str().as_encoded_bytes(),
        )?;
        let mut destinations = BTreeMap::new();
        reserve_native(
            &mut destinations,
            input.agent.reader,
            input.agent_source_entry,
            Path::new(&input.rendered.relative_entry),
        )?;
        for artifact in input.artifacts {
            hash_part(
                &mut fingerprint,
                &serde_json::to_vec(artifact.selection()).map_err(io::Error::other)?,
            )?;
            match artifact {
                PreparedSelection::NativeText {
                    artifact,
                    source_entry,
                    rendered,
                } => {
                    if !matches!(artifact.selection.artifact_type.as_str(), "agent" | "skill")
                        || rendered.provenance.binding != stored.request.binding
                    {
                        return Err(io::ErrorKind::InvalidInput.into());
                    }
                    artifact.reader.check_request(current)?;
                    Workspace::source_entry(artifact.reader, source_entry)?;
                    Workspace::relative(Path::new(&rendered.relative_entry))?;
                    artifact.reader.revalidate_content()?;
                    reserve_native(
                        &mut destinations,
                        artifact.reader,
                        source_entry,
                        Path::new(&rendered.relative_entry),
                    )?;
                    hash_part(&mut fingerprint, b"native-text")?;
                    hash_part(&mut fingerprint, &artifact.reader.fingerprint())?;
                    hash_part(
                        &mut fingerprint,
                        source_entry.as_os_str().as_encoded_bytes(),
                    )?;
                    hash_rendered(&mut fingerprint, rendered)?;
                }
                PreparedSelection::InlineRule(artifact) => {
                    if artifact.selection.artifact_type != "rule" {
                        return Err(io::ErrorKind::InvalidInput.into());
                    }
                    artifact.reader.check_request(current)?;
                    artifact.reader.revalidate_content()?;
                    hash_part(&mut fingerprint, b"inline-rule")?;
                    hash_part(&mut fingerprint, &artifact.reader.fingerprint())?;
                }
                PreparedSelection::ReadContext {
                    selection,
                    original,
                } => {
                    if !matches!(selection.artifact_type.as_str(), "knowledge" | "ast")
                        || !Arc::ptr_eq(original, current)
                    {
                        return Err(io::ErrorKind::InvalidInput.into());
                    }
                    contexts.selected.push(ReadContext {
                        selection: (*selection).clone(),
                    });
                    original.check()?;
                    hash_part(&mut fingerprint, b"read-context")?;
                }
            }
        }
        contexts.validate(&stored.session)?;
        let fingerprint = hex(&fingerprint.finalize());
        let slot = self.slot(access, id)?;
        let mut state = slot
            .lock()
            .map_err(|_| io::Error::other("Live custody lock"))?;
        super::preparation_guard::preparation_matches(&state, access)?;
        state.bind_generation(&stored.session.cursor.generation)?;
        if state.active.is_some() {
            return Err(LiveError::Busy);
        }
        if stored
            .prepared_fingerprint
            .as_ref()
            .is_some_and(|previous| previous != &fingerprint)
        {
            return Err(LiveError::Conflict);
        }
        if state.workspace.is_none() {
            let workspace = match stored.workspace.clone() {
                Some(record) if stored.session.state == SessionState::Ready => {
                    Workspace::restore(access, record, self.limits.native)?
                }
                Some(_) => return Err(LiveError::NotReady),
                None => Workspace::create(access, self.limits.native)?,
            };
            state.workspace = Some(Arc::new(Mutex::new(workspace)));
        }
        // The exact returned birth handle is already retained in state before any further IO.
        let workspace = Arc::clone(state.workspace.as_ref().ok_or(LiveError::NotReady)?);
        let record = {
            let mut workspace = workspace
                .lock()
                .map_err(|_| io::Error::other("Live workspace lock"))?;
            workspace.rebind(access)?;
            workspace.record(self.limits.native, current)?
        };
        let birth_effect = if stored.workspace.is_none() {
            let (_, effect) = self.journal(access)?.change_with_effect(
                current,
                || access.write(),
                |sessions| {
                    let actual = sessions.get_mut(id).ok_or(io::ErrorKind::NotFound)?;
                    if actual.request != stored.request
                        || actual.session.cursor != stored.session.cursor
                    {
                        return Err(io::ErrorKind::AlreadyExists.into());
                    }
                    if actual
                        .workspace
                        .as_ref()
                        .is_some_and(|previous| previous != &record)
                    {
                        return Err(io::ErrorKind::AlreadyExists.into());
                    }
                    actual.workspace = Some(record.clone());
                    Ok(())
                },
            )?;
            effect
        } else {
            if stored.workspace.as_ref() != Some(&record) {
                return Err(LiveError::Conflict);
            }
            graphit_core::native_document_file::NativeDocumentJournalEffect::Unchanged
        };
        let result = (|| {
            let mut workspace = workspace
                .lock()
                .map_err(|_| io::Error::other("Live workspace lock"))?;
            workspace.rebind(access)?;
            if stored.workspace.is_some() {
                if !workspace.matches(
                    Path::new(&input.rendered.relative_entry),
                    &input.rendered.bytes,
                    current,
                )? || !workspace.matches_assets(
                    input.agent.reader,
                    input.agent_source_entry,
                    Path::new(&input.rendered.relative_entry),
                    current,
                )? {
                    return Err(LiveError::Conflict);
                }
                for artifact in input.artifacts {
                    if let PreparedSelection::NativeText {
                        artifact,
                        source_entry,
                        rendered,
                    } = artifact
                        && (!workspace.matches(
                            Path::new(&rendered.relative_entry),
                            &rendered.bytes,
                            current,
                        )? || !workspace.matches_assets(
                            artifact.reader,
                            source_entry,
                            Path::new(&rendered.relative_entry),
                            current,
                        )?)
                    {
                        return Err(LiveError::Conflict);
                    }
                }
            } else {
                workspace.copy_assets(
                    input.agent.reader,
                    input.agent_source_entry,
                    Path::new(&input.rendered.relative_entry),
                    current,
                )?;
                workspace.write(
                    Path::new(&input.rendered.relative_entry),
                    &input.rendered.bytes,
                    current,
                )?;
                for artifact in input.artifacts {
                    if let PreparedSelection::NativeText {
                        artifact,
                        source_entry,
                        rendered,
                    } = artifact
                    {
                        workspace.copy_assets(
                            artifact.reader,
                            source_entry,
                            Path::new(&rendered.relative_entry),
                            current,
                        )?;
                        workspace.write(
                            Path::new(&rendered.relative_entry),
                            &rendered.bytes,
                            current,
                        )?;
                    }
                }
            }
            drop(workspace);
            if self.stop.is_cancelled() {
                return Err(io::ErrorKind::Interrupted.into());
            }
            if !(input.allowed)() {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            current.check()?;
            access.write()?;
            let result = self.journal(access)?.change(
                current,
                || access.write(),
                |sessions| {
                    let actual = sessions.get_mut(id).ok_or(io::ErrorKind::NotFound)?;
                    if actual.request != stored.request
                        || actual.session.cursor != stored.session.cursor
                    {
                        return Err(io::ErrorKind::AlreadyExists.into());
                    }
                    actual.workspace = Some(record);
                    actual.effective_agent = Some(input.rendered.provenance.effective_name.clone());
                    actual.prepared_fingerprint = Some(fingerprint);
                    actual.read_contexts = Some(contexts);
                    append(
                        actual,
                        None,
                        EventData::Prep {
                            message: "Selected sources prepared".into(),
                            artifact: None,
                        },
                    )?;
                    if actual.session.state != SessionState::Ready {
                        append(
                            actual,
                            None,
                            EventData::State {
                                state: SessionState::Ready,
                            },
                        )?;
                    }
                    Ok(actual.session.clone())
                },
            );
            state.changed.send_replace(());
            let result = result?;
            state.prepared_original = Some(Arc::clone(current));
            Ok(result)
        })();
        result.map_err(|error: LiveError| error.retain_prior_effect(birth_effect))
    }
}

fn reserve_native(
    destinations: &mut BTreeMap<PathBuf, NodeKind>,
    reader: &CapturedSourceReader,
    source_entry: &Path,
    native_entry: &Path,
) -> Result<(), LiveError> {
    if !source_entry.as_os_str().is_empty() {
        Workspace::relative(source_entry)?;
    }
    reserve_destination(destinations, native_entry, NodeKind::File)?;
    let mount = native_entry.parent().ok_or(io::ErrorKind::InvalidInput)?;
    for node in reader.nodes() {
        if node.relative() != source_entry {
            reserve_destination(destinations, &mount.join(node.relative()), node.kind())?;
        }
    }
    Ok(())
}
/// Validate the complete projection before creating a workspace; only directories may be shared.
fn reserve_destination(
    destinations: &mut BTreeMap<PathBuf, NodeKind>,
    relative: &Path,
    kind: NodeKind,
) -> Result<(), LiveError> {
    Workspace::relative(relative)?;
    for ancestor in relative
        .ancestors()
        .skip(1)
        .filter(|path| !path.as_os_str().is_empty())
    {
        match destinations.entry(ancestor.to_owned()) {
            Entry::Vacant(entry) => {
                entry.insert(NodeKind::Directory);
            }
            Entry::Occupied(entry) if entry.get() == &NodeKind::Directory => {}
            Entry::Occupied(_) => return Err(LiveError::Conflict),
        }
    }
    match destinations.entry(relative.to_owned()) {
        Entry::Vacant(entry) => {
            entry.insert(kind);
            Ok(())
        }
        Entry::Occupied(entry)
            if kind == NodeKind::Directory && entry.get() == &NodeKind::Directory =>
        {
            Ok(())
        }
        Entry::Occupied(_) => Err(LiveError::Conflict),
    }
}

fn hash_rendered(hash: &mut Sha256, rendered: &RenderedContent) -> io::Result<()> {
    hash_part(hash, rendered.relative_entry.as_bytes())?;
    hash_part(hash, rendered.provenance.renderer_schema.as_bytes())?;
    hash_part(hash, rendered.provenance.effective_name.as_bytes())?;
    hash_part(hash, &rendered.provenance.input_fingerprint)?;
    hash_part(hash, &rendered.provenance.output_fingerprint)?;
    hash_part(hash, &rendered.bytes)
}

fn hash_part(hash: &mut Sha256, bytes: &[u8]) -> io::Result<()> {
    let length = u64::try_from(bytes.len()).map_err(|_| io::ErrorKind::FileTooLarge)?;
    hash.update(length.to_be_bytes());
    hash.update(bytes);
    Ok(())
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                char::from(DIGITS[usize::from(byte >> 4)]),
                char::from(DIGITS[usize::from(byte & 15)]),
            ]
        })
        .collect()
}
