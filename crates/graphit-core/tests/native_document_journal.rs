use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits},
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    installation_identity::InstallationId,
    native_document_file::{
        NativeDocumentFileLimits, NativeDocumentJournalWriteBounds, NativeDocumentNamespace,
    },
    project_paths::InstallationPaths,
};
use serde_json::value::RawValue;
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const NS: &str = ".graphit-provider-documents.state";
fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn fixture() -> (tempfile::TempDir, PathBuf, CapturedPublicationScope) {
    let tmp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    fs::create_dir(root.join("owner")).unwrap();
    let installation =
        InstallationPaths::resolve(Brand::default(), &root, None, Some(&root.join("owner")))
            .unwrap();
    let scope = CapturedPublicationScope::installation(
        &installation,
        InstallationId::new("journal-fixture").unwrap(),
        [root.clone()],
        ScopeLimits::new(4, 65536, 65536).unwrap(),
        &native_capture_request(),
    )
    .unwrap();
    (tmp, root, scope)
}
fn slot(
    scope: &CapturedPublicationScope,
    root: &Path,
    ctx: &SnapshotContext,
) -> NativeDocumentNamespace {
    // The journal intentionally exceeds the native input cap of one byte.
    NativeDocumentNamespace::capture(
        scope,
        root,
        NativeDocumentFileLimits::new(1, 65536, 256, 65536).unwrap(),
        ctx,
    )
    .unwrap()
}
fn raw(value: &str) -> &RawValue {
    serde_json::from_str(value).unwrap()
}
fn minimum(value: &RawValue) -> usize {
    NativeDocumentNamespace::admit_journal_body(value, 4096).unwrap()
}
fn write_bounds(cap: usize) -> NativeDocumentJournalWriteBounds {
    NativeDocumentJournalWriteBounds::new(cap, cap, cap).unwrap()
}
fn journal(root: &Path) -> PathBuf {
    root.join(NS).join("documents.json")
}

#[test]
fn absent_private_birth_update_and_borrowed_body_use_independent_journal_cap() {
    let (_tmp, root, scope) = fixture();
    let ctx = context();
    let ns = slot(&scope, &root, &ctx);
    let guard = ns.lock(&ctx).unwrap();
    let absent = guard.read_journal(4096).unwrap();
    assert!(absent.body().unwrap().is_none());
    assert!(!journal(&root).exists());
    let body = raw("{ \"owned\": \"private-launch-fixture\", \"rows\": [] }");
    NativeDocumentNamespace::admit_journal_body(body, 4096).unwrap();
    let first = guard
        .replace_journal(&absent, body, write_bounds(4096))
        .unwrap();
    assert_eq!(first.body().unwrap().unwrap().get(), body.get());
    let file = fs::File::open(journal(&root)).unwrap();
    graphit_fs::verify_private_file(&file).unwrap();
    drop(file);
    let next = raw(
        "{\"revision\":2,\"rows\":[{\"binding\":\"kiro-v2\"},{\"binding\":\"kiro-v3-preview\"}]}",
    );
    let second = guard
        .replace_journal(&first, next, write_bounds(4096))
        .unwrap();
    assert_eq!(second.body().unwrap().unwrap().get(), next.get());
    assert_eq!(
        guard
            .read_journal(4096)
            .unwrap()
            .body()
            .unwrap()
            .unwrap()
            .get(),
        next.get()
    );
    assert_eq!(fs::read_dir(root.join(NS)).unwrap().count(), 2);
}
#[test]
fn exact_worst_envelope_capacity_and_one_extra_fail_before_scratch_birth() {
    let (_tmp, root, scope) = fixture();
    let ctx = context();
    let ns = slot(&scope, &root, &ctx);
    let guard = ns.lock(&ctx).unwrap();
    let body = raw("{\"value\":\"a\"}");
    let cap = minimum(body);
    assert!(NativeDocumentNamespace::admit_journal_body(body, cap - 1).is_err());
    let absent = guard.read_journal(cap).unwrap();
    let first = guard
        .replace_journal(&absent, body, write_bounds(cap))
        .unwrap();
    let before = fs::read(journal(&root)).unwrap();

    let error = guard
        .replace_journal(&first, raw("{\"value\":\"aa\"}"), write_bounds(cap))
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
    assert_eq!(fs::read(journal(&root)).unwrap(), before);

    assert_eq!(fs::read_dir(root.join(NS)).unwrap().count(), 2);
    for bad in [0, usize::MAX] {
        assert_eq!(
            NativeDocumentNamespace::admit_journal_body(body, bad)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    for value in ["[]", "null", "true", "1", "\"value\""] {
        assert_eq!(
            NativeDocumentNamespace::admit_journal_body(raw(value), 4096)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
#[test]
fn original_request_and_snapshot_capacity_are_fenced_before_document_io() {
    let (_tmp, root, scope) = fixture();
    let first_ctx = context();
    let ns = slot(&scope, &root, &first_ctx);
    let first_guard = ns.lock(&first_ctx).unwrap();
    let old = first_guard.read_journal(4096).unwrap();
    drop(first_guard);
    let second_ctx = context();
    let other = slot(&scope, &root, &second_ctx);
    let guard = other.lock(&second_ctx).unwrap();
    let error = guard
        .replace_journal(&old, raw("{}"), write_bounds(4096))
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);

    assert!(!journal(&root).exists());
    let own = guard.read_journal(4096).unwrap();
    assert_eq!(
        guard
            .replace_journal(&own, raw("{}"), write_bounds(4095))
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(!journal(&root).exists());
}

#[test]
fn equal_bytes_foreign_replacement_and_stale_same_inode_body_are_preserved() {
    let (_tmp, root, scope) = fixture();
    let ctx = context();
    let ns = slot(&scope, &root, &ctx);
    let guard = ns.lock(&ctx).unwrap();
    let absent = guard.read_journal(4096).unwrap();
    let first = guard
        .replace_journal(&absent, raw("{\"revision\":1}"), write_bounds(4096))
        .unwrap();
    let bytes = fs::read(journal(&root)).unwrap();
    fs::rename(journal(&root), root.join("original")).unwrap();
    let mut replacement = graphit_fs::create_private_file(&journal(&root)).unwrap();
    std::io::Write::write_all(&mut replacement, &bytes).unwrap();
    drop(replacement);
    assert_eq!(
        guard.read_journal(4096).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
    assert!(
        guard
            .replace_journal(&first, raw("{}"), write_bounds(4096))
            .is_err()
    );
    assert_eq!(fs::read(journal(&root)).unwrap(), bytes);
    fs::remove_file(journal(&root)).unwrap();
    fs::rename(root.join("original"), journal(&root)).unwrap();
    let changed = String::from_utf8(bytes.clone())
        .unwrap()
        .replace("\"revision\":1", "\"revision\":2");
    fs::write(journal(&root), changed.as_bytes()).unwrap();
    assert!(
        guard
            .replace_journal(&first, raw("{}"), write_bounds(4096))
            .is_err()
    );
    assert_eq!(fs::read(journal(&root)).unwrap(), changed.as_bytes());
}
#[test]
fn closed_header_validation_precedes_snapshot_and_body_schema_is_deferred() {
    let (_tmp, root, scope) = fixture();
    let ctx = context();
    let ns = slot(&scope, &root, &ctx);
    let guard = ns.lock(&ctx).unwrap();
    let absent = guard.read_journal(4096).unwrap();
    let valid = guard
        .replace_journal(
            &absent,
            raw("{\"nested\":{\"key\":1,\"key\":2}}"),
            write_bounds(4096),
        )
        .unwrap();
    assert!(valid.body().unwrap().unwrap().get().contains("\"key\":2"));
    let bytes = fs::read(journal(&root)).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    let variants = [
        text.replacen("{", "{\"unknown\":1,", 1),
        text.replacen("\"body\":", "\"body\":{},\"body\":", 1),
        text.replacen("\"body\":", "\"other\":", 1),
        format!("{text} true"),
        text.replacen("\"physical_id\":", "\"physical_id\":null,\"saved\":", 1),
        text.replacen(
            "\"body\":{\"nested\":{\"key\":1,\"key\":2}}",
            "\"body\":[]",
            1,
        ),
    ];
    for bad in variants {
        fs::write(journal(&root), bad.as_bytes()).unwrap();
        assert!(guard.read_journal(4096).is_err());
        assert!(guard.read_journal(4096).is_err());
        assert_eq!(fs::read(journal(&root)).unwrap(), bad.as_bytes());
    }
    fs::write(journal(&root), &bytes).unwrap();
    assert!(guard.read_journal(bytes.len()).is_ok());
    assert_eq!(
        guard.read_journal(bytes.len() - 1).err().unwrap().kind(),
        io::ErrorKind::FileTooLarge
    );
}
#[test]
fn cancellation_and_expiry_preserve_the_fixed_state_and_permanent_lock() {
    let (_tmp, root, scope) = fixture();
    let cancel = Cancellation::default();
    let ctx = SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel.clone());
    let ns = slot(&scope, &root, &ctx);
    let guard = ns.lock(&ctx).unwrap();
    let old = guard.read_journal(4096).unwrap();
    let permit = write_bounds(4096);
    cancel.cancel();
    assert_eq!(
        guard
            .replace_journal(&old, raw("{}"), permit)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(
        guard.read_journal(4096).err().unwrap().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(!journal(&root).exists());
    drop(guard);
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert_eq!(
        ns.lock(&expired).err().unwrap().kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(fs::metadata(root.join(NS).join("lock")).unwrap().len(), 0);
}
#[cfg(unix)]
#[test]
fn permissive_hardlinked_and_symlink_journals_are_not_repaired_or_adopted() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (_tmp, root, scope) = fixture();
    let ctx = context();
    let ns = slot(&scope, &root, &ctx);
    let guard = ns.lock(&ctx).unwrap();
    let absent = guard.read_journal(4096).unwrap();
    guard
        .replace_journal(&absent, raw("{}"), write_bounds(4096))
        .unwrap();
    let bytes = fs::read(journal(&root)).unwrap();
    fs::set_permissions(journal(&root), fs::Permissions::from_mode(0o644)).unwrap();
    assert!(guard.read_journal(4096).is_err());
    assert!(guard.read_journal(4096).is_err());
    assert_eq!(
        fs::metadata(journal(&root)).unwrap().permissions().mode() & 0o777,
        0o644
    );
    fs::set_permissions(journal(&root), fs::Permissions::from_mode(0o600)).unwrap();
    fs::hard_link(journal(&root), root.join("linked")).unwrap();
    assert!(guard.read_journal(4096).is_err());
    assert!(guard.read_journal(4096).is_err());
    fs::remove_file(root.join("linked")).unwrap();
    fs::rename(journal(&root), root.join("foreign")).unwrap();
    symlink(root.join("foreign"), journal(&root)).unwrap();
    assert!(guard.read_journal(4096).is_err());
    assert!(guard.read_journal(4096).is_err());
    assert_eq!(fs::read(root.join("foreign")).unwrap(), bytes);
    assert!(
        fs::symlink_metadata(journal(&root))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn unchanged_file_and_namespace_ids_cannot_move_the_journal_to_another_parent() {
    let (_tmp, root, scope) = fixture();
    let ctx = context();
    let ns = slot(&scope, &root, &ctx);
    let guard = ns.lock(&ctx).unwrap();
    let absent = guard.read_journal(4096).unwrap();
    guard
        .replace_journal(&absent, raw("{\"revision\":1}"), write_bounds(4096))
        .unwrap();
    let before = fs::read(journal(&root)).unwrap();
    drop(guard);
    let other = root.join("other-parent");
    fs::create_dir(&other).unwrap();
    fs::rename(root.join(NS), other.join(NS)).unwrap();
    let new_ctx = context();
    let new_ns = slot(&scope, &other, &new_ctx);
    let new_guard = new_ns.lock(&new_ctx).unwrap();
    assert_eq!(
        new_guard.read_journal(4096).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read(journal(&other)).unwrap(), before);
    assert_eq!(fs::read_dir(other.join(NS)).unwrap().count(), 2);
}

#[test]
fn missing_parent_bootstrap_and_journal_phase_updates_use_actual_bounds() {
    let (_tmp, root, scope) = fixture();
    let parent = root.join("not-yet").join("nested");
    let starting = raw("{\"phase\":\"starting\"}");
    let projected = raw("{\"phase\":\"projected\"}");
    let first_size = minimum(starting);
    let second_size = minimum(projected);
    let cap = 4096;
    let ctx = context();

    let first_write = NativeDocumentJournalWriteBounds::new(cap, 0, first_size).unwrap();
    let second_write = NativeDocumentJournalWriteBounds::new(cap, first_size, second_size).unwrap();

    assert!(!parent.parent().unwrap().exists());
    assert!(!root.join(NS).exists());
    let ns = slot(&scope, &parent, &ctx);
    let guard = ns.lock(&ctx).unwrap();
    let absent = guard.read_journal(cap).unwrap();
    assert!(absent.body().unwrap().is_none());
    let first = guard
        .replace_journal(&absent, starting, first_write)
        .unwrap();
    let second = guard
        .replace_journal(&first, projected, second_write)
        .unwrap();
    assert_eq!(second.body().unwrap().unwrap().get(), projected.get());
    let observed = guard.read_journal(cap).unwrap();
    assert_eq!(observed.body().unwrap().unwrap().get(), projected.get());

    assert_eq!(fs::read_dir(parent.join(NS)).unwrap().count(), 2);
}

#[test]
fn previous_and_desired_bounds_are_checked_before_target_io_or_scratch_birth() {
    let (_tmp, root, scope) = fixture();
    let ctx = context();
    let ns = slot(&scope, &root, &ctx);
    let guard = ns.lock(&ctx).unwrap();
    let missing = guard.read_journal(4096).unwrap();
    let body = raw("{\"value\":\"a\"}");
    let size = minimum(body);
    // Positive O is conservative and accepts Missing.
    let first = guard
        .replace_journal(
            &missing,
            body,
            NativeDocumentJournalWriteBounds::new(4096, size, size).unwrap(),
        )
        .unwrap();
    let before = fs::read(journal(&root)).unwrap();
    for (old, desired, value) in [
        (0, size, body),
        (before.len() - 1, size, body),
        (4096, size - 1, body),
    ] {
        let permit = NativeDocumentJournalWriteBounds::new(4096, old, desired).unwrap();

        assert_eq!(
            guard
                .replace_journal(&first, value, permit)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::FileTooLarge
        );

        assert_eq!(fs::read(journal(&root)).unwrap(), before);
        assert_eq!(fs::read_dir(root.join(NS)).unwrap().count(), 2);
    }
}

#[test]
fn journal_reads_are_repeatable_and_respect_actual_file_bound() {
    let (_tmp, root, scope) = fixture();
    let ctx = context();
    let ns = slot(&scope, &root, &ctx);
    let guard = ns.lock(&ctx).unwrap();
    let missing = guard.read_journal(4096).unwrap();
    assert!(missing.body().unwrap().is_none());
    guard
        .replace_journal(&missing, raw("{}"), write_bounds(4096))
        .unwrap();
    let length = fs::metadata(journal(&root)).unwrap().len() as usize;
    for _ in 0..2 {
        assert_eq!(
            guard
                .read_journal(length)
                .unwrap()
                .body()
                .unwrap()
                .unwrap()
                .get(),
            "{}"
        );
    }
    assert_eq!(
        guard.read_journal(length - 1).err().unwrap().kind(),
        io::ErrorKind::FileTooLarge
    );
}

#[test]
fn cancellation_and_expiry_reject_journal_io() {
    let (_tmp, root, scope) = fixture();
    let setup = context();
    let ns = slot(&scope, &root, &setup);
    let cancel = Cancellation::default();
    let ctx = SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel.clone());

    let write = write_bounds(4096);
    let guard = ns.lock(&ctx).unwrap();
    let missing = guard.read_journal(4096).unwrap();

    cancel.cancel();
    assert_eq!(
        guard.read_journal(4096).err().unwrap().kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(
        guard
            .replace_journal(&missing, raw("{}"), write)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );

    drop(guard);
    let deadline = Instant::now() + Duration::from_millis(200);
    let expired = SnapshotContext::new(deadline, Cancellation::default());

    let write = write_bounds(4096);
    let guard = ns.lock(&expired).unwrap();
    let missing = guard.read_journal(4096).unwrap();
    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));

    assert_eq!(
        guard.read_journal(4096).err().unwrap().kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(
        guard
            .replace_journal(&missing, raw("{}"), write)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );

    assert!(!journal(&root).exists());
    assert_eq!(fs::read_dir(root.join(NS)).unwrap().count(), 1);
}

fn native_capture_request() -> std::sync::Arc<graphit_core::file_io::SnapshotContext> {
    std::sync::Arc::new(graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        graphit_core::file_lock::Cancellation::default(),
    ))
}
