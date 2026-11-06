//! Shared provider documents: managed fields and binding references, never whole-file ownership.
use super::{
    ContentError, ContentLimits, DocumentFormat, FieldValue, NativeBinding, NativeDocument,
    RenderContext,
    document::{TupleEdit, TupleFields, TupleMatch},
    inspect_kiro_coordinator_profile,
    kiro_hooks::KiroHookPlan,
    mcp_config::{ManagedField, NativeMcpPlan},
};
use crate::policy::{RequestContext, SharedWriteLimits};
use graphit_core::{
    artifact_store::{
        ArtifactKey, ArtifactStore, ArtifactWriter, NativeDocumentCompletion,
        NativeDocumentOperationId, NativeDocumentOutcome, NativeDocumentRecovery,
        NativeDocumentRecoveryRequest, NativeDocumentResolution, ScopeBindingRecord, ScopeLimits,
        StoreOwner,
    },
    native_document_file::{
        CapturedDocumentDirectory, NativeDocumentDirectoryIdentity, NativeDocumentObservation,
        NativeDocumentReceipt,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
mod hooks;
use hooks::CommandHookEntry;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    brand: String,
    scope: StoreOwner,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct McpEntry {
    path: Vec<String>,
    owner: Owner,
    bindings: Vec<NativeBinding>,
    fields: Vec<ManagedField>,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum HookDocumentFormat {
    Native(DocumentFormat),
    KiroProfile,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HookEntry {
    identifier: String,
    owner: Owner,
    binding: NativeBinding,
    path: Vec<String>,
    format: HookDocumentFormat,
    fields: TupleFields,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "entry", deny_unknown_fields)]
enum Entry {
    Mcp(McpEntry),
    Hook(HookEntry),
    CommandHook(CommandHookEntry),
}
impl Entry {
    fn mcp(&self) -> Option<&McpEntry> {
        match self {
            Self::Mcp(entry) => Some(entry),
            Self::Hook(_) | Self::CommandHook(_) => None,
        }
    }
    fn mcp_mut(&mut self) -> Option<&mut McpEntry> {
        match self {
            Self::Mcp(entry) => Some(entry),
            Self::Hook(_) | Self::CommandHook(_) => None,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    operation: NativeDocumentOperationId,
    key: ArtifactKey,
    scope: Box<serde_json::value::RawValue>,
    previous: Option<NativeDocumentReceipt>,
    owner: Owner,
    after: Vec<Entry>,
    recorded: bool,
    released: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    parent: NativeDocumentDirectoryIdentity,
    name: String,
    entries: Vec<Entry>,
    pending: Option<Pending>,
}
/// Common private state root supplied by the host, independent of an installation brand.
pub struct SharedDocumentStore {
    root: PathBuf,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedDocumentChange {
    Created,
    Updated,
    Unchanged,
    Removed,
    Forgotten,
}
/// Holds the provider lock before the existing Core writer for the whole operation.
pub struct SharedDocumentWriter<'r, 'c> {
    core: ArtifactWriter,
    persistence: Persistence,
    parent: &'r CapturedDocumentDirectory,
    owner: Owner,
    request: &'r RequestContext<'c>,
    limits: SharedWriteLimits,
    scope: ScopeBindingRecord,
    scope_limits: ScopeLimits,
    _lock: File,
}
struct Persistence {
    path: PathBuf,
    state: State,
    max_bytes: usize,
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn conflict() -> io::Error {
    io::Error::other("managed native field changed")
}
fn pending() -> io::Error {
    io::Error::new(
        io::ErrorKind::WouldBlock,
        "shared document recovery required",
    )
}
fn codec(error: super::DocumentError) -> io::Error {
    let kind = match error {
        super::DocumentError::Interrupted => io::ErrorKind::Interrupted,
        super::DocumentError::TimedOut => io::ErrorKind::TimedOut,
        super::DocumentError::LimitExceeded => io::ErrorKind::FileTooLarge,
        _ => io::ErrorKind::InvalidData,
    };
    io::Error::new(kind, error)
}

fn content_codec(error: ContentError) -> io::Error {
    let kind = match error {
        ContentError::Cancelled => io::ErrorKind::Interrupted,
        ContentError::TimedOut => io::ErrorKind::TimedOut,
        ContentError::LimitExceeded => io::ErrorKind::FileTooLarge,
        ContentError::UnsupportedCapability => io::ErrorKind::PermissionDenied,
        _ => io::ErrorKind::InvalidData,
    };
    io::Error::new(kind, error)
}

fn artifact(error: graphit_core::artifact_store::ArtifactError) -> io::Error {
    io::Error::new(error.kind(), format!("native document {:?}", error.stage()))
}
fn refs(path: &[String]) -> Vec<&str> {
    path.iter().map(String::as_str).collect()
}
impl SharedDocumentStore {
    pub fn open(root: &Path) -> io::Result<Self> {
        if !root.is_absolute() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let directory = match fs::symlink_metadata(root) {
            Ok(meta) if meta.is_dir() && !meta.is_symlink() => open_private_directory(root)?,
            Ok(_) => return Err(invalid("shared state root is not a directory")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                graphit_fs::create_private_directory(root)?
            }
            Err(e) => return Err(e),
        };
        graphit_fs::verify_private_directory(&directory)?;
        Ok(Self {
            root: root.canonicalize()?,
        })
    }
    pub fn writer<'r, 'c>(
        &self,
        parent: &'r CapturedDocumentDirectory,
        name: &str,
        core: &ArtifactStore,
        limits: SharedWriteLimits,
        request: &'r RequestContext<'c>,
    ) -> io::Result<SharedDocumentWriter<'r, 'c>> {
        request.remaining()?;
        let mut components = Path::new(name).components();
        if name.contains('\0')
            || !matches!(components.next(), Some(std::path::Component::Normal(_)))
            || components.next().is_some()
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let identity = parent.directory_identity();
        let mut hash = Sha256::new();
        hash.update(identity.routing_key());
        hash.update(name.as_bytes());
        let mut key = String::with_capacity(64);
        for byte in hash.finalize() {
            use std::fmt::Write as _;
            write!(&mut key, "{byte:02x}").expect("writing to String cannot fail");
        }
        let lock_path = self.root.join(format!("{key}.lock"));
        let lock = match graphit_fs::create_private_lock_file(&lock_path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => private_open(&lock_path, true)?,
            Err(e) => return Err(e),
        };
        loop {
            request.remaining()?;
            match lock.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(10)),
                Err(TryLockError::Error(error)) => return Err(error),
            }
        }
        parent.revalidate(request.snapshot())?;
        let path = self.root.join(format!("{key}.json"));
        let state = match private_open(&path, false) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(limits.journal_bytes as u64 + 1)
                    .read_to_end(&mut bytes)?;
                if bytes.len() > limits.journal_bytes {
                    return Err(invalid("shared state is too large"));
                }
                serde_json::from_slice::<State>(&bytes)
                    .map_err(|_| invalid("invalid shared state"))?
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => State {
                parent: identity.clone(),
                name: name.into(),
                entries: Vec::new(),
                pending: None,
            },
            Err(e) => return Err(e),
        };
        if state.parent != identity || state.name != name {
            return Err(invalid("shared document identity mismatch"));
        }
        let owner = Owner {
            brand: core.scope().brand().key().into(),
            scope: core.owner().clone(),
        };
        let scope = core.scope().binding()?;
        let writer = core
            .writer_with_context(request.snapshot())
            .map_err(artifact)?;
        let mut result = SharedDocumentWriter {
            core: writer,
            persistence: Persistence {
                path,
                state,
                max_bytes: limits.journal_bytes,
            },
            parent,
            owner,
            request,
            limits,
            scope,
            scope_limits: core.scope().limits(),
            _lock: lock,
        };
        result.recover()?;
        Ok(result)
    }
}
fn open_private_directory(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        options.custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}
fn private_open(path: &Path, write: bool) -> io::Result<File> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.is_symlink() {
        return Err(invalid("shared state is not a regular file"));
    }
    let file = OpenOptions::new().read(true).write(write).open(path)?;
    graphit_fs::verify_private_file(&file)?;
    Ok(file)
}
impl Persistence {
    fn save(&self) -> io::Result<()> {
        let bytes = serde_json::to_vec(&self.state).map_err(|_| invalid("invalid shared state"))?;
        if bytes.len() > self.max_bytes {
            return Err(invalid("shared state is too large"));
        }
        let stage = self
            .path
            .with_extension(format!("{}.tmp", NativeDocumentOperationId::generate()));
        let mut file = graphit_fs::create_private_file(&stage)?;
        let write = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            // The private Windows handle excludes delete sharing needed by rename.
            drop(file);
            fs::rename(&stage, &self.path)?;
            Ok(())
        })();
        if write.is_err() {
            // This private stage was created by this call and was never handed to Core.
            let _ = fs::remove_file(&stage);
        }
        write
    }
    fn recorded(&mut self, outcome: NativeDocumentOutcome) -> io::Result<()> {
        let operation = self.state.pending.as_mut().ok_or_else(pending)?;
        if outcome == NativeDocumentOutcome::Applied {
            self.state.entries = operation.after.clone();
        }
        operation.recorded = true;
        self.save()
    }
    fn released(&mut self) -> io::Result<()> {
        self.state.pending.as_mut().ok_or_else(pending)?.released = true;
        self.save()
    }
    fn clear(&mut self) -> io::Result<()> {
        self.state.pending = None;
        self.save()
    }
}
fn settle(
    persistence: &mut Persistence,
    resolution: NativeDocumentResolution<'_>,
) -> io::Result<NativeDocumentOutcome> {
    let outcome = resolution.outcome();
    persistence.recorded(outcome)?;
    let completion = resolution.acknowledge().map_err(artifact)?;
    finish(persistence, completion)?;
    Ok(outcome)
}
fn finish(
    persistence: &mut Persistence,
    completion: NativeDocumentCompletion<'_>,
) -> io::Result<()> {
    persistence.recorded(completion.outcome())?;
    persistence.released()?;
    completion.release().map_err(artifact)?;
    persistence.clear()
}
impl SharedDocumentWriter<'_, '_> {
    pub fn recover(&mut self) -> io::Result<Option<NativeDocumentOutcome>> {
        let Some(operation) = self.persistence.state.pending.clone() else {
            if self
                .core
                .load()
                .map_err(artifact)?
                .as_ref()
                .and_then(|state| state.pending())
                .is_some()
            {
                return Err(pending());
            }
            return Ok(None);
        };
        if operation.owner != self.owner {
            return Err(pending());
        }
        let snapshot = self.core.load().map_err(artifact)?;
        if snapshot.as_ref().and_then(|v| v.pending()).is_none() {
            if operation.released {
                self.persistence.clear()?;
                return Ok(None);
            }
            let observed = self.parent.read(
                self.persistence.state.name.as_ref(),
                self.request.snapshot(),
            )?;
            if !operation.recorded && observed.receipt() == operation.previous.as_ref() {
                // Logical intent preceded Core preparation; discard only metadata.
                self.persistence.clear()?;
                return Ok(Some(NativeDocumentOutcome::NotPublished));
            }
            return Err(pending());
        }
        let destination = self
            .parent
            .canonical_path()
            .join(&self.persistence.state.name);
        let scope = ScopeBindingRecord::pure_restore(
            operation.scope.get().as_bytes(),
            self.scope_limits,
            self.limits.journal_bytes,
        )?;
        let recovery = self
            .core
            .recover_native_document(
                NativeDocumentRecoveryRequest {
                    operation_id: operation.operation,
                    key: &operation.key,
                    destination: &destination,
                    parent_identity: &self.persistence.state.parent,
                    previous: operation.previous.as_ref(),
                    scope: &scope,
                },
                self.parent,
            )
            .map_err(artifact)?;
        let outcome = match recovery {
            NativeDocumentRecovery::Resolution(resolution) => {
                settle(&mut self.persistence, resolution)?
            }
            NativeDocumentRecovery::Completion(completion) => {
                let outcome = completion.outcome();
                finish(&mut self.persistence, completion)?;
                outcome
            }
        };
        Ok(Some(outcome))
    }
    pub fn sync(&mut self, plan: &NativeMcpPlan) -> io::Result<SharedDocumentChange> {
        self.apply(plan, false)
    }
    pub fn remove(&mut self, plan: &NativeMcpPlan) -> io::Result<SharedDocumentChange> {
        self.apply(plan, true)
    }
    pub fn forget(&mut self, plan: &NativeMcpPlan) -> io::Result<SharedDocumentChange> {
        self.recover()?;
        self.request.remaining()?;
        let Some(index) = self
            .persistence
            .state
            .entries
            .iter()
            .position(|entry| entry.mcp().is_some_and(|entry| entry.path == plan.entry))
        else {
            return Ok(SharedDocumentChange::Unchanged);
        };
        let entry = self.persistence.state.entries[index]
            .mcp_mut()
            .expect("matched MCP entry");
        if entry.owner != self.owner {
            return Err(conflict());
        }
        if !entry.bindings.contains(&plan.binding) {
            return Ok(SharedDocumentChange::Unchanged);
        }
        entry.bindings.retain(|binding| *binding != plan.binding);
        if entry.bindings.is_empty() {
            self.persistence.state.entries.remove(index);
        }
        self.persistence.save()?;
        Ok(SharedDocumentChange::Forgotten)
    }
    pub fn sync_hook(
        &mut self,
        plan: &KiroHookPlan,
        format: DocumentFormat,
    ) -> io::Result<SharedDocumentChange> {
        self.apply_hook(plan, format, None, false)
    }
    pub fn remove_hook(
        &mut self,
        plan: &KiroHookPlan,
        format: DocumentFormat,
    ) -> io::Result<SharedDocumentChange> {
        self.apply_hook(plan, format, None, true)
    }
    /// Edit only an owned hook tuple in an existing current Coordinator profile.
    /// Literal body and foreign metadata values survive; YAML presentation may normalize.
    pub fn sync_kiro_profile_hook(
        &mut self,
        plan: &KiroHookPlan,
        content: ContentLimits,
    ) -> io::Result<SharedDocumentChange> {
        self.apply_hook(plan, DocumentFormat::Json, Some(content), false)
    }
    pub fn remove_kiro_profile_hook(
        &mut self,
        plan: &KiroHookPlan,
        content: ContentLimits,
    ) -> io::Result<SharedDocumentChange> {
        self.apply_hook(plan, DocumentFormat::Json, Some(content), true)
    }
    pub fn forget_hook(&mut self, plan: &KiroHookPlan) -> io::Result<SharedDocumentChange> {
        self.recover()?;
        self.request.remaining()?;
        let Some(index) = self.persistence.state.entries.iter().position(
            |entry| matches!(entry, Entry::Hook(entry) if entry.identifier == plan.identifier()),
        ) else {
            return Ok(SharedDocumentChange::Unchanged);
        };
        let Entry::Hook(entry) = &self.persistence.state.entries[index] else {
            unreachable!()
        };
        if entry.owner != self.owner {
            return Err(conflict());
        }
        if entry.binding != plan.binding()
            || refs(&entry.path) != plan.array_path()
            || !matches!(entry.fields.get("trigger"), Some(Some(FieldValue::String(trigger))) if trigger == plan.native_trigger())
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.persistence.state.entries.remove(index);
        self.persistence.save()?;
        Ok(SharedDocumentChange::Forgotten)
    }
    fn apply_hook(
        &mut self,
        plan: &KiroHookPlan,
        format: DocumentFormat,
        profile_limits: Option<ContentLimits>,
        remove: bool,
    ) -> io::Result<SharedDocumentChange> {
        self.recover()?;
        self.request.remaining()?;
        if format == DocumentFormat::Toml
            || self
                .persistence
                .state
                .entries
                .iter()
                .any(|entry| matches!(entry, Entry::Hook(entry) if entry.binding != plan.binding()))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let format_key = if profile_limits.is_some() {
            HookDocumentFormat::KiroProfile
        } else {
            HookDocumentFormat::Native(format)
        };
        let mut after = self.persistence.state.entries.clone();
        let index = after.iter().position(
            |entry| matches!(entry, Entry::Hook(entry) if entry.identifier == plan.identifier()),
        );
        if let Some(index) = index {
            let Entry::Hook(entry) = &after[index] else {
                unreachable!()
            };
            if entry.owner != self.owner {
                return Err(conflict());
            }
            if entry.binding != plan.binding()
                || entry.format != format_key
                || refs(&entry.path) != plan.array_path()
                || !matches!(entry.fields.get("trigger"), Some(Some(FieldValue::String(trigger))) if trigger == plan.native_trigger())
            {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        } else if remove {
            return Ok(SharedDocumentChange::Unchanged);
        }
        let observed = self.parent.read(
            self.persistence.state.name.as_ref(),
            self.request.snapshot(),
        )?;
        let input = match profile_limits {
            Some(_) => observed.bytes().ok_or(io::ErrorKind::NotFound)?,
            None => observed.bytes().unwrap_or(b"{}"),
        };
        let profile = if let Some(limits) = profile_limits {
            if input.len() > self.limits.input_bytes {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            let name = self
                .persistence
                .state
                .name
                .strip_suffix(".md")
                .ok_or(io::ErrorKind::InvalidInput)?;
            let mut context = RenderContext::new(
                ContentLimits {
                    max_output_bytes: limits.max_output_bytes.min(self.limits.input_bytes),
                    ..limits
                },
                self.request.cancellation(),
                self.request.snapshot().required_deadline()?,
            )
            .map_err(content_codec)?;
            let (metadata, body) = inspect_kiro_coordinator_profile(input, name, &mut context)
                .map_err(content_codec)?;
            let metadata = super::render::json(&metadata, &mut context).map_err(content_codec)?;
            Some((body, metadata))
        } else {
            None
        };
        let codec_input = profile
            .as_ref()
            .map_or(input, |(_, bytes)| bytes.as_slice());
        let mut document =
            NativeDocument::parse(format, codec_input, self.limits, self.request).map_err(codec)?;
        if profile.is_none() {
            let version = plan.version();
            match document.get(&["version"]).map_err(codec)? {
                Some(FieldValue::String(actual)) if actual == version => {}
                None if index.is_none() => document
                    .set(&["version"], FieldValue::String(version.into()))
                    .map_err(codec)?,
                _ => return Err(conflict()),
            }
        }
        let FieldValue::Object(tuple) = plan.tuple() else {
            return Err(invalid("invalid hook tuple"));
        };
        let desired: TupleFields = plan
            .critical_fields()
            .iter()
            .map(|key| ((*key).into(), tuple.get(*key).cloned()))
            .collect();
        let named_collision = (plan.binding() == NativeBinding::KiroV3)
            .then(|| TupleFields::from([("name".into(), desired.get("name").cloned().flatten())]));
        let collision = named_collision.as_ref().unwrap_or(&desired);
        let change;
        if let Some(index) = index {
            let Entry::Hook(entry) = &mut after[index] else {
                unreachable!()
            };
            let TupleMatch::Unique(tuple_index) = document
                .tuple_match(plan.array_path(), &entry.fields)
                .map_err(codec)?
            else {
                return Err(conflict());
            };
            if remove {
                document
                    .edit_tuple(plan.array_path(), TupleEdit::Remove(tuple_index))
                    .map_err(codec)?;
                after.remove(index);
                change = SharedDocumentChange::Removed;
            } else {
                match document
                    .tuple_match(plan.array_path(), collision)
                    .map_err(codec)?
                {
                    TupleMatch::Missing => {}
                    TupleMatch::Unique(matched) if matched == tuple_index => {}
                    _ => return Err(conflict()),
                }
                change = if entry.fields == desired {
                    SharedDocumentChange::Unchanged
                } else {
                    SharedDocumentChange::Updated
                };
                if entry.fields != desired {
                    document
                        .edit_tuple(plan.array_path(), TupleEdit::Update(tuple_index, &desired))
                        .map_err(codec)?;
                    entry.fields = desired;
                }
            }
        } else {
            if document
                .tuple_match(plan.array_path(), collision)
                .map_err(codec)?
                != TupleMatch::Missing
            {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "foreign hook tuple already exists",
                ));
            }
            document
                .edit_tuple(plan.array_path(), TupleEdit::Append(plan.tuple()))
                .map_err(codec)?;
            after.push(Entry::Hook(HookEntry {
                identifier: plan.identifier().into(),
                owner: self.owner.clone(),
                binding: plan.binding(),
                path: plan.array_path().iter().map(|key| (*key).into()).collect(),
                format: format_key,
                fields: desired,
            }));
            change = SharedDocumentChange::Created;
        }
        if let Some((body, _)) = profile {
            let header = document.render().map_err(codec)?;
            self.request.remaining()?;
            // The existing document producer already bounded and validated this JSON.
            let metadata: serde_json::Value = serde_json::from_str(&header)
                .map_err(|_| invalid("invalid edited Kiro metadata"))?;
            let limits = profile_limits.expect("profile retains its concrete limits");
            let mut context = RenderContext::new(
                ContentLimits {
                    max_output_bytes: limits.max_output_bytes.min(self.limits.output_bytes),
                    ..limits
                },
                self.request.cancellation(),
                self.request.snapshot().required_deadline()?,
            )
            .map_err(content_codec)?;
            context.tree(&metadata, 1).map_err(content_codec)?;
            let desired =
                super::render::markdown(&metadata, body, &mut context).map_err(content_codec)?;
            self.publish_bytes(&observed, input, &desired, after, change)
        } else {
            self.publish_document(&observed, input, &document, after, change)
        }
    }
    fn apply(&mut self, plan: &NativeMcpPlan, remove: bool) -> io::Result<SharedDocumentChange> {
        self.recover()?;
        self.request.remaining()?;
        if plan.entry.len()
            != if plan.binding == NativeBinding::OpenCode {
                3
            } else {
                2
            }
            || (plan.binding == NativeBinding::OpenCode && plan.entry[..2] != ["mcp", "servers"])
            || plan.entry.iter().any(String::is_empty)
            || plan
                .fields
                .iter()
                .any(|f| !f.path.starts_with(&plan.entry) || f.path.len() <= plan.entry.len())
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let observed = self.parent.read(
            self.persistence.state.name.as_ref(),
            self.request.snapshot(),
        )?;
        let empty = if plan.format == DocumentFormat::Toml {
            ""
        } else {
            "{}"
        };
        let input = observed.bytes().unwrap_or(empty.as_bytes());
        let mut document =
            NativeDocument::parse(plan.format, input, self.limits, self.request).map_err(codec)?;
        let mut after = self.persistence.state.entries.clone();
        let index = after
            .iter()
            .position(|entry| entry.mcp().is_some_and(|entry| entry.path == plan.entry));
        if let Some(index) = index {
            let entry = after[index].mcp().expect("matched MCP entry");
            if entry.owner != self.owner {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "MCP entry belongs to another owner",
                ));
            }
            if !entry.bindings.contains(&plan.binding) {
                if remove {
                    return Ok(SharedDocumentChange::Unchanged);
                }
                if entry.fields != plan.fields {
                    return Err(conflict());
                }
            }
            {
                for field in &entry.fields {
                    if document.get(&refs(&field.path)).map_err(codec)?
                        != Some(
                            field
                                .native_value()
                                .map_err(|_| invalid("invalid managed value"))?,
                        )
                    {
                        return Err(conflict());
                    }
                }
            }
        } else {
            if remove {
                return Ok(SharedDocumentChange::Unchanged);
            }
            match document.get(&refs(&plan.entry)) {
                Ok(None) => {}
                Err(error) if error != super::DocumentError::WrongType => return Err(codec(error)),
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "foreign MCP entry already exists",
                    ));
                }
            }
        }
        let change;
        if remove {
            let index = index.expect("remove has an existing entry");
            let entry = after[index].mcp_mut().expect("matched MCP entry");
            entry.bindings.retain(|v| *v != plan.binding);
            if entry.bindings.is_empty() {
                {
                    for field in &entry.fields {
                        document.remove(&refs(&field.path)).map_err(codec)?;
                    }
                    if matches!(document.get(&refs(&plan.entry)), Ok(Some(FieldValue::Object(ref v))) if v.is_empty())
                    {
                        document.remove(&refs(&plan.entry)).map_err(codec)?;
                    }
                }
                after.remove(index);
            }
            change = SharedDocumentChange::Removed;
        } else if let Some(index) = index {
            let entry = after[index].mcp_mut().expect("matched MCP entry");
            if entry.bindings.iter().any(|v| *v != plan.binding) && entry.fields != plan.fields {
                return Err(conflict());
            }
            for field in &plan.fields {
                if !entry.fields.iter().any(|old| old.path == field.path)
                    && document.get(&refs(&field.path)).map_err(codec)?.is_some()
                {
                    return Err(conflict());
                }
            }
            for old in &entry.fields {
                if !plan.fields.iter().any(|field| field.path == old.path) {
                    document.remove(&refs(&old.path)).map_err(codec)?;
                }
            }
            if !entry.bindings.contains(&plan.binding) {
                entry.bindings.push(plan.binding);
            }
            change = if entry.fields == plan.fields {
                SharedDocumentChange::Unchanged
            } else {
                SharedDocumentChange::Updated
            };
            entry.fields = plan.fields.clone();
            for field in &plan.fields {
                document
                    .set(
                        &refs(&field.path),
                        field
                            .native_value()
                            .map_err(|_| invalid("invalid managed value"))?,
                    )
                    .map_err(codec)?;
            }
        } else {
            for field in &plan.fields {
                document
                    .set(
                        &refs(&field.path),
                        field
                            .native_value()
                            .map_err(|_| invalid("invalid managed value"))?,
                    )
                    .map_err(codec)?;
            }
            after.push(Entry::Mcp(McpEntry {
                path: plan.entry.clone(),
                owner: self.owner.clone(),
                bindings: vec![plan.binding],
                fields: plan.fields.clone(),
            }));
            change = SharedDocumentChange::Created;
        }
        self.publish_document(&observed, input, &document, after, change)
    }
    fn publish_document(
        &mut self,
        observed: &NativeDocumentObservation,
        input: &[u8],
        document: &NativeDocument<'_, '_>,
        after: Vec<Entry>,
        change: SharedDocumentChange,
    ) -> io::Result<SharedDocumentChange> {
        let desired = document.render().map_err(codec)?;
        self.publish_bytes(observed, input, desired.as_bytes(), after, change)
    }
    fn publish_bytes(
        &mut self,
        observed: &NativeDocumentObservation,
        input: &[u8],
        desired: &[u8],
        after: Vec<Entry>,
        change: SharedDocumentChange,
    ) -> io::Result<SharedDocumentChange> {
        self.request.remaining()?;
        if desired == input {
            if after == self.persistence.state.entries {
                return Ok(change);
            }
            self.persistence.state.entries = after;
            self.persistence.save()?;
            return Ok(change);
        }
        let operation = NativeDocumentOperationId::generate();
        let key = ArtifactKey::new(
            "native-shared",
            "document",
            self.persistence
                .path
                .file_stem()
                .and_then(|v| v.to_str())
                .ok_or_else(|| invalid("invalid shared key"))?,
        )?;
        self.persistence.state.pending = Some(Pending {
            operation,
            key: key.clone(),
            scope: serde_json::value::to_raw_value(&self.scope)
                .map_err(|_| invalid("invalid scope binding"))?,
            previous: observed.receipt().cloned(),
            owner: self.owner.clone(),
            after,
            recorded: false,
            released: false,
        });
        self.persistence.save()?;
        let stage =
            match self
                .core
                .prepare_native_document(operation, &key, self.parent, observed, desired)
            {
                Ok(stage) => stage,
                Err(failure) => {
                    let (error, resolution) = failure.into_parts();
                    if let Some(resolution) = resolution {
                        settle(&mut self.persistence, resolution)?;
                    }
                    return Err(artifact(error));
                }
            };
        match stage.publish() {
            Ok(resolution) => {
                settle(&mut self.persistence, resolution)?;
                Ok(change)
            }
            Err(failure) => {
                let (error, resolution) = failure.into_parts();
                if let Some(resolution) = resolution {
                    settle(&mut self.persistence, resolution)?;
                }
                Err(artifact(error))
            }
        }
    }
}

#[cfg(test)]
mod tests;
