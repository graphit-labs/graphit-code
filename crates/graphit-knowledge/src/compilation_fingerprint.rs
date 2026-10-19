//! A bounded fingerprint of an admitted authored-document compilation.
//!
//! This is recipe data, not a capture grant, freshness witness or publication
//! pin. The retained source and metadata remain with their existing owners.

use crate::publication_metadata::CompiledDocument;
use graphit_core::file_io::SnapshotContext;
use sha2::{Digest, Sha256};
use std::{error::Error, fmt, io, io::Write, num::NonZeroUsize, sync::Arc};

const DOMAIN: &[u8] = b"graphit.knowledge.compilation-fingerprint\0";
const RECIPE: &str = "authored-document-v1";
const COMPILATION_RULES: [&str; 6] = [
    "title-explicit-h1-fallback-trim.v1",
    "description-yaml-text-integral.v1",
    "kind-explicit-or-supplied.v1",
    "summary-editorial-prefix-utf8.v1",
    "serde-saphyr@1.3.0",
    "pulldown-cmark@0.13.4",
];

#[derive(Clone, Copy, Debug)]
pub struct CompilationFingerprintLimits {
    /// Complete emitted preimage, including domain, NUL and JSON escapes.
    pub max_preimage_bytes: NonZeroUsize,
}

/// The private constructor accepts only a completed recipe calculation.
#[derive(Eq, PartialEq)]
pub struct CompilationFingerprint([u8; 32]);
impl CompilationFingerprint {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
impl fmt::Debug for CompilationFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CompilationFingerprint(..)")
    }
}

#[derive(Debug)]
pub enum CompilationFingerprintError {
    OriginalRequest(io::Error),
    Json(serde_json::Error),
    Limit(&'static str),
    Overflow(&'static str),
}
impl fmt::Display for CompilationFingerprintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OriginalRequest(_) => {
                formatter.write_str("original captured-source request failed")
            }
            Self::Json(_) => formatter.write_str("compiled-document JSON encoding failed"),
            Self::Limit(label) => write!(formatter, "compilation fingerprint exceeded {label}"),
            Self::Overflow(label) => {
                write!(formatter, "compilation fingerprint overflowed {label}")
            }
        }
    }
}
impl Error for CompilationFingerprintError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::OriginalRequest(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Limit(_) | Self::Overflow(_) => None,
        }
    }
}
type Result<T> = std::result::Result<T, CompilationFingerprintError>;
type CompilationRecipe<'a> = (
    u8,
    &'static str,
    [&'static str; 6],
    (&'a str, &'a str, u64),
    (&'a str, &'a str, &'a str, Option<&'a str>, &'a str, bool),
);

fn recipe<'a>(compiled: &'a CompiledDocument<'_>) -> CompilationRecipe<'a> {
    let policy = compiled.recipe_policy();
    (
        1,
        RECIPE,
        COMPILATION_RULES,
        (
            &policy.fallback_title,
            &policy.kind_on_absent,
            compiled.summary_limit(),
        ),
        (
            compiled.whole_source(),
            compiled.title(),
            compiled.kind(),
            compiled.description(),
            compiled.summary(),
            compiled.summary_truncated(),
        ),
    )
}

/// Hash the fixed recipe with serde_json's compact UTF-8 encoder and SHA-256.
///
/// Request checks preserve the original capture binding and cancellation; they
/// do not revalidate physical files. No full preimage or text copy is allocated.
/// A terminal SDK writing error can allocate its O(1) error container; the text
/// limit is not an allocator quota or a hard RSS/OOM guarantee.
pub fn fingerprint_compiled(
    request: &Arc<SnapshotContext>,
    compiled: &CompiledDocument<'_>,
    limits: CompilationFingerprintLimits,
) -> Result<CompilationFingerprint> {
    compiled
        .check_request(request)
        .map_err(CompilationFingerprintError::OriginalRequest)?;
    let mut writer = FingerprintWriter::new(request, compiled, limits);
    writer
        .write_all(DOMAIN)
        .map_err(|_| writer.take_failure())?;
    compiled
        .check_request(request)
        .map_err(CompilationFingerprintError::OriginalRequest)?;
    let encoded = serde_json::to_writer(&mut writer, &recipe(compiled));
    writer.finish(encoded)
}

fn checked_emitted(current: usize, additional: usize, limit: usize) -> Result<usize> {
    let next = current
        .checked_add(additional)
        .ok_or(CompilationFingerprintError::Overflow("preimage bytes"))?;
    if next > limit {
        return Err(CompilationFingerprintError::Limit("preimage bytes"));
    }
    Ok(next)
}

struct FingerprintWriter<'a, 'source> {
    request: &'a Arc<SnapshotContext>,
    compiled: &'a CompiledDocument<'source>,
    limit: usize,
    emitted: usize,
    hasher: Sha256,
    first_failure: Option<CompilationFingerprintError>,
}
impl<'a, 'source> FingerprintWriter<'a, 'source> {
    fn new(
        request: &'a Arc<SnapshotContext>,
        compiled: &'a CompiledDocument<'source>,
        limits: CompilationFingerprintLimits,
    ) -> Self {
        Self {
            request,
            compiled,
            limit: limits.max_preimage_bytes.get(),
            emitted: 0,
            hasher: Sha256::new(),
            first_failure: None,
        }
    }

    fn transport(error: &CompilationFingerprintError) -> io::Error {
        let kind = match error {
            CompilationFingerprintError::OriginalRequest(error) => error.kind(),
            _ => io::ErrorKind::InvalidData,
        };
        io::Error::from(kind)
    }

    fn fail(&mut self, error: CompilationFingerprintError) -> io::Error {
        let first = self.first_failure.get_or_insert(error);
        Self::transport(first)
    }

    fn take_failure(&mut self) -> CompilationFingerprintError {
        // Every error returned by this private writer records its original
        // failure first. Callers cannot supply a writer or a Serialize callback.
        self.first_failure
            .take()
            .expect("writer errors record their first failure")
    }

    fn controlled_write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(error) = &self.first_failure {
            return Err(Self::transport(error));
        }
        if let Err(error) = self.compiled.check_request(self.request) {
            return Err(self.fail(CompilationFingerprintError::OriginalRequest(error)));
        }
        let next = match checked_emitted(self.emitted, bytes.len(), self.limit) {
            Ok(next) => next,
            Err(error) => return Err(self.fail(error)),
        };
        if bytes.is_empty() {
            return Ok(0);
        }
        self.hasher.update(bytes);
        self.emitted = next;
        Ok(bytes.len())
    }

    fn finish(mut self, encoded: serde_json::Result<()>) -> Result<CompilationFingerprint> {
        // The real owned error wins over the SDK's transport and later context
        // changes. Dropping `encoded` also drops its terminal error container.
        if let Some(error) = self.first_failure.take() {
            return Err(error);
        }
        encoded.map_err(CompilationFingerprintError::Json)?;
        let bytes = self.hasher.finalize().into();
        self.compiled
            .check_request(self.request)
            .map_err(CompilationFingerprintError::OriginalRequest)?;
        Ok(CompilationFingerprint(bytes))
    }
}
impl Write for FingerprintWriter<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.controlled_write(bytes)
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        // std's default retries Interrupted and skips empty buffers. Both would
        // violate original cancellation and the controlled writer contract.
        self.controlled_write(bytes).map(|_| ())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(error) = &self.first_failure {
            return Err(Self::transport(error));
        }
        self.compiled
            .check_request(self.request)
            .map_err(|error| self.fail(CompilationFingerprintError::OriginalRequest(error)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::DocumentParseLimits,
        publication_metadata::{
            PublicationMetadataLimits, PublicationMetadataPolicy, compile_captured_document,
        },
    };
    use graphit_core::{
        artifact_store::{
            CapturedPublicationScope, CapturedSourceReader, CopyLimits, CopyPolicy,
            RetainedBytesLimits, RetainedFileSet, ScopeLimits, SnapshotPolicy, SourceReadLimits,
        },
        brand::Brand,
        file_io::SnapshotContext,
        file_lock::Cancellation,
        project_paths::{InstallationPaths, ProjectPaths},
    };
    use graphit_fs::NodeKind;
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
        time::{Duration, Instant},
    };

    const SOURCE: &[u8] = b"# H\n\nbody\n";

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            for _ in 0..16 {
                let nonce = NEXT
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                    .unwrap();
                let path = std::env::temp_dir().join(format!(
                    "graphit-knowledge-fp-unit-{}-{nonce}",
                    std::process::id()
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("create fixture directory: {error}"),
                }
            }
            panic!("fixture directory collision limit exceeded");
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            if let Err(error) = fs::remove_dir_all(&self.0) {
                if std::thread::panicking() {
                    eprintln!("fixture cleanup during unwinding failed: {error}");
                } else {
                    panic!("fixture cleanup failed: {error}");
                }
            }
        }
    }

    struct Fixture {
        // Native owners drop before directory cleanup, including on unwinding.
        reader: CapturedSourceReader,
        request: Arc<SnapshotContext>,
        _scope: CapturedPublicationScope,
        _paths: ProjectPaths,
        _directory: Directory,
    }
    impl Fixture {
        fn new() -> Self {
            let directory = Directory::new();
            let root = directory.0.join("project");
            fs::create_dir_all(root.join(".graphit")).unwrap();
            fs::write(root.join("document.md"), SOURCE).unwrap();
            let installation = InstallationPaths::resolve(
                Brand::default(),
                &directory.0,
                None,
                Some(&directory.0.join("global")),
            )
            .unwrap();
            let paths = ProjectPaths::at(&root, &installation).unwrap();
            let context = SnapshotContext::new(
                Instant::now() + Duration::from_secs(30),
                Cancellation::default(),
            );
            let request = Arc::new(context);
            let scope = CapturedPublicationScope::project(
                &paths,
                "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
                ScopeLimits::new(4, 4096, 256).unwrap(),
                &request,
            )
            .unwrap();
            let retained = RetainedBytesLimits::new(SOURCE.len(), 1, 1).unwrap();
            let policy = SnapshotPolicy::new(
                CopyPolicy::RejectFileLinks,
                CopyLimits::new(SOURCE.len() as u64, 1, 1, 1).unwrap(),
                RetainedFileSet::new([PathBuf::new()], retained).unwrap(),
                retained,
            )
            .unwrap();
            let reader = CapturedSourceReader::capture_project(
                &scope,
                Path::new("document.md"),
                NodeKind::File,
                &policy,
                SourceReadLimits::new(SOURCE.len(), 4096, SOURCE.len() as u64).unwrap(),
                &request,
            )
            .unwrap();
            Self {
                reader,
                request,
                _scope: scope,
                _paths: paths,
                _directory: directory,
            }
        }
        fn compile(&self) -> CompiledDocument<'_> {
            let nz = |n| NonZeroUsize::new(n).unwrap();
            let parse = DocumentParseLimits {
                max_document_bytes: nz(SOURCE.len()),
                max_input_bytes: nz(4096),
                max_frontmatter_bytes: nz(4096),
                max_yaml_events: nz(100),
                max_markdown_events: nz(100),
                max_yaml_depth: nz(10),
                max_scalar_bytes: nz(4096),
                max_links: nz(10),
                max_projection_bytes: nz(10_000),
                max_summary_bytes: nz(32),
            };
            let metadata = PublicationMetadataLimits {
                max_input_bytes: nz(4096),
                max_title_bytes: nz(1024),
                max_kind_bytes: nz(1024),
                max_description_bytes: nz(1024),
                max_effective_summary_bytes: nz(1024),
                max_effective_metadata_bytes: nz(4096),
                max_owned_metadata_bytes: nz(4096),
                max_root_entries: nz(100),
                max_typed_nodes: nz(100),
                max_typed_depth: nz(10),
                max_typed_scalar_bytes: nz(4096),
                max_typed_events: nz(200),
                max_typed_lookahead_characters: nz(1024),
                max_typed_flow_depth: nz(10),
            };
            compile_captured_document(
                &self.reader,
                Path::new(""),
                &self.request,
                PublicationMetadataPolicy {
                    fallback_title: "F".into(),
                    kind_on_absent: "guide".into(),
                },
                parse,
                metadata,
            )
            .unwrap()
        }
    }

    fn limits(bytes: usize) -> CompilationFingerprintLimits {
        CompilationFingerprintLimits {
            max_preimage_bytes: NonZeroUsize::new(bytes).unwrap(),
        }
    }
    fn io_kind(error: &CompilationFingerprintError) -> io::ErrorKind {
        match error {
            CompilationFingerprintError::OriginalRequest(error) => error.kind(),
            other => panic!("expected original request error: {other:?}"),
        }
    }

    #[test]
    fn sdk_cancelled_writer_terminates_with_original_interrupted() {
        let fixture = Fixture::new();
        let compiled = fixture.compile();
        let mut writer = FingerprintWriter::new(&fixture.request, &compiled, limits(319));
        let before = writer.hasher.clone().finalize();
        fixture.request.cancellation().cancel();
        let encoded = serde_json::to_writer(&mut writer, &recipe(&compiled));
        assert_eq!(
            encoded.as_ref().unwrap_err().io_error_kind(),
            Some(io::ErrorKind::Interrupted)
        );
        assert_eq!(writer.emitted, 0);
        assert_eq!(writer.hasher.clone().finalize(), before);
        let error = writer.finish(encoded).unwrap_err();
        assert_eq!(io_kind(&error), io::ErrorKind::Interrupted);
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(fixture.reader.retained(Path::new("")).unwrap(), SOURCE);
    }

    #[test]
    fn empty_writes_and_flush_check_context_without_mutation() {
        let fixture = Fixture::new();
        let compiled = fixture.compile();
        let mut writer = FingerprintWriter::new(&fixture.request, &compiled, limits(319));
        let before = writer.hasher.clone().finalize();
        assert_eq!(writer.write(b"").unwrap(), 0);
        writer.write_all(b"").unwrap();
        writer.flush().unwrap();
        assert_eq!(writer.emitted, 0);
        assert_eq!(writer.hasher.clone().finalize(), before);
        writer.write_all(b"").unwrap();
        writer.flush().unwrap();
        fixture.request.cancellation().cancel();
        assert_eq!(
            writer.write_all(b"").unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(
            writer.write_all(b"x").unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(
            writer.flush().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(writer.emitted, 0);
        assert_eq!(writer.hasher.clone().finalize(), before);
        assert_eq!(
            io_kind(&writer.finish(Ok(())).unwrap_err()),
            io::ErrorKind::Interrupted
        );
    }

    #[test]
    fn admitted_write_then_cancel_preserves_hash_and_rejects_every_next_operation() {
        let fixture = Fixture::new();
        let compiled = fixture.compile();
        let mut writer = FingerprintWriter::new(&fixture.request, &compiled, limits(319));
        writer.write_all(DOMAIN).unwrap();
        let before = writer.hasher.clone().finalize();
        fixture.request.cancellation().cancel();
        assert_eq!(
            writer.write_all(b"[").unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(
            writer.write_all(b"").unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(
            writer.flush().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(writer.emitted, DOMAIN.len());
        assert_eq!(writer.hasher.clone().finalize(), before);
        assert_eq!(
            io_kind(&writer.finish(Ok(())).unwrap_err()),
            io::ErrorKind::Interrupted
        );
    }

    #[test]
    fn local_limit_poisons_writer_and_wins_over_later_sdk_and_cancellation() {
        let fixture = Fixture::new();
        let compiled = fixture.compile();
        let mut writer = FingerprintWriter::new(&fixture.request, &compiled, limits(52));
        writer.write_all(DOMAIN).unwrap();
        let encoded = serde_json::to_writer(&mut writer, &recipe(&compiled));
        assert_eq!(
            encoded.as_ref().unwrap_err().io_error_kind(),
            Some(io::ErrorKind::InvalidData)
        );
        // The SDK's denied string chunk leaves enough room for a smaller write,
        // which must still fail because the operation has already been poisoned.
        assert!(writer.emitted < 52);
        let emitted = writer.emitted;
        let before = writer.hasher.clone().finalize();
        assert_eq!(
            writer.write_all(b"x").unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        fixture.request.cancellation().cancel();
        assert_eq!(
            writer.write_all(b"").unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            writer.flush().unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(writer.emitted, emitted);
        assert_eq!(writer.hasher.clone().finalize(), before);
        assert!(matches!(
            writer.finish(encoded).unwrap_err(),
            CompilationFingerprintError::Limit("preimage bytes")
        ));
    }

    #[test]
    fn denied_chunk_does_not_mutate_hash_or_byte_count() {
        let fixture = Fixture::new();
        let compiled = fixture.compile();
        let mut writer = FingerprintWriter::new(&fixture.request, &compiled, limits(42));
        writer.write_all(DOMAIN).unwrap();
        let before = writer.hasher.clone().finalize();
        assert!(writer.write_all(b"x").is_err());
        assert!(writer.write_all(b"").is_err());
        assert_eq!(writer.emitted, DOMAIN.len());
        assert_eq!(writer.hasher.clone().finalize(), before);
        assert!(matches!(
            writer.finish(Ok(())).unwrap_err(),
            CompilationFingerprintError::Limit("preimage bytes")
        ));
    }

    #[test]
    fn final_original_check_suppresses_a_successfully_encoded_candidate() {
        let fixture = Fixture::new();
        let compiled = fixture.compile();
        let mut writer = FingerprintWriter::new(&fixture.request, &compiled, limits(319));
        writer.write_all(DOMAIN).unwrap();
        let encoded = serde_json::to_writer(&mut writer, &recipe(&compiled));
        assert!(encoded.is_ok());
        assert_eq!(writer.emitted, 319);
        fixture.request.cancellation().cancel();
        assert_eq!(
            io_kind(&writer.finish(encoded).unwrap_err()),
            io::ErrorKind::Interrupted
        );
        assert_eq!(compiled.whole_source().as_bytes(), SOURCE);
    }

    #[test]
    fn emitted_byte_bounds_reject_overflow_and_limit_before_mutation() {
        assert!(matches!(
            checked_emitted(usize::MAX, 1, usize::MAX),
            Err(CompilationFingerprintError::Overflow("preimage bytes"))
        ));
        assert_eq!(checked_emitted(41, 1, 42).unwrap(), 42);
        assert!(matches!(
            checked_emitted(42, 1, 42),
            Err(CompilationFingerprintError::Limit("preimage bytes"))
        ));
        assert_eq!(checked_emitted(42, 0, 42).unwrap(), 42);
    }
}
