//! Finite PROJECT selection. Discovery never opens unselected document payload.
use super::*;
use crate::ignore_rules::{CapturedIgnoreRules, IgnoreScope};
use graphit_fs::read_directory::{DirectoryEntry, DirectoryEntryKind};
use std::{collections::HashMap, ffi::OsStr};

/// Borrowed criteria are data; only the native scope/request can capture them.
pub struct ProjectSelectionCriteria<'a> {
    pub subtree: Option<&'a Path>,
    pub extras: &'a [&'a Path],
    pub include_readme: bool,
    /// Effective profile selectors: leading-dot suffixes or exact basenames.
    pub extensions: &'a [&'a str],
    pub ignore: ProjectIgnoreCriteria<'a>,
}
pub struct ProjectIgnoreCriteria<'a> {
    pub protected_defaults: &'a [&'a str],
    pub custom_control: Option<&'a str>,
    pub git_control: Option<&'a str>,
}
/// Concrete input, individual payload/path and recursion limits.
/// Corpus size and scan work are not request quotas.
#[derive(Clone, Copy)]
pub struct ProjectSelectionLimits {
    pub max_depth: usize,
    pub max_input_items: usize,
    pub max_input_bytes: usize,
    pub max_observed_path_bytes: usize,
    pub max_selected_file_bytes: u64,
    pub max_control_file_bytes: u64,
}
impl ProjectSelectionLimits {
    fn validate(self) -> io::Result<()> {
        if [
            self.max_depth,
            self.max_input_items,
            self.max_input_bytes,
            self.max_observed_path_bytes,
        ]
        .contains(&0)
            || self.max_depth > 256
            || self.max_selected_file_bytes == 0
            || self.max_control_file_bytes == 0
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(())
    }
}
// Inventory membership matters only when it can affect this captured selection.
// Ignore controls remain dependencies even when their own names are not selected.
fn relevant_entry(
    criteria: &Criteria,
    scope: &IgnoreScope,
    parent: &Path,
    entry: &DirectoryEntry,
) -> io::Result<bool> {
    if [criteria.git.as_deref(), criteria.custom.as_deref()]
        .into_iter()
        .flatten()
        .any(|name| entry.name == OsStr::new(name))
    {
        return Ok(true);
    }
    let path = parent.join(&entry.name);
    crate::native_path::valid_native(&path)?;
    let directory = entry.kind == DirectoryEntryKind::Directory;
    // README selection chooses its first candidate before applying ignore/change
    // filters. Even an ignored candidate can suppress the next visible README.
    if criteria.readme
        && parent.as_os_str().is_empty()
        && !directory
        && path
            .file_stem()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.eq_ignore_ascii_case("readme"))
        && criteria.allowed_extension(&path)
    {
        return Ok(true);
    }
    let intersects = |root: &Path| path.starts_with(root) || (directory && root.starts_with(&path));
    if criteria
        .changes
        .as_ref()
        .is_some_and(|changes| !changes.iter().any(|root| intersects(root)))
    {
        return Ok(false);
    }
    let selected = criteria
        .subtree
        .as_ref()
        .is_some_and(|root| intersects(root))
        || criteria
            .extras
            .iter()
            .any(|extra| path == *extra || (directory && extra.starts_with(&path)));
    if !selected {
        return Ok(false);
    }
    if directory {
        return scope.should_descend(&path);
    }
    Ok(criteria.allowed_extension(&path) && !scope.is_ignored(path, false)?)
}

fn separator_bytes(parent: &Path) -> usize {
    if parent.as_os_str().is_empty() {
        0
    } else if cfg!(windows) {
        2
    } else {
        1
    }
}
#[derive(Clone)]
struct Criteria {
    subtree: Option<PathBuf>,
    extras: Vec<PathBuf>,
    readme: bool,
    extensions: Vec<String>,
    defaults: Vec<String>,
    custom: Option<String>,
    git: Option<String>,
    changes: Option<Vec<PathBuf>>,
}
impl Criteria {
    fn allowed_extension(&self, path: &Path) -> bool {
        let basename = path.file_name().and_then(OsStr::to_str);
        let extension = path.extension().and_then(OsStr::to_str);
        self.extensions.iter().any(|selector| {
            if let Some(suffix) = selector.strip_prefix('.') {
                extension.is_some_and(|actual| actual.eq_ignore_ascii_case(suffix))
            } else {
                basename.is_some_and(|actual| actual.eq_ignore_ascii_case(selector))
            }
        })
    }
    fn borrowed(&self) -> io::Result<(Vec<&Path>, Vec<&str>, Vec<&str>)> {
        let mut extras = Vec::new();
        extras
            .try_reserve_exact(self.extras.len())
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        let mut extensions = Vec::new();
        extensions
            .try_reserve_exact(self.extensions.len())
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        let mut defaults = Vec::new();
        defaults
            .try_reserve_exact(self.defaults.len())
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        extras.extend(self.extras.iter().map(PathBuf::as_path));
        extensions.extend(self.extensions.iter().map(String::as_str));
        defaults.extend(self.defaults.iter().map(String::as_str));
        Ok((extras, extensions, defaults))
    }
}
pub(super) struct SelectedBacking {
    criteria: Criteria,
    limits: ProjectSelectionLimits,
    witness: Witness,
}
struct Inventory {
    relative: Arc<Path>,
    observation: tree::Observation,
    entries: Vec<DirectoryEntry>,
    scope: IgnoreScope,
    scanned: bool,
    control_indices: std::ops::Range<usize>,
}
struct Control {
    relative: PathBuf,
    observation: tree::Observation,
    digest: [u8; 32],
}
struct InventoryReceipt {
    scope: IgnoreScope,
    scanned: bool,
    relative: Arc<Path>,
    observation: tree::Observation,
    entries: Vec<DirectoryEntry>,
    control_indices: std::ops::Range<usize>,
}
struct Witness {
    limits: ProjectSelectionLimits,
    controls: Vec<Control>,
    selected: Vec<PendingFile>,
    inventories: Vec<InventoryReceipt>,
    inventory_indices: Arc<HashMap<Arc<Path>, usize>>,
}
impl Witness {
    fn same(&self, other: &Self) -> bool {
        self.inventories.len() == other.inventories.len()
            && self.controls.len() == other.controls.len()
            && self.selected.len() == other.selected.len()
            && self
                .selected
                .iter()
                .zip(&other.selected)
                .all(|(a, b)| a.relative == b.relative && a.observation == b.observation)
            && self
                .inventories
                .iter()
                .zip(&other.inventories)
                .all(|(a, b)| {
                    a.relative == b.relative
                        && a.observation.matches_directory_binding(&b.observation)
                        && a.scanned == b.scanned
                        && a.control_indices == b.control_indices
                        && a.entries.len() == b.entries.len()
                        && a.entries
                            .iter()
                            .zip(&b.entries)
                            .all(|(a, b)| a.name == b.name && a.kind == b.kind)
                })
            && self.controls.iter().zip(&other.controls).all(|(a, b)| {
                a.relative == b.relative && a.observation == b.observation && a.digest == b.digest
            })
    }
}
struct PendingFile {
    relative: PathBuf,
    observation: tree::Observation,
}
// Only the file currently in use retains its no-delete ancestor chain.
struct OpenedSelection {
    handle: Arc<NativeHandle>,
    _ancestors: AncestorHandles,
}
struct Engine {
    // Corpus observations contain no handles; the Source birth-root admission is last.
    controls: Vec<Control>,
    inventories: Vec<Inventory>,
    inventory_indices: Arc<HashMap<Arc<Path>, usize>>,
    criteria: Criteria,
    limits: ProjectSelectionLimits,
    selected: BTreeMap<PathBuf, tree::Observation>,
    source: NativeSource,
}
fn add(used: usize, amount: usize, limit: usize) -> io::Result<usize> {
    used.checked_add(amount)
        .filter(|n| *n <= limit)
        .ok_or_else(|| io::ErrorKind::FileTooLarge.into())
}
fn malformed() -> io::Error {
    io::ErrorKind::InvalidInput.into()
}
fn changed(boundary: &'static str) -> io::Error {
    tracing::debug!(boundary, "Native PROJECT selection changed");
    io::ErrorKind::InvalidData.into()
}
fn validate_criteria(
    input: &ProjectSelectionCriteria<'_>,
    limits: ProjectSelectionLimits,
    request: &std::sync::Arc<crate::file_io::SnapshotContext>,
) -> io::Result<usize> {
    request.check()?;
    limits.validate()?;
    let mut count = 0usize;
    let mut bytes = 0usize;
    let mut admit = |length: usize| -> io::Result<()> {
        request.check()?;
        count = add(count, 1, limits.max_input_items)?;
        bytes = add(bytes, length, limits.max_input_bytes)?;
        Ok(())
    };
    if let Some(path) = input.subtree {
        admit(native_name_bytes(path.as_os_str())?)?;
    }
    for path in input.extras {
        admit(native_name_bytes(path.as_os_str())?)?;
    }
    for text in input
        .extensions
        .iter()
        .chain(input.ignore.protected_defaults)
        .copied()
        .chain(input.ignore.custom_control)
        .chain(input.ignore.git_control)
    {
        admit(text.len())?;
    }
    if let Some(path) = input.subtree {
        crate::native_path::valid_native(path)?;
        if path.as_os_str() != OsStr::new(".") {
            crate::file_io::validate_relative(path, false)?;
        }
    }
    for path in input.extras {
        crate::native_path::valid_native(path)?;
        crate::file_io::validate_relative(path, false)?;
    }
    for suffix in input.extensions {
        request.check()?;
        let extension = suffix.strip_prefix('.').unwrap_or(suffix);
        if extension.is_empty() || matches!(*suffix, "." | "..") {
            return Err(malformed());
        }
        for ch in extension.chars() {
            request.check()?;
            if matches!(ch, '/' | '\\' | ':')
                || (suffix.starts_with('.') && ch == '.')
                || ch.is_control()
                || ch.is_whitespace()
            {
                return Err(malformed());
            }
        }
    }
    for name in [input.ignore.custom_control, input.ignore.git_control]
        .into_iter()
        .flatten()
    {
        crate::file_io::validate_relative(Path::new(name), false)?;
        if Path::new(name).components().count() != 1 {
            return Err(malformed());
        }
    }
    request.check()?;
    Ok(bytes)
}
fn open_observed_directory(
    source: &NativeSource,
    relative: &Path,
    limits: ProjectSelectionLimits,
    matches: &impl Fn(&Path, &tree::Observation) -> bool,
) -> io::Result<OpenedSelection> {
    let request = source.request();
    request.check()?;
    if relative.components().count() > limits.max_depth
        || native_name_bytes(relative.as_os_str())? > limits.max_observed_path_bytes
    {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let mut opened = OpenedSelection {
        handle: Arc::clone(&source.admission.anchor.root),
        _ancestors: AncestorHandles(Vec::new()),
    };
    let mut prefix = PathBuf::new();
    if !matches(&prefix, &opened.handle.observe(request)?) {
        return Err(changed("root_binding"));
    }
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(changed("directory_component"));
        };
        prefix.push(name);
        let next = {
            let file = opened.handle.lock()?;
            NativeHandle::open_child(&file, name, Some(NodeKind::Directory), request)?
        };
        if !matches(&prefix, &next.observe(request)?) {
            return Err(changed("directory_binding"));
        }
        let parent = std::mem::replace(&mut opened.handle, next);
        opened._ancestors.0.push(parent);
    }
    request.check()?;
    Ok(opened)
}
fn open_observed_file<'a>(
    source: &NativeSource,
    relative: &Path,
    observation: &tree::Observation,
    limits: ProjectSelectionLimits,
    expected: &impl Fn(&Path) -> Option<&'a tree::Observation>,
) -> io::Result<OpenedSelection> {
    let parent = relative.parent().ok_or(io::ErrorKind::InvalidData)?;
    let mut opened = open_observed_directory(source, parent, limits, &|path, current| {
        expected(path).is_some_and(|expected| expected.matches_directory_binding(current))
    })?;
    let next = {
        let file = opened.handle.lock()?;
        NativeHandle::open_child(
            &file,
            relative.file_name().ok_or(io::ErrorKind::InvalidData)?,
            Some(NodeKind::File),
            source.request(),
        )?
    };
    if &next.observe(source.request())? != observation {
        return Err(changed("file_binding"));
    }
    let parent = std::mem::replace(&mut opened.handle, next);
    opened._ancestors.0.push(parent);
    source.request().check()?;
    Ok(opened)
}
impl Engine {
    fn directory_observation(&self, path: &Path) -> Option<&tree::Observation> {
        self.inventory_indices
            .get(path)
            .map(|&index| &self.inventories[index].observation)
    }
    fn open_directory(&self, index: usize) -> io::Result<OpenedSelection> {
        open_observed_directory(
            &self.source,
            &self.inventories[index].relative,
            self.limits,
            &|path, current| {
                self.directory_observation(path)
                    .is_some_and(|expected| expected.matches_directory_binding(current))
            },
        )
    }

    fn path(&self, parent: &Path, name: &OsStr) -> io::Result<PathBuf> {
        let bytes = native_name_bytes(parent.as_os_str())?
            .checked_add(native_name_bytes(name)?)
            .and_then(|n| n.checked_add(separator_bytes(parent)))
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if bytes > self.limits.max_observed_path_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }

        let path = parent.join(name);
        crate::file_io::validate_relative(&path, false)?;
        Ok(path)
    }
    fn named_path(&self, index: usize, name: &OsStr) -> io::Result<PathBuf> {
        let parent = &self.inventories[index].relative;
        let size = native_name_bytes(parent.as_os_str())?
            .checked_add(native_name_bytes(name)?)
            .and_then(|n| n.checked_add(separator_bytes(parent)))
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if size > self.limits.max_observed_path_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }

        let path = self.inventories[index].relative.join(name);
        crate::file_io::validate_relative(&path, false)?;
        Ok(path)
    }
    fn entry_path(&self, index: usize, entry: usize) -> io::Result<PathBuf> {
        let parent = &self.inventories[index].relative;
        let name = &self.inventories[index].entries[entry].name;
        let size = native_name_bytes(parent.as_os_str())?
            .checked_add(native_name_bytes(name)?)
            .and_then(|n| n.checked_add(separator_bytes(parent)))
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if size > self.limits.max_observed_path_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }

        let path = self.inventories[index]
            .relative
            .join(&self.inventories[index].entries[entry].name);
        crate::file_io::validate_relative(&path, false)?;
        Ok(path)
    }
    fn observation(&self, path: &Path) -> io::Result<()> {
        crate::native_path::valid_native(path)?;
        let bytes = native_name_bytes(path.as_os_str())?;
        if bytes > self.limits.max_observed_path_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }

        Ok(())
    }
    fn inventory(
        &mut self,
        relative: &Path,
        handle: Arc<NativeHandle>,
    ) -> io::Result<Vec<DirectoryEntry>> {
        if relative.components().count() > self.limits.max_depth {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let request = self.source.request().clone();
        let original = handle.observe(&request)?;
        let file = handle.lock()?;

        let mut iterator =
            read_directory::DirectoryEntries::new(&file, tree::BUFFER_BYTES, &mut || {
                request.check()
            })?;
        drop(file);
        let mut result = Vec::new();
        while let Some(entry) = iterator.next_entry(tree::BUFFER_BYTES, &mut || request.check())? {
            let path = native_name_bytes(relative.as_os_str())?
                .checked_add(native_name_bytes(&entry.name)?)
                .and_then(|n| n.checked_add(separator_bytes(relative)))
                .ok_or(io::ErrorKind::FileTooLarge)?;
            if path > self.limits.max_observed_path_bytes {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            result
                .try_reserve(1)
                .map_err(|_| io::ErrorKind::OutOfMemory)?;
            result.push(entry);
        }
        result.sort_by(|a, b| a.name.cmp(&b.name));
        if !original.matches_directory_binding(&handle.observe(&request)?) {
            return Err(changed("inventory_capture"));
        }
        drop(iterator);
        request.check()?;
        Ok(result)
    }
    fn control(
        &mut self,
        directory: &Path,
        handle: &Arc<NativeHandle>,
        entries: &[DirectoryEntry],
        name: Option<&str>,
    ) -> io::Result<Option<Vec<u8>>> {
        let Some(name) = name else {
            return Ok(None);
        };
        let path = self.path(directory, OsStr::new(name))?;
        let Some(entry) = entries.iter().find(|entry| entry.name == OsStr::new(name)) else {
            return Ok(None);
        };
        if entry.kind != DirectoryEntryKind::File {
            return Err(changed("control_kind"));
        }

        let request = self.source.request().clone();
        let control = {
            let parent = handle.lock()?;
            NativeHandle::open_child(&parent, OsStr::new(name), Some(NodeKind::File), &request)?
        };
        let observation = control.observe(&request)?;
        if observation.bytes() > self.limits.max_control_file_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }

        let length =
            usize::try_from(observation.bytes()).map_err(|_| io::ErrorKind::FileTooLarge)?;

        request.check()?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        bytes.resize(length, 0);
        let mut file = control.lock()?;
        let mut offset = 0;
        while offset < length {
            request.check()?;
            let end = offset.saturating_add(tree::BUFFER_BYTES).min(length);
            let read = file.read(&mut bytes[offset..end])?;
            if read == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            offset += read;
        }
        drop(file);
        if control.observe(&request)? != observation {
            return Err(changed("control_read"));
        }
        let digest = Sha256::digest(&bytes).into();
        self.controls
            .try_reserve_exact(1)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        self.controls.push(Control {
            relative: path,
            observation,
            digest,
        });
        request.check()?;
        Ok(Some(bytes))
    }
    fn directory(
        &mut self,
        relative: PathBuf,
        handle: Arc<NativeHandle>,
        parent: Option<usize>,
    ) -> io::Result<usize> {
        if let Some(&index) = self.inventory_indices.get(relative.as_path()) {
            return Ok(index);
        }
        if parent.is_none() {
            self.observation(&relative)?;
        }
        let scanned = !(parent.is_none()
            && self.criteria.subtree.is_none()
            && self.criteria.extras.is_empty()
            && !self.criteria.readme);
        let entries = if scanned {
            self.inventory(&relative, Arc::clone(&handle))?
        } else {
            Vec::new()
        };

        let custom_name = self.criteria.custom.clone();
        let git_name = self.criteria.git.clone();
        let control_start = self.controls.len();
        let git = if scanned {
            self.control(&relative, &handle, &entries, git_name.as_deref())?
        } else {
            None
        };
        let custom = if !scanned || custom_name == git_name {
            None
        } else {
            self.control(&relative, &handle, &entries, custom_name.as_deref())?
        };
        let rules = CapturedIgnoreRules {
            git_name: git_name.as_deref(),
            git: git.as_deref(),
            custom_name: custom_name.as_deref(),
            custom: custom.as_deref(),
        };
        // Logical matching paths/state are admitted; SDK matcher allocation itself

        let request = self.source.request().clone();
        let scope = match parent {
            Some(index) => self.inventories[index].scope.captured_child(
                relative.file_name().ok_or(io::ErrorKind::InvalidData)?,
                rules,
                &mut || request.check(),
            )?,
            None => IgnoreScope::captured_root(
                self.source.admission.scope.owner_root(),
                &self.criteria.defaults,
                custom_name.as_deref(),
                rules,
                &mut || request.check(),
            )?,
        };
        let observation = handle.observe(&request)?;
        self.inventories
            .try_reserve_exact(1)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        let index = self.inventories.len();
        let relative: Arc<Path> = relative.into();
        let indices = Arc::make_mut(&mut self.inventory_indices);
        indices
            .try_reserve(1)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        indices.insert(Arc::clone(&relative), index);
        self.inventories.push(Inventory {
            relative,
            observation,
            entries,
            scope,
            scanned,
            control_indices: control_start..self.controls.len(),
        });
        request.check()?;
        Ok(index)
    }
    fn child_directory(&mut self, parent: usize, path: PathBuf) -> io::Result<usize> {
        if let Some(&index) = self.inventory_indices.get(path.as_path()) {
            return Ok(index);
        }
        if path.components().count() > self.limits.max_depth {
            return Err(io::ErrorKind::FileTooLarge.into());
        }

        self.observation(&path)?;
        let name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
        let request = self.source.request().clone();
        let parent_handle = self.open_directory(parent)?;
        let handle = {
            let file = parent_handle.handle.lock()?;
            NativeHandle::open_child(&file, name, Some(NodeKind::Directory), &request)?
        };
        self.directory(path, handle, Some(parent))
    }
    fn select(&mut self, parent: usize, path: PathBuf, kind: DirectoryEntryKind) -> io::Result<()> {
        let request = self.source.request().clone();
        request.check()?;
        if !self.criteria.allowed_extension(&path)
            || self.inventories[parent].scope.is_ignored(&path, false)?
        {
            return Ok(());
        }
        if kind != DirectoryEntryKind::File {
            return Err(changed("selected_file_kind"));
        }
        if self.selected.contains_key(&path) {
            return Ok(());
        }
        self.observation(&path)?;
        let parent_handle = self.open_directory(parent)?;
        let handle = {
            let file = parent_handle.handle.lock()?;
            NativeHandle::open_child(
                &file,
                path.file_name().ok_or(io::ErrorKind::InvalidInput)?,
                Some(NodeKind::File),
                &request,
            )?
        };
        let observation = handle.observe(&request)?;
        if observation.bytes() > self.limits.max_selected_file_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }

        self.selected.insert(path, observation);
        request.check()
    }
    fn resolve(
        &mut self,
        path: &Path,
        directory: bool,
    ) -> io::Result<Option<(usize, PathBuf, DirectoryEntryKind)>> {
        let count = path.components().count();
        let mut parent = 0;
        for (index, component) in path.components().enumerate() {
            let Component::Normal(name) = component else {
                return Err(malformed());
            };
            let kind = self.inventories[parent]
                .entries
                .iter()
                .find(|entry| entry.name == name)
                .map(|entry| entry.kind);
            let current = self.named_path(parent, name)?;
            let Some(kind) = kind else {
                return Ok(None);
            };
            if index + 1 == count && !directory {
                return Ok(Some((parent, current, kind)));
            }
            if kind != DirectoryEntryKind::Directory {
                return Err(changed("ancestor_kind"));
            }
            if !self.inventories[parent].scope.should_descend(&current)? {
                return Ok(None);
            }
            parent = self.child_directory(parent, current)?;
        }
        Ok(Some((
            parent,
            PathBuf::new(),
            DirectoryEntryKind::Directory,
        )))
    }
    fn walk(&mut self, index: usize) -> io::Result<()> {
        let count = self.inventories[index].entries.len();
        for n in 0..count {
            self.source.request().check()?;
            let kind = self.inventories[index].entries[n].kind;
            let path = self.entry_path(index, n)?;
            if kind == DirectoryEntryKind::Directory {
                if self.inventories[index].scope.should_descend(&path)? {
                    let child = self.child_directory(index, path)?;
                    self.walk(child)?;
                }
            } else {
                self.select(index, path, kind)?;
            }
        }
        self.source.request().check()
    }
    fn witness(&mut self) -> io::Result<Witness> {
        let mut selected = Vec::new();
        selected
            .try_reserve_exact(self.selected.len())
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        for (relative, observation) in &self.selected {
            selected.push(PendingFile {
                relative: relative.clone(),
                observation: observation.clone(),
            });
        }
        let mut inventories = Vec::new();
        for directory in &mut self.inventories {
            inventories
                .try_reserve_exact(1)
                .map_err(|_| io::ErrorKind::OutOfMemory)?;
            let mut entries = Vec::new();
            for entry in std::mem::take(&mut directory.entries) {
                self.source.request().check()?;
                if relevant_entry(
                    &self.criteria,
                    &directory.scope,
                    &directory.relative,
                    &entry,
                )? {
                    entries.push(entry);
                }
            }
            inventories.push(InventoryReceipt {
                scope: directory.scope.clone(),
                scanned: directory.scanned,
                relative: directory.relative.clone(),
                observation: directory.observation.clone(),
                entries,
                control_indices: directory.control_indices.clone(),
            });
        }
        Ok(Witness {
            limits: self.limits,
            selected,
            inventories,
            inventory_indices: Arc::clone(&self.inventory_indices),
            controls: std::mem::take(&mut self.controls),
        })
    }
    fn open_pending(
        &self,
        relative: &Path,
        observation: &tree::Observation,
    ) -> io::Result<OpenedSelection> {
        open_observed_file(&self.source, relative, observation, self.limits, &|path| {
            self.directory_observation(path)
        })
    }
    fn fence(&self) -> io::Result<()> {
        self.source.admission.revalidate()?;
        for index in 0..self.inventories.len() {
            self.open_directory(index)?;
        }
        for control in &self.controls {
            open_observed_file(
                &self.source,
                &control.relative,
                &control.observation,
                self.limits,
                &|path| self.directory_observation(path),
            )?;
        }
        for (relative, observation) in &self.selected {
            self.open_pending(relative, observation)?;
        }
        self.source.request().check()
    }
}

pub(super) fn capture(
    scope: &CapturedPublicationScope,
    input: ProjectSelectionCriteria<'_>,
    limits: ProjectSelectionLimits,
    request: &std::sync::Arc<crate::file_io::SnapshotContext>,
) -> io::Result<super::super::SourceSnapshot> {
    capture_inner(scope, input, limits, request, None, None, true)
}
pub(super) fn capture_metadata(
    scope: &CapturedPublicationScope,
    input: ProjectSelectionCriteria<'_>,
    limits: ProjectSelectionLimits,
    request: &std::sync::Arc<crate::file_io::SnapshotContext>,
) -> io::Result<super::super::SourceSnapshot> {
    capture_inner(scope, input, limits, request, None, None, false)
}
/// Capture only the changed part of an existing selection policy. Directory
/// events use the same walker; untouched payloads are never opened or hashed.
pub(super) fn capture_changes_metadata(
    scope: &CapturedPublicationScope,
    input: ProjectSelectionCriteria<'_>,
    changes: &std::collections::BTreeSet<PathBuf>,
    limits: ProjectSelectionLimits,
    request: &Arc<crate::file_io::SnapshotContext>,
) -> io::Result<super::super::SourceSnapshot> {
    capture_inner(
        scope,
        input,
        limits,
        request,
        None,
        Some(changes.iter().cloned().collect::<Vec<_>>().as_slice()),
        false,
    )
}

pub(super) fn covers(source: &NativeSource, path: &Path) -> io::Result<bool> {
    source.request().check()?;
    crate::file_io::validate_relative(path, false)?;
    let selected = source
        .selected
        .as_ref()
        .ok_or(io::ErrorKind::InvalidInput)?;
    let criteria = &selected.criteria;
    if let Some(changes) = &criteria.changes {
        return Ok(changes.iter().any(|root| path.starts_with(root)));
    }
    Ok(criteria
        .subtree
        .as_ref()
        .is_some_and(|root| path.starts_with(root))
        || criteria.extras.iter().any(|extra| path == extra)
        || (criteria.readme
            && path
                .parent()
                .is_some_and(|parent| parent.as_os_str().is_empty())
            && path
                .file_stem()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.eq_ignore_ascii_case("readme"))))
}

/// Enumerate the admitted PROJECT selection without opening or hashing document
/// payloads. Each returned FILE must still be captured before it is published.
pub fn selected_project_paths(
    scope: &CapturedPublicationScope,
    input: ProjectSelectionCriteria<'_>,
    limits: ProjectSelectionLimits,
    request: &Arc<crate::file_io::SnapshotContext>,
) -> io::Result<Vec<PathBuf>> {
    scope.require_project()?;
    let mut engine = discover(scope, input, limits, request, None, None)?;
    engine.fence()?;
    engine.witness()?.check(&engine.source, &engine.criteria)?;
    Ok(engine.selected.into_keys().collect())
}

/// Enumerate only the portion affected by admitted watcher events, preserving
/// the same selection and ignore policy without reading document payloads.
pub fn selected_project_changed_paths(
    scope: &CapturedPublicationScope,
    input: ProjectSelectionCriteria<'_>,
    changes: &std::collections::BTreeSet<PathBuf>,
    limits: ProjectSelectionLimits,
    request: &Arc<crate::file_io::SnapshotContext>,
) -> io::Result<Vec<PathBuf>> {
    scope.require_project()?;
    let changes = changes.iter().cloned().collect::<Vec<_>>();
    let mut engine = discover(scope, input, limits, request, None, Some(&changes))?;
    engine.fence()?;
    engine.witness()?.check(&engine.source, &engine.criteria)?;
    Ok(engine.selected.into_keys().collect())
}

fn discover(
    scope: &CapturedPublicationScope,
    input: ProjectSelectionCriteria<'_>,
    limits: ProjectSelectionLimits,
    request: &Arc<crate::file_io::SnapshotContext>,
    admission: Option<Arc<RootReadAdmission>>,
    changes: Option<&[PathBuf]>,
) -> io::Result<Engine> {
    validate_criteria(&input, limits, request)?;
    let changes = changes
        .map(|paths| {
            paths
                .iter()
                .map(|path| {
                    request.check()?;
                    crate::native_path::valid_native(path)?;
                    if !path.as_os_str().is_empty() && path.as_os_str() != OsStr::new(".") {
                        crate::file_io::validate_relative(path, false)?;
                    }
                    if native_name_bytes(path.as_os_str())? > limits.max_observed_path_bytes {
                        return Err(io::ErrorKind::InvalidInput.into());
                    }
                    Ok(if path.as_os_str() == OsStr::new(".") {
                        PathBuf::new()
                    } else {
                        path.to_owned()
                    })
                })
                .collect::<io::Result<Vec<_>>>()
        })
        .transpose()?;

    let mut extras = Vec::new();
    extras
        .try_reserve_exact(input.extras.len())
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    for path in input.extras {
        request.check()?;
        extras.push((*path).to_owned());
    }
    let mut extensions = Vec::new();
    extensions
        .try_reserve_exact(input.extensions.len())
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    for suffix in input.extensions {
        request.check()?;
        extensions.push((*suffix).to_owned());
    }
    let mut defaults = Vec::new();
    defaults
        .try_reserve_exact(input.ignore.protected_defaults.len())
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    for pattern in input.ignore.protected_defaults {
        request.check()?;
        defaults.push((*pattern).to_owned());
    }
    let criteria = Criteria {
        subtree: input.subtree.map(|path| {
            if path.as_os_str() == OsStr::new(".") {
                PathBuf::new()
            } else {
                path.to_owned()
            }
        }),
        extras,
        readme: input.include_readme,
        extensions,
        defaults,
        custom: input.ignore.custom_control.map(str::to_owned),
        git: input.ignore.git_control.map(str::to_owned),
        changes,
    };
    let admission = match admission {
        Some(value) => {
            value.revalidate()?;
            value
        }
        None => Arc::new(RootReadAdmission::new(scope, request)?),
    };
    let selection = PathBuf::new();
    let source = NativeSource {
        selection,
        prefixes: Vec::new(),
        ancestor_bindings: Vec::new(),
        kind: NodeKind::Directory,
        directories: DirectoryLatches(Vec::new()),
        selected: None,
        admission,
    };
    let mut engine = Engine {
        source,
        criteria,
        limits,
        inventories: Vec::new(),
        inventory_indices: Arc::new(HashMap::new()),
        controls: Vec::new(),
        selected: BTreeMap::new(),
    };
    let root = Arc::clone(&engine.source.admission.anchor.root);
    engine.directory(PathBuf::new(), root, None)?;
    // References and clones below are admitted logical copies; no root/context
    // is reconstructed from their data.
    if let Some(changes) = engine.criteria.changes.clone() {
        let mut candidates = std::collections::BTreeSet::new();
        for changed in changes {
            if let Some(subtree) = &engine.criteria.subtree {
                if changed.starts_with(subtree) {
                    candidates.insert(changed.clone());
                } else if subtree.starts_with(&changed) {
                    candidates.insert(subtree.clone());
                }
            }
            for extra in &engine.criteria.extras {
                if extra.starts_with(&changed) {
                    candidates.insert(extra.clone());
                }
            }
        }
        for candidate in candidates {
            if let Some((parent, path, kind)) = engine.resolve(&candidate, false)? {
                if kind == DirectoryEntryKind::Directory {
                    if engine.criteria.extras.contains(&candidate) {
                        return Err(changed("extra_file_kind"));
                    }
                    if let Some((index, _, _)) = engine.resolve(&candidate, true)? {
                        engine.walk(index)?;
                    }
                } else {
                    engine.select(parent, path, kind)?;
                }
            }
        }
    } else {
        if let Some(subtree) = engine.criteria.subtree.clone()
            && let Some((index, _, _)) = engine.resolve(&subtree, true)?
        {
            engine.walk(index)?;
        }
        for n in 0..engine.criteria.extras.len() {
            let extra = engine.criteria.extras[n].clone();
            if let Some((parent, path, kind)) = engine.resolve(&extra, false)? {
                if kind == DirectoryEntryKind::Directory {
                    return Err(changed("extra_file_kind"));
                }
                engine.select(parent, path, kind)?;
            }
        }
    }
    if engine.criteria.readme {
        let candidate = engine.inventories[0]
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.kind != DirectoryEntryKind::Directory)
            .filter(|(_, entry)| {
                let path = Path::new(&entry.name);
                path.file_stem()
                    .and_then(OsStr::to_str)
                    .is_some_and(|stem| stem.eq_ignore_ascii_case("readme"))
                    && engine.criteria.allowed_extension(path)
            })
            .filter_map(|(index, entry)| entry.name.to_str().map(|name| (index, name)))
            .min_by(|a, b| a.1.cmp(b.1))
            .map(|(index, _)| index);
        if let Some(index) = candidate {
            let kind = engine.inventories[0].entries[index].kind;
            let path = engine.entry_path(0, index)?;
            if engine
                .criteria
                .changes
                .as_ref()
                .is_none_or(|changes| changes.iter().any(|root| path.starts_with(root)))
            {
                engine.select(0, path, kind)?;
            }
        }
    }

    Ok(engine)
}

fn capture_inner(
    scope: &CapturedPublicationScope,
    input: ProjectSelectionCriteria<'_>,
    limits: ProjectSelectionLimits,
    request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    previous: Option<(&Arc<RootReadAdmission>, &Witness)>,
    changes: Option<&[PathBuf]>,
    retain: bool,
) -> io::Result<super::super::SourceSnapshot> {
    // Recapture reuses its admission and expected selection together; a fresh
    // capture has neither. Keep the integrity check bound to that admission.
    let admission = previous.map(|(admission, _)| Arc::clone(admission));
    let mut engine = discover(scope, input, limits, request, admission, changes)?;
    engine.fence()?;
    let witness = engine.witness()?;
    if previous.is_some_and(|(_, expected)| !expected.same(&witness)) {
        return Err(changed("witness_generation"));
    }
    // Validate selected names and directory bindings before reading payload.
    witness.check(&engine.source, &engine.criteria)?;
    let node_count = engine
        .inventories
        .len()
        .checked_add(engine.selected.len())
        .ok_or(io::ErrorKind::FileTooLarge)?;

    let mut nodes = Vec::new();
    nodes
        .try_reserve_exact(node_count)
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    for directory in &engine.inventories {
        if directory.relative.as_os_str().is_empty()
            || engine
                .selected
                .range::<Path, _>((
                    std::ops::Bound::Included(directory.relative.as_ref()),
                    std::ops::Bound::Unbounded,
                ))
                .next()
                .is_some_and(|(path, _)| path.starts_with(directory.relative.as_ref()))
        {
            let directory_handle = open_observed_directory(
                &engine.source,
                &directory.relative,
                limits,
                &|path, current| {
                    engine
                        .directory_observation(path)
                        .is_some_and(|expected| expected.matches_directory_binding(current))
                },
            )?;
            let file = directory_handle.handle.lock()?;
            request.check()?;
            nodes.push(SourceNode::from_native_file(
                &directory.relative,
                Content::Directory,
                &file,
            )?);
        }
    }
    // The complete selected union is checked before payload buffers.
    request.check()?;
    let buffer_bytes = if engine.selected.is_empty() {
        0
    } else {
        tree::BUFFER_BYTES
    };

    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(buffer_bytes)
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    buffer.resize(buffer_bytes, 0);
    let mut retained = BTreeMap::new();
    for (relative, observation) in &engine.selected {
        request.check()?;
        let handle = engine.open_pending(relative, observation)?;
        let mut file = handle.handle.lock()?;
        request.check()?;
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        if retain {
            bytes
                .try_reserve_exact(
                    usize::try_from(observation.bytes())
                        .map_err(|_| io::ErrorKind::FileTooLarge)?,
                )
                .map_err(|_| io::ErrorKind::OutOfMemory)?;
        }
        let mut digest = Sha256::new();
        let mut left = observation.bytes();
        while left != 0 {
            request.check()?;
            let wanted = usize::try_from(left.min(buffer.len() as u64))
                .map_err(|_| io::ErrorKind::FileTooLarge)?;
            let read = file.read(&mut buffer[..wanted])?;
            if read == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            digest.update(&buffer[..read]);
            if retain {
                bytes.extend_from_slice(&buffer[..read]);
            }
            left -= read as u64;
            #[cfg(test)]
            run_selection_hook(SelectionStep::ReadChunk, relative, request);
        }
        request.check()?;
        if tree::Observation::from_file(&file)? != *observation {
            return Err(changed("selected_file_read"));
        }
        nodes.push(SourceNode::from_native_file(
            relative,
            Content::File {
                bytes: observation.bytes(),
                digest: digest.finalize().into(),
            },
            &file,
        )?);
        if retain {
            retained.insert(relative.clone(), bytes);
        }
    }
    engine.fence()?;
    witness.check(&engine.source, &engine.criteria)?;
    nodes.sort_by(|a, b| a.relative.cmp(&b.relative));
    let plan = SourcePlan::from_native_nodes(scope.owner_root().to_owned(), nodes);
    engine.source.selected = Some(SelectedBacking {
        criteria: engine.criteria,
        limits,
        witness,
    });
    request.check()?;
    super::super::SourceSnapshot::from_native_parts(
        Arc::new(engine.source),
        plan,
        retained,
        NodeKind::Directory,
        super::super::snapshot::CapturePolicy::Selected,
    )
}
pub(super) fn recapture(source: &NativeSource) -> io::Result<super::super::SourceSnapshot> {
    let selected = source.selected.as_ref().ok_or(io::ErrorKind::InvalidData)?;

    let (extras, extensions, defaults) = selected.criteria.borrowed()?;
    let input = ProjectSelectionCriteria {
        subtree: selected.criteria.subtree.as_deref().map(|p| {
            if p.as_os_str().is_empty() {
                Path::new(".")
            } else {
                p
            }
        }),
        extras: &extras,
        include_readme: selected.criteria.readme,
        extensions: &extensions,
        ignore: ProjectIgnoreCriteria {
            protected_defaults: &defaults,
            custom_control: selected.criteria.custom.as_deref(),
            git_control: selected.criteria.git.as_deref(),
        },
    };
    let result = capture_inner(
        &source.admission.scope,
        input,
        selected.limits,
        source.request(),
        Some((&source.admission, &selected.witness)),
        selected.criteria.changes.as_deref(),
        false,
    );
    drop(extras);
    drop(extensions);
    drop(defaults);
    result
}
pub(super) fn check(source: &NativeSource) -> io::Result<()> {
    if let Some(selected) = &source.selected {
        selected.witness.check(source, &selected.criteria)?;
    }
    source.request().check()
}
pub(super) fn check_binding(
    source: &NativeSource,
    relative: &Path,
    directory: bool,
) -> io::Result<()> {
    if let Some(selected) = &source.selected {
        let parent = if directory {
            relative
        } else {
            relative.parent().ok_or_else(|| changed("file_parent"))?
        };
        selected
            .witness
            .check_ancestors(source, parent, &selected.criteria)?;
    }
    source.request().check()
}
impl Witness {
    fn directory_observation(&self, path: &Path) -> Option<&tree::Observation> {
        self.inventory_indices
            .get(path)
            .map(|&index| &self.inventories[index].observation)
    }
    // A FILE read validates only its own chain and applicable control names.
    // The complete inventory and other FILES belong to explicit global checks.
    fn check_ancestors(
        &self,
        source: &NativeSource,
        parent: &Path,
        criteria: &Criteria,
    ) -> io::Result<()> {
        let request = source.request();
        #[cfg(test)]
        {
            let mut prefix = PathBuf::new();
            run_selection_hook(SelectionStep::DirectoryBinding, &prefix, request);
            for component in parent.components() {
                prefix.push(component.as_os_str());
                run_selection_hook(SelectionStep::DirectoryBinding, &prefix, request);
            }
        }
        let opened = open_observed_directory(source, parent, self.limits, &|path, current| {
            self.directory_observation(path)
                .is_some_and(|expected| expected.matches_directory_binding(current))
        })?;
        let mut prefix = PathBuf::new();
        let mut components = parent.components();
        for handle in opened
            ._ancestors
            .0
            .iter()
            .chain(std::iter::once(&opened.handle))
        {
            request.check()?;
            let index = *self
                .inventory_indices
                .get(prefix.as_path())
                .ok_or_else(|| changed("ancestor_inventory"))?;
            let inventory = &self.inventories[index];
            if inventory.scanned {
                let controls = &self.controls[inventory.control_indices.clone()];
                for name in [
                    criteria.git.as_deref(),
                    criteria
                        .custom
                        .as_deref()
                        .filter(|_| criteria.custom != criteria.git),
                ]
                .into_iter()
                .flatten()
                {
                    request.check()?;
                    #[cfg(test)]
                    run_selection_hook(SelectionStep::ControlBinding, &prefix.join(name), request);
                    let expected = controls
                        .iter()
                        .find(|control| control.relative.file_name() == Some(OsStr::new(name)));
                    let current = {
                        let file = handle.lock()?;
                        NativeHandle::open_child(
                            &file,
                            OsStr::new(name),
                            Some(NodeKind::File),
                            request,
                        )
                    };
                    match (expected, current) {
                        (Some(expected), Ok(current)) => {
                            if current.observe(request)? != expected.observation {
                                return Err(changed("control_binding"));
                            }
                        }
                        (None, Err(error)) if error.kind() == io::ErrorKind::NotFound => {}
                        (_, Err(error)) => return Err(error),
                        (None, Ok(_)) => return Err(changed("control_presence")),
                    }
                }
            }
            if !inventory
                .observation
                .matches_directory_binding(&handle.observe(request)?)
            {
                return Err(changed("control_parent_binding"));
            }
            if let Some(component) = components.next() {
                prefix.push(component.as_os_str());
            }
        }
        request.check()
    }
    fn check(&self, source: &NativeSource, criteria: &Criteria) -> io::Result<()> {
        let request = source.request();
        let limits = self.limits;
        for inventory in &self.inventories {
            #[cfg(test)]
            run_selection_hook(
                SelectionStep::DirectoryBinding,
                &inventory.relative,
                request,
            );
            request.check()?;
            let current =
                open_observed_directory(source, &inventory.relative, limits, &|path, current| {
                    self.directory_observation(path)
                        .is_some_and(|expected| expected.matches_directory_binding(current))
                })?;
            if !inventory.scanned {
                drop(current);
                continue;
            }

            let mut seen = Vec::new();
            seen.try_reserve_exact(inventory.entries.len())
                .map_err(|_| io::ErrorKind::OutOfMemory)?;
            seen.resize(inventory.entries.len(), false);

            let file = current.handle.lock()?;
            let mut iter =
                read_directory::DirectoryEntries::new(&file, tree::BUFFER_BYTES, &mut || {
                    request.check()
                })?;
            drop(file);
            while let Some(entry) = iter.next_entry(tree::BUFFER_BYTES, &mut || request.check())? {
                let length = native_name_bytes(inventory.relative.as_os_str())?
                    .checked_add(native_name_bytes(&entry.name)?)
                    .and_then(|n| n.checked_add(separator_bytes(&inventory.relative)))
                    .ok_or(io::ErrorKind::FileTooLarge)?;
                if length > limits.max_observed_path_bytes {
                    return Err(io::ErrorKind::FileTooLarge.into());
                }
                if !relevant_entry(criteria, &inventory.scope, &inventory.relative, &entry)? {
                    continue;
                }
                let at = inventory
                    .entries
                    .binary_search_by(|old| old.name.cmp(&entry.name))
                    .map_err(|_| changed("inventory_new_entry"))?;
                if seen[at] || inventory.entries[at].kind != entry.kind {
                    return Err(changed("inventory_entry_kind_or_duplicate"));
                }
                seen[at] = true;
                drop(entry);
            }
            if seen.iter().any(|value| !*value)
                || !inventory
                    .observation
                    .matches_directory_binding(&current.handle.observe(request)?)
            {
                return Err(changed("inventory_membership_or_binding"));
            }
            drop(iter);
            drop(seen);
            drop(current);
        }
        for control in &self.controls {
            #[cfg(test)]
            run_selection_hook(SelectionStep::ControlBinding, &control.relative, request);
            open_observed_file(
                source,
                &control.relative,
                &control.observation,
                limits,
                &|path| self.directory_observation(path),
            )?;
        }
        for selected in &self.selected {
            open_observed_file(
                source,
                &selected.relative,
                &selected.observation,
                limits,
                &|path| self.directory_observation(path),
            )?;
        }
        request.check()
    }
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum SelectionStep {
    ReadChunk,
    DirectoryBinding,
    ControlBinding,
}
#[cfg(test)]
struct SelectionHook {
    step: SelectionStep,
    at: PathBuf,
    action: Box<dyn FnOnce(&SnapshotContext)>,
}
#[cfg(test)]
std::thread_local! {
    static SELECTED_STEP: std::cell::RefCell<Option<SelectionHook>> = const {
        std::cell::RefCell::new(None)
    };
}
#[cfg(test)]
fn run_selection_hook(
    step: SelectionStep,
    at: &Path,
    request: &std::sync::Arc<crate::file_io::SnapshotContext>,
) {
    let hook = SELECTED_STEP.with(|slot| {
        let matches = slot
            .borrow()
            .as_ref()
            .is_some_and(|hook| hook.step == step && hook.at == at);
        if matches {
            slot.borrow_mut().take()
        } else {
            None
        }
    });
    if let Some(hook) = hook {
        (hook.action)(request);
    }
}
#[cfg(test)]
mod tests;
