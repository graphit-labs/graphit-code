//! Finalized ordinary ZIP bytes retained under one original request.
use graphit_core::artifact_store::{
    CapturedSourceFile, CapturedSourceReader, CopyPolicy, SourceReadLimits,
};
use graphit_fs::NodeKind;
use graphit_hub::{
    file_payload::stage_file_payload,
    policy::{HubResourcePolicy, RequestContext},
    remote_storage::hash_reader,
};
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::Arc,
};
use tempfile::TempDir;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

pub(super) struct Archive {
    pub(super) reader: CapturedSourceReader,
    pub(super) file: CapturedSourceFile,
    pub(super) sha256: [u8; 32],
    _owner: TempDir,
}

fn zip_error(error: zip::result::ZipError) -> io::Error {
    match error {
        zip::result::ZipError::Io(error) => error,
        error => io::Error::new(io::ErrorKind::InvalidData, error),
    }
}

/// Source is already captured from the admitted PROJECT artifact selection.
/// The spool is private; extracting it here uses the consumer's ordinary ZIP
/// validation before any payload/catalog I/O, without retaining file bodies.
pub(super) fn spool(
    source: &CapturedSourceReader,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
) -> io::Result<Archive> {
    check()?;
    source.check_request(original.snapshot())?;
    let owner = tempfile::Builder::new()
        .prefix("hub-file-publication-")
        .tempdir()?;
    let target = owner.path().join("payload.zip");
    let mut file = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&target)?;
    let mut archive = ZipWriter::new(&mut file);
    let options = SimpleFileOptions::default()
        .last_modified_time(zip::DateTime::default())
        .compression_method(CompressionMethod::Stored);
    let mut found_root = false;
    let mut buffer = [0u8; 64 * 1024];
    for node in source.nodes() {
        check()?;
        if node.relative().as_os_str().is_empty() {
            if node.kind() != NodeKind::Directory {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            found_root = true;
            continue;
        }
        let relative = node.relative().to_str().ok_or(io::ErrorKind::InvalidData)?;
        let name = relative.replace(std::path::MAIN_SEPARATOR, "/");
        let options = options
            .large_file(node.bytes() >= u64::from(u32::MAX))
            .unix_permissions(node.mode().unix_bits().ok_or(io::ErrorKind::InvalidData)?);
        if node.kind() == NodeKind::Directory {
            archive.add_directory(name, options).map_err(zip_error)?;
        } else {
            archive.start_file(name, options).map_err(zip_error)?;
            let captured = source.open_file(node.relative())?;
            let mut stream = captured.stream(0)?;
            let mut remaining = node.bytes();
            while remaining != 0 {
                check()?;
                let wanted = usize::try_from(remaining.min(buffer.len() as u64))
                    .map_err(io::Error::other)?;
                let read = stream.read(&mut buffer[..wanted])?;
                if read == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                archive.write_all(&buffer[..read])?;
                remaining -= read as u64;
            }
            if stream.read(&mut buffer[..1])? != 0 {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
    }
    if !found_root {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    archive.finish().map_err(zip_error)?;
    check()?;
    if file.metadata()?.len() > policy.copy().total_file_bytes_limit() {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    source.revalidate_content()?;
    let archive = received(owner, file, policy, original, check)?;
    source.revalidate_bindings()?;
    Ok(archive)
}

/// Retain exactly the finalized received ZIP, without rebuilding its entries.
pub(super) fn received(
    owner: TempDir,
    mut file: File,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
) -> io::Result<Archive> {
    check()?;
    original.check()?;
    if file.metadata()?.len() > policy.copy().total_file_bytes_limit() {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    // The ordinary FILE consumer rejects ambiguous names, links, special modes,
    // duplicate headers, invalid CRC and path/resource overflows here as well.
    let verified = stage_file_payload(&mut file, owner.path(), policy.copy(), original.snapshot())?;
    drop(verified);
    file.seek(SeekFrom::Start(0))?;
    let sha256 = hash_reader(&mut file, original.snapshot())?;
    file.seek(SeekFrom::Start(0))?;
    check()?;
    let reader = CapturedSourceReader::capture(
        owner.path(),
        NodeKind::Directory,
        &policy.snapshot_policy(CopyPolicy::RejectFileLinks, [])?,
        SourceReadLimits::new(64 * 1024, 64 * 1024, policy.copy().total_file_bytes_limit())?,
        Arc::clone(original.snapshot()),
    )?;
    let captured = reader.open_file(Path::new("payload.zip"))?;
    check()?;
    Ok(Archive {
        _owner: owner,
        reader,
        file: captured,
        sha256,
    })
}
