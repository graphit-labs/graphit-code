use super::*;
use crate::{
    artifact_store::{CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet},
    file_lock::Cancellation,
};
use std::{
    cell::RefCell,
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

type ReadHook = (usize, Box<dyn FnMut(&OpenFile)>);

#[test]
fn installation_batch_cancel_after_actual_read_returns_no_partial_ranges_or_refund() {
    use crate::{
        artifact_store::{CapturedPublicationScope, ScopeLimits},
        brand::Brand,
        installation_identity::InstallationId,
        project_paths::InstallationPaths,
    };
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("installation");
    fs::create_dir_all(root.join("runtime/native")).unwrap();
    fs::write(root.join("runtime/native/body"), b"native-data").unwrap();
    let paths =
        InstallationPaths::resolve(Brand::default(), temporary.path(), None, Some(&root)).unwrap();
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ));
    let request = Arc::clone(&original);
    let scope = CapturedPublicationScope::installation(
        &paths,
        InstallationId::new("batch-installation").unwrap(),
        [root.clone()],
        ScopeLimits::new(4, 4096, 256).unwrap(),
        &request,
    )
    .unwrap();
    let reader = CapturedSourceReader::capture_native_scope(
        &scope,
        &Path::new("runtime").join("native"),
        NodeKind::Directory,
        &policy(&[]),
        limits(),
        &request,
    )
    .unwrap();
    let file = reader.open_file(Path::new("body")).unwrap();
    let plan = [
        SourceRangeRequest::new(&file, 0, 3).unwrap(),
        SourceRangeRequest::new(&file, 7, 4).unwrap(),
    ];
    let cancel = original.cancellation().clone();
    hook(1, move |_| cancel.cancel());
    assert_eq!(
        reader
            .read_ranges(&plan, AggregateReadLimits::new(2, 7).unwrap())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
    READ_HOOK.with(|slot| assert!(slot.borrow().is_none()));

    drop(file);
    drop(reader);
    drop(scope);
}
thread_local! { static READ_HOOK: RefCell<Option<ReadHook>> = const { RefCell::new(None) }; }
pub(super) fn after_read(file: &OpenFile) {
    let hook = READ_HOOK.with(|cell| {
        let mut value = cell.borrow_mut();
        if let Some((remaining, _)) = value.as_mut() {
            *remaining -= 1;
            if *remaining == 0 {
                return value.take();
            }
        }
        None
    });
    if let Some((_, mut hook)) = hook {
        hook(file);
    }
}
pub(super) fn hook(reads: usize, f: impl FnMut(&OpenFile) + 'static) {
    READ_HOOK.with(|cell| {
        assert!(cell.borrow().is_none());
        *cell.borrow_mut() = Some((reads, Box::new(f)));
    });
}
type MetadataHook = Box<dyn FnOnce(&OpenFile)>;
thread_local! {
    static METADATA_HOOK: RefCell<Option<MetadataHook>> = const { RefCell::new(None) };
}
pub(super) fn metadata_observed(file: &OpenFile) {
    let hook = METADATA_HOOK.with(|cell| cell.borrow_mut().take());
    if let Some(hook) = hook {
        hook(file);
    }
}
struct MetadataHookGuard;
impl Drop for MetadataHookGuard {
    fn drop(&mut self) {
        METADATA_HOOK.with(|cell| {
            cell.borrow_mut().take();
        });
    }
}
fn metadata_hook(f: impl FnOnce(&OpenFile) + 'static) -> MetadataHookGuard {
    METADATA_HOOK.with(|cell| {
        assert!(cell.borrow().is_none());
        *cell.borrow_mut() = Some(Box::new(f));
    });
    MetadataHookGuard
}
fn policy(selected: &[PathBuf]) -> SnapshotPolicy {
    let retention = RetainedBytesLimits::new(1024, 8, 4096).unwrap();
    SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(4 * 1024 * 1024, 64, 4096, 8).unwrap(),
        RetainedFileSet::new(selected.iter().cloned(), retention).unwrap(),
        retention,
    )
    .unwrap()
}
pub(super) fn limits() -> SourceReadLimits {
    SourceReadLimits::new(4096, 3, 1024).unwrap()
}
pub(super) fn context() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    ))
}
pub(super) fn reader(
    path: &Path,
    kind: NodeKind,
    limits: SourceReadLimits,
    ctx: Arc<SnapshotContext>,
) -> CapturedSourceReader {
    CapturedSourceReader::capture(path, kind, &policy(&[]), limits, ctx).unwrap()
}
pub(super) fn fixture(
    limits: SourceReadLimits,
) -> (
    tempfile::TempDir,
    CapturedSourceReader,
    Arc<SnapshotContext>,
) {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("data"), b"0123456789").unwrap();
    let ctx = context();
    let r = reader(temp.path(), NodeKind::Directory, limits, Arc::clone(&ctx));
    (temp, r, ctx)
}
pub(super) fn kind<T>(result: io::Result<T>) -> io::ErrorKind {
    result.err().expect("must deny").kind()
}

#[test]
fn positive_limits_and_opaque_send_sync_traits() {
    for i in 0..3 {
        let mut values = [1; 3];
        values[i] = 0;
        assert_eq!(
            kind(SourceReadLimits::new(
                values[0],
                values[1],
                values[2] as u64
            )),
            io::ErrorKind::InvalidInput
        );
    }
    fn sync<T: Send + Sync>() {}
    fn send<T: Send>() {}
    sync::<CapturedSourceReader>();
    sync::<CapturedSourceFile>();
    send::<SourceRangeStream>();
}
#[test]
fn generated_payload_streams_captured_eof_without_tree_quotas_or_retention() {
    let temp = tempfile::tempdir().unwrap();
    let nested = temp.path().join("generated/a/b");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("ação.txt"), b"0123456789").unwrap();
    fs::write(temp.path().join("second"), b"another generated file").unwrap();
    let ctx = context();
    let retention = RetainedBytesLimits::new(1, 1, 1).unwrap();
    let limited = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(1, 2, 4, 1).unwrap(),
        RetainedFileSet::new([], retention).unwrap(),
        retention,
    )
    .unwrap();
    assert_eq!(
        kind(CapturedSourceReader::capture(
            temp.path(),
            NodeKind::Directory,
            &limited,
            limits(),
            Arc::clone(&ctx),
        )),
        io::ErrorKind::FileTooLarge
    );
    let reader = CapturedSourceReader::capture_generated(
        temp.path(),
        NodeKind::Directory,
        SourceReadLimits::generated(3, 2).unwrap(),
        Arc::clone(&ctx),
    )
    .unwrap();
    assert_eq!(reader.nodes().len(), 6);
    assert_eq!(reader.retained_bytes(), 0);
    let file = reader
        .open_file(Path::new("generated/a/b/ação.txt"))
        .unwrap();
    let mut content = Vec::new();
    file.stream(0).unwrap().read_to_end(&mut content).unwrap();
    assert_eq!(content, b"0123456789");
    assert_eq!(kind(file.read_range(0, 4)), io::ErrorKind::FileTooLarge);
    assert!(reader.check_request(&context()).is_err());
    reader.revalidate_bindings().unwrap();
    reader.revalidate_content().unwrap();
    fs::write(temp.path().join("new-file"), b"changed inventory").unwrap();
    assert_eq!(
        kind(reader.revalidate_bindings()),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn generated_capture_preserves_source_changes_and_cancellation() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("data"), b"original").unwrap();
    let ctx = context();
    let reader = CapturedSourceReader::capture_generated(
        temp.path(),
        NodeKind::Directory,
        SourceReadLimits::generated(8, 2).unwrap(),
        Arc::clone(&ctx),
    )
    .unwrap();
    assert_eq!(
        kind(reader.open_file(Path::new("../data"))),
        io::ErrorKind::InvalidInput
    );
    let file = reader.open_file(Path::new("data")).unwrap();
    fs::write(temp.path().join("data"), b"different").unwrap();
    assert_eq!(kind(file.stream(0)), io::ErrorKind::InvalidData);
    ctx.cancellation().cancel();
    assert_eq!(
        kind(reader.revalidate_content()),
        io::ErrorKind::Interrupted
    );
}

#[cfg(unix)]
#[test]
fn generated_capture_rejects_file_links() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("data"), b"original").unwrap();
    std::os::unix::fs::symlink("data", temp.path().join("linked")).unwrap();
    assert_eq!(
        kind(CapturedSourceReader::capture_generated(
            temp.path(),
            NodeKind::Directory,
            SourceReadLimits::generated(8, 2).unwrap(),
            context(),
        )),
        io::ErrorKind::InvalidInput
    );
}
#[test]
fn large_readonly_asset_retains_only_manifest() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("manifest"), b"meta").unwrap();
    let asset = temp.path().join("data");
    fs::write(&asset, vec![73; 1024 * 1024]).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&asset, fs::Permissions::from_mode(0o444)).unwrap();
    }
    let ctx = context();
    let r = CapturedSourceReader::capture(
        temp.path(),
        NodeKind::Directory,
        &policy(&[PathBuf::from("manifest")]),
        limits(),
        Arc::clone(&ctx),
    )
    .unwrap();
    assert_eq!(r.retained_bytes(), 4);
    assert_eq!(r.retained(Path::new("manifest")), Some(&b"meta"[..]));
    assert!(r.retained(Path::new("data")).is_none());
    assert_eq!(r.nodes().len(), 3);

    let f = r.open_file(Path::new("data")).unwrap();
    assert_eq!(f.len(), 1024 * 1024);
    assert!(!f.is_empty());
    assert_eq!(f.read_range(100, 7).unwrap().as_ref(), vec![73; 7]);

    r.revalidate_content().unwrap();
    r.revalidate_content().unwrap();

    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
}
#[test]
fn ranges_exact_limit_overflow_eof_and_empty_never_charge_twice() {
    let (_temp, r, _) = fixture(SourceReadLimits::new(4, 2, 10).unwrap());
    let f = r.open_file(Path::new("data")).unwrap();
    assert_eq!(f.read_range(2, 4).unwrap().as_ref(), b"2345");

    assert_eq!(kind(f.read_range(2, 5)), io::ErrorKind::FileTooLarge);
    for (start, len) in [(9, 2), (11, 0), (u64::MAX, 1)] {
        assert_eq!(kind(f.read_range(start, len)), io::ErrorKind::InvalidInput);
    }
    assert!(f.read_range(10, 0).unwrap().is_empty());
    assert!(f.read_range(0, 0).unwrap().is_empty());
}
#[test]
fn stream_blocks_independent_positions_and_admission_end_is_not_fake_eof() {
    let (_temp, r, _) = fixture(SourceReadLimits::new(10, 3, 5).unwrap());
    let f = r.open_file(Path::new("data")).unwrap();
    let mut a = f.stream(0).unwrap();
    let mut b = f.stream(5).unwrap();
    let mut buf = [0; 8];
    assert_eq!(a.read(&mut buf).unwrap(), 3);
    assert_eq!(&buf[..3], b"012");
    assert_eq!(b.read(&mut buf).unwrap(), 3);
    assert_eq!(&buf[..3], b"567");
    assert_eq!(a.read(&mut buf).unwrap(), 2);
    assert_eq!(&buf[..2], b"34");
    assert_eq!(kind(a.read(&mut buf)), io::ErrorKind::FileTooLarge);
    assert_eq!(b.read(&mut buf).unwrap(), 2);
    assert_eq!(&buf[..2], b"89");
    assert_eq!(b.read(&mut buf).unwrap(), 0);
    assert_eq!(a.read(&mut []).unwrap(), 0);

    assert_eq!(kind(f.stream(11)), io::ErrorKind::InvalidInput);
}

#[test]
fn reader_retains_original_context_identity() {
    let (_temp, r, ctx) = fixture(limits());
    let binding = r.shared.snapshot.binding().unwrap();
    assert!(r.matches_binding(&binding));
    let (_other_root, other, _) = fixture(limits());
    assert_eq!(r.fingerprint(), other.fingerprint());
    assert!(!r.matches_binding(&other.shared.snapshot.binding().unwrap()));
    let similar = SnapshotContext::new(ctx.deadline().unwrap(), ctx.cancellation().clone());
    assert_eq!(kind(r.check_request(&similar)), io::ErrorKind::InvalidInput);
    r.check_request(&ctx).unwrap();
    let shared = Arc::clone(&ctx);
    r.check_request(&shared).unwrap();
    assert_eq!(
        r.open_file(Path::new("data"))
            .unwrap()
            .read_range(0, 4)
            .unwrap()
            .as_ref(),
        b"0123"
    );
}

#[test]
fn cancel_after_middle_and_last_native_read_preserves_admission_and_actual_reads() {
    for reads in [1, 2] {
        let (_temp, r, ctx) = fixture(SourceReadLimits::new(6, 3, 6).unwrap());
        let f = r.open_file(Path::new("data")).unwrap();
        let cancel = ctx.cancellation().clone();
        hook(reads, move |_| cancel.cancel());
        assert_eq!(kind(f.read_range(0, 6)), io::ErrorKind::Interrupted);

        assert_eq!(kind(f.read_range(0, 0)), io::ErrorKind::Interrupted);
    }
}
#[test]
fn stream_last_read_cancel_returns_error_and_keeps_actual_offset() {
    let (_temp, r, ctx) = fixture(limits());
    let f = r.open_file(Path::new("data")).unwrap();
    let mut stream = f.stream(0).unwrap();
    let cancel = ctx.cancellation().clone();
    hook(1, move |_| cancel.cancel());
    let mut buf = [0; 3];
    assert_eq!(kind(stream.read(&mut buf)), io::ErrorKind::Interrupted);
    assert_eq!(buf, b"012"[..]);
    assert_eq!(stream.position, 3);

    assert_eq!(kind(stream.read(&mut [])), io::ErrorKind::Interrupted);
}
#[test]
fn deadline_before_entry_and_after_last_native_read_never_returns_success() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("data");
    fs::write(&path, b"abc").unwrap();
    let ctx = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(1),
        Cancellation::default(),
    ));
    let r = reader(&path, NodeKind::File, limits(), Arc::clone(&ctx));
    let f = r.open_file(Path::new("")).unwrap();
    let deadline = ctx.deadline().unwrap();
    hook(1, move |_| {
        let wait = deadline.saturating_duration_since(Instant::now());
        std::thread::sleep(wait + Duration::from_millis(2));
    });
    assert_eq!(kind(f.read_range(0, 3)), io::ErrorKind::TimedOut);

    assert_eq!(kind(r.open_file(Path::new(""))), io::ErrorKind::TimedOut);
    assert_eq!(kind(r.revalidate_content()), io::ErrorKind::TimedOut);
}
#[test]
fn stream_keeps_open_file_alive_after_reader_drop() {
    let (_temp, r, _ctx) = fixture(limits());
    let f = r.open_file(Path::new("data")).unwrap();
    let mut stream = f.stream(0).unwrap();
    drop(f);
    drop(r);
    let mut buf = [0; 3];
    assert_eq!(stream.read(&mut buf).unwrap(), 3);
    assert_eq!(&buf, b"012");
}

#[test]
fn poisoned_file_write_lock_denies_metadata_and_body_reads() {
    let (_temp, r, _) = fixture(SourceReadLimits::new(10, 3, 10).unwrap());
    let f = r.open_file(Path::new("data")).unwrap();
    assert_eq!(f.read_range(0, 1).unwrap().as_ref(), b"0");
    let fd = Arc::clone(&f.file);
    assert!(
        std::thread::spawn(move || {
            let _guard = fd.fd.write().unwrap();
            panic!("private poison fixture");
        })
        .join()
        .is_err()
    );
    assert_eq!(kind(f.read_range(0, 1)), io::ErrorKind::Other);
    assert_eq!(kind(f.last_modified()), io::ErrorKind::Other);
}
#[test]
fn native_relative_validation_rejects_directories_and_invalid_or_absent_files() {
    let (_temp, r, _) = fixture(SourceReadLimits::new(10, 3, 10).unwrap());
    for path in ["", "../data", "./data", "/data", "missing"] {
        assert!(r.open_file(Path::new(path)).is_err());
    }

    let f = r.open_file(Path::new("data")).unwrap();
    drop(f);
}
#[test]
fn concurrent_positional_reads_have_independent_offsets_on_same_descriptor() {
    let (_temp, r, _) = fixture(limits());
    let file = Arc::new(r.open_file(Path::new("data")).unwrap());
    let jobs: Vec<_> = (0..8)
        .map(|offset| {
            let file = Arc::clone(&file);
            std::thread::spawn(move || file.read_range(offset, 2).unwrap())
        })
        .collect();
    for (offset, job) in jobs.into_iter().enumerate() {
        assert_eq!(
            job.join().unwrap().as_ref(),
            format!("{}{}", offset, offset + 1).as_bytes()
        );
    }
}

#[test]
fn file_replacement_growth_and_body_mutation_deny_without_adoption() {
    for attack in 0..3 {
        let (temp, r, _) = fixture(limits());
        let f = r.open_file(Path::new("data")).unwrap();
        let path = temp.path().join("data");
        match attack {
            0 => {
                fs::rename(&path, temp.path().join("old")).unwrap();
                fs::write(&path, b"0123456789").unwrap();
            }
            1 => fs::write(&path, b"01234567890").unwrap(),
            _ => fs::write(&path, b"abcdefghij").unwrap(),
        };
        assert!(f.read_range(0, 1).is_err());
        assert!(f.last_modified().is_err());
        assert!(r.open_file(Path::new("data")).is_err());

        assert!(r.revalidate_content().is_err());
    }
}
#[cfg(unix)]
#[test]
fn same_fd_mode_change_after_read_and_regular_hardlinks_are_qualified() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("data");
    fs::write(&path, b"abc").unwrap();
    fs::hard_link(&path, temp.path().join("alias")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    let ctx = context();
    let r = reader(&path, NodeKind::File, limits(), Arc::clone(&ctx));
    let f = r.open_file(Path::new("")).unwrap();
    assert_eq!(f.read_range(0, 3).unwrap().as_ref(), b"abc");
    let changed = path.clone();
    hook(1, move |_| {
        fs::set_permissions(&changed, fs::Permissions::from_mode(0o400)).unwrap()
    });
    assert_eq!(kind(f.read_range(0, 3)), io::ErrorKind::InvalidData);

    assert_eq!(fs::read(&path).unwrap(), b"abc");
}
#[cfg(unix)]
#[test]
fn root_alias_and_captured_ancestor_replacement_never_accept_equal_bytes() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    fs::write(first.join("data"), b"abc").unwrap();
    fs::hard_link(first.join("data"), second.join("data")).unwrap();
    let alias = temp.path().join("root");
    symlink(&first, &alias).unwrap();
    let ctx = context();
    let r = reader(
        &alias.join("data"),
        NodeKind::File,
        limits(),
        Arc::clone(&ctx),
    );
    let f = r.open_file(Path::new("")).unwrap();
    fs::remove_file(&alias).unwrap();
    symlink(&second, &alias).unwrap();
    assert_eq!(kind(f.read_range(0, 3)), io::ErrorKind::InvalidData);

    assert_eq!(kind(f.last_modified()), io::ErrorKind::InvalidData);
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("dir")).unwrap();
    fs::write(temp.path().join("dir/data"), b"abc").unwrap();
    let ctx = context();
    let r = reader(temp.path(), NodeKind::Directory, limits(), Arc::clone(&ctx));
    let f = r.open_file(Path::new("dir/data")).unwrap();
    fs::rename(temp.path().join("dir"), temp.path().join("old")).unwrap();
    fs::create_dir(temp.path().join("dir")).unwrap();
    fs::hard_link(temp.path().join("old/data"), temp.path().join("dir/data")).unwrap();
    assert!(f.read_range(0, 3).is_err());
    assert!(f.last_modified().is_err());
}
#[cfg(unix)]
#[test]
fn non_unicode_names_are_rejected_with_a_clear_error() {
    use std::os::unix::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    let relative = PathBuf::from(std::ffi::OsString::from_vec(vec![0xff, b'x']));
    fs::write(temp.path().join(&relative), b"native").unwrap();
    let ctx = context();
    for result in [
        CapturedSourceReader::capture(
            temp.path(),
            NodeKind::Directory,
            &policy(&[]),
            limits(),
            Arc::clone(&ctx),
        ),
        CapturedSourceReader::capture_generated(
            temp.path(),
            NodeKind::Directory,
            SourceReadLimits::generated(8, 2).unwrap(),
            ctx,
        ),
    ] {
        let error = result.err().expect("non-Unicode path must be rejected");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("Unicode"));
    }
}

#[test]
fn metadata_same_descriptor_preserves_payload_quota_and_subsequent_range() {
    let (_temp, r, _) = fixture(SourceReadLimits::new(3, 3, 3).unwrap());
    let file = r.open_file(Path::new("data")).unwrap();
    let (expected, identity) = {
        let fd = file.file.fd.write().unwrap();
        (
            fd.metadata().unwrap().modified().unwrap(),
            crate::native_file_identity::handle_id(&fd).unwrap(),
        )
    };
    let stream = file.stream(0).unwrap();

    for _ in 0..2 {
        assert_eq!(file.last_modified().unwrap(), expected);
    }
    // Metadata does not acquire an extra payload-reader permit or reset quota.
    drop(stream);
    assert_eq!(file.read_range(4, 3).unwrap().as_ref(), b"456");

    assert_eq!(file.last_modified().unwrap(), expected);

    let fd = file.file.fd.write().unwrap();
    assert!(identity == crate::native_file_identity::handle_id(&fd).unwrap());
}

#[test]
fn metadata_original_cancellation_before_and_after_observation_denies_without_payload() {
    for after_observation in [false, true] {
        let (_temp, r, ctx) = fixture(limits());
        let file = r.open_file(Path::new("data")).unwrap();
        let cancellation = ctx.cancellation().clone();
        let _hook = if after_observation {
            Some(metadata_hook(move |_| cancellation.cancel()))
        } else {
            cancellation.cancel();
            None
        };

        assert_eq!(kind(file.last_modified()), io::ErrorKind::Interrupted);

        METADATA_HOOK.with(|cell| assert!(cell.borrow().is_none()));
    }
}

#[test]
fn metadata_original_deadline_before_and_after_observation_denies_without_payload() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("data");
    fs::write(&path, b"abc").unwrap();
    let ctx = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(2),
        Cancellation::default(),
    ));
    let r = reader(&path, NodeKind::File, limits(), Arc::clone(&ctx));
    let file = r.open_file(Path::new("")).unwrap();
    let deadline = ctx.deadline().unwrap();
    let _hook = metadata_hook(move |_| {
        std::thread::sleep(
            deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(2),
        );
    });

    assert_eq!(kind(file.last_modified()), io::ErrorKind::TimedOut);

    METADATA_HOOK.with(|cell| assert!(cell.borrow().is_none()));
    // The same original request is now expired before another observation.
    assert_eq!(kind(file.last_modified()), io::ErrorKind::TimedOut);
}

#[test]
fn metadata_changed_before_or_after_observation_never_returns_historical_time() {
    for after_observation in [false, true] {
        let (temp, r, _) = fixture(limits());
        let file = r.open_file(Path::new("data")).unwrap();
        let writer = fs::OpenOptions::new()
            .write(true)
            .open(temp.path().join("data"))
            .unwrap();
        let expected = file.last_modified().unwrap();
        let changed = expected.checked_add(Duration::from_secs(60)).unwrap();
        let update = move || {
            writer
                .set_times(fs::FileTimes::new().set_modified(changed))
                .unwrap();
            assert_ne!(writer.metadata().unwrap().modified().unwrap(), expected);
        };
        let _hook = if after_observation {
            Some(metadata_hook(move |_| update()))
        } else {
            update();
            None
        };

        assert_eq!(kind(file.last_modified()), io::ErrorKind::InvalidData);

        METADATA_HOOK.with(|cell| assert!(cell.borrow().is_none()));
        let fd = file.file.fd.write().unwrap();
        // Directly exercise same-FD equality without pathname checks masking it.
        assert_eq!(
            kind(
                r.shared
                    .snapshot
                    .plan()
                    .reader_last_modified(file.file.index, &fd)
            ),
            io::ErrorKind::InvalidData
        );
    }
}

#[test]
fn metadata_preserves_pre_epoch_system_time_when_filesystem_supports_it() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("data");
    fs::write(&path, b"abc").unwrap();
    let writer = fs::OpenOptions::new().write(true).open(&path).unwrap();
    let requested = std::time::UNIX_EPOCH
        .checked_sub(Duration::from_secs(60))
        .unwrap();
    if let Err(error) = writer.set_times(fs::FileTimes::new().set_modified(requested)) {
        assert!(
            matches!(
                error.kind(),
                io::ErrorKind::Unsupported | io::ErrorKind::InvalidInput
            ),
            "unexpected error setting fixture mtime: {error}"
        );
        eprintln!("pre-epoch mtime unsupported by fixture filesystem: {error}");
        return;
    }
    let actual = writer.metadata().unwrap().modified().unwrap();
    if actual != requested {
        eprintln!("pre-epoch mtime not retained by fixture filesystem: {actual:?}");
        return;
    }
    let ctx = context();
    let r = reader(&path, NodeKind::File, limits(), Arc::clone(&ctx));
    let file = r.open_file(Path::new("")).unwrap();

    let observed = file.last_modified().unwrap();
    assert_eq!(observed, actual);
    assert_eq!(
        std::time::UNIX_EPOCH.duration_since(observed).unwrap(),
        Duration::from_secs(60)
    );

    eprintln!("pre-epoch mtime qualified on fixture filesystem");
}

#[test]
fn metadata_test_hook_is_single_use_and_cleared_on_early_error() {
    let (_temp, r, ctx) = fixture(limits());
    let file = r.open_file(Path::new("data")).unwrap();
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let seen = std::rc::Rc::clone(&calls);
    {
        let _hook = metadata_hook(move |_| seen.set(seen.get() + 1));
        file.last_modified().unwrap();
        file.last_modified().unwrap();
        assert_eq!(calls.get(), 1);
    }
    {
        let _hook = metadata_hook(|_| panic!("cancelled entry must not observe metadata"));
        ctx.cancellation().cancel();
        assert_eq!(kind(file.last_modified()), io::ErrorKind::Interrupted);
    }
    METADATA_HOOK.with(|cell| assert!(cell.borrow().is_none()));
}
#[test]
fn whole_content_revalidation_and_cancel_preserve_source() {
    let (temp, r, ctx) = fixture(limits());
    r.revalidate_content().unwrap();

    assert_eq!(fs::read(temp.path().join("data")).unwrap(), b"0123456789");
    ctx.cancellation().cancel();
    assert_eq!(kind(r.revalidate_content()), io::ErrorKind::Interrupted);
}

#[cfg(unix)]
#[test]
fn complete_same_fd_observation_rejects_changes_even_when_identity_is_unchanged() {
    use std::os::unix::fs::PermissionsExt;
    for attack in 0..3 {
        let (_temp, r, _ctx) = fixture(limits());
        let f = r.open_file(Path::new("data")).unwrap();
        let fd = f.file.fd.write().unwrap();
        let old_id = crate::native_file_identity::handle_id(&fd).unwrap();
        let path = _temp.path().join("data");
        match attack {
            0 => fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap(),
            1 => fs::write(&path, b"short").unwrap(),
            _ => fs::write(&path, b"abcdefghij").unwrap(),
        }
        assert!(old_id == crate::native_file_identity::handle_id(&fd).unwrap());
        // Directly qualify the retained FD, so path/root fences cannot mask a
        // regression that compares only ID in this independent observation.
        assert_eq!(
            kind(
                r.shared
                    .snapshot
                    .plan()
                    .check_reader_handle(f.file.index, &fd)
            ),
            io::ErrorKind::InvalidData
        );
    }
}

#[test]
fn cancelled_or_expired_factory_and_first_access_never_admit_body_io() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("data");
    fs::write(&path, b"abc").unwrap();
    for expired in [false, true] {
        let cancellation = Cancellation::default();
        if !expired {
            cancellation.cancel();
        }
        let ctx = Arc::new(SnapshotContext::new(
            if expired {
                Instant::now() - Duration::from_secs(1)
            } else {
                Instant::now() + Duration::from_secs(60)
            },
            cancellation,
        ));
        let expected = if expired {
            io::ErrorKind::TimedOut
        } else {
            io::ErrorKind::Interrupted
        };
        assert_eq!(
            kind(CapturedSourceReader::capture(
                &path,
                NodeKind::File,
                &policy(&[]),
                limits(),
                Arc::clone(&ctx)
            )),
            expected
        );
    }
    let ctx = context();
    let r = reader(&path, NodeKind::File, limits(), Arc::clone(&ctx));
    let f = r.open_file(Path::new("")).unwrap();
    ctx.cancellation().cancel();
    assert_eq!(kind(f.read_range(0, 3)), io::ErrorKind::Interrupted);
    assert_eq!(kind(f.stream(0)), io::ErrorKind::Interrupted);
    assert_eq!(kind(r.open_file(Path::new(""))), io::ErrorKind::Interrupted);
}

#[test]
fn generated_metadata_reads_only_declared_ranges_without_payload_or_inventory_capture() {
    use std::io::{Seek, SeekFrom, Write};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("nodes.parquet");
    let mut output = fs::File::create(&path).unwrap();
    output.write_all(b"PAR1").unwrap();
    output.set_len(8 * 1024 * 1024 * 1024).unwrap();
    output.seek(SeekFrom::End(-4)).unwrap();
    output.write_all(b"tail").unwrap();
    drop(output);
    fs::create_dir(temp.path().join("unselected")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("missing", temp.path().join("unselected/dangling")).unwrap();
    let ctx = context();
    let reader = CapturedSourceReader::capture_generated_metadata(
        temp.path(),
        &[PathBuf::from("nodes.parquet")],
        SourceReadLimits::generated(16, 4).unwrap(),
        Arc::clone(&ctx),
    )
    .unwrap();
    assert_eq!(reader.nodes().count(), 2);
    assert!(reader.nodes().all(|node| node.digest().is_none()));
    assert_eq!(reader.retained_bytes(), 0);
    let file = reader.open_file(Path::new("nodes.parquet")).unwrap();
    assert_eq!(
        file.read_range(file.len() - 4, 4).unwrap().as_ref(),
        b"tail"
    );
    assert_eq!(
        reader
            .open_file(Path::new("unselected/dangling"))
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::NotFound
    );
    // New unrelated output is not part of this generation's declaration.
    fs::write(temp.path().join("other.parquet"), b"later").unwrap();
    reader.revalidate_bindings().unwrap();
    assert_eq!(
        reader.revalidate_content().unwrap_err().kind(),
        io::ErrorKind::Unsupported
    );
    fs::write(&path, b"changed").unwrap();
    assert!(file.read_range(0, 4).is_err());
}

#[test]
fn generated_metadata_rejects_links_escape_and_cancellation() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("data"), b"bytes").unwrap();
    let ctx = context();
    let limits = SourceReadLimits::generated(16, 4).unwrap();
    assert!(
        CapturedSourceReader::capture_generated_metadata(
            temp.path(),
            &[PathBuf::from("../data")],
            limits,
            Arc::clone(&ctx)
        )
        .is_err()
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("data", temp.path().join("link")).unwrap();
        assert!(
            CapturedSourceReader::capture_generated_metadata(
                temp.path(),
                &[PathBuf::from("link")],
                limits,
                Arc::clone(&ctx)
            )
            .is_err()
        );
    }
    let reader = CapturedSourceReader::capture_generated_metadata(
        temp.path(),
        &[PathBuf::from("data")],
        limits,
        Arc::clone(&ctx),
    )
    .unwrap();
    let file = reader.open_file(Path::new("data")).unwrap();
    ctx.cancellation().cancel();
    assert_eq!(
        file.read_range(0, 4).err().unwrap().kind(),
        io::ErrorKind::Interrupted
    );
}
