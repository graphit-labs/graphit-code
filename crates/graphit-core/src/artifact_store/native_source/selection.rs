//! PROJECT document selection. Ignore rules and logical paths are domain data.
use super::super::{
    PublicationScope, SnapshotContext, SourceSnapshot,
    snapshot::{RetainedSources, validate_relative},
    tree::{BUFFER_BYTES, Content, Mode, RelativePath, SourceNode, SourcePlan},
};
use crate::ignore_rules::{CapturedIgnoreRules, IgnoreScope};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct ProjectSelectionCriteria<'a> {
    pub subtree: Option<&'a Path>,
    pub extras: &'a [&'a Path],
    pub include_readme: bool,
    pub extensions: &'a [&'a str],
    pub ignore: ProjectIgnoreCriteria<'a>,
}
pub struct ProjectIgnoreCriteria<'a> {
    pub protected_defaults: &'a [&'a str],
    pub custom_control: Option<&'a str>,
    pub git_control: Option<&'a str>,
}
/// Bounds on selector inputs and individual document/control contents.
#[derive(Clone, Copy)]
pub struct ProjectSelectionLimits {
    pub max_input_items: usize,
    pub max_input_bytes: usize,
    pub max_selected_file_bytes: u64,
    pub max_control_file_bytes: u64,
}
#[derive(Clone)]
pub(in crate::artifact_store) struct SelectionCoverage {
    subtree: Option<PathBuf>,
    extras: Vec<PathBuf>,
    readme: bool,
    changes: Option<Vec<PathBuf>>,
}
impl SelectionCoverage {
    pub fn covers(&self, path: &Path) -> bool {
        if let Some(changes) = &self.changes {
            return changes.iter().any(|root| path.starts_with(root));
        }
        self.subtree
            .as_ref()
            .is_some_and(|root| path.starts_with(root))
            || self.extras.iter().any(|extra| path == extra)
            || (self.readme
                && path
                    .parent()
                    .is_some_and(|parent| parent.as_os_str().is_empty())
                && path
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| name.eq_ignore_ascii_case("readme")))
    }
    fn intersects(&self, path: &Path, directory: bool) -> bool {
        self.changes.as_ref().is_none_or(|changes| {
            changes
                .iter()
                .any(|root| path.starts_with(root) || (directory && root.starts_with(path)))
        })
    }
    fn descends(&self, path: &Path) -> bool {
        self.intersects(path, true)
            && (self
                .subtree
                .as_ref()
                .is_some_and(|root| path.starts_with(root) || root.starts_with(path))
                || self.extras.iter().any(|extra| extra.starts_with(path)))
    }
}
fn normalized(path: &Path) -> io::Result<PathBuf> {
    if path == Path::new(".") {
        return Ok(PathBuf::new());
    }
    validate_relative(path)?;
    Ok(path.to_owned())
}
fn coverage(
    input: &ProjectSelectionCriteria<'_>,
    changes: Option<&BTreeSet<PathBuf>>,
) -> io::Result<SelectionCoverage> {
    Ok(SelectionCoverage {
        subtree: input.subtree.map(normalized).transpose()?,
        extras: input
            .extras
            .iter()
            .map(|path| normalized(path))
            .collect::<io::Result<_>>()?,
        readme: input.include_readme,
        changes: changes
            .map(|paths| {
                paths
                    .iter()
                    .map(|path| normalized(path))
                    .collect::<io::Result<_>>()
            })
            .transpose()?,
    })
}
fn validate(
    input: &ProjectSelectionCriteria<'_>,
    changes: Option<&BTreeSet<PathBuf>>,
    limits: ProjectSelectionLimits,
    request: &SnapshotContext,
) -> io::Result<()> {
    request.check()?;
    if limits.max_input_items == 0
        || limits.max_input_bytes == 0
        || limits.max_selected_file_bytes == 0
        || limits.max_control_file_bytes == 0
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut items = 0usize;
    let mut bytes = 0usize;
    let mut add = |length: usize| -> io::Result<()> {
        items = items.checked_add(1).ok_or(io::ErrorKind::FileTooLarge)?;
        bytes = bytes
            .checked_add(length)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if items > limits.max_input_items || bytes > limits.max_input_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        Ok(())
    };
    for path in input
        .subtree
        .into_iter()
        .chain(input.extras.iter().copied())
        .chain(changes.into_iter().flatten().map(PathBuf::as_path))
    {
        normalized(path)?;
        add(crate::file_io::native_name_bytes(path.as_os_str())?)?;
    }
    for text in input
        .extensions
        .iter()
        .chain(input.ignore.protected_defaults)
        .copied()
        .chain(input.ignore.custom_control)
        .chain(input.ignore.git_control)
    {
        add(text.len())?;
    }
    for selector in input.extensions {
        let extension = selector.strip_prefix('.').unwrap_or(selector);
        if extension.is_empty()
            || extension.chars().any(|ch| {
                ch.is_control()
                    || ch.is_whitespace()
                    || matches!(ch, '/' | '\\' | ':')
                    || (selector.starts_with('.') && ch == '.')
            })
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
    }
    for control in [input.ignore.git_control, input.ignore.custom_control]
        .into_iter()
        .flatten()
    {
        validate_relative(Path::new(control))?;
        if Path::new(control).components().count() != 1 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
    }
    request.check()
}
fn allowed(path: &Path, extensions: &[&str]) -> bool {
    extensions
        .iter()
        .any(|selector| match selector.strip_prefix('.') {
            Some(suffix) => path
                .extension()
                .and_then(OsStr::to_str)
                .is_some_and(|actual| actual.eq_ignore_ascii_case(suffix)),
            None => path
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|actual| actual.eq_ignore_ascii_case(selector)),
        })
}
fn control(path: &Path, limit: u64, request: &SnapshotContext) -> io::Result<Option<Vec<u8>>> {
    request.check()?;
    match fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => return Err(io::ErrorKind::InvalidData.into()),
        Ok(_) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    }
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !file.metadata()?.is_file() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let cap = limit.checked_add(1).ok_or(io::ErrorKind::InvalidInput)?;
    let mut output = Vec::new();
    let mut reader = file.take(cap);
    let mut buffer = [0; 8192];
    loop {
        request.check()?;
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        output.extend_from_slice(&buffer[..count]);
    }
    if output.len() as u64 > limit {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    request.check()?;
    Ok(Some(output))
}
fn ignore_scope(
    root: &Path,
    relative: &Path,
    parent: Option<&IgnoreScope>,
    input: &ProjectSelectionCriteria<'_>,
    limits: ProjectSelectionLimits,
    request: &SnapshotContext,
) -> io::Result<IgnoreScope> {
    let directory = root.join(relative);
    let git = input
        .ignore
        .git_control
        .map(|name| {
            control(
                &directory.join(name),
                limits.max_control_file_bytes,
                request,
            )
        })
        .transpose()?
        .flatten();
    let custom = if input.ignore.custom_control == input.ignore.git_control {
        git.clone()
    } else {
        input
            .ignore
            .custom_control
            .map(|name| {
                control(
                    &directory.join(name),
                    limits.max_control_file_bytes,
                    request,
                )
            })
            .transpose()?
            .flatten()
    };
    let rules = CapturedIgnoreRules {
        git_name: input.ignore.git_control,
        git: git.as_deref(),
        custom_name: input.ignore.custom_control,
        custom: custom.as_deref(),
    };
    match parent {
        Some(parent) => parent.captured_child(
            relative.file_name().ok_or(io::ErrorKind::InvalidInput)?,
            rules,
            &mut || request.check(),
        ),
        None => IgnoreScope::captured_root(
            root,
            &input
                .ignore
                .protected_defaults
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<Vec<_>>(),
            input.ignore.custom_control,
            rules,
            &mut || request.check(),
        ),
    }
}
fn discover(
    scope: &PublicationScope,
    input: &ProjectSelectionCriteria<'_>,
    limits: ProjectSelectionLimits,
    request: &SnapshotContext,
    changes: Option<&BTreeSet<PathBuf>>,
) -> io::Result<(BTreeSet<PathBuf>, SelectionCoverage)> {
    if !matches!(scope.owner(), super::super::StoreOwner::Project { .. }) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    validate(input, changes, limits, request)?;
    let coverage = coverage(input, changes)?;
    let root = scope.owner_root();
    if !fs::metadata(root)?.is_dir() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut selected = BTreeSet::new();
    if input.subtree.is_none() && input.extras.is_empty() && !input.include_readme {
        return Ok((selected, coverage));
    }
    if let Some(subtree) = &coverage.subtree {
        match fs::metadata(root.join(subtree)) {
            Ok(metadata) if !metadata.is_dir() => return Err(io::ErrorKind::InvalidData.into()),
            Ok(_) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
    }
    for extra in &coverage.extras {
        match fs::metadata(root.join(extra)) {
            Ok(metadata) if !metadata.is_file() => return Err(io::ErrorKind::InvalidData.into()),
            Ok(_) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
    }
    let mut scopes = BTreeMap::new();
    scopes.insert(
        PathBuf::new(),
        ignore_scope(root, Path::new(""), None, input, limits, request)?,
    );
    let mut readme_seen = false;
    let mut walk = walkdir::WalkDir::new(root)
        .follow_root_links(true)
        .follow_links(true)
        .sort_by_file_name()
        .into_iter();
    while let Some(entry) = walk.next() {
        request.check()?;
        let entry = match entry {
            Ok(entry) => entry,
            Err(error)
                if error
                    .io_error()
                    .is_some_and(|error| error.kind() == io::ErrorKind::NotFound) =>
            {
                continue;
            }
            Err(error) => return Err(io::Error::other(error)),
        };
        let relative = entry.path().strip_prefix(root).map_err(io::Error::other)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        crate::native_path::valid_native(relative)?;
        let directory = entry.file_type().is_dir();
        let parent = relative.parent().ok_or(io::ErrorKind::InvalidInput)?;
        let policy = scopes.get(parent).ok_or(io::ErrorKind::InvalidData)?;
        if directory {
            if !coverage.descends(relative) || !policy.should_descend(relative)? {
                walk.skip_current_dir();
                continue;
            }
            let child = ignore_scope(root, relative, Some(policy), input, limits, request)?;
            scopes.insert(relative.to_owned(), child);
        } else if entry.file_type().is_file() {
            let readme = input.include_readme
                && !readme_seen
                && parent.as_os_str().is_empty()
                && relative
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| name.eq_ignore_ascii_case("readme"))
                && allowed(relative, input.extensions);
            readme_seen |= readme;
            let wanted = readme
                || coverage
                    .subtree
                    .as_ref()
                    .is_some_and(|subtree| relative.starts_with(subtree))
                || coverage.extras.iter().any(|extra| relative == extra);
            if wanted
                && coverage.intersects(relative, false)
                && allowed(relative, input.extensions)
                && !policy.is_ignored(relative, false)?
            {
                if entry.metadata().map_err(io::Error::other)?.len()
                    > limits.max_selected_file_bytes
                {
                    return Err(io::ErrorKind::FileTooLarge.into());
                }
                selected.insert(relative.to_owned());
            }
        }
    }
    request.check()?;
    Ok((selected, coverage))
}
pub fn selected_project_paths(
    scope: &PublicationScope,
    input: ProjectSelectionCriteria<'_>,
    limits: ProjectSelectionLimits,
    request: &Arc<SnapshotContext>,
) -> io::Result<Vec<PathBuf>> {
    discover(scope, &input, limits, request, None).map(|(paths, _)| paths.into_iter().collect())
}
pub fn selected_project_changed_paths(
    scope: &PublicationScope,
    input: ProjectSelectionCriteria<'_>,
    changes: &BTreeSet<PathBuf>,
    limits: ProjectSelectionLimits,
    request: &Arc<SnapshotContext>,
) -> io::Result<Vec<PathBuf>> {
    discover(scope, &input, limits, request, Some(changes))
        .map(|(paths, _)| paths.into_iter().collect())
}
fn capture_inner(
    scope: &PublicationScope,
    input: ProjectSelectionCriteria<'_>,
    limits: ProjectSelectionLimits,
    request: &Arc<SnapshotContext>,
    changes: Option<&BTreeSet<PathBuf>>,
    retain: bool,
) -> io::Result<SourceSnapshot> {
    let (paths, coverage) = discover(scope, &input, limits, request, changes)?;
    let root = scope.owner_root();
    let mut nodes = BTreeMap::new();
    let mut retained = RetainedSources::new();
    for relative in std::iter::once(PathBuf::new()).chain(paths.iter().flat_map(|path| {
        path.parent()
            .into_iter()
            .flat_map(|parent| parent.ancestors().map(Path::to_owned))
    })) {
        if nodes.contains_key(&relative) {
            continue;
        }
        request.check()?;
        let metadata = fs::metadata(root.join(&relative))?;
        nodes.insert(
            relative.clone(),
            SourceNode {
                relative: RelativePath(relative),
                mode: Mode::observe(&metadata),
                content: Content::Directory,
            },
        );
    }
    let mut buffer = vec![0; BUFFER_BYTES];
    for relative in paths {
        request.check()?;
        let mut file = fs::File::open(root.join(&relative))?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut digest = Sha256::new();
        let mut length = 0u64;
        let mut body = Vec::new();
        loop {
            request.check()?;
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            length = length
                .checked_add(count as u64)
                .ok_or(io::ErrorKind::FileTooLarge)?;
            if length > limits.max_selected_file_bytes {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            digest.update(&buffer[..count]);
            if retain {
                body.extend_from_slice(&buffer[..count]);
            }
        }
        nodes.insert(
            relative.clone(),
            SourceNode {
                relative: RelativePath(relative.clone()),
                mode: Mode::observe(&metadata),
                content: Content::File {
                    bytes: length,
                    digest: digest.finalize().into(),
                },
            },
        );
        if retain {
            retained.insert(relative, body);
        }
    }
    request.check()?;
    Ok(SourceSnapshot::selected(
        SourcePlan::from_nodes(root.to_owned(), nodes.into_values().collect()),
        retained,
        coverage,
    ))
}
pub(in crate::artifact_store) fn capture(
    scope: &PublicationScope,
    input: ProjectSelectionCriteria<'_>,
    limits: ProjectSelectionLimits,
    request: &Arc<SnapshotContext>,
) -> io::Result<SourceSnapshot> {
    capture_inner(scope, input, limits, request, None, true)
}
pub(in crate::artifact_store) fn capture_metadata(
    scope: &PublicationScope,
    input: ProjectSelectionCriteria<'_>,
    limits: ProjectSelectionLimits,
    request: &Arc<SnapshotContext>,
) -> io::Result<SourceSnapshot> {
    capture_inner(scope, input, limits, request, None, false)
}
pub(in crate::artifact_store) fn capture_changes_metadata(
    scope: &PublicationScope,
    input: ProjectSelectionCriteria<'_>,
    changes: &BTreeSet<PathBuf>,
    limits: ProjectSelectionLimits,
    request: &Arc<SnapshotContext>,
) -> io::Result<SourceSnapshot> {
    capture_inner(scope, input, limits, request, Some(changes), false)
}

#[cfg(test)]
mod tests;
