use super::{
    Entry, PackageError, PackageKind, copy_checked, envelope, invalid, source_name,
    validate_payload_entries,
};
use graphit_core::{
    artifact_store::{CapturedSourceReader, SnapshotContext},
    local_files::atomic_replace_with,
};
use graphit_fs::NodeKind;
use std::{io, path::Path};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

/// Stream a captured completed payload to an atomically replaced package file.
/// The caller rejects destinations inside its source root before capture.
pub fn export(
    source: &CapturedSourceReader,
    kind: PackageKind,
    destination: &Path,
    context: &SnapshotContext,
) -> Result<PackageKind, PackageError> {
    export_payload(source, kind, destination, context, None)
}

/// Export a completed captured graph and its Source/corpus sidecars as one
/// native AST package. The fixed archive prefix does not alter physical paths,
/// authorize access or recapture the producer's publication.
pub fn export_ast_graph(
    source: &CapturedSourceReader,
    destination: &Path,
    context: &SnapshotContext,
) -> Result<PackageKind, PackageError> {
    export_payload(
        source,
        PackageKind::Ast,
        destination,
        context,
        Some(Path::new("graph.icebug")),
    )
}

fn export_payload(
    source: &CapturedSourceReader,
    kind: PackageKind,
    destination: &Path,
    context: &SnapshotContext,
    prefix: Option<&Path>,
) -> Result<PackageKind, PackageError> {
    source.check_request(context)?;
    let mut entries = Vec::new();
    let mut root_found = false;
    for node in source.nodes() {
        context.check()?;
        if node.relative().as_os_str().is_empty() {
            if node.kind() != NodeKind::Directory {
                return Err(invalid("export source must be a directory"));
            }
            root_found = true;
            if prefix.is_none() {
                continue;
            }
        }
        let relative = prefix.map_or_else(
            || node.relative().to_owned(),
            |prefix| prefix.join(node.relative()),
        );
        let directory = node.kind() == NodeKind::Directory;
        entries.push(Entry {
            name: source_name(&relative, directory)?,
            path: relative,
            directory,
            bytes: node.bytes(),
            mode: node.mode().unix_bits().unwrap_or_default(),
        });
    }
    if !root_found {
        return Err(invalid("captured payload root is missing"));
    }
    validate_payload_entries(kind, &entries)?;
    atomic_replace_with(destination, None, context.cancellation(), |file| {
        let result = (|| -> Result<(), PackageError> {
            let mut archive = ZipWriter::new(&mut *file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            archive.start_file("package.json", options.unix_permissions(0o600))?;
            envelope::write(&mut archive, kind)?;
            for node in source
                .nodes()
                .filter(|node| prefix.is_some() || !node.relative().as_os_str().is_empty())
            {
                context.check()?;
                let directory = node.kind() == NodeKind::Directory;
                let relative = prefix.map_or_else(
                    || node.relative().to_owned(),
                    |prefix| prefix.join(node.relative()),
                );
                let name = source_name(&relative, directory)?;
                let mut options = options.large_file(node.bytes() >= u64::from(u32::MAX));
                if let Some(bits) = node.mode().unix_bits() {
                    options = options.system(zip::System::Unix).unix_permissions(bits);
                } else if let Some(readonly) = node.mode().windows_readonly() {
                    // DOS attribute bits are defined by the Windows SDK, not native Unix modes.
                    #[cfg(windows)]
                    {
                        use windows_sys::Win32::Storage::FileSystem::{
                            FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_READONLY,
                        };
                        let attributes = if directory {
                            FILE_ATTRIBUTE_DIRECTORY
                        } else {
                            0
                        } | if readonly { FILE_ATTRIBUTE_READONLY } else { 0 };
                        options = options
                            .system(zip::System::Dos)
                            .external_attributes(attributes);
                    }
                    #[cfg(not(windows))]
                    {
                        let _ = readonly;
                        return Err(invalid("captured Windows mode on a non-Windows source"));
                    }
                }
                if directory {
                    archive.add_directory(name, options)?;
                } else {
                    archive.start_file(name, options)?;
                    let captured = source.open_file(node.relative())?;
                    let count = copy_checked(
                        &mut captured.stream(0)?,
                        &mut archive,
                        node.bytes(),
                        context,
                    )?;
                    if count != node.bytes() {
                        return Err(invalid("captured file length changed"));
                    }
                }
            }
            archive.finish()?;
            source.revalidate_content()?;
            source.check_request(context)?;
            Ok(())
        })();
        result.map_err(io::Error::other)
    })?;
    Ok(kind)
}
