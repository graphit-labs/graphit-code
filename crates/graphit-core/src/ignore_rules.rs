//! Immutable ignore scopes: project defaults, custom files, then Git files.

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

const FILE_LIMIT: u64 = 256 * 1024;

#[derive(Debug)]
struct Policy {
    root: PathBuf,
    custom: Option<String>,
    defaults: Gitignore,
}

#[derive(Debug)]
struct Layer {
    parent: Option<Arc<Layer>>,
    directory: PathBuf,
    git: Gitignore,
    custom: Gitignore,
}

/// Load once per indexing/watch batch; existing clones retain their snapshot.
#[derive(Clone, Debug)]
pub struct IgnoreScope {
    policy: Arc<Policy>,
    layer: Arc<Layer>,
}

impl IgnoreScope {
    pub fn load(
        project_root: impl AsRef<Path>,
        start_relative: impl AsRef<Path>,
        custom_filename: Option<&str>,
        defaults: &[&str],
    ) -> io::Result<Self> {
        let root = fs::canonicalize(project_root)?;
        if !root.is_dir() {
            return Err(invalid_input("project root must be a directory"));
        }
        let start = relative(start_relative.as_ref())?;
        let custom = custom_filename
            .map(basename)
            .transpose()?
            .map(|name| name.to_string_lossy().into_owned());
        let mut builder = GitignoreBuilder::new(&root);
        for pattern in defaults {
            builder.add_line(None, pattern).map_err(invalid_rules)?;
        }
        let policy = Arc::new(Policy {
            root,
            custom,
            defaults: builder.build().map_err(invalid_rules)?,
        });
        let mut scope = Self {
            layer: load_layer(&policy, PathBuf::new(), None)?,
            policy,
        };
        for part in start.components() {
            scope = scope.child(part.as_os_str())?;
        }
        Ok(scope)
    }

    pub(crate) fn captured_root(
        root: &Path,
        defaults: &[String],
        custom: Option<&str>,
        rules: CapturedIgnoreRules<'_>,
        before: &mut impl FnMut() -> io::Result<()>,
    ) -> io::Result<Self> {
        let mut builder = GitignoreBuilder::new(root);
        for pattern in defaults {
            before()?;
            builder.add_line(None, pattern).map_err(invalid_rules)?;
        }
        before()?;
        let policy = Arc::new(Policy {
            root: root.to_owned(),
            custom: custom.map(str::to_owned),
            defaults: builder.build().map_err(invalid_rules)?,
        });
        let layer = captured_layer(&policy, PathBuf::new(), None, rules, before)?;
        before()?;
        Ok(Self { policy, layer })
    }
    pub(crate) fn captured_child(
        &self,
        name: &std::ffi::OsStr,
        rules: CapturedIgnoreRules<'_>,
        before: &mut impl FnMut() -> io::Result<()>,
    ) -> io::Result<Self> {
        before()?;
        let directory = self.layer.directory.join(basename(Path::new(name))?);
        let layer = captured_layer(
            &self.policy,
            directory,
            Some(self.layer.clone()),
            rules,
            before,
        )?;
        before()?;
        Ok(Self {
            policy: Arc::clone(&self.policy),
            layer,
        })
    }

    pub fn root(&self) -> &Path {
        &self.policy.root
    }

    pub fn directory(&self) -> &Path {
        &self.layer.directory
    }

    /// Adds rules for one child without mutating the parent or its siblings.
    pub fn child(&self, name: impl AsRef<Path>) -> io::Result<Self> {
        let name = basename(name.as_ref())?;
        let directory = self.layer.directory.join(name);
        let metadata = fs::symlink_metadata(self.policy.root.join(&directory))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(invalid_input(
                "scope child must be a directory without a symlink",
            ));
        }
        Ok(Self {
            policy: self.policy.clone(),
            layer: load_layer(&self.policy, directory, Some(self.layer.clone()))?,
        })
    }

    /// Queries project-relative paths without reading them; deleted paths work too.
    pub fn is_ignored(&self, path: impl AsRef<Path>, is_dir: bool) -> io::Result<bool> {
        let path = self.checked_path(path.as_ref())?;
        Ok(self.ignored_absolute(&path, is_dir))
    }

    fn ignored_absolute(&self, path: &Path, is_dir: bool) -> bool {
        // A FILE negation cannot reopen an excluded parent. Resolve each native
        // prefix through the same matcher, including scoped/deleted-path queries.
        path.ancestors()
            .take_while(|prefix| *prefix != self.policy.root)
            .enumerate()
            .any(|(index, prefix)| self.ignored_prefix(prefix, index != 0 || is_dir))
    }

    fn ignored_prefix(&self, path: &Path, is_dir: bool) -> bool {
        if let Some(ignored) = matched(&self.policy.defaults, path, is_dir) {
            return ignored;
        }
        for custom in [true, false] {
            let mut layer = Some(self.layer.as_ref());
            while let Some(current) = layer {
                let layer_root = self.policy.root.join(&current.directory);
                // Rules apply inside their directory, never to that directory itself.
                if path == layer_root || !path.starts_with(&layer_root) {
                    layer = current.parent.as_deref();
                    continue;
                }
                let rules = if custom {
                    &current.custom
                } else {
                    &current.git
                };
                if let Some(ignored) = matched(rules, path, is_dir) {
                    return ignored;
                }
                layer = current.parent.as_deref();
            }
        }
        false
    }

    /// Only permitted parents are traversed; FILE negations do not reopen them.
    pub fn should_descend(&self, path: impl AsRef<Path>) -> io::Result<bool> {
        let absolute = self.checked_path(path.as_ref())?;
        Ok(!self.ignored_absolute(&absolute, true))
    }

    fn checked_path(&self, path: &Path) -> io::Result<PathBuf> {
        let path = relative(path)?;
        if !path.starts_with(&self.layer.directory) {
            return Err(invalid_input("path is outside the current ignore scope"));
        }
        Ok(self.policy.root.join(path))
    }
}

fn load_layer(
    policy: &Policy,
    directory: PathBuf,
    parent: Option<Arc<Layer>>,
) -> io::Result<Arc<Layer>> {
    let absolute = policy.root.join(&directory);
    let git = read_rules(&absolute, ".gitignore")?;
    let custom = match policy.custom.as_deref() {
        Some(name) if name != ".gitignore" => read_rules(&absolute, name)?,
        _ => Gitignore::empty(),
    };
    Ok(Arc::new(Layer {
        parent,
        directory,
        git,
        custom,
    }))
}

pub(crate) struct CapturedIgnoreRules<'a> {
    pub git_name: Option<&'a str>,
    pub git: Option<&'a [u8]>,
    pub custom_name: Option<&'a str>,
    pub custom: Option<&'a [u8]>,
}
fn captured_layer(
    policy: &Policy,
    directory: PathBuf,
    parent: Option<Arc<Layer>>,
    rules: CapturedIgnoreRules<'_>,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<Arc<Layer>> {
    let absolute = policy.root.join(&directory);
    let git = match (rules.git_name, rules.git) {
        (Some(name), Some(bytes)) => parse_rules(&absolute, name, bytes, before)?,
        _ => Gitignore::empty(),
    };
    let custom = match (rules.custom_name, rules.custom) {
        (Some(name), Some(bytes)) => parse_rules(&absolute, name, bytes, before)?,
        _ => Gitignore::empty(),
    };
    before()?;
    Ok(Arc::new(Layer {
        parent,
        directory,
        git,
        custom,
    }))
}

fn read_rules(directory: &Path, name: &str) -> io::Result<Gitignore> {
    let path = directory.join(name);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Gitignore::empty()),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() {
        return Err(invalid_input(
            "ignore source must be a regular file, without symlinks",
        ));
    }
    let file = File::open(&path)?;
    let mut bytes = Vec::new();
    file.take(FILE_LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err(invalid_rules("ignore file exceeds 256 KiB"));
    }
    parse_rules(directory, name, &bytes, &mut || Ok(()))
}
fn parse_rules(
    directory: &Path,
    name: &str,
    bytes: &[u8],
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<Gitignore> {
    before()?;
    let path = directory.join(name);
    let text = std::str::from_utf8(bytes).map_err(invalid_rules)?;
    let mut builder = GitignoreBuilder::new(directory);
    for line in text.strip_prefix('\u{feff}').unwrap_or(text).lines() {
        before()?;
        builder
            .add_line(Some(path.clone()), line)
            .map_err(invalid_rules)?;
    }
    before()?;
    builder.build().map_err(invalid_rules)
}

fn matched(rules: &Gitignore, path: &Path, is_dir: bool) -> Option<bool> {
    // Last direct match wins; only an unmatched path inherits a parent's match.
    match rules.matched_path_or_any_parents(path, is_dir) {
        Match::Ignore(_) => Some(true),
        Match::Whitelist(_) => Some(false),
        Match::None => None,
    }
}

fn relative(path: &Path) -> io::Result<PathBuf> {
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Normal(name) => result.push(name),
            Component::CurDir => {}
            _ => {
                return Err(invalid_input(
                    "ignore paths must be relative without traversal",
                ));
            }
        }
    }
    Ok(result)
}

fn basename(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    let path = path.as_ref();
    let mut parts = path.components();
    if matches!(parts.next(), Some(Component::Normal(_))) && parts.next().is_none() {
        Ok(PathBuf::from(path.file_name().expect("normal basename")))
    } else {
        Err(invalid_input(
            "expected a single directory or ignore-file name",
        ))
    }
}

fn invalid_input(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn invalid_rules(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}
