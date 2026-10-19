//! Ordinary file artifacts use an owned ZIP stage, separate from native packages.
use crate::native_package::{self, PackageError};
use graphit_core::artifact_store::{CopyLimits, SnapshotContext};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use tempfile::TempDir;
use zip::{CompressionMethod, ZipArchive};

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn package_error(error: PackageError) -> io::Error {
    match error {
        PackageError::Io(error) => error,
        PackageError::Zip(error) => zip_error(error),
        error => io::Error::new(io::ErrorKind::InvalidData, error),
    }
}

fn zip_error(error: zip::result::ZipError) -> io::Error {
    match error {
        zip::result::ZipError::Io(error) => error,
        error => io::Error::new(io::ErrorKind::InvalidData, error),
    }
}

struct Entry {
    index: usize,
    path: PathBuf,
    directory: bool,
    bytes: u64,
    mode: u32,
}

/// Extract a verified, complete ordinary ZIP into an exclusively owned stage.
///
/// The caller verifies the downloaded object's digest and retains its admission.
/// Neither this directory nor its contents grant access or publish an artifact.
/// Directory permissions remain writable inside the private stage so cancellation
/// and dropping the owner can remove it, including archives with read-only dirs.
pub fn stage_file_payload(
    input: &mut File,
    parent: &Path,
    limits: CopyLimits,
    original: &SnapshotContext,
) -> io::Result<TempDir> {
    original.check()?;
    if input.metadata()?.len() > limits.total_file_bytes_limit() {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    input.seek(SeekFrom::Start(0))?;
    let spool = input.try_clone()?;
    let mut archive = ZipArchive::new(input).map_err(zip_error)?;
    original.check()?;
    if archive.len() > limits.node_limit() {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    native_package::reject_duplicate_headers(&spool, &archive, original).map_err(package_error)?;

    let mut entries = Vec::new();
    let mut explicit = BTreeSet::new();
    let mut nodes: BTreeMap<String, (PathBuf, bool)> = BTreeMap::new();
    let mut path_bytes = 0usize;
    let mut declared_bytes = 0u64;
    for index in 0..archive.len() {
        original.check()?;
        let file = archive.by_index(index).map_err(zip_error)?;
        if file.encrypted()
            || !matches!(
                file.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
        {
            return Err(invalid("encrypted or unsupported ZIP method"));
        }
        let name = std::str::from_utf8(file.name_raw())
            .map_err(|_| invalid("ZIP member names must be raw UTF-8"))?;
        let directory = file.is_dir();
        let mode =
            native_package::ordinary_mode(file.unix_mode(), directory).map_err(package_error)?;
        let path = native_package::canonical_name(name, directory).map_err(package_error)?;
        if directory && file.size() != 0 {
            return Err(invalid("ZIP directory carries file content"));
        }
        if !explicit.insert(path.clone()) {
            return Err(invalid("duplicate ZIP member"));
        }
        if path.components().count() > limits.depth_limit() {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        for node in path
            .ancestors()
            .take_while(|node| !node.as_os_str().is_empty())
        {
            let directory = directory || node != path.as_path();
            let key = node.to_string_lossy().to_lowercase();
            if let Some((prior, prior_directory)) = nodes.get(&key) {
                if prior.as_path() != node || !directory || !*prior_directory {
                    return Err(invalid("colliding ZIP member paths"));
                }
            } else {
                path_bytes = path_bytes
                    .checked_add(native_package::path_units(node)?)
                    .ok_or(io::ErrorKind::FileTooLarge)?;
                nodes.insert(key, (node.to_owned(), directory));
                if nodes
                    .len()
                    .checked_add(1)
                    .is_none_or(|count| count > limits.node_limit())
                    || path_bytes > limits.total_relative_path_bytes_limit()
                {
                    return Err(io::ErrorKind::FileTooLarge.into());
                }
            }
        }
        declared_bytes = declared_bytes
            .checked_add(file.size())
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if declared_bytes > limits.total_file_bytes_limit() {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        entries.push(Entry {
            index,
            path,
            directory,
            bytes: file.size(),
            mode,
        });
    }
    original.check()?;
    let stage = tempfile::Builder::new()
        .prefix("graphit-file-payload-")
        .tempdir_in(parent)?;
    for (path, directory) in nodes.values() {
        original.check()?;
        if *directory {
            fs::create_dir_all(stage.path().join(path))?;
        }
    }
    let mut copied_bytes = 0u64;
    for entry in entries {
        original.check()?;
        let mut file = archive.by_index(entry.index).map_err(zip_error)?;
        if entry.directory {
            native_package::copy_checked(&mut file, &mut io::sink(), 0, original)?;
            continue;
        }
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(stage.path().join(entry.path))?;
        let remaining = limits
            .total_file_bytes_limit()
            .checked_sub(copied_bytes)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        let copied = native_package::copy_checked(&mut file, &mut output, remaining, original)?;
        if copied != entry.bytes {
            return Err(invalid("ZIP member length differs from metadata"));
        }
        copied_bytes = copied_bytes
            .checked_add(copied)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            output.set_permissions(fs::Permissions::from_mode(entry.mode))?;
        }
        #[cfg(not(unix))]
        let _ = entry.mode;
    }
    original.check()?;
    Ok(stage)
}
