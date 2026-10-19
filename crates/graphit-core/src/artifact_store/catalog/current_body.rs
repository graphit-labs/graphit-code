//! Catalogue reads through the writer's retained control directory.
use super::super::{ArtifactWriter, Document, codec};
use crate::{
    native_document_file::{FileObservation, observe_handle_checked},
    private_document::{self, Access, CapturedFile, Observed},
};
use std::{
    ffi::OsStr,
    fs::File,
    io::{self, Seek},
};

const NAME: &str = "artifact-store.json";

pub(super) struct CurrentCatalogueBody<'writer> {
    captured: CapturedFile,
    writer: &'writer ArtifactWriter,
}
fn request(
    writer: &ArtifactWriter,
) -> io::Result<&std::sync::Arc<crate::file_io::SnapshotContext>> {
    writer
        .native
        .as_ref()
        .map(|native| &native.request)
        .ok_or_else(|| io::ErrorKind::InvalidInput.into())
}
fn parent(writer: &ArtifactWriter) -> io::Result<&File> {
    let native = writer.native.as_ref().ok_or(io::ErrorKind::InvalidInput)?;
    writer
        .control
        .held_native_file(&native.chain, &native.request)
}
fn open_current(writer: &ArtifactWriter) -> io::Result<File> {
    let request = request(writer)?;
    graphit_fs::write_relative::open_private_file_relative(
        parent(writer)?,
        OsStr::new(NAME),
        64 * 1024,
        &mut || request.check(),
    )
}
fn observe(writer: &ArtifactWriter, file: &File) -> io::Result<FileObservation> {
    let request = request(writer)?;
    observe_handle_checked(file, writer.store.limits.max_document_bytes, &mut || {
        request.check()
    })
}
fn access<'a>(limit: usize, check: &'a dyn Fn() -> io::Result<()>) -> Access<'a> {
    Access {
        limit,
        access_limit: limit,
        context: None,
        check,
    }
}
fn check_binding(
    writer: &ArtifactWriter,
    file: &File,
    expected: &FileObservation,
) -> io::Result<()> {
    parent(writer)?;
    if observe(writer, file)? != *expected || observe(writer, &open_current(writer)?)? != *expected
    {
        return Err(private_document::conflict());
    }
    check_parent(writer)
}
impl<'writer> CurrentCatalogueBody<'writer> {
    pub(super) fn open(writer: &'writer ArtifactWriter) -> io::Result<Option<Self>> {
        let limit = writer.store.limits.max_document_bytes;
        private_document::validate_limit(limit)?;
        let file = match open_current(writer) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                check_parent(writer)?;
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let observed = observe(writer, &file)?;
        let check = || request(writer)?.check();
        let captured = CapturedFile::from_opened(
            file,
            observed,
            &access(limit, &check),
            || {
                #[cfg(test)]
                super::test_hooks::run(
                    &super::test_hooks::AFTER_READ,
                    &writer.store.manifest_path(),
                );
            },
            |file, expected| check_binding(writer, file, expected),
        )?;
        check_parent(writer)?;
        Ok(Some(Self { captured, writer }))
    }
    pub(super) fn document(&self, writer: &ArtifactWriter) -> io::Result<Document> {
        self.check_writer(writer)?;
        request(writer)?.check()?;
        let decoded = codec::decode(&self.captured.value.bytes, &writer.store)?;
        decoded
            .validate(&writer.store)
            .map_err(|_| private_document::conflict())?;
        check_binding(writer, &self.captured.file, &self.captured.value.metadata)?;
        Ok(decoded)
    }
    fn check_writer(&self, writer: &ArtifactWriter) -> io::Result<()> {
        if std::ptr::eq(self.writer, writer) {
            Ok(())
        } else {
            Err(io::ErrorKind::InvalidInput.into())
        }
    }
    pub(super) fn revalidate(&mut self, writer: &ArtifactWriter) -> io::Result<()> {
        self.check_writer(writer)?;
        check_binding(writer, &self.captured.file, &self.captured.value.metadata)?;
        self.captured.file.rewind()?;
        let check = || request(writer)?.check();
        private_document::compare_bytes(
            &mut self.captured.file,
            &self.captured.value.bytes,
            &access(writer.store.limits.max_document_bytes, &check),
        )?;
        check_binding(writer, &self.captured.file, &self.captured.value.metadata)
    }
    pub(super) fn close(self) -> Observed {
        let CapturedFile { file, value } = self.captured;
        drop(file);
        value
    }
}
pub(super) fn check_absent(writer: &ArtifactWriter) -> io::Result<()> {
    let result = match open_current(writer) {
        Ok(_) => Err(private_document::conflict()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    };
    result.and(check_parent(writer))
}
pub(super) fn check_parent(writer: &ArtifactWriter) -> io::Result<()> {
    parent(writer)?;
    request(writer)?.check()
}
