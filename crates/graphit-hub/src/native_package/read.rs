use super::{
    Entry, PackageError, PackageKind, canonical_name, copy_checked, envelope, invalid,
    validate_entries,
};
use graphit_core::artifact_store::{CapturedSourceReader, CopyLimits, SnapshotContext};
use graphit_fs::NodeKind;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
};
use tempfile::{NamedTempFile, TempDir};
use zip::{CompressionMethod, ZipArchive};

/// SDK reads and seeks on the owned spool retain the original request checks.
struct CheckedFile<'a> {
    file: File,
    context: &'a SnapshotContext,
}
impl Read for CheckedFile<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.context.check()?;
        self.file.read(buffer)
    }
}
impl Seek for CheckedFile<'_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.context.check()?;
        self.file.seek(position)
    }
}
#[allow(clippy::unnecessary_cast)]
pub(crate) fn ordinary_mode(mode: Option<u32>, directory: bool) -> Result<u32, PackageError> {
    // libc mode constants have different integer types across supported OSes.
    let mode = mode.ok_or_else(|| invalid("unsupported ZIP file attributes"))?;
    let kind = mode & libc::S_IFMT as u32;
    let expected = if directory {
        libc::S_IFDIR as u32
    } else {
        libc::S_IFREG as u32
    };
    if kind != expected || mode & !(libc::S_IFMT as u32 | 0o777) != 0 {
        return Err(invalid(
            "ZIP entry is not an ordinary regular file or directory",
        ));
    }
    Ok(mode & 0o777)
}

pub(crate) fn reject_duplicate_headers<R: Read + Seek>(
    spool: &File,
    archive: &ZipArchive<R>,
    context: &SnapshotContext,
) -> Result<(), PackageError> {
    use zip::read::{ZipFile, ZipFileEntry};
    use zip::result::{ZipError, ZipResult};
    use zip::unstable::stream::{ZipStreamReader, ZipStreamVisitor};

    struct Headers<'a> {
        remaining: usize,
        context: &'a SnapshotContext,
    }
    impl ZipStreamVisitor for Headers<'_> {
        fn visit_file<R: Read>(&mut self, _: &mut ZipFile<'_, R>) -> ZipResult<()> {
            Err(ZipError::InvalidArchive("expected central metadata".into()))
        }
        fn visit_additional_metadata(&mut self, _: &ZipFileEntry<'_>) -> ZipResult<()> {
            self.context.check()?;
            self.remaining = self
                .remaining
                .checked_sub(1)
                .ok_or_else(|| ZipError::InvalidArchive("duplicate central entry name".into()))?;
            Ok(())
        }
    }
    // ZipArchive deduplicates raw names. The pinned SDK visitor preserves every
    // central header, so count disagreement rejects the archive before extraction.
    let mut input = CheckedFile {
        file: spool.try_clone()?,
        context,
    };
    input.seek(SeekFrom::Start(archive.central_directory_start()))?;
    let mut headers = Headers {
        remaining: archive.len(),
        context,
    };
    ZipStreamReader::new(input).visit(&mut headers)?;
    if headers.remaining != 0 {
        return Err(invalid("incomplete central metadata"));
    }
    Ok(())
}

type Prepared<'a> = (
    NamedTempFile,
    ZipArchive<CheckedFile<'a>>,
    PackageKind,
    Vec<Entry>,
);
fn prepare<'a>(
    source: &CapturedSourceReader,
    expected: Option<PackageKind>,
    limits: CopyLimits,
    context: &'a SnapshotContext,
) -> Result<Prepared<'a>, PackageError> {
    source.check_request(context)?;
    let mut nodes = source.nodes();
    let node = nodes
        .next()
        .ok_or_else(|| invalid("input archive is missing"))?;
    if node.kind() != NodeKind::File || nodes.next().is_some() {
        return Err(invalid("input archive must be one captured regular file"));
    }
    if node.bytes() > limits.total_file_bytes_limit() {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let captured = source.open_file(node.relative())?;
    let mut spool = NamedTempFile::new()?;
    if copy_checked(
        &mut captured.stream(0)?,
        spool.as_file_mut(),
        limits.total_file_bytes_limit(),
        context,
    )? != node.bytes()
    {
        return Err(invalid("captured archive length changed"));
    }
    source.revalidate_content()?;
    let input = CheckedFile {
        file: spool.as_file().try_clone()?,
        context,
    };
    let mut archive = ZipArchive::new(input)?;
    if archive.len() > limits.node_limit() {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    reject_duplicate_headers(spool.as_file(), &archive, context)?;
    let mut entries = Vec::new();
    let mut kind = None;
    for index in 0..archive.len() {
        context.check()?;
        let mut file = archive.by_index(index)?;
        if file.encrypted()
            || !matches!(
                file.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
        {
            return Err(invalid("encrypted or unsupported ZIP method"));
        }
        let name = std::str::from_utf8(file.name_raw())
            .map_err(|_| invalid("package names must be raw UTF-8"))?
            .to_owned();
        let directory = file.is_dir();
        let mode = ordinary_mode(file.unix_mode(), directory)?;
        let path = canonical_name(&name, directory)?;
        if name == "package.json" {
            if kind.is_some() || directory || file.size() > limits.total_file_bytes_limit() {
                return Err(invalid("duplicate or invalid package envelope"));
            }
            kind = Some(envelope::read(&mut file)?);
        } else {
            if directory {
                if file.size() != 0 {
                    return Err(invalid("directory entry carries file content"));
                }
                copy_checked(&mut file, &mut io::sink(), 0, context)?;
            }
            entries.push(Entry {
                name,
                path,
                directory,
                bytes: file.size(),
                mode,
            });
        }
    }
    let kind = kind.ok_or_else(|| invalid("package.json envelope is missing"))?;
    if expected.is_some_and(|expected| kind != expected) {
        return Err(invalid("package kind does not match the requested kind"));
    }
    validate_entries(kind, &entries, limits)?;
    source.check_request(context)?;
    Ok((spool, archive, kind, entries))
}
fn read_files(
    archive: &mut ZipArchive<CheckedFile<'_>>,
    entries: &[Entry],
    limits: CopyLimits,
    context: &SnapshotContext,
    destination: Option<&Path>,
) -> Result<(), PackageError> {
    let mut total = 0u64;
    for entry in entries.iter().filter(|entry| !entry.directory) {
        let mut file = archive.by_name(&entry.name)?;
        let remaining = limits
            .total_file_bytes_limit()
            .checked_sub(total)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        let copied = if let Some(root) = destination {
            let path = root.join(&entry.path);
            let mut output = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)?;
            let copied = copy_checked(&mut file, &mut output, remaining, context)?;
            output.sync_all()?;
            set_mode(&path, entry.mode)?;
            copied
        } else {
            copy_checked(&mut file, &mut io::sink(), remaining, context)?
        };
        if copied != entry.bytes {
            return Err(invalid("ZIP entry length does not match its metadata"));
        }
        total = total
            .checked_add(copied)
            .ok_or(io::ErrorKind::FileTooLarge)?;
    }
    Ok(())
}
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
    #[cfg(windows)]
    {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_readonly(mode & 0o222 == 0);
        fs::set_permissions(path, permissions)
    }
}
fn make_stage_removable(root: &Path, entries: &[Entry]) {
    // Only paths created in this owned temporary tree are changed during cleanup.
    let mut ordered: Vec<_> = entries.iter().collect();
    ordered.sort_by_key(|entry| entry.path.components().count());
    for entry in ordered {
        let path = root.join(&entry.path);
        #[cfg(unix)]
        {
            let _ = set_mode(&path, if entry.directory { 0o700 } else { 0o600 });
        }
        #[cfg(windows)]
        {
            if let Ok(metadata) = fs::metadata(&path) {
                let mut permissions = metadata.permissions();
                permissions.set_readonly(false);
                let _ = fs::set_permissions(path, permissions);
            }
        }
    }
}
/// Validate all member metadata and bodies, including SDK CRC checks, without installation.
pub fn inspect(
    source: &CapturedSourceReader,
    expected: Option<PackageKind>,
    limits: CopyLimits,
    context: &SnapshotContext,
) -> Result<PackageKind, PackageError> {
    let (_spool, mut archive, kind, entries) = prepare(source, expected, limits, context)?;
    read_files(&mut archive, &entries, limits, context, None)?;
    source.revalidate_content()?;
    source.check_request(context)?;
    Ok(kind)
}
/// Owned physical payload awaiting caller admission and exclusive publication.
/// Dropping it attempts cleanup only of its temporary tree.
pub struct StagedImport<'a> {
    stage: Option<TempDir>,
    entries: Vec<Entry>,
    destination: PathBuf,
    kind: PackageKind,
    source: &'a CapturedSourceReader,
    context: &'a SnapshotContext,
}
impl StagedImport<'_> {
    /// The extracted payload, before publication into the destination.
    pub fn path(&self) -> &Path {
        self.stage.as_ref().expect("owned stage is present").path()
    }
    pub fn kind(&self) -> PackageKind {
        self.kind
    }
    /// Recheck the captured source and original request, then publish without clobbering.
    /// This does not grant authority or validate native dataset semantics.
    pub fn publish(mut self) -> Result<PackageKind, PackageError> {
        self.source.revalidate_content()?;
        self.source.check_request(self.context)?;
        graphit_fs::rename_exclusive(self.path(), &self.destination)?;
        // Rename consumed the tree. Do not let TempDir remove a newly reused old path.
        let _ = self.stage.take().expect("owned stage is present").keep();
        Ok(self.kind)
    }
}
impl Drop for StagedImport<'_> {
    fn drop(&mut self) {
        if let Some(stage) = &self.stage {
            make_stage_removable(stage.path(), &self.entries);
        }
    }
}
/// Extract and physically validate a payload in an owned same-parent temporary tree.
/// The destination remains absent until the caller consumes the result with `publish`.
pub fn stage_import<'a>(
    source: &'a CapturedSourceReader,
    expected: Option<PackageKind>,
    destination: &Path,
    limits: CopyLimits,
    context: &'a SnapshotContext,
) -> Result<StagedImport<'a>, PackageError> {
    if !destination.is_absolute()
        || destination
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
        || destination.file_name().is_none()
    {
        return Err(invalid(
            "import destination must be an absolute normal path",
        ));
    }
    match fs::symlink_metadata(destination) {
        Ok(_) => return Err(io::ErrorKind::AlreadyExists.into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let parent = destination
        .parent()
        .ok_or_else(|| invalid("import destination parent is missing"))?;
    if !fs::symlink_metadata(parent)?.is_dir() {
        return Err(invalid("import destination parent must be a directory"));
    }
    let (_spool, mut archive, kind, entries) = prepare(source, expected, limits, context)?;
    let stage = tempfile::Builder::new()
        .prefix(".graphit-package-")
        .tempdir_in(parent)?;
    let result = (|| -> Result<(), PackageError> {
        let mut directories: Vec<_> = entries.iter().filter(|entry| entry.directory).collect();
        directories.sort_by_key(|entry| entry.path.components().count());
        for entry in &directories {
            context.check()?;
            fs::create_dir(stage.path().join(&entry.path))?;
        }
        read_files(&mut archive, &entries, limits, context, Some(stage.path()))?;
        for entry in directories.into_iter().rev() {
            context.check()?;
            set_mode(&stage.path().join(&entry.path), entry.mode)?;
        }
        source.revalidate_content()?;
        source.check_request(context)?;
        Ok(())
    })();
    if result.is_err() {
        make_stage_removable(stage.path(), &entries);
    }
    result?;
    Ok(StagedImport {
        stage: Some(stage),
        entries,
        destination: destination.to_owned(),
        kind,
        source,
        context,
    })
}
/// Physically validate and publish a payload into a missing destination.
pub fn import(
    source: &CapturedSourceReader,
    expected: Option<PackageKind>,
    destination: &Path,
    limits: CopyLimits,
    context: &SnapshotContext,
) -> Result<PackageKind, PackageError> {
    stage_import(source, expected, destination, limits, context)?.publish()
}
