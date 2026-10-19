//! Physical transport for completed native AST and Knowledge payloads.
//! Semantic index validation and generation freezing belong to the native producer.
mod envelope;
mod read;
#[cfg(test)]
mod tests;
mod write;

use graphit_core::artifact_store::{CopyLimits, SnapshotContext};
pub use read::{StagedImport, import, inspect, stage_import};
pub(crate) use read::{ordinary_mode, reject_duplicate_headers};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    str::FromStr,
};
pub use write::{export, export_ast_graph};

/// Native payload family carried by the package envelope.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageKind {
    Ast,
    Knowledge,
}
impl fmt::Display for PackageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ast => "ast",
            Self::Knowledge => "knowledge",
        })
    }
}
impl FromStr for PackageKind {
    type Err = PackageError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "ast" => Ok(Self::Ast),
            "knowledge" => Ok(Self::Knowledge),
            _ => Err(invalid("kind must be ast or knowledge")),
        }
    }
}

/// An underlying filesystem/SDK failure, or a rejected package.
#[derive(Debug)]
pub enum PackageError {
    Io(io::Error),
    Zip(zip::result::ZipError),
    Json(serde_json::Error),
    Invalid(String),
}
impl fmt::Display for PackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "package filesystem operation: {e}"),
            Self::Zip(e) => write!(f, "ZIP operation: {e}"),
            Self::Json(e) => write!(f, "package envelope: {e}"),
            Self::Invalid(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for PackageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Zip(e) => Some(e),
            Self::Json(e) => Some(e),
            Self::Invalid(_) => None,
        }
    }
}
impl From<io::Error> for PackageError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<zip::result::ZipError> for PackageError {
    fn from(e: zip::result::ZipError) -> Self {
        Self::Zip(e)
    }
}
impl From<serde_json::Error> for PackageError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
impl From<io::ErrorKind> for PackageError {
    fn from(kind: io::ErrorKind) -> Self {
        Self::Io(kind.into())
    }
}
fn invalid(message: impl Into<String>) -> PackageError {
    PackageError::Invalid(message.into())
}

#[derive(Debug)]
struct Entry {
    name: String,
    path: PathBuf,
    directory: bool,
    bytes: u64,
    mode: u32,
}
pub(crate) fn canonical_name(name: &str, directory: bool) -> Result<PathBuf, PackageError> {
    let body = if directory {
        name.strip_suffix('/')
            .ok_or_else(|| invalid("directory name must end in /"))?
    } else {
        name
    };
    if body.is_empty()
        || body.starts_with('/')
        || body.contains('\\')
        || body.chars().any(char::is_control)
    {
        return Err(invalid("package path is not canonical UTF-8 POSIX"));
    }
    let mut path = PathBuf::new();
    for component in body.split('/') {
        let stem = component
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit()
                && stem.as_bytes()[3] != b'0');
        if component.is_empty()
            || matches!(component, "." | "..")
            || component.ends_with(['.', ' '])
            || component.contains([':', '*', '?', '"', '<', '>', '|'])
            || reserved
        {
            return Err(invalid(
                "package path contains an invalid or reserved component",
            ));
        }
        path.push(component);
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(invalid("package path contains a nonrelative component"));
    }
    Ok(path)
}
fn source_name(path: &Path, directory: bool) -> Result<String, PackageError> {
    let mut names = Vec::new();
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err(invalid("captured path is not relative"));
        };
        names.push(
            component
                .to_str()
                .ok_or_else(|| invalid("package payload paths must be UTF-8"))?,
        );
    }
    let mut name = names.join("/");
    if directory {
        name.push('/');
    }
    canonical_name(&name, directory)?;
    Ok(name)
}
pub(crate) fn path_units(path: &Path) -> io::Result<usize> {
    #[cfg(unix)]
    {
        Ok(path.as_os_str().as_encoded_bytes().len())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str()
            .encode_wide()
            .count()
            .checked_mul(size_of::<u16>())
            .ok_or_else(|| io::ErrorKind::FileTooLarge.into())
    }
}
fn validate_entries(
    kind: PackageKind,
    entries: &[Entry],
    limits: CopyLimits,
) -> Result<(), PackageError> {
    if entries
        .len()
        .checked_add(1)
        .is_none_or(|nodes| nodes > limits.node_limit())
    {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let mut paths = 0usize;
    let mut bytes = 0u64;
    for entry in entries {
        paths = paths
            .checked_add(path_units(&entry.path)?)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        bytes = bytes
            .checked_add(entry.bytes)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if paths > limits.total_relative_path_bytes_limit()
            || bytes > limits.total_file_bytes_limit()
            || entry.path.components().count() > limits.depth_limit()
        {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
    }
    validate_payload_entries(kind, entries)
}

fn validate_payload_entries(kind: PackageKind, entries: &[Entry]) -> Result<(), PackageError> {
    let mut map = BTreeMap::new();
    let mut folded = BTreeSet::new();
    for entry in entries {
        if map.insert(entry.path.as_path(), entry.directory).is_some()
            || !folded.insert(entry.name.trim_end_matches('/').to_lowercase())
        {
            return Err(invalid("duplicate or colliding package path"));
        }
        let root = entry
            .path
            .components()
            .next()
            .and_then(|part| part.as_os_str().to_str())
            .ok_or_else(|| invalid("missing payload root"))?;
        let allowed = match kind {
            PackageKind::Ast => root == "graph.icebug",
            PackageKind::Knowledge => root == "index.lance",
        };
        if !allowed {
            return Err(invalid("unexpected payload root"));
        }
    }
    for entry in entries {
        for parent in entry
            .path
            .ancestors()
            .skip(1)
            .filter(|path| !path.as_os_str().is_empty())
        {
            if map.get(parent) != Some(&true) {
                return Err(invalid(
                    "payload parent directory is missing or conflicts with a file",
                ));
            }
        }
    }
    let required = match kind {
        PackageKind::Ast => "graph.icebug",
        PackageKind::Knowledge => "index.lance",
    };
    if map.get(Path::new(required)) != Some(&true) {
        return Err(invalid(format!("required directory {required} is missing")));
    }
    let nonempty = |root: &str| {
        entries.iter().any(|entry| {
            !entry.directory && entry.path.starts_with(root) && entry.path.components().count() > 1
        })
    };
    if !nonempty(required) {
        return Err(invalid("required payload bundle is empty"));
    }
    if kind == PackageKind::Ast {
        for split in ["graph.icebug/search.lance", "graph.icebug/native_nn.lance"] {
            if entries.iter().any(|entry| entry.path.starts_with(split)) {
                return Err(invalid("AST text and NN must share corpus.lance"));
            }
        }
        if map.get(Path::new("graph.icebug/schema.cypher")) != Some(&false) {
            return Err(invalid("graph.icebug/schema.cypher must be a regular file"));
        }
        for root in ["graph.icebug/source.lance", "graph.icebug/corpus.lance"] {
            if let Some(directory) = map.get(Path::new(root))
                && (!directory || !nonempty(root))
            {
                return Err(invalid(format!(
                    "optional {root} must be a nonempty directory"
                )));
            }
        }
    }
    Ok(())
}
pub(crate) fn copy_checked(
    input: &mut impl Read,
    output: &mut impl Write,
    limit: u64,
    context: &SnapshotContext,
) -> io::Result<u64> {
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        context.check()?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            context.check()?;
            return Ok(total);
        }
        total = total
            .checked_add(count as u64)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if total > limit {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        output.write_all(&buffer[..count])?;
    }
}
