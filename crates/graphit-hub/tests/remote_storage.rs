use graphit_core::{
    file_io::SnapshotContext, file_lock::Cancellation, project_identity::ProjectId,
};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, ProjectionLimits,
    remote_storage::{hash_reader, registry_key, release_prefix},
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    io::{self, Cursor, Read, Seek, SeekFrom, Write},
    time::{Duration, Instant},
};
use zip::{ZipWriter, write::SimpleFileOptions};

fn limits() -> ProjectionLimits {
    ProjectionLimits::new(4096, 32 * 1024, 128, 8192, 128).unwrap()
}
fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(10),
        Cancellation::default(),
    )
}
fn producer() -> ProjectId {
    "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap()
}

#[test]
fn keys_preserve_full_identity_and_literal_version() {
    let id = ArtifactId::new("pkg/x", limits()).unwrap();
    let version = ConcreteVersion::new("branch/x", limits()).unwrap();
    assert_eq!(
        registry_key(producer(), ArtifactType::Skill, &id).unwrap(),
        "v1/projects/01KSH1CRFFG8Z74B5ZS78WW808/registry/skill/cGtnL3g.json"
    );
    assert_eq!(
        release_prefix(producer(), ArtifactType::Skill, &id, &version, &[0; 32]).unwrap(),
        format!(
            "v1/projects/01KSH1CRFFG8Z74B5ZS78WW808/artifacts/skill/cGtnL3g/YnJhbmNoL3g/{}/",
            "00".repeat(32)
        )
    );
    let producers = [producer(), "01M1YEAE1WYMWG9KJKV1XQC4T4".parse().unwrap()];
    let mut keys = HashSet::new();
    for project in producers {
        for kind in [ArtifactType::Skill, ArtifactType::Command] {
            for name in ["pkg/x", "other/x", "é/x", "e\u{301}/x"] {
                let id = ArtifactId::new(name, limits()).unwrap();
                assert!(keys.insert(registry_key(project, kind, &id).unwrap()));
                for version in ["branch/x", "branch/Δ", "1.0.0"] {
                    let version = ConcreteVersion::new(version, limits()).unwrap();
                    for digest in [[0; 32], [255; 32]] {
                        assert!(keys.insert(
                            release_prefix(project, kind, &id, &version, &digest).unwrap()
                        ));
                    }
                }
            }
        }
    }
}

#[test]
fn backend_key_limit_is_enforced() {
    let mut longest = 0;
    for length in 680..810 {
        let text = "a".repeat(length);
        let id = text
            .as_bytes()
            .chunks(200)
            .map(|part| std::str::from_utf8(part).unwrap())
            .collect::<Vec<_>>()
            .join("/");
        let id = ArtifactId::new(&id, limits()).unwrap();
        for kind in ArtifactType::ALL {
            match registry_key(producer(), kind, &id) {
                Ok(key) => {
                    assert!(key.len() <= 1024);
                    longest = longest.max(key.len());
                }
                Err(error) => assert_eq!(error.kind(), io::ErrorKind::InvalidInput),
            }
        }
    }
    assert_eq!(longest, 1024);
    let id = ArtifactId::new("pkg/x", limits()).unwrap();
    let version = ConcreteVersion::new(&"v".repeat(800), limits()).unwrap();
    assert_eq!(
        release_prefix(producer(), ArtifactType::Skill, &id, &version, &[0; 32])
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn finalized_zip_hash_covers_headers_and_all_bytes() {
    let mut file = tempfile::tempfile().unwrap();
    {
        let mut archive = ZipWriter::new(&mut file);
        archive
            .start_file("SKILL.md", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"# Skill\nA real payload.\n").unwrap();
        archive.finish().unwrap();
    }
    file.seek(SeekFrom::Start(0)).unwrap();
    let digest = hash_reader(&mut file, &context()).unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    assert_eq!(digest, <[u8; 32]>::from(Sha256::digest(&bytes)));
    let length = bytes.len();
    assert_ne!(
        digest,
        hash_reader(&mut Cursor::new(&bytes[..length - 1]), &context()).unwrap()
    );
    bytes[0] ^= 1;
    assert_ne!(
        digest,
        hash_reader(&mut Cursor::new(bytes), &context()).unwrap()
    );
}

struct ShortReader {
    cursor: Cursor<Vec<u8>>,
    interrupted: bool,
}
impl Read for ShortReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if !self.interrupted {
            self.interrupted = true;
            return Err(io::ErrorKind::Interrupted.into());
        }
        let size = buffer.len().min(7);
        self.cursor.read(&mut buffer[..size])
    }
}
#[test]
fn hash_handles_short_reads_and_interruption() {
    let bytes = vec![42; 140_000];
    let expected: [u8; 32] = Sha256::digest(&bytes).into();
    assert_eq!(
        hash_reader(
            &mut ShortReader {
                cursor: Cursor::new(bytes),
                interrupted: false
            },
            &context()
        )
        .unwrap(),
        expected
    );
}

struct ReadFailure;
impl Read for ReadFailure {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "actual reader failure",
        ))
    }
}
struct CancellingReader<'a>(&'a Cancellation);
impl Read for CancellingReader<'_> {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        self.0.cancel();
        Err(io::ErrorKind::Interrupted.into())
    }
}
#[test]
fn original_context_and_io_cause_are_preserved() {
    let failure = hash_reader(&mut ReadFailure, &context()).unwrap_err();
    assert_eq!(failure.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(failure.to_string(), "actual reader failure");
    let cancelled = Cancellation::default();
    let original =
        SnapshotContext::new(Instant::now() + Duration::from_secs(10), cancelled.clone());
    assert_eq!(
        hash_reader(&mut CancellingReader(&cancelled), &original)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_millis(1),
        Cancellation::default(),
    );
    assert_eq!(
        hash_reader(&mut ReadFailure, &expired).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
}
