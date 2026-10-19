//! Real Core FILE capture -> P1 -> bounded compilation recipe.
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
use graphit_knowledge::{
    compilation_fingerprint::{
        CompilationFingerprintError, CompilationFingerprintLimits, fingerprint_compiled,
    },
    document::DocumentParseLimits,
    publication_metadata::{
        CompiledDocument, PublicationMetadataLimits, PublicationMetadataPolicy,
        compile_captured_document,
    },
};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs, io,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const DOMAIN_ORACLE: &[u8] = b"graphit.knowledge.compilation-fingerprint\0";

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..16 {
            let nonce = NEXT
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                .unwrap();
            let path = std::env::temp_dir().join(format!(
                "graphit-knowledge-fp-public-{}-{nonce}",
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

fn request() -> Arc<SnapshotContext> {
    let context = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    );
    Arc::new(context)
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
    fn new(source: &str) -> Self {
        Self::with_deadline(source, Instant::now() + Duration::from_secs(30))
    }
    fn with_deadline(source: &str, deadline: Instant) -> Self {
        let directory = Directory::new();
        let root = directory.0.join("project");
        fs::create_dir_all(root.join(".graphit")).unwrap();
        fs::write(root.join("document.md"), source).unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            &directory.0,
            None,
            Some(&directory.0.join("global")),
        )
        .unwrap();
        let paths = ProjectPaths::at(&root, &installation).unwrap();
        let request = Arc::new(SnapshotContext::new(deadline, Cancellation::default()));
        let scope = CapturedPublicationScope::project(
            &paths,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            ScopeLimits::new(4, 4096, 256).unwrap(),
            &request,
        )
        .unwrap();
        let retained = RetainedBytesLimits::new(source.len(), 1, 1).unwrap();
        let policy = SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            CopyLimits::new(source.len() as u64, 1, 1, 1).unwrap(),
            RetainedFileSet::new([PathBuf::new()], retained).unwrap(),
            retained,
        )
        .unwrap();
        let reader = CapturedSourceReader::capture_project(
            &scope,
            Path::new("document.md"),
            NodeKind::File,
            &policy,
            SourceReadLimits::new(source.len(), 4096, source.len() as u64).unwrap(),
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
    fn compile(
        &self,
        source_bytes: usize,
        fallback: &str,
        kind: &str,
        summary_cap: usize,
    ) -> CompiledDocument<'_> {
        let nz = |n| NonZeroUsize::new(n).unwrap();
        let parse = DocumentParseLimits {
            max_document_bytes: nz(source_bytes),
            max_input_bytes: nz(4096),
            max_frontmatter_bytes: nz(4096),
            max_yaml_events: nz(100),
            max_markdown_events: nz(100),
            max_yaml_depth: nz(10),
            max_scalar_bytes: nz(4096),
            max_links: nz(10),
            max_projection_bytes: nz(10_000),
            max_summary_bytes: nz(summary_cap),
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
                fallback_title: fallback.into(),
                kind_on_absent: kind.into(),
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
fn literal_digest(json: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(DOMAIN_ORACLE);
    digest.update(json);
    digest.finalize().into()
}

struct Oracle {
    id: &'static str,
    source: &'static str,
    fallback: &'static str,
    policy_kind: &'static str,
    summary_cap: usize,
    title: &'static str,
    kind: &'static str,
    description: Option<&'static str>,
    summary: &'static str,
    truncated: bool,
    json: &'static str,
    preimage_bytes: usize,
    digest: [u8; 32],
    physical_digest: [u8; 32],
}

// Independent literal preimages and digests from the approved static V2 packet.
// These expected values were not generated by the Rust producer or its helpers.
const ORACLES: [Oracle; 9] = [
    Oracle {
        id: "base319",
        source: "# H\n\nbody\n",
        fallback: "F",
        policy_kind: "guide",
        summary_cap: 32,
        title: "H",
        kind: "guide",
        description: None,
        summary: "body",
        truncated: false,
        json: r####"[1,"authored-document-v1",["title-explicit-h1-fallback-trim.v1","description-yaml-text-integral.v1","kind-explicit-or-supplied.v1","summary-editorial-prefix-utf8.v1","serde-saphyr@1.3.0","pulldown-cmark@0.13.4"],["F","guide",32],["# H\n\nbody\n","H","guide",null,"body",false]]"####,
        preimage_bytes: 319,
        digest: [
            0xa7, 0x44, 0xb7, 0xdd, 0x3e, 0x91, 0x05, 0x54, 0x94, 0xfa, 0xf5, 0xbe, 0x00, 0x7d,
            0xc9, 0x56, 0xb5, 0x72, 0x9b, 0xe8, 0xf8, 0xd0, 0x4b, 0x2c, 0x3e, 0x82, 0x7d, 0x34,
            0x5b, 0x7f, 0xea, 0x9c,
        ],
        physical_digest: [
            0x33, 0x4b, 0xcd, 0x32, 0x6b, 0xa3, 0x41, 0x92, 0x7a, 0x56, 0xbe, 0xa9, 0xdd, 0x85,
            0xb6, 0x03, 0x4f, 0xea, 0xa5, 0x4d, 0xf9, 0xd5, 0x13, 0x5b, 0x50, 0xda, 0xe5, 0xb3,
            0x72, 0xa4, 0xc9, 0xd3,
        ],
    },
    Oracle {
        id: "unused-fallback",
        source: "# H\n\nbody\n",
        fallback: "unused \" \\ \t\r\n",
        policy_kind: "guide",
        summary_cap: 32,
        title: "H",
        kind: "guide",
        description: None,
        summary: "body",
        truncated: false,
        json: r####"[1,"authored-document-v1",["title-explicit-h1-fallback-trim.v1","description-yaml-text-integral.v1","kind-explicit-or-supplied.v1","summary-editorial-prefix-utf8.v1","serde-saphyr@1.3.0","pulldown-cmark@0.13.4"],["unused \" \\ \t\r\n","guide",32],["# H\n\nbody\n","H","guide",null,"body",false]]"####,
        preimage_bytes: 337,
        digest: [
            0xcf, 0x6e, 0x38, 0x3a, 0xaa, 0x72, 0xb2, 0xf8, 0x43, 0x2d, 0xa0, 0x17, 0xd5, 0xdb,
            0xd3, 0xe4, 0x54, 0x7f, 0xf3, 0x45, 0xc8, 0x60, 0x26, 0xc5, 0x3f, 0xf1, 0xa7, 0x66,
            0x9f, 0xb0, 0xe8, 0xb0,
        ],
        physical_digest: [
            0x33, 0x4b, 0xcd, 0x32, 0x6b, 0xa3, 0x41, 0x92, 0x7a, 0x56, 0xbe, 0xa9, 0xdd, 0x85,
            0xb6, 0x03, 0x4f, 0xea, 0xa5, 0x4d, 0xf9, 0xd5, 0x13, 0x5b, 0x50, 0xda, 0xe5, 0xb3,
            0x72, 0xa4, 0xc9, 0xd3,
        ],
    },
    Oracle {
        id: "unused-summary-cap",
        source: "# H\n\nbody\n",
        fallback: "F",
        policy_kind: "guide",
        summary_cap: 33,
        title: "H",
        kind: "guide",
        description: None,
        summary: "body",
        truncated: false,
        json: r####"[1,"authored-document-v1",["title-explicit-h1-fallback-trim.v1","description-yaml-text-integral.v1","kind-explicit-or-supplied.v1","summary-editorial-prefix-utf8.v1","serde-saphyr@1.3.0","pulldown-cmark@0.13.4"],["F","guide",33],["# H\n\nbody\n","H","guide",null,"body",false]]"####,
        preimage_bytes: 319,
        digest: [
            0x34, 0x39, 0xa0, 0x18, 0x2b, 0xa4, 0xea, 0x87, 0x48, 0xa6, 0x13, 0xe5, 0x69, 0xc7,
            0x43, 0x2d, 0xf2, 0x8a, 0x68, 0xb7, 0x3b, 0xcf, 0xfd, 0xb7, 0x67, 0xe2, 0xc3, 0xc4,
            0x07, 0x41, 0x4a, 0x33,
        ],
        physical_digest: [
            0x33, 0x4b, 0xcd, 0x32, 0x6b, 0xa3, 0x41, 0x92, 0x7a, 0x56, 0xbe, 0xa9, 0xdd, 0x85,
            0xb6, 0x03, 0x4f, 0xea, 0xa5, 0x4d, 0xf9, 0xd5, 0x13, 0x5b, 0x50, 0xda, 0xe5, 0xb3,
            0x72, 0xa4, 0xc9, 0xd3,
        ],
    },
    Oracle {
        id: "empty-description",
        source: "---\ndescription: \"\"\n---\n# H\n\nbody\n",
        fallback: "F",
        policy_kind: "guide",
        summary_cap: 32,
        title: "H",
        kind: "guide",
        description: Some(""),
        summary: "body",
        truncated: false,
        json: r####"[1,"authored-document-v1",["title-explicit-h1-fallback-trim.v1","description-yaml-text-integral.v1","kind-explicit-or-supplied.v1","summary-editorial-prefix-utf8.v1","serde-saphyr@1.3.0","pulldown-cmark@0.13.4"],["F","guide",32],["---\ndescription: \"\"\n---\n# H\n\nbody\n","H","guide","","body",false]]"####,
        preimage_bytes: 346,
        digest: [
            0xf4, 0x73, 0xc5, 0xcd, 0x33, 0x6a, 0x5e, 0x8d, 0x60, 0xdd, 0x8f, 0x13, 0x63, 0xe3,
            0xf7, 0x04, 0x39, 0x96, 0x7a, 0x73, 0x73, 0xe6, 0x03, 0xc9, 0x98, 0x1c, 0x6f, 0xf0,
            0xbd, 0x4e, 0x3f, 0xb7,
        ],
        physical_digest: [
            0x81, 0x76, 0x73, 0xba, 0xf7, 0x2f, 0x30, 0xbc, 0xa3, 0x39, 0x6d, 0x14, 0xf9, 0x9b,
            0xa5, 0x58, 0x3e, 0xf3, 0xda, 0x75, 0xed, 0x0f, 0xf7, 0x73, 0x1c, 0xe9, 0x83, 0x5f,
            0xdf, 0x00, 0xb4, 0x29,
        ],
    },
    Oracle {
        id: "integral-description",
        source: "---\ndescription: \"  x\\t\\ny  \"\n---\n# H\n\nbody\n",
        fallback: "F",
        policy_kind: "guide",
        summary_cap: 32,
        title: "H",
        kind: "guide",
        description: Some("  x\t\ny  "),
        summary: "x\t\ny",
        truncated: false,
        json: r####"[1,"authored-document-v1",["title-explicit-h1-fallback-trim.v1","description-yaml-text-integral.v1","kind-explicit-or-supplied.v1","summary-editorial-prefix-utf8.v1","serde-saphyr@1.3.0","pulldown-cmark@0.13.4"],["F","guide",32],["---\ndescription: \"  x\\t\\ny  \"\n---\n# H\n\nbody\n","H","guide","  x\t\ny  ","x\t\ny",false]]"####,
        preimage_bytes: 370,
        digest: [
            0xcb, 0x0c, 0xe3, 0xe1, 0x92, 0xd2, 0x35, 0x27, 0x2a, 0x04, 0x26, 0xd6, 0x7f, 0xda,
            0x9a, 0x81, 0x16, 0x12, 0x46, 0xe8, 0x6b, 0xf0, 0xbc, 0x37, 0x48, 0x76, 0x90, 0x39,
            0xd9, 0x62, 0x57, 0xa0,
        ],
        physical_digest: [
            0x55, 0xf8, 0xc4, 0xc5, 0xd2, 0xa5, 0x68, 0x4c, 0x20, 0x9d, 0x40, 0x1d, 0x57, 0xb1,
            0x88, 0x20, 0xfe, 0xf8, 0x0f, 0x2e, 0x6c, 0x51, 0xc3, 0xaf, 0x68, 0x96, 0xd0, 0x24,
            0x21, 0x92, 0x0b, 0xf7,
        ],
    },
    Oracle {
        id: "bom-crlf-empty",
        source: "﻿---\r\ndescription: \"\"\r\n---\r\n# H\r\n\r\nbody\r\n",
        fallback: "F",
        policy_kind: "guide",
        summary_cap: 32,
        title: "H",
        kind: "guide",
        description: Some(""),
        summary: "body",
        truncated: false,
        json: r####"[1,"authored-document-v1",["title-explicit-h1-fallback-trim.v1","description-yaml-text-integral.v1","kind-explicit-or-supplied.v1","summary-editorial-prefix-utf8.v1","serde-saphyr@1.3.0","pulldown-cmark@0.13.4"],["F","guide",32],["﻿---\r\ndescription: \"\"\r\n---\r\n# H\r\n\r\nbody\r\n","H","guide","","body",false]]"####,
        preimage_bytes: 361,
        digest: [
            0x7f, 0xf3, 0x28, 0x95, 0x0c, 0x9e, 0x54, 0xec, 0x7e, 0x62, 0xdb, 0x3f, 0xb1, 0xb4,
            0x0c, 0xe2, 0x5b, 0xbc, 0x34, 0xc0, 0xfd, 0xe0, 0xe3, 0xda, 0xe0, 0x4d, 0x98, 0x30,
            0xeb, 0x3c, 0x3c, 0x62,
        ],
        physical_digest: [
            0x15, 0x8f, 0xdb, 0xcd, 0xa5, 0xe8, 0x0d, 0xed, 0xa5, 0xec, 0x30, 0x3d, 0x9c, 0xf8,
            0xaa, 0x71, 0x1f, 0xaf, 0x53, 0xc4, 0x26, 0xb1, 0x67, 0x16, 0x72, 0x80, 0x5d, 0x24,
            0xdc, 0x8b, 0x2c, 0x0c,
        ],
    },
    Oracle {
        id: "unicode-body",
        source: "# H\n\nação 🍀\n",
        fallback: "F",
        policy_kind: "guide",
        summary_cap: 32,
        title: "H",
        kind: "guide",
        description: None,
        summary: "ação 🍀",
        truncated: false,
        json: r####"[1,"authored-document-v1",["title-explicit-h1-fallback-trim.v1","description-yaml-text-integral.v1","kind-explicit-or-supplied.v1","summary-editorial-prefix-utf8.v1","serde-saphyr@1.3.0","pulldown-cmark@0.13.4"],["F","guide",32],["# H\n\nação 🍀\n","H","guide",null,"ação 🍀",false]]"####,
        preimage_bytes: 333,
        digest: [
            0xd3, 0x62, 0xc2, 0x05, 0x04, 0x0c, 0xe3, 0x07, 0x24, 0xb7, 0xc1, 0xb1, 0xa6, 0xd6,
            0x85, 0xb0, 0xaa, 0x1d, 0xf0, 0x13, 0x80, 0xc6, 0x4c, 0xaa, 0x23, 0x61, 0xf9, 0xec,
            0x5c, 0x0a, 0xd0, 0x62,
        ],
        physical_digest: [
            0xa0, 0xc7, 0xb2, 0x6c, 0xc0, 0xc9, 0x88, 0x7f, 0xae, 0x64, 0xe0, 0x20, 0xd7, 0x91,
            0x2b, 0x0d, 0x7e, 0x71, 0xbc, 0x48, 0x96, 0x5e, 0x52, 0x17, 0x26, 0xf7, 0x90, 0x4b,
            0xa5, 0xb0, 0xa8, 0x10,
        ],
    },
    Oracle {
        id: "unicode-summary-cut",
        source: "# H\n\nação 🍀\n",
        fallback: "F",
        policy_kind: "guide",
        summary_cap: 10,
        title: "H",
        kind: "guide",
        description: None,
        summary: "ação ",
        truncated: true,
        json: r####"[1,"authored-document-v1",["title-explicit-h1-fallback-trim.v1","description-yaml-text-integral.v1","kind-explicit-or-supplied.v1","summary-editorial-prefix-utf8.v1","serde-saphyr@1.3.0","pulldown-cmark@0.13.4"],["F","guide",10],["# H\n\nação 🍀\n","H","guide",null,"ação ",true]]"####,
        preimage_bytes: 328,
        digest: [
            0x29, 0x7f, 0x23, 0x20, 0x6a, 0x9f, 0x24, 0x2e, 0x9b, 0x3a, 0x8e, 0xe7, 0x67, 0x31,
            0x43, 0xee, 0xbd, 0x6f, 0x48, 0xf4, 0xef, 0xd9, 0xf1, 0xf6, 0xb2, 0xe9, 0x8b, 0x69,
            0x9a, 0xfc, 0x9b, 0x66,
        ],
        physical_digest: [
            0xa0, 0xc7, 0xb2, 0x6c, 0xc0, 0xc9, 0x88, 0x7f, 0xae, 0x64, 0xe0, 0x20, 0xd7, 0x91,
            0x2b, 0x0d, 0x7e, 0x71, 0xbc, 0x48, 0x96, 0x5e, 0x52, 0x17, 0x26, 0xf7, 0x90, 0x4b,
            0xa5, 0xb0, 0xa8, 0x10,
        ],
    },
    Oracle {
        id: "kind-policy-custom",
        source: "# H\n\nbody\n",
        fallback: "F",
        policy_kind: "domain/release",
        summary_cap: 32,
        title: "H",
        kind: "domain/release",
        description: None,
        summary: "body",
        truncated: false,
        json: r####"[1,"authored-document-v1",["title-explicit-h1-fallback-trim.v1","description-yaml-text-integral.v1","kind-explicit-or-supplied.v1","summary-editorial-prefix-utf8.v1","serde-saphyr@1.3.0","pulldown-cmark@0.13.4"],["F","domain/release",32],["# H\n\nbody\n","H","domain/release",null,"body",false]]"####,
        preimage_bytes: 337,
        digest: [
            0x7c, 0x3e, 0x91, 0x14, 0xb1, 0x3e, 0x57, 0x5f, 0xd9, 0x5a, 0x2e, 0x7c, 0x27, 0xab,
            0x01, 0xce, 0x0c, 0x47, 0xb7, 0x9a, 0xf6, 0x93, 0x8d, 0x8e, 0x70, 0x95, 0xfd, 0xa9,
            0x7f, 0xa4, 0x9e, 0xc5,
        ],
        physical_digest: [
            0x33, 0x4b, 0xcd, 0x32, 0x6b, 0xa3, 0x41, 0x92, 0x7a, 0x56, 0xbe, 0xa9, 0xdd, 0x85,
            0xb6, 0x03, 0x4f, 0xea, 0xa5, 0x4d, 0xf9, 0xd5, 0x13, 0x5b, 0x50, 0xda, 0xe5, 0xb3,
            0x72, 0xa4, 0xc9, 0xd3,
        ],
    },
];

#[test]
fn nine_independent_recipe_oracles_match_real_captures_and_exact_bounds() {
    let mut outputs = Vec::new();
    for oracle in &ORACLES {
        assert_eq!(DOMAIN_ORACLE.len(), 42);
        assert_eq!(
            DOMAIN_ORACLE.len() + oracle.json.len(),
            oracle.preimage_bytes,
            "{}",
            oracle.id
        );
        assert_eq!(
            literal_digest(oracle.json.as_bytes()),
            oracle.digest,
            "{}",
            oracle.id
        );
        let decoded: serde_json::Value = serde_json::from_str(oracle.json).unwrap();
        assert_eq!(decoded[4][0].as_str(), Some(oracle.source));
        assert_eq!(decoded[3][0].as_str(), Some(oracle.fallback));
        assert_eq!(decoded[3][1].as_str(), Some(oracle.policy_kind));
        assert_eq!(decoded[3][2].as_u64(), Some(oracle.summary_cap as u64));

        let fixture = Fixture::new(oracle.source);
        // Compilation depends on retained bytes after the captured path is removed.
        #[cfg(unix)]
        fs::remove_file(fixture._paths.root().join("document.md")).unwrap();
        let compiled = fixture.compile(
            oracle.source.len(),
            oracle.fallback,
            oracle.policy_kind,
            oracle.summary_cap,
        );
        assert_eq!(compiled.whole_source(), oracle.source);
        assert_eq!(
            compiled.whole_source().as_ptr(),
            fixture.reader.retained(Path::new("")).unwrap().as_ptr()
        );
        assert_eq!(compiled.title(), oracle.title);
        assert_eq!(compiled.kind(), oracle.kind);
        assert_eq!(compiled.description(), oracle.description);
        assert_eq!(compiled.summary(), oracle.summary);
        assert_eq!(compiled.summary_truncated(), oracle.truncated);
        assert_eq!(
            compiled.physical_source_digest().as_bytes(),
            &oracle.physical_digest
        );
        let fingerprint =
            fingerprint_compiled(&fixture.request, &compiled, limits(oracle.preimage_bytes))
                .unwrap();
        assert_eq!(fingerprint.as_bytes(), &oracle.digest, "{}", oracle.id);
        assert_ne!(
            fingerprint.as_bytes(),
            compiled.physical_source_digest().as_bytes()
        );
        assert_eq!(
            fingerprint_compiled(&fixture.request, &compiled, limits(oracle.preimage_bytes))
                .unwrap(),
            fingerprint
        );
        assert!(matches!(
            fingerprint_compiled(
                &fixture.request,
                &compiled,
                limits(oracle.preimage_bytes - 1)
            ),
            Err(CompilationFingerprintError::Limit("preimage bytes"))
        ));
        assert_eq!(
            fixture.reader.retained(Path::new("")).unwrap(),
            oracle.source.as_bytes()
        );
        assert_eq!(compiled.description(), oracle.description);
        assert_eq!(compiled.summary(), oracle.summary);
        // Inline recipe data can outlive the captured body and metadata owners.
        outputs.push(fingerprint);
    }
    // Same source and effective metadata, changed unused fallback or summary cap.
    assert_eq!(ORACLES[0].physical_digest, ORACLES[1].physical_digest);
    assert_eq!(ORACLES[0].physical_digest, ORACLES[2].physical_digest);
    assert_ne!(outputs[0], outputs[1]);
    assert_ne!(outputs[0], outputs[2]);
    // Same source, changed kind policy; all policy bytes belong to this recipe.
    assert_eq!(ORACLES[0].physical_digest, ORACLES[8].physical_digest);
    assert_ne!(outputs[0], outputs[8]);
    assert_eq!(outputs[0].as_bytes(), &ORACLES[0].digest);
}

#[test]
fn authored_empty_and_custom_kind_and_nullable_description_are_encoded_literally() {
    let rules = [
        "title-explicit-h1-fallback-trim.v1",
        "description-yaml-text-integral.v1",
        "kind-explicit-or-supplied.v1",
        "summary-editorial-prefix-utf8.v1",
        "serde-saphyr@1.3.0",
        "pulldown-cmark@0.13.4",
    ];
    for (source, kind, description) in [
        (
            "---\ntype: \"\"\ndescription: null\n---\n# H\n\nbody\n",
            "",
            None,
        ),
        (
            "---\ntype: custom/release\ndescription: \"\"\n---\n# H\n\nbody\n",
            "custom/release",
            Some(""),
        ),
    ] {
        let fixture = Fixture::new(source);
        let compiled = fixture.compile(source.len(), "F", "guide", 32);
        assert_eq!(compiled.kind(), kind);
        assert_eq!(compiled.description(), description);
        // Literal expected values and recipe order; no producer encoder/helper.
        let json = serde_json::to_vec(&(
            1u8,
            "authored-document-v1",
            rules,
            ("F", "guide", 32u64),
            (source, "H", kind, description, "body", false),
        ))
        .unwrap();
        let expected = literal_digest(&json);
        let exact = DOMAIN_ORACLE.len() + json.len();
        assert_eq!(
            fingerprint_compiled(&fixture.request, &compiled, limits(exact))
                .unwrap()
                .as_bytes(),
            &expected
        );
        assert!(matches!(
            fingerprint_compiled(&fixture.request, &compiled, limits(exact - 1)),
            Err(CompilationFingerprintError::Limit("preimage bytes"))
        ));
    }
}

#[test]
fn original_cancel_after_compilation_preserves_cause_and_borrowed_inputs() {
    let source = ORACLES[0].source;
    let fixture = Fixture::new(source);
    let compiled = fixture.compile(source.len(), "F", "guide", 32);
    fixture.request.cancellation().cancel();
    let error = fingerprint_compiled(&fixture.request, &compiled, limits(319)).unwrap_err();
    assert!(
        matches!(&error, CompilationFingerprintError::OriginalRequest(e) if e.kind() == io::ErrorKind::Interrupted)
    );
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<io::Error>()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(
        fixture.reader.retained(Path::new("")).unwrap(),
        source.as_bytes()
    );
    assert_eq!(compiled.whole_source(), source);
    assert_eq!(compiled.title(), "H");
    assert_eq!(compiled.kind(), "guide");
    assert_eq!(compiled.summary(), "body");
    assert_eq!(
        compiled.physical_source_digest().as_bytes(),
        &ORACLES[0].physical_digest
    );
}

#[test]
fn foreign_request_cannot_rebind_compiled_document_or_read_inputs() {
    let source = ORACLES[0].source;
    let fixture = Fixture::new(source);
    let compiled = fixture.compile(source.len(), "F", "guide", 32);
    let foreign = request();
    let error = fingerprint_compiled(&foreign, &compiled, limits(319)).unwrap_err();
    assert!(
        matches!(&error, CompilationFingerprintError::OriginalRequest(e) if e.kind() == io::ErrorKind::InvalidInput)
    );
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<io::Error>()
            .is_some()
    );
    assert_eq!(
        fingerprint_compiled(&fixture.request, &compiled, limits(319))
            .unwrap()
            .as_bytes(),
        &ORACLES[0].digest
    );
}

#[test]
fn prefix_bounds_reject_whole_results() {
    for cap in [41, 42] {
        let fixture = Fixture::new("# H\n\nbody\n");
        let compiled = fixture.compile("# H\n\nbody\n".len(), "F", "guide", 32);
        assert!(matches!(
            fingerprint_compiled(&fixture.request, &compiled, limits(cap)),
            Err(CompilationFingerprintError::Limit("preimage bytes"))
        ));
        assert_eq!(compiled.whole_source(), "# H\n\nbody\n");
    }
}

#[test]
fn original_deadline_rejects_fingerprint_after_successful_compilation() {
    let deadline = Instant::now() + Duration::from_secs(1);
    let fixture = Fixture::with_deadline("# H\n\nbody\n", deadline);
    let compiled = fixture.compile("# H\n\nbody\n".len(), "F", "guide", 32);
    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
    let error = fingerprint_compiled(&fixture.request, &compiled, limits(319)).unwrap_err();
    assert!(
        matches!(&error, CompilationFingerprintError::OriginalRequest(e) if e.kind() == io::ErrorKind::TimedOut)
    );
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<io::Error>()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(compiled.whole_source(), "# H\n\nbody\n");
}
