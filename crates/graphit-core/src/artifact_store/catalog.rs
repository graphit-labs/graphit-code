//! Private catalog persistence. In-process scratch origin is not a durable Stage grant.
use super::{
    ArtifactResult, ArtifactStage, ArtifactStore, ArtifactWriter, Document, at, codec, control,
};
use crate::{
    native_document_file::FileObservation,
    native_file_identity::PhysicalId,
    private_document::{
        self, Access, CapturedFile, CreatedScratch as PrivateScratch, Observed,
        replacement_identity,
    },
};
use std::{
    fs::File,
    io::{self, Seek, Write},
    path::Path,
};

fn access(limit: usize) -> Access<'static> {
    Access {
        limit,
        access_limit: limit,
        context: None,
        check: &|| Ok(()),
    }
}
fn open(path: &Path, limit: usize) -> io::Result<Option<CapturedFile>> {
    CapturedFile::open(path, &access(limit), || {
        #[cfg(test)]
        test_hooks::run(&test_hooks::AFTER_READ, path);
    })
}
impl CapturedFile {
    fn document(&self, store: &ArtifactStore) -> io::Result<Document> {
        let decoded = codec::decode(&self.value.bytes, store)?;
        decoded.validate(store).map_err(|_| conflict())?;
        if observe(
            &store.manifest_path(),
            &self.file,
            store.limits.max_document_bytes,
        )? != self.value.metadata
        {
            return Err(conflict());
        }
        Ok(decoded)
    }
}

pub(super) use crate::private_document::validate_limit;

pub(super) fn read(store: &ArtifactStore) -> io::Result<Option<Document>> {
    let limit = store.limits.max_document_bytes;
    validate_limit(limit)?;
    open(&store.manifest_path(), limit)?
        .map(|captured| captured.document(store))
        .transpose()
}

/// Only a physical read constructs this base; it grants no scratch authority.
/// Its exact old handle survives until the shared publisher closes it.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod current_body;

pub(super) struct LoadedCatalog<'writer> {
    captured: Option<Baseline<'writer>>,
    pub(super) document: Document,
    writer: &'writer ArtifactWriter,
}
impl LoadedCatalog<'_> {
    pub(super) fn validate_writer(&self, writer: &ArtifactWriter) -> io::Result<()> {
        if std::ptr::eq(self.writer, writer) {
            Ok(())
        } else {
            Err(io::ErrorKind::InvalidInput.into())
        }
    }
}

/// Physical provenance is selected before opening, never tagged after a read.
enum Baseline<'writer> {
    Ordinary {
        value: CapturedFile,
        _writer: &'writer ArtifactWriter,
    },
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    Native(current_body::CurrentCatalogueBody<'writer>),
}
impl<'writer> Baseline<'writer> {
    fn document(&self, writer: &ArtifactWriter) -> io::Result<Document> {
        match self {
            Self::Ordinary { value, .. } => value.document(&writer.store),
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            Self::Native(body) => body.document(writer),
        }
    }
    fn revalidate(&mut self, writer: &ArtifactWriter) -> io::Result<()> {
        match self {
            Self::Ordinary { value, .. } => value.revalidate(
                &writer.store.manifest_path(),
                &access(writer.store.limits.max_document_bytes),
            ),
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            Self::Native(body) => body.revalidate(writer),
        }
    }
    fn close(self) -> Observed {
        match self {
            Self::Ordinary { value, .. } => {
                let CapturedFile { file, value } = value;
                drop(file);
                value
            }
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            Self::Native(body) => body.close(),
        }
    }
}

pub(super) fn current_boundary(writer: &ArtifactWriter) -> io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    if writer.native.is_some() {
        return current_body::check_parent(writer);
    }
    writer.control.revalidate(&writer.store.scope, None)
}
fn load_baseline(writer: &ArtifactWriter) -> io::Result<Option<Baseline<'_>>> {
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    if writer.native.is_some() {
        return current_body::CurrentCatalogueBody::open(writer)
            .map(|body| body.map(Baseline::Native));
    }
    open(
        &writer.store.manifest_path(),
        writer.store.limits.max_document_bytes,
    )
    .map(|captured| {
        captured.map(|body| Baseline::Ordinary {
            value: body,
            _writer: writer,
        })
    })
}
fn absent_baseline(writer: &ArtifactWriter) -> io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    if writer.native.is_some() {
        return current_body::check_absent(writer);
    }
    verify_closed(
        &writer.store.manifest_path(),
        None,
        writer.store.limits.max_document_bytes,
    )
}
pub(super) fn read_loaded(writer: &ArtifactWriter) -> io::Result<LoadedCatalog<'_>> {
    let store = &writer.store;
    validate_limit(store.limits.max_document_bytes)?;
    let captured = load_baseline(writer)?;
    let document = match &captured {
        Some(value) => value.document(writer)?,
        None => Document::empty(store)?,
    };
    current_boundary(writer)?;
    Ok(LoadedCatalog {
        document,
        captured,
        writer,
    })
}

fn observe(path: &Path, file: &File, access_limit: usize) -> io::Result<FileObservation> {
    private_document::observe(path, file, &access(access_limit))
}
fn compare_bytes(file: &mut File, bytes: &[u8]) -> io::Result<()> {
    private_document::compare_bytes(file, bytes, &access(bytes.len().max(1)))
}
// Only the shared SDK origin constructor creates this private wrapper.
struct CreatedScratch(PrivateScratch);
impl CreatedScratch {
    fn path(&self) -> &Path {
        self.0.path()
    }
    fn identity(&self) -> &PhysicalId {
        self.0.identity()
    }
    fn create(writer: &ArtifactWriter) -> io::Result<(Self, File)> {
        let path = private_document::random_path(writer.store.runtime_root(), ".artifact-store-")?;
        let (scratch, file) = PrivateScratch::create(
            path,
            &|| writer.control.revalidate(&writer.store.scope, None),
            create_file,
        )?;
        Ok((Self(scratch), file))
    }
    fn verify(&self, writer: &ArtifactWriter, file: &File) -> io::Result<()> {
        self.0.verify(file, &|| {
            writer.control.revalidate(&writer.store.scope, None)
        })
    }
    fn cleanup(&self, writer: &ArtifactWriter) -> io::Result<()> {
        #[cfg(test)]
        test_hooks::run(&test_hooks::BEFORE_CLEANUP, self.path());
        self.0
            .cleanup(&|| writer.control.revalidate(&writer.store.scope, None))
    }
}

pub(super) fn save(
    writer: &ArtifactWriter,
    document: &Document,
    stage: ArtifactStage,
) -> ArtifactResult<()> {
    let limit = writer.store.limits.max_document_bytes;
    validate_limit(limit).map_err(|e| at(stage, e))?;
    let old = load_baseline(writer).map_err(|e| at(stage, e))?;
    if let Some(old) = &old {
        old.document(writer).map_err(|e| at(stage, e))?;
    }
    save_from_base(writer, document, old, stage)
}

/// The already decoded base cannot silently adopt a newly observed catalog.
pub(super) fn save_loaded(
    writer: &ArtifactWriter,
    loaded: LoadedCatalog<'_>,
    scope: &super::ScopeBindingRecord,
    pending: Option<&super::native_document::Operation>,
    stage: ArtifactStage,
) -> ArtifactResult<()> {
    loaded.validate_writer(writer).map_err(|e| at(stage, e))?;
    let LoadedCatalog {
        document,
        mut captured,
        writer: _,
    } = loaded;
    let projection = codec::DocumentProjection::new(&document, scope, pending);
    let limit = writer.store.limits.max_document_bytes;
    validate_limit(limit).map_err(|e| at(stage, e))?;
    match &mut captured {
        Some(old) => old.revalidate(writer),
        None => absent_baseline(writer),
    }
    .map_err(|e| at(stage, e))?;
    save_from_base(writer, &projection, captured, stage)
}

fn save_from_base(
    writer: &ArtifactWriter,
    document: &(impl serde::Serialize + ?Sized),
    mut old: Option<Baseline<'_>>,
    stage: ArtifactStage,
) -> ArtifactResult<()> {
    let destination = writer.store.manifest_path();
    let (scratch, mut file) = CreatedScratch::create(writer).map_err(|e| at(stage, e))?;
    let mut encoded = None;
    let prepared = (|| {
        #[cfg(test)]
        test_hooks::run(&test_hooks::AFTER_BIRTH, scratch.path());
        scratch.verify(writer, &file)?;
        let output = encoded.insert(codec::encode_for_writer(document, writer)?);
        let bytes = output.as_slice();
        file.write_all(bytes)?;
        sync(&file)?;
        scratch.verify(writer, &file)?;
        file.rewind()?;
        compare_bytes(&mut file, bytes)?;
        #[cfg(test)]
        test_hooks::run(&test_hooks::AFTER_COMPARE, scratch.path());
        scratch.verify(writer, &file)?;
        if let Some(old) = &mut old {
            old.revalidate(writer)?;
        }
        Ok(())
    })();
    drop(file);
    let old = old.map(Baseline::close);
    let result = match prepared {
        Err(e) => Err(at(stage, e)),
        Ok(()) => match &encoded {
            Some(output) => publish(
                writer,
                &scratch,
                &destination,
                old.as_ref(),
                output.as_slice(),
                stage,
            ),
            None => Err(at(stage, conflict())),
        },
    };
    let result = control::checked(result, scratch.cleanup(writer), stage);
    // Keep encoded bytes alive until scratch cleanup completes.
    drop(encoded);
    result
}

fn publish(
    writer: &ArtifactWriter,
    scratch: &CreatedScratch,
    destination: &Path,
    old: Option<&Observed>,
    bytes: &[u8],
    stage: ArtifactStage,
) -> ArtifactResult<()> {
    let limit = writer.store.limits.max_document_bytes;
    // Reopen/check after incompatible handles close, then close these too.
    #[cfg(test)]
    test_hooks::run(&test_hooks::AFTER_CLOSE, scratch.path());
    verify_closed(destination, old, limit).map_err(|e| at(stage, e))?;
    let file = crate::native_document_file::open_file(scratch.path()).map_err(|e| at(stage, e))?;
    scratch.verify(writer, &file).map_err(|e| at(stage, e))?;
    let mut file = file;
    compare_bytes(&mut file, bytes).map_err(|e| at(stage, e))?;
    scratch.verify(writer, &file).map_err(|e| at(stage, e))?;
    drop(file);
    current_boundary(writer).map_err(|e| at(stage, e))?;
    let expected = replacement_identity(scratch.identity(), old.map(|o| &o.metadata.identity))
        .map_err(|e| at(stage, e))?;
    #[cfg(test)]
    test_hooks::run(&test_hooks::BEFORE_NATIVE, scratch.path());
    // Cooperative lock and final comparisons detect observed changes, not CAS.
    let native = replace(scratch.path(), destination, old.is_some());
    #[cfg(test)]
    test_hooks::run(&test_hooks::AFTER_NATIVE, scratch.path());
    let observed = (|| {
        writer.control.revalidate(&writer.store.scope, None)?;
        observe_effect(destination, &expected, bytes, old, native.is_err(), limit)
    })();
    control::checked(native.map_err(|e| at(stage, e)), observed, stage)
}

fn verify_closed(path: &Path, expected: Option<&Observed>, limit: usize) -> io::Result<()> {
    private_document::verify_closed(path, expected, &access(limit))
}
fn observe_effect(
    path: &Path,
    expected: &PhysicalId,
    bytes: &[u8],
    old: Option<&Observed>,
    native_failed: bool,
    limit: usize,
) -> io::Result<()> {
    private_document::observe_effect(
        path,
        private_document::ExpectedEffect {
            identity: expected,
            bytes,
            old,
            native_failed,
            retain: false,
        },
        &access(limit),
    )
    .map(|_| ())
}

#[cfg(test)]
#[test]
fn windows_catalog_replacement_identity_preserves_old_creation_without_weakening_equality() {
    let stage = PhysicalId::Windows {
        volume: 7,
        index: [3; 16],
        created: 11,
    };
    let old = PhysicalId::Windows {
        volume: 7,
        index: [2; 16],
        created: 5,
    };
    let expected = replacement_identity(&stage, Some(&old)).unwrap();
    assert!(
        expected
            == PhysicalId::Windows {
                volume: 7,
                index: [3; 16],
                created: 5
            }
    );
    assert!(expected != stage && expected != old);
    assert!(replacement_identity(&stage, None).unwrap() == stage);
    let foreign_volume = PhysicalId::Windows {
        volume: 8,
        index: [2; 16],
        created: 5,
    };
    assert_eq!(
        replacement_identity(&stage, Some(&foreign_volume))
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
}

fn replace(stage: &Path, destination: &Path, exists: bool) -> io::Result<()> {
    #[cfg(test)]
    if let Some(replace) = test_hooks::NATIVE.with(|h| h.borrow_mut().take()) {
        return replace(stage, destination, exists);
    }
    private_document::replace(stage, destination, exists)
}

fn conflict() -> io::Error {
    io::ErrorKind::InvalidData.into()
}

fn create_file(path: &Path) -> io::Result<File> {
    #[cfg(test)]
    if let Some(create) = test_hooks::CREATE.with(|h| h.borrow_mut().take()) {
        return create(path);
    }
    private_document::create_file(path)
}

fn sync(file: &File) -> io::Result<()> {
    #[cfg(test)]
    if let Some(sync) = test_hooks::SYNC.with(|h| h.borrow_mut().take()) {
        return sync(file);
    }
    file.sync_all()
}

#[cfg(test)]
pub(super) mod test_hooks {
    use super::*;
    use std::{cell::RefCell, thread::LocalKey};
    type Hook = RefCell<Option<Box<dyn FnOnce(&Path)>>>;
    type Native = RefCell<Option<Box<dyn FnOnce(&Path, &Path, bool) -> io::Result<()>>>>;
    type Create = RefCell<Option<Box<dyn FnOnce(&Path) -> io::Result<File>>>>;
    type Sync = RefCell<Option<Box<dyn FnOnce(&File) -> io::Result<()>>>>;
    thread_local! {
        pub(in crate::artifact_store) static AFTER_READ: Hook = const { RefCell::new(None) };
        pub(in crate::artifact_store) static AFTER_BIRTH: Hook = const { RefCell::new(None) };
        pub(in crate::artifact_store) static BEFORE_NATIVE: Hook = const { RefCell::new(None) };
        pub(in crate::artifact_store) static AFTER_NATIVE: Hook = const { RefCell::new(None) };
        pub(in crate::artifact_store) static NATIVE: Native = const { RefCell::new(None) };
        pub(in crate::artifact_store) static CREATE: Create = const { RefCell::new(None) };
        pub(in crate::artifact_store) static SYNC: Sync = const { RefCell::new(None) };
        pub(in crate::artifact_store) static AFTER_COMPARE: Hook = const { RefCell::new(None) };
        pub(in crate::artifact_store) static AFTER_CLOSE: Hook = const { RefCell::new(None) };
        pub(in crate::artifact_store) static BEFORE_CLEANUP: Hook = const { RefCell::new(None) };
    }
    pub(in crate::artifact_store) fn run(hook: &'static LocalKey<Hook>, path: &Path) {
        if let Some(action) = hook.with(|h| h.borrow_mut().take()) {
            action(path);
        }
    }
}
