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
    artifact_store::{ArtifactEffect, ArtifactStore, StoreOwner},
    local_files,
    native_path::NativePath,
};
use secrecy::zeroize::Zeroizing;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Write},
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
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    destination: NativePath,
    entries: Vec<Entry>,
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
/// Serializes edits to the same shared document and compares its bytes before replacement.
pub struct SharedDocumentWriter<'r, 'c> {
    persistence: Persistence,
    destination: PathBuf,
    owner: Owner,
    request: &'r RequestContext<'c>,
    limits: SharedWriteLimits,
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

/// A provider document changed, but its ownership metadata could not be saved.
#[derive(Debug)]
pub struct SharedDocumentPublicationError {
    source: io::Error,
}
impl SharedDocumentPublicationError {
    pub fn effect(&self) -> ArtifactEffect {
        ArtifactEffect::Applied
    }
}
impl std::fmt::Display for SharedDocumentPublicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("provider document applied; ownership metadata save failed")
    }
}
impl std::error::Error for SharedDocumentPublicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
fn refs(path: &[String]) -> Vec<&str> {
    path.iter().map(String::as_str).collect()
}
impl SharedDocumentStore {
    pub fn open(root: &Path) -> io::Result<Self> {
        if !root.is_absolute() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        fs::create_dir_all(root)?;
        Ok(Self {
            root: root.to_owned(),
        })
    }
    /// Find only present Kiro profiles with hook fields owned by this checkout.
    pub fn owned_kiro_profiles(
        &self,
        parent: &Path,
        core: &ArtifactStore,
        limits: SharedWriteLimits,
        request: &RequestContext<'_>,
    ) -> io::Result<Vec<String>> {
        let parent = match parent.canonicalize() {
            Ok(parent) => parent,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut profiles = std::collections::BTreeSet::new();
        for file in fs::read_dir(&self.root)? {
            request.check()?;
            let file = file?;
            let name = file.file_name();
            let Some(name) = name.to_str().and_then(|name| name.strip_suffix(".json")) else {
                continue;
            };
            if name.len() != 64 || !name.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                continue;
            }
            let path = file.path();
            let Some((bytes, _)) = local_files::read_regular(&path, limits.output_bytes)? else {
                continue;
            };
            graphit_fs::verify_private_file_path(&path)?;
            let state: State =
                serde_json::from_slice(&bytes).map_err(|_| invalid("invalid shared state"))?;
            let destination = state.destination.as_path();
            if destination.parent() != Some(parent.as_path()) || !destination.try_exists()? {
                continue;
            }
            if state.entries.iter().any(|entry| {
                matches!(entry, Entry::Hook(entry)
                if entry.format == HookDocumentFormat::KiroProfile
                    && entry.owner.scope == *core.owner()
                    && entry.owner.brand == core.scope().brand().key())
            }) {
                let name = destination
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| invalid("invalid Kiro profile name"))?;
                profiles.insert(name.to_owned());
            }
        }
        Ok(profiles.into_iter().collect())
    }
    pub fn writer<'r, 'c>(
        &self,
        parent: &'r Path,
        name: &str,
        core: &ArtifactStore,
        limits: SharedWriteLimits,
        request: &'r RequestContext<'c>,
    ) -> io::Result<SharedDocumentWriter<'r, 'c>> {
        request.remaining()?;
        super::super::projection::validate_name(name).map_err(io::Error::other)?;
        fs::create_dir_all(parent)?;
        let configured = parent.join(name);
        let destination = match fs::canonicalize(&configured) {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                parent.canonicalize()?.join(name)
            }
            Err(error) => return Err(error),
        };
        let identity = NativePath::new(destination.clone())?;
        let mut hash = Sha256::new();
        hash.update(serde_json::to_vec(&identity)?);
        let mut key = String::with_capacity(64);
        for byte in hash.finalize() {
            use std::fmt::Write as _;
            write!(&mut key, "{byte:02x}").expect("String write");
        }
        let lock_path = self.root.join(format!("{key}.lock"));
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        loop {
            request.remaining()?;
            match lock.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(10)),
                Err(TryLockError::Error(error)) => return Err(error),
            }
        }
        let path = self.root.join(format!("{key}.json"));
        let state = match local_files::read_regular(&path, limits.output_bytes)? {
            Some((bytes, _)) => {
                graphit_fs::verify_private_file(&File::open(&path)?)?;
                serde_json::from_slice::<State>(&bytes)
                    .map_err(|_| invalid("invalid shared state"))?
            }
            None => State {
                destination: identity.clone(),
                entries: Vec::new(),
            },
        };
        if state.destination != identity {
            return Err(invalid("shared document destination mismatch"));
        }
        Ok(SharedDocumentWriter {
            persistence: Persistence {
                path,
                state,
                max_bytes: limits.output_bytes,
            },
            destination,
            owner: Owner {
                brand: core.scope().brand().key().into(),
                scope: core.owner().clone(),
            },
            request,
            limits,
            _lock: lock,
        })
    }
}
impl Persistence {
    fn save(&self) -> io::Result<()> {
        self.save_bytes(&self.encode(&self.state.entries)?)
    }
    fn encode(&self, entries: &[Entry]) -> io::Result<Zeroizing<Vec<u8>>> {
        let state = State {
            destination: self.state.destination.clone(),
            entries: entries.to_owned(),
        };
        let bytes = Zeroizing::new(
            serde_json::to_vec(&state).map_err(|_| invalid("invalid shared state"))?,
        );
        if bytes.len() > self.max_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        Ok(bytes)
    }
    fn save_bytes(&self, bytes: &[u8]) -> io::Result<()> {
        let mut stage = tempfile::Builder::new()
            .prefix(".graphit-shared-")
            .make_in(
                self.path.parent().ok_or(io::ErrorKind::InvalidInput)?,
                graphit_fs::create_private_file,
            )?;
        stage.write_all(bytes)?;
        stage.as_file().sync_all()?;
        stage
            .into_temp_path()
            .persist(&self.path)
            .map_err(|error| error.error)?;
        Ok(())
    }
}
impl SharedDocumentWriter<'_, '_> {
    fn read(&self) -> io::Result<Option<Zeroizing<Vec<u8>>>> {
        self.request.remaining()?;
        Ok(
            local_files::read_regular(&self.destination, self.limits.input_bytes)?
                .map(|(bytes, _)| bytes),
        )
    }
    pub fn sync(&mut self, plan: &NativeMcpPlan) -> io::Result<SharedDocumentChange> {
        self.apply(plan, false)
    }
    pub fn remove(&mut self, plan: &NativeMcpPlan) -> io::Result<SharedDocumentChange> {
        self.apply(plan, true)
    }
    pub fn forget(&mut self, plan: &NativeMcpPlan) -> io::Result<SharedDocumentChange> {
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
        let observed = self.read()?;
        let input = match profile_limits {
            Some(_) => observed
                .as_ref()
                .map(|bytes| bytes.as_slice())
                .ok_or(io::ErrorKind::NotFound)?,
            None => observed
                .as_ref()
                .map(|bytes| bytes.as_slice())
                .unwrap_or(b"{}"),
        };
        let profile = if let Some(limits) = profile_limits {
            if input.len() > self.limits.input_bytes {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            let name = self
                .destination
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".md"))
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
        let observed = self.read()?;
        let empty = if plan.format == DocumentFormat::Toml {
            ""
        } else {
            "{}"
        };
        let input = observed
            .as_ref()
            .map(|bytes| bytes.as_slice())
            .unwrap_or(empty.as_bytes());
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
        observed: &Option<Zeroizing<Vec<u8>>>,
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
        observed: &Option<Zeroizing<Vec<u8>>>,
        input: &[u8],
        desired: &[u8],
        after: Vec<Entry>,
        change: SharedDocumentChange,
    ) -> io::Result<SharedDocumentChange> {
        self.request.remaining()?;
        let metadata = self.persistence.encode(&after)?;
        if &self.read()? != observed {
            return Err(conflict());
        }
        if desired == input {
            if after != self.persistence.state.entries {
                self.persistence.save_bytes(&metadata)?;
                self.persistence.state.entries = after;
            }
            return Ok(change);
        }
        let permissions = fs::metadata(&self.destination)
            .ok()
            .map(|metadata| metadata.permissions());
        local_files::atomic_replace(
            &self.destination,
            desired,
            permissions,
            self.request.cancellation(),
        )?;
        self.persistence.save_bytes(&metadata).map_err(|source| {
            io::Error::new(source.kind(), SharedDocumentPublicationError { source })
        })?;
        self.persistence.state.entries = after;
        Ok(change)
    }
}

#[cfg(test)]
mod tests;
