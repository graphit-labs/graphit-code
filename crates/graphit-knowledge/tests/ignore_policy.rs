use graphit_core::{
    brand::Brand, config::ConfigSnapshot, file_io::SnapshotContext, file_lock::Cancellation,
    ignore_rules::IgnoreScope,
};
use graphit_knowledge::ignore_policy::{KnowledgeIgnorePolicy, KnowledgeIgnorePolicyError};
use std::{
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const EXPECTED: &str = include_str!("fixtures/ignore_policy/defaults.txt");

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}

fn snapshot(key: &str) -> ConfigSnapshot {
    ConfigSnapshot::new(&Brand::new(key).unwrap(), []).unwrap()
}

#[test]
fn complete_defaults_match_reviewed_fixture_and_control_names() {
    let context = context();
    let policy = KnowledgeIgnorePolicy::resolve(&context, &snapshot("graphit")).unwrap();
    let expected: Vec<_> = EXPECTED.lines().collect();
    assert_eq!(expected.len(), 80);
    assert_eq!(policy.patterns(), expected);
    assert_eq!(policy.patterns().iter().map(String::len).max(), Some(17));
    assert_eq!(policy.brand_directory(), ".graphit");
    assert_eq!(policy.custom_filename(), ".wikiignore");
    assert_eq!(policy.git_filename(), ".gitignore");
}

#[test]
fn policy_outlives_snapshot_and_preserves_original_request() {
    let cancellation = Cancellation::default();
    let original = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancellation.clone(),
    );
    let policy = {
        let snapshot = snapshot(" Acme-Code ");
        KnowledgeIgnorePolicy::resolve(&original, &snapshot).unwrap()
    };
    let expected: Vec<_> = EXPECTED.lines().collect();
    for (index, (actual, expected)) in policy.patterns().iter().zip(expected).enumerate() {
        if index == 70 {
            assert_eq!(actual, ".acme-code/");
        } else {
            assert_eq!(actual, expected);
        }
    }
    assert_eq!(policy.patterns().len(), 80);
    assert_eq!(policy.brand_directory(), ".acme-code");

    cancellation.cancel();
    let error = KnowledgeIgnorePolicy::resolve(&original, &snapshot("acme-code")).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    // Previously returned data remains immutable; it grants no physical access.
    assert_eq!(policy.patterns()[70], ".acme-code/");
}

#[test]
fn longest_valid_brand_resolves_complete_policy() {
    let context = context();
    let key = "a".repeat(64);
    let snapshot = snapshot(&key);
    let policy = KnowledgeIgnorePolicy::resolve(&context, &snapshot).unwrap();
    assert_eq!(policy.patterns().len(), 80);
    assert_eq!(policy.patterns()[70], format!(".{key}/"));
    assert_eq!(policy.patterns()[70].len(), 66);
    assert_eq!(policy.brand_directory(), format!(".{key}"));
    for (index, expected) in EXPECTED.lines().enumerate() {
        if index != 70 {
            assert_eq!(policy.patterns()[index], expected);
        }
    }
}

#[test]
fn cancelled_and_expired_requests_fail_without_projection() {
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let cancelled = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancellation.clone(),
    );
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    let both = SnapshotContext::new(Instant::now(), cancellation);
    let snapshot = snapshot("graphit");
    for (context, kind) in [
        (&cancelled, io::ErrorKind::Interrupted),
        (&expired, io::ErrorKind::TimedOut),
        (&both, io::ErrorKind::Interrupted),
    ] {
        let error = KnowledgeIgnorePolicy::resolve(context, &snapshot).unwrap_err();
        assert!(matches!(error, KnowledgeIgnorePolicyError::Context(_)));
        assert_eq!(error.kind(), kind);
    }
}

#[test]
fn resolved_defaults_use_core_matcher_and_remain_protected() {
    let context = context();
    let policy = KnowledgeIgnorePolicy::resolve(&context, &snapshot("acme-code")).unwrap();
    let root = TempRoot::new().unwrap();
    // Matcher fixture setup uses the existing Core loader. It is not request
    // accounting or an implementation of the future native control producer.
    fs::write(
        root.path().join(policy.custom_filename()),
        "!node_modules/\n!.acme-code/\n",
    )
    .unwrap();
    fs::write(
        root.path().join(policy.git_filename()),
        "docs/git-only.md\n",
    )
    .unwrap();
    let defaults: Vec<_> = policy.patterns().iter().map(String::as_str).collect();
    let scope = IgnoreScope::load(
        root.path(),
        Path::new(""),
        Some(policy.custom_filename()),
        &defaults,
    )
    .unwrap();
    for path in [
        "docs/program.o",
        "docs/photo.png",
        "docs/archive.zip",
        "docs/report.docx",
        "docs/app.min.js",
        "package-lock.json",
        "Cargo.lock",
        "node_modules/guide.md",
        "build/guide.md",
        ".vscode/guide.md",
        "docs/.DS_Store",
        "docs/Thumbs.db",
        ".acme-code/guide.md",
        ".codex/guide.md",
        ".kimi-code/guide.md",
        "docs/git-only.md",
    ] {
        assert!(scope.is_ignored(path, false).unwrap(), "{path}");
    }
    for path in [
        "docs/guide.md",
        ".visible/notes.md",
        "graphit.lock.json",
        ".graphit/guide.md",
    ] {
        assert!(!scope.is_ignored(path, false).unwrap(), "{path}");
    }
    assert!(!scope.should_descend("node_modules").unwrap());
    assert!(!scope.should_descend(".acme-code").unwrap());
}

#[test]
fn public_errors_preserve_category_and_cause_without_input_text() {
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let cancelled = SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancellation);
    let snapshot = snapshot("acme-code");
    let context_error = KnowledgeIgnorePolicy::resolve(&cancelled, &snapshot).unwrap_err();
    assert_eq!(context_error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(
        context_error
            .source()
            .unwrap()
            .downcast_ref::<io::Error>()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted,
    );
    assert!(!context_error.to_string().contains("acme-code"));

    // A real failed reserve supplies the public allocation cause; no OOM seam.
    let allocation = Vec::<u8>::new().try_reserve(usize::MAX).unwrap_err();
    let allocation_error = KnowledgeIgnorePolicyError::Allocation(allocation);
    assert_eq!(allocation_error.kind(), io::ErrorKind::OutOfMemory);
    assert!(
        allocation_error
            .source()
            .unwrap()
            .downcast_ref::<std::collections::TryReserveError>()
            .is_some()
    );
    assert!(!allocation_error.to_string().contains("acme-code"));
}

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> io::Result<Self> {
        static NONCE: AtomicU64 = AtomicU64::new(0);
        for _ in 0..16 {
            let nonce = NONCE
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| io::Error::from(io::ErrorKind::FileTooLarge))?;
            let path = std::env::temp_dir().join(format!(
                "graphit-knowledge-ignore-policy-{}-{nonce}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(io::ErrorKind::AlreadyExists.into())
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
