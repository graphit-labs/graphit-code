//! Shared private metadata I/O. Data never recreates scratch origin or a lock.
use crate::{
    file_io::SnapshotContext,
    local_files::regular_metadata,
    native_document_file::{FileObservation, observe_handle, open_file},
    native_file_identity::{PhysicalId, handle_id, physical_id},
    native_path::valid_path,
};
use secrecy::zeroize::Zeroizing;
use std::{
    fs::{self, File},
    io::{self, Read, Seek},
    path::{Path, PathBuf},
};

pub(crate) struct Access<'a> {
    pub limit: usize,
    pub access_limit: usize,
    pub context: Option<&'a SnapshotContext>,
    pub check: &'a dyn Fn() -> io::Result<()>,
}
impl Access<'_> {
    pub fn check(&self) -> io::Result<()> {
        if let Some(context) = self.context {
            context.check()?;
        }
        (self.check)()
    }
    fn read(&self, file: &mut File, buffer: &mut [u8]) -> io::Result<usize> {
        self.check()?;
        file.read(buffer)
    }
}
pub(crate) struct Observed {
    pub metadata: FileObservation,
    pub bytes: Zeroizing<Vec<u8>>,
}
pub(crate) struct CapturedFile {
    pub file: File,
    pub value: Observed,
}
pub(crate) fn validate_limit(limit: usize) -> io::Result<()> {
    if limit == 0
        || limit
            .checked_add(1)
            .and_then(|n| u64::try_from(n).ok())
            .is_none()
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}
pub(crate) fn conflict() -> io::Error {
    io::ErrorKind::InvalidData.into()
}

pub(crate) fn observe(
    path: &Path,
    file: &File,
    access: &Access<'_>,
) -> io::Result<FileObservation> {
    access.check()?;
    if let Err(error) = graphit_fs::verify_private_file(file) {
        if error.kind() != io::ErrorKind::PermissionDenied {
            return Err(error);
        }
        // A replaced pathname is a changed observation even when unlinking its
        // previous inode makes the retained descriptor fail the private-file policy.
        access.check()?;
        let changed = match (regular_metadata(path), handle_id(file)) {
            (Ok(Some(metadata)), Ok(retained)) => {
                physical_id(path, &metadata).is_ok_and(|current| current != retained)
            }
            _ => false,
        };
        access.check()?;
        if changed {
            return Err(conflict());
        }
        return Err(error);
    }
    let observed = observe_handle(file, access.access_limit)?;
    let metadata = regular_metadata(path)?.ok_or_else(conflict)?;
    if physical_id(path, &metadata)? != observed.identity {
        return Err(conflict());
    }
    access.check()?;
    Ok(observed)
}
pub(crate) fn capture(
    path: &Path,
    access: &Access<'_>,
) -> io::Result<Option<(File, FileObservation)>> {
    access.check()?;
    let Some(metadata) = regular_metadata(path)? else {
        return Ok(None);
    };
    if metadata.len() > u64::try_from(access.limit).map_err(|_| io::ErrorKind::InvalidInput)? {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let expected = physical_id(path, &metadata)?;
    let file = open_file(path)?;
    let observed = observe(path, &file, access)?;
    if observed.identity != expected {
        return Err(conflict());
    }
    if usize::try_from(observed.length).map_err(|_| io::ErrorKind::FileTooLarge)? > access.limit {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    Ok(Some((file, observed)))
}
fn read_bytes(
    file: &mut File,
    length: usize,
    expected: Option<&[u8]>,
    retain: bool,
    access: &Access<'_>,
) -> io::Result<Zeroizing<Vec<u8>>> {
    access.check()?;
    let mut bytes = Zeroizing::new(Vec::new());
    if retain {
        bytes
            .try_reserve_exact(length)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
    }
    let mut buffer = Zeroizing::new([0; 64 * 1024]);
    let mut consumed = 0;
    while consumed < length {
        let wanted = (length - consumed).min(buffer.len());
        let read = access.read(file, &mut buffer[..wanted])?;
        if read == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        access.check()?;
        if let Some(expected) = expected
            && buffer[..read] != expected[consumed..consumed + read]
        {
            return Err(conflict());
        }
        if retain {
            bytes.extend_from_slice(&buffer[..read]);
        }
        consumed += read;
    }
    Ok(bytes)
}
impl CapturedFile {
    pub fn open(
        path: &Path,
        access: &Access<'_>,
        after_read: impl FnOnce(),
    ) -> io::Result<Option<Self>> {
        let Some((file, metadata)) = capture(path, access)? else {
            return Ok(None);
        };
        Self::from_opened(file, metadata, access, after_read, |file, expected| {
            if observe(path, file, access)? != *expected {
                return Err(conflict());
            }
            Ok(())
        })
        .map(Some)
    }

    /// Shared synchronous content loop; opening policy belongs to the caller.
    /// Native provenance comes from its actual held-parent producer, never this
    /// I/O helper or an observation supplied by an external record.
    pub(crate) fn from_opened(
        mut file: File,
        metadata: FileObservation,
        access: &Access<'_>,
        after_read: impl FnOnce(),
        observe_current: impl Fn(&File, &FileObservation) -> io::Result<()>,
    ) -> io::Result<Self> {
        let length = usize::try_from(metadata.length).map_err(|_| io::ErrorKind::FileTooLarge)?;
        let bytes = read_bytes(&mut file, length, None, true, access)?;
        after_read();
        if access.read(&mut file, &mut [0; 1])? != 0 {
            return Err(conflict());
        }
        observe_current(&file, &metadata)?;
        access.check()?;
        Ok(Self {
            file,
            value: Observed { metadata, bytes },
        })
    }
    pub fn revalidate(&mut self, path: &Path, access: &Access<'_>) -> io::Result<()> {
        if observe(path, &self.file, access)? != self.value.metadata {
            return Err(conflict());
        }
        self.file.rewind()?;
        compare_bytes(&mut self.file, &self.value.bytes, access)?;
        if observe(path, &self.file, access)? != self.value.metadata {
            return Err(conflict());
        }
        Ok(())
    }
}
pub(crate) fn compare_bytes(file: &mut File, bytes: &[u8], access: &Access<'_>) -> io::Result<()> {
    if bytes.len() <= 64 {
        compare_bytes_buffered::<64>(file, bytes, access)
    } else {
        compare_bytes_buffered::<{ 64 * 1024 }>(file, bytes, access)
    }
}
fn compare_bytes_buffered<const BUFFER_BYTES: usize>(
    file: &mut File,
    bytes: &[u8],
    access: &Access<'_>,
) -> io::Result<()> {
    access.check()?;
    let mut buffer = Zeroizing::new([0; BUFFER_BYTES]);
    for chunk in bytes.chunks(buffer.len()) {
        let mut read = 0;
        while read < chunk.len() {
            let amount = access.read(file, &mut buffer[read..chunk.len()])?;
            if amount == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            read += amount;
        }
        access.check()?;
        if buffer[..chunk.len()] != *chunk {
            return Err(conflict());
        }
    }
    if access.read(file, &mut buffer[..1])? != 0 {
        return Err(conflict());
    }
    Ok(())
}
pub(crate) fn verify_closed(
    path: &Path,
    expected: Option<&Observed>,
    access: &Access<'_>,
) -> io::Result<()> {
    let Some(expected) = expected else {
        access.check()?;
        return if regular_metadata(path)?.is_none() {
            Ok(())
        } else {
            Err(conflict())
        };
    };
    let Some((mut file, metadata)) = capture(path, access)? else {
        return Err(conflict());
    };
    if metadata != expected.metadata {
        return Err(conflict());
    }
    compare_bytes(&mut file, &expected.bytes, access)?;
    if observe(path, &file, access)? != metadata {
        return Err(conflict());
    }
    Ok(())
}
/// Retain only when the consumer needs a fresh snapshot; catalog comparison is streaming.
pub(crate) struct ExpectedEffect<'a> {
    pub identity: &'a PhysicalId,
    pub bytes: &'a [u8],
    pub old: Option<&'a Observed>,
    pub native_failed: bool,
    pub retain: bool,
}
pub(crate) fn observe_effect(
    path: &Path,
    effect: ExpectedEffect<'_>,
    access: &Access<'_>,
) -> io::Result<Option<Observed>> {
    let ExpectedEffect {
        identity: expected,
        bytes,
        old,
        native_failed,
        retain,
    } = effect;
    let Some((mut file, metadata)) = capture(path, access)? else {
        return if native_failed && old.is_none() {
            Ok(None)
        } else {
            Err(conflict())
        };
    };
    let expected_bytes = if metadata.identity == *expected && metadata.length == bytes.len() as u64
    {
        bytes
    } else if let Some(old) = old
        && native_failed
        && old.metadata == metadata
    {
        &old.bytes
    } else {
        return Err(conflict());
    };
    let observed_bytes = read_bytes(
        &mut file,
        expected_bytes.len(),
        Some(expected_bytes),
        retain,
        access,
    )?;
    if access.read(&mut file, &mut [0; 1])? != 0 || observe(path, &file, access)? != metadata {
        return Err(conflict());
    }
    Ok(Some(Observed {
        metadata,
        bytes: observed_bytes,
    }))
}

// Origin remains in process, without decoder, Clone, Drop deletion or recovery adoption.
pub(crate) struct CreatedScratch {
    path: PathBuf,
    identity: PhysicalId,
}
impl CreatedScratch {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn identity(&self) -> &PhysicalId {
        &self.identity
    }
    pub fn create(
        path: PathBuf,
        check: &dyn Fn() -> io::Result<()>,
        create: impl FnOnce(&Path) -> io::Result<File>,
    ) -> io::Result<(Self, File)> {
        valid_path(&path)?;
        check()?;
        let file = create(&path)?;
        let identity = handle_id(&file)?;
        identity.validate()?;
        Ok((Self { path, identity }, file))
    }
    pub fn verify(&self, file: &File, check: &dyn Fn() -> io::Result<()>) -> io::Result<()> {
        check()?;
        graphit_fs::verify_private_file(file)?;
        if handle_id(file)? != self.identity
            || physical_id(&self.path, &fs::symlink_metadata(&self.path)?)? != self.identity
        {
            return Err(conflict());
        }
        Ok(())
    }
    pub fn cleanup(&self, check: &dyn Fn() -> io::Result<()>) -> io::Result<()> {
        check()?;
        let Some(metadata) = regular_metadata(&self.path)? else {
            return Ok(());
        };
        if physical_id(&self.path, &metadata)? != self.identity {
            return Err(conflict());
        }
        let file = open_file(&self.path)?;
        self.verify(&file, check)?;
        drop(file);
        check()?;
        if physical_id(&self.path, &fs::symlink_metadata(&self.path)?)? != self.identity {
            return Err(conflict());
        }
        fs::remove_file(&self.path)
    }
}
pub(crate) fn random_path(parent: &Path, prefix: &str) -> io::Result<PathBuf> {
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(io::Error::other)?;
    let path = parent.join(format!("{prefix}{:032x}.tmp", u128::from_le_bytes(random)));
    valid_path(&path)?;
    Ok(path)
}
pub(crate) fn replacement_identity(
    stage: &PhysicalId,
    old: Option<&PhysicalId>,
) -> io::Result<PhysicalId> {
    match (stage, old) {
        (
            PhysicalId::Windows { volume, index, .. },
            Some(PhysicalId::Windows {
                volume: old_volume,
                created,
                ..
            }),
        ) if volume == old_volume => Ok(PhysicalId::Windows {
            volume: *volume,
            index: *index,
            created: *created,
        }),
        (PhysicalId::Unix { .. }, Some(PhysicalId::Unix { .. })) | (_, None) => Ok(stage.clone()),
        _ => Err(conflict()),
    }
}
pub(crate) fn replace(stage: &Path, destination: &Path, exists: bool) -> io::Result<()> {
    if !exists {
        graphit_fs::rename_exclusive(stage, destination)
    } else {
        #[cfg(windows)]
        {
            graphit_fs::replace_existing_file(stage, destination)
        }
        #[cfg(unix)]
        {
            fs::rename(stage, destination)
        }
    }
}
pub(crate) fn create_file(path: &Path) -> io::Result<File> {
    graphit_fs::create_private_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_lock::Cancellation;
    use std::{
        io::Write,
        time::{Duration, Instant},
    };

    #[test]
    fn comparison_preserves_small_large_and_eof_boundaries() {
        for length in [0, 1, 63, 64, 65, 65536, 65537] {
            let bytes = vec![b'v'; length];
            let mut file = tempfile::tempfile().unwrap();
            file.write_all(&bytes).unwrap();
            file.rewind().unwrap();
            let context = SnapshotContext::new(
                Instant::now() + Duration::from_secs(30),
                Cancellation::default(),
            );
            let access = Access {
                limit: length.max(1),
                access_limit: 256,
                context: Some(&context),
                check: &|| Ok(()),
            };
            compare_bytes(&mut file, &bytes, &access).unwrap();
            assert_eq!(file.stream_position().unwrap(), length as u64);

            file.write_all(b"extra").unwrap();
            file.rewind().unwrap();
            assert_eq!(
                compare_bytes(&mut file, &bytes, &access)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData
            );
            if length != 0 {
                file.set_len((length - 1) as u64).unwrap();
                file.rewind().unwrap();
                assert_eq!(
                    compare_bytes(&mut file, &bytes, &access)
                        .unwrap_err()
                        .kind(),
                    io::ErrorKind::UnexpectedEof
                );
                file.set_len(length as u64).unwrap();
                file.rewind().unwrap();
                file.write_all(&bytes).unwrap();
                file.seek(std::io::SeekFrom::End(-1)).unwrap();
                file.write_all(b"changed").unwrap();
                file.set_len(length as u64).unwrap();
                file.rewind().unwrap();
                assert_eq!(
                    compare_bytes(&mut file, &bytes, &access)
                        .unwrap_err()
                        .kind(),
                    io::ErrorKind::InvalidData
                );
            }
        }
    }

    #[test]
    fn comparison_keeps_original_cancellation_and_deadline_before_io() {
        for length in [64, 65] {
            let bytes = vec![b'v'; length];
            let mut file = tempfile::tempfile().unwrap();
            file.write_all(&bytes).unwrap();
            for expected in [io::ErrorKind::Interrupted, io::ErrorKind::TimedOut] {
                file.rewind().unwrap();
                let cancellation = Cancellation::default();
                let deadline = if expected == io::ErrorKind::Interrupted {
                    cancellation.cancel();
                    Instant::now() + Duration::from_secs(30)
                } else {
                    Instant::now() - Duration::from_secs(1)
                };
                let context = SnapshotContext::new(deadline, cancellation);
                let access = Access {
                    limit: length,
                    access_limit: 256,
                    context: Some(&context),
                    check: &|| Ok(()),
                };
                assert_eq!(
                    compare_bytes(&mut file, &bytes, &access)
                        .unwrap_err()
                        .kind(),
                    expected
                );
                assert_eq!(file.stream_position().unwrap(), 0);
            }
        }
    }
}
