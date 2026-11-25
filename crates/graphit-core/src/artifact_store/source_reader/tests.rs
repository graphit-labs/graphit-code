use super::*;
use crate::{
    artifact_store::{CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet},
    file_lock::Cancellation,
};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
fn policy(selected: &[PathBuf]) -> SnapshotPolicy {
    let retention = RetainedBytesLimits::new(1024, 8).unwrap();
    SnapshotPolicy::new(
        CopyPolicy::FollowLinks,
        CopyLimits::new(4 * 1024 * 1024, 64).unwrap(),
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
    result.err().expect("operation must fail").kind()
}
#[test]
fn positive_limits_and_send_sync_files() {
    for index in 0..3 {
        let mut values = [1; 3];
        values[index] = 0;
        assert_eq!(
            kind(SourceReadLimits::new(
                values[0],
                values[1],
                values[2] as u64
            )),
            io::ErrorKind::InvalidInput
        );
    }
    fn traits<T: Send + Sync>() {}
    traits::<CapturedSourceReader>();
    traits::<CapturedSourceFile>();
    traits::<SourceRangeStream>();
}
#[test]
fn streams_are_positional_and_keep_file_after_reader_drop() {
    let (temp, r, _ctx) = fixture(limits());
    let file = r.open_file(Path::new("data")).unwrap();
    let mut first = file.stream(1).unwrap();
    let mut second = file.stream(5).unwrap();
    drop(file);
    drop(r);
    let mut bytes = [0; 3];
    first.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"123");
    second.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"567");
    assert_eq!(fs::read(temp.path().join("data")).unwrap(), b"0123456789");
}
#[test]
fn stream_limits_are_errors_and_real_eof_is_empty() {
    let (_temp, r, _ctx) = fixture(SourceReadLimits::new(10, 2, 3).unwrap());
    let file = r.open_file(Path::new("data")).unwrap();
    let mut stream = file.stream(0).unwrap();
    let mut bytes = [0; 3];
    stream.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"012");
    assert_eq!(kind(stream.read(&mut bytes)), io::ErrorKind::FileTooLarge);
    assert_eq!(file.stream(10).unwrap().read(&mut bytes).unwrap(), 0);
    assert_eq!(kind(file.stream(11)), io::ErrorKind::InvalidInput);
}
#[test]
fn cancellation_stops_existing_stream_and_metadata() {
    let (_temp, r, ctx) = fixture(limits());
    let file = r.open_file(Path::new("data")).unwrap();
    let mut stream = file.stream(0).unwrap();
    let mut bytes = [0; 3];
    assert_eq!(stream.read(&mut bytes).unwrap(), 3);
    ctx.cancellation().cancel();
    assert_eq!(kind(stream.read(&mut bytes)), io::ErrorKind::Interrupted);
    assert_eq!(kind(file.last_modified()), io::ErrorKind::Interrupted);
    assert_eq!(kind(file.read_range(0, 1)), io::ErrorKind::Interrupted);
}
#[test]
fn content_hash_is_explicit_and_not_a_directory_or_request_identity_proof() {
    let (_temp, r, ctx) = fixture(limits());
    let fresh = context();
    r.validate_file_bytes(Path::new("data"), b"0123456789", &fresh)
        .unwrap();
    assert_eq!(
        kind(r.validate_file_bytes(Path::new("data"), b"012345678x", &ctx)),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        kind(r.validate_file_bytes(Path::new(""), b"", &ctx)),
        io::ErrorKind::InvalidInput
    );
}
#[test]
fn a_new_request_validates_immutable_bytes_after_the_capture_request_ends() {
    let (_temp, r, captured) = fixture(limits());
    let file = r.open_file(Path::new("data")).unwrap();
    captured.cancellation().cancel();
    let current = context();
    r.validate_file_bytes(Path::new("data"), b"0123456789", &current)
        .unwrap();
    assert_eq!(
        kind(r.validate_file_bytes(Path::new("data"), b"012345678x", &current)),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        kind(r.open_file(Path::new("data"))),
        io::ErrorKind::Interrupted
    );
    assert_eq!(kind(file.stream(0)), io::ErrorKind::Interrupted);
    current.cancellation().cancel();
    assert_eq!(
        kind(r.validate_file_bytes(Path::new("data"), b"0123456789", &current)),
        io::ErrorKind::Interrupted
    );
}
#[cfg(unix)]
#[test]
fn source_reader_rejects_a_non_unicode_root_without_lossy_conversion() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(OsString::from_vec(vec![b'r', 0xff]));
    assert!(root.to_str().is_none());
    let native_rejection = match fs::create_dir(&root) {
        Ok(()) => {
            fs::write(root.join("data"), b"body").unwrap();
            None
        }
        Err(error) => {
            assert_eq!(
                error.raw_os_error(),
                Some(rustix::io::Errno::ILSEQ.raw_os_error())
            );
            Some(error)
        }
    };
    let ctx = context();
    let result = CapturedSourceReader::capture(
        &root,
        NodeKind::Directory,
        &policy(&[]),
        limits(),
        ctx.clone(),
    );
    let captured = result.err().unwrap();
    let generated =
        CapturedSourceReader::capture_generated(&root, NodeKind::Directory, limits(), ctx)
            .err()
            .unwrap();
    for error in [captured, generated] {
        match &native_rejection {
            Some(_) => {
                // Creation rejects the byte sequence, while lookup reports absence.
                // SourcePlan propagates that lookup error before reader validation.
                let metadata_error = fs::metadata(&root).unwrap_err();
                assert_eq!(
                    metadata_error.raw_os_error(),
                    Some(rustix::io::Errno::NOENT.raw_os_error())
                );
                assert_eq!(error.raw_os_error(), metadata_error.raw_os_error());
                assert_eq!(error.kind(), metadata_error.kind());
            }
            None => {
                assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
                assert!(error.to_string().contains("not Unicode"));
            }
        }
    }
}
#[cfg(unix)]
#[test]
#[ignore = "isolated child of open_file_rejects_an_already_present_fifo_without_blocking"]
fn fifo_open_probe() {
    let root = PathBuf::from(
        std::env::var_os("GRAPHIT_SOURCE_FIFO_PROBE")
            .expect("isolated FIFO probe requires its parent"),
    );
    let r = reader(&root, NodeKind::Directory, limits(), context());
    let path = root.join("data");
    fs::remove_file(&path).unwrap();
    #[cfg(not(target_os = "macos"))]
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &path,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::FileTypeExt;
        assert!(
            std::process::Command::new("/usr/bin/mkfifo")
                .args(["-m", "600"])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        assert!(fs::symlink_metadata(&path).unwrap().file_type().is_fifo());
    }
    assert_eq!(
        kind(r.open_file(Path::new("data"))),
        io::ErrorKind::InvalidInput
    );
}
#[cfg(unix)]
#[test]
fn open_file_rejects_an_already_present_fifo_without_blocking() {
    use std::process::{Child, Command, Stdio};
    struct Probe(Child);
    impl Drop for Probe {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    // The parent owns cleanup even when the child must be killed on regression.
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("data"), b"0123456789").unwrap();
    let mut probe = Probe(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "artifact_store::source_reader::tests::fifo_open_probe",
                "--ignored",
            ])
            .env("GRAPHIT_SOURCE_FIFO_PROBE", temp.path())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = probe.0.try_wait().unwrap() {
            assert!(status.success(), "isolated FIFO probe failed: {status}");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "opening an already-present FIFO blocked; child will be terminated"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn metadata_manifest_never_invents_content_digest() {
    let (temp, _, ctx) = fixture(limits());
    let r = CapturedSourceReader::capture_generated_metadata(
        temp.path(),
        &[PathBuf::from("data")],
        limits(),
        ctx.clone(),
    )
    .unwrap();
    assert!(r.nodes().all(|node| node.digest().is_none()));
    assert_eq!(
        kind(r.validate_file_bytes(Path::new("data"), b"0123456789", &ctx)),
        io::ErrorKind::Unsupported
    );
    assert_eq!(
        r.open_file(Path::new("data"))
            .unwrap()
            .read_range(2, 3)
            .unwrap()
            .as_ref(),
        b"234"
    );
    fs::write(temp.path().join("data"), b"abcdefghij").unwrap();
    let mut current = Vec::new();
    r.open_file(Path::new("data"))
        .unwrap()
        .stream(0)
        .unwrap()
        .read_to_end(&mut current)
        .unwrap();
    assert_eq!(current, b"abcdefghij");
}
#[test]
fn file_changes_use_real_open_handle_and_explicit_hash_check() {
    let (temp, r, ctx) = fixture(limits());
    let file = r.open_file(Path::new("data")).unwrap();
    let target = temp.path().join("other");
    fs::write(&target, b"new-content").unwrap();
    fs::rename(&target, temp.path().join("data")).unwrap();
    assert_eq!(file.read_range(0, 3).unwrap().as_ref(), b"012");
    let current = r.open_file(Path::new("data")).unwrap();
    let body = current.read_range(0, 11).unwrap();
    assert_eq!(
        kind(r.validate_file_bytes(Path::new("data"), body.as_ref(), &ctx)),
        io::ErrorKind::InvalidData
    );
}
#[test]
fn complete_stream_rejects_changed_bytes_before_open_and_stays_failed() {
    let (temp, r, _ctx) = fixture(limits());
    fs::write(temp.path().join("data"), b"abcdefghij").unwrap();
    let file = r.open_file(Path::new("data")).unwrap();
    let mut stream = file.stream(0).unwrap();
    assert_eq!(
        kind(stream.read_to_end(&mut Vec::new())),
        io::ErrorKind::InvalidData
    );
    assert_eq!(kind(stream.read(&mut [0; 1])), io::ErrorKind::InvalidData);
    // A suffix has no claim to the complete manifest's digest.
    let mut suffix = Vec::new();
    file.stream(1).unwrap().read_to_end(&mut suffix).unwrap();
    assert_eq!(suffix, b"bcdefghij");
}
#[test]
fn complete_stream_accepts_identical_content_after_path_replacement() {
    let (temp, r, _ctx) = fixture(limits());
    let replacement = temp.path().join("replacement");
    fs::write(&replacement, b"0123456789").unwrap();
    fs::remove_file(temp.path().join("data")).unwrap();
    fs::rename(replacement, temp.path().join("data")).unwrap();
    let file = r.open_file(Path::new("data")).unwrap();
    let mut output = Vec::new();
    file.stream(0).unwrap().read_to_end(&mut output).unwrap();
    assert_eq!(output, b"0123456789");
}
#[test]
fn content_changes_after_complete_eof_do_not_revoke_the_finished_read() {
    let (temp, r, _ctx) = fixture(limits());
    let file = r.open_file(Path::new("data")).unwrap();
    let mut stream = file.stream(0).unwrap();
    let mut output = Vec::new();
    stream.read_to_end(&mut output).unwrap();
    fs::write(temp.path().join("data"), b"abcdefghij").unwrap();
    assert_eq!(stream.read(&mut [0; 1]).unwrap(), 0);
    assert_eq!(output, b"0123456789");
    assert_eq!(
        kind(file.stream(0).unwrap().read_to_end(&mut Vec::new())),
        io::ErrorKind::InvalidData
    );
}
#[test]
fn complete_stream_checks_actual_eof_and_recorded_length() {
    let (temp, r, _ctx) = fixture(limits());
    let file = r.open_file(Path::new("data")).unwrap();
    // Growth after open cannot hide behind the length observed by that open.
    fs::write(temp.path().join("data"), b"0123456789x").unwrap();
    assert_eq!(
        kind(file.stream(0).unwrap().read_to_end(&mut Vec::new())),
        io::ErrorKind::InvalidData
    );
    fs::write(temp.path().join("data"), b"012").unwrap();
    let shorter = r.open_file(Path::new("data")).unwrap();
    assert_eq!(
        kind(shorter.stream(0).unwrap().read_to_end(&mut Vec::new())),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        kind(file.stream(0).unwrap().read_to_end(&mut Vec::new())),
        io::ErrorKind::UnexpectedEof
    );
}
#[test]
fn complete_empty_stream_validates_the_empty_digest() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("empty");
    fs::write(&path, b"").unwrap();
    let r = reader(&path, NodeKind::File, limits(), context());
    let file = r.open_file(Path::new("")).unwrap();
    assert_eq!(file.stream(0).unwrap().read(&mut [0; 1]).unwrap(), 0);
}
#[test]
fn current_modification_time_and_hardlinks_are_ordinary_files() {
    let (temp, r, _ctx) = fixture(limits());
    let file = r.open_file(Path::new("data")).unwrap();
    fs::hard_link(temp.path().join("data"), temp.path().join("alias")).unwrap();
    assert_eq!(
        file.last_modified().unwrap(),
        fs::metadata(temp.path().join("alias"))
            .unwrap()
            .modified()
            .unwrap()
    );
    assert_eq!(file.read_range(0, 2).unwrap().as_ref(), b"01");
}
#[test]
fn expired_capture_returns_deadline_before_opening_source() {
    let ctx = Arc::new(SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    ));
    assert_eq!(
        kind(CapturedSourceReader::capture_generated(
            Path::new("missing"),
            NodeKind::File,
            limits(),
            ctx
        )),
        io::ErrorKind::TimedOut
    );
}
