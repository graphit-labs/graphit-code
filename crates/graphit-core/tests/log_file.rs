use graphit_core::{
    brand::Brand,
    file_lock::{Cancellation, FileLock},
    log_file::{AppendOutcome, Durability, Limits, LogError, LogFile, LogPaths, Options, Stage},
    project_paths::{InstallationPaths, ProjectPaths},
};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

mod support;
use support::Fixture;

fn options(file: usize, record: usize, batch: usize, retain: usize) -> Options {
    Options {
        limits: Limits {
            max_file_bytes: file,
            max_record_bytes: record,
            max_batch_bytes: batch,
            retain_bytes: retain,
        },
        durability: Durability::Buffered,
        lock_timeout: Duration::from_secs(3),
    }
}
fn open(path: &Path, options: Options) -> LogFile {
    LogFile::open(
        LogPaths::explicit(path).unwrap(),
        options,
        Cancellation::default(),
    )
    .unwrap()
}
fn assert_io(error: LogError, stage: Stage, kind: io::ErrorKind) {
    assert!(
        matches!(error, LogError::Io { stage: s, kind: k, .. } if s == stage && k == kind),
        "{error:?}"
    );
}
#[cfg(unix)]
#[test]
fn project_runtime_shared_with_identity_and_core_is_rechecked_on_log_reopen() {
    use graphit_core::artifact_store::{
        ArtifactLimits, ArtifactStore, CapturedPublicationScope, CopyLimits, ScopeLimits,
    };
    use graphit_core::project_identity::{ProjectIdentityStore, ProjectMetadata};
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let project = fixture.project();
    fs::create_dir(project.brand_directory()).unwrap();
    fs::set_permissions(project.brand_directory(), fs::Permissions::from_mode(0o755)).unwrap();
    let runtime = project
        .project_runtime("01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(), &[])
        .unwrap();
    let log = LogFile::open(
        LogPaths::project(
            &project,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            "ast",
        )
        .unwrap(),
        options(1024, 128, 256, 512),
        Cancellation::default(),
    )
    .unwrap();
    graphit_fs::verify_private_directory(&File::open(&runtime).unwrap()).unwrap();
    let identity = ProjectIdentityStore::at(&project)
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap()
        .ensure(&ProjectMetadata::new("logged").unwrap())
        .unwrap();
    let scope = CapturedPublicationScope::project(
        &project,
        identity.id(),
        ScopeLimits::new(1, 16 * 1024, 1024).unwrap(),
        &project_birth_request(),
    )
    .unwrap();
    let copy = CopyLimits::new(8192, 32, 4096, 8).unwrap();
    let artifacts = ArtifactStore::new(scope, ArtifactLimits::new(64 * 1024, 1024, copy).unwrap());
    let writer = artifacts
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap();
    assert!(writer.load().unwrap().is_none());

    assert!(!runtime.join("artifact-store.json").exists());
    drop(writer);
    assert_eq!(
        fs::metadata(project.brand_directory())
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o755
    );
    let prepared = log.paths().clone();
    drop(log);
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o755)).unwrap();
    let error = LogFile::open(
        prepared.clone(),
        options(1024, 128, 256, 512),
        Cancellation::default(),
    )
    .unwrap_err();
    assert_io(error, Stage::Directories, io::ErrorKind::PermissionDenied);
    assert_eq!(fs::read(runtime.join("logs/ast.log")).unwrap(), b"");
    assert_eq!(
        fs::metadata(&runtime).unwrap().permissions().mode() & 0o7777,
        0o755
    );
    let moved = fixture.path("moved-runtime");
    fs::rename(&runtime, &moved).unwrap();
    let error = LogFile::open(
        prepared,
        options(1024, 128, 256, 512),
        Cancellation::default(),
    )
    .unwrap_err();
    assert_io(error, Stage::Directories, io::ErrorKind::NotFound);
    assert!(!runtime.exists());
    assert_eq!(fs::read(moved.join("logs/ast.log")).unwrap(), b"");
}

#[cfg(unix)]
#[test]
fn project_logging_rejects_public_runtime_without_creating_logs_or_repairing_it() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let project = fixture.project();
    let runtime = project
        .project_runtime("01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(), &[])
        .unwrap();
    fs::create_dir_all(&runtime).unwrap();
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(runtime.join("foreign"), b"preserve").unwrap();
    let error = LogFile::open(
        LogPaths::project(
            &project,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            "ast",
        )
        .unwrap(),
        options(1024, 128, 256, 512),
        Cancellation::default(),
    )
    .unwrap_err();
    assert_io(error, Stage::Directories, io::ErrorKind::PermissionDenied);
    assert!(!runtime.join("logs").exists());
    assert_eq!(fs::read(runtime.join("foreign")).unwrap(), b"preserve");
    assert_eq!(
        fs::metadata(runtime).unwrap().permissions().mode() & 0o7777,
        0o755
    );
}

#[test]
fn managed_scopes_brands_and_explicit_override_are_captured() {
    let fixture = Fixture::new();
    for brand in ["graphit", "acme-code"] {
        let install = InstallationPaths::resolve(
            Brand::new(brand).unwrap(),
            fixture.root(),
            None,
            Some(&fixture.path(&format!("global-{brand}"))),
        )
        .unwrap();
        let project = ProjectPaths::at(fixture.root(), &install).unwrap();
        let paths = LogPaths::project(
            &project,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            "ast",
        )
        .unwrap();
        assert_eq!(
            paths.path(),
            install
                .root()
                .join("projects/01ARZ3NDEKTSV4RRFFQ69G5FAV/logs/ast.log")
        );
        let log =
            LogFile::open(paths, options(1024, 128, 256, 512), Cancellation::default()).unwrap();
        assert_eq!(fs::metadata(log.paths().path()).unwrap().len(), 0);
        log.append(b"project\n").unwrap();
        let global = LogFile::open(
            LogPaths::installation(&install, "framework").unwrap(),
            options(1024, 128, 256, 512),
            Cancellation::default(),
        )
        .unwrap();
        global.append(b"global\n").unwrap();
        assert_eq!(fs::read(log.paths().path()).unwrap(), b"project\n");
        assert_eq!(fs::read(global.paths().path()).unwrap(), b"global\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for dir in [
                install.root().join("projects"),
                project
                    .project_runtime("01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(), &[])
                    .unwrap(),
                log.paths().path().parent().unwrap().to_owned(),
                install.root().to_owned(),
                global.paths().path().parent().unwrap().to_owned(),
            ] {
                assert_eq!(
                    fs::metadata(dir).unwrap().permissions().mode() & 0o777,
                    0o700
                );
            }
        }
    }
    let override_paths = LogPaths::explicit(fixture.path("override.log")).unwrap();
    let a = LogFile::open(
        override_paths.clone(),
        options(1024, 128, 256, 512),
        Cancellation::default(),
    )
    .unwrap();
    let b = LogFile::open(
        override_paths,
        options(1024, 128, 256, 512),
        Cancellation::default(),
    )
    .unwrap();
    a.append(b"first\n").unwrap();
    b.append(b"second\n").unwrap();
    assert_eq!(fs::read(a.paths().path()).unwrap(), b"first\nsecond\n");
    assert_eq!(a.paths().lock_path(), b.paths().lock_path());
    assert!(LogPaths::explicit("relative.log").is_err());
    assert!(LogPaths::explicit(fixture.path("missing/override.log")).is_err());
    assert!(!fixture.path("missing").exists());
    let paths = fixture.project();
    for invalid in [
        "",
        "../escape",
        "nul",
        "con",
        "com1",
        "lpt9",
        "UPPER",
        "a.b",
        "a/b",
        "á",
        &"a".repeat(65),
    ] {
        assert_eq!(
            LogPaths::project(
                &paths,
                "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
                invalid
            )
            .unwrap_err(),
            LogError::InvalidPath
        );
    }
}

#[test]
fn options_and_cancel_validate_before_io_and_empty_append_is_a_noop() {
    let fixture = Fixture::new();
    let path = fixture.path("absent.log");
    for invalid in [
        options(0, 1, 1, 0),
        options(8, 0, 1, 0),
        options(8, 4, 3, 0),
        options(8, 4, 9, 0),
        options(8, 4, 4, 5),
        options(usize::MAX, usize::MAX, usize::MAX, 0),
        Options {
            lock_timeout: Duration::MAX,
            ..options(8, 4, 4, 4)
        },
    ] {
        assert_eq!(
            LogFile::open(
                LogPaths::explicit(&path).unwrap(),
                invalid,
                Cancellation::default()
            )
            .unwrap_err(),
            LogError::InvalidOptions
        );
        assert!(!path.exists());
    }
    let cancelled = Cancellation::default();
    cancelled.cancel();
    assert_io(
        LogFile::open(
            LogPaths::project(
                &fixture.project(),
                "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
                "cancel",
            )
            .unwrap(),
            options(8, 4, 4, 4),
            cancelled,
        )
        .unwrap_err(),
        Stage::Lock,
        io::ErrorKind::Interrupted,
    );
    assert!(!fixture.path(".graphit").exists());
    let token = Cancellation::default();
    let log = LogFile::open(
        LogPaths::explicit(&path).unwrap(),
        options(12, 4, 8, 4),
        token.clone(),
    )
    .unwrap();
    fs::remove_file(&path).unwrap();
    token.cancel();
    assert_eq!(log.append(b"").unwrap(), AppendOutcome::default());
    assert!(!path.exists());
    assert_io(
        log.append(b"ok\n").unwrap_err(),
        Stage::Lock,
        io::ErrorKind::Interrupted,
    );
}

#[test]
fn batches_are_complete_utf8_and_all_limits_count_the_lf() {
    let fixture = Fixture::new();
    let path = fixture.path("limits.log");
    let log = open(&path, options(12, 4, 8, 4));
    assert_eq!(log.append(b"abc\nabc\n").unwrap().records, 2);
    for (bytes, error) in [
        (b"abcd\n".as_slice(), LogError::RecordLimit),
        (b"a\nb\nc\nd\nx".as_slice(), LogError::RecordLimit),
        (b"abc".as_slice(), LogError::InvalidRecord),
        (b"\xff\n".as_slice(), LogError::InvalidRecord),
    ] {
        assert_eq!(log.append(bytes).unwrap_err(), error);
        assert_eq!(fs::read(&path).unwrap(), b"abc\nabc\n");
    }
    log.append(b"x\r\n\n").unwrap();
    assert_eq!(fs::metadata(&path).unwrap().len(), 12);
    assert!(log.append(b"abc\n").unwrap().compacted);
    assert!(fs::metadata(path).unwrap().len() <= 12);
}

#[test]
fn compaction_keeps_exact_boundaries_and_latest_whole_unicode_records() {
    let fixture = Fixture::new();
    for (name, limit, source, new, expected) in [
        (
            "exact",
            options(9, 3, 3, 6),
            "aa\nbb\ncc\n",
            "dd\n",
            "bb\ncc\ndd\n",
        ),
        (
            "cut",
            options(12, 4, 4, 8),
            "aaa\nbb\ncc\ndd\n",
            "eee\n",
            "cc\ndd\neee\n",
        ),
        (
            "unicode",
            options(12, 4, 4, 8),
            "中\n文\n字\n",
            "新\n",
            "文\n字\n新\n",
        ),
        ("zero", options(8, 4, 4, 0), "aaa\nbbb\n", "ccc\n", "ccc\n"),
    ] {
        let path = fixture.path(name);
        fs::write(&path, source).unwrap();
        let log = open(&path, limit);
        let clone = log.clone();
        let result = log.append(new.as_bytes()).unwrap();
        assert!(result.compacted);
        assert_eq!(result.recovered_bytes, 0);
        assert_eq!(
            result.discarded_bytes,
            (source.len() + new.len() - expected.len()) as u64
        );
        assert_eq!(fs::read(&path).unwrap(), expected.as_bytes());
        for _ in 0..8 {
            clone.append(new.as_bytes()).unwrap();
        }
        assert!(fs::metadata(&path).unwrap().len() <= limit.limits.max_file_bytes as u64);
        assert!(fs::read_to_string(&path).unwrap().ends_with(new));
        assert!(log.paths().lock_path().exists());
    }
}

#[test]
fn huge_sparse_source_only_needs_a_bounded_tail() {
    let fixture = Fixture::new();
    let path = fixture.path("sparse.log");
    let mut source = File::create(&path).unwrap();
    let size = 4_u64 * 1024 * 1024 * 1024;
    source.set_len(size).unwrap();
    source.seek(SeekFrom::Start(size - 5)).unwrap();
    source.write_all(b"\nend\n").unwrap();
    drop(source);
    let log = open(&path, options(128, 8, 8, 64));
    let outcome = log.append(b"new\n").unwrap();
    assert!(outcome.compacted);
    assert_eq!(outcome.discarded_bytes, size - 4);
    assert_eq!(fs::read(path).unwrap(), b"end\nnew\n");
}

#[test]
fn partial_eof_recovers_only_a_known_boundary_and_invalid_tail_is_rejected() {
    let fixture = Fixture::new();
    for (name, source, recovered, expected) in [
        ("prefix", "ok\npart", 4, "ok\nnew\n"),
        ("first", "part", 4, "new\n"),
        ("max", "12345678", 8, "new\n"),
    ] {
        let path = fixture.path(name);
        fs::write(&path, source).unwrap();
        let outcome = open(&path, options(32, 8, 8, 16)).append(b"new\n").unwrap();
        assert_eq!(outcome.recovered_bytes, recovered);
        assert_eq!(fs::read_to_string(path).unwrap(), expected);
    }
    for bytes in [b"123456789".as_slice(), b"ok\n123456789".as_slice()] {
        let path = fixture.path("corrupt");
        fs::write(&path, bytes).unwrap();
        assert_eq!(
            open(&path, options(32, 8, 8, 16))
                .append(b"new\n")
                .unwrap_err(),
            LogError::Corrupt
        );
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    let path = fixture.path("invalid-utf8");
    fs::write(&path, b"a\n\xff\n").unwrap();
    assert_eq!(
        open(&path, options(4, 2, 2, 2)).append(b"b\n").unwrap_err(),
        LogError::Corrupt
    );
    assert_eq!(fs::read(path).unwrap(), b"a\n\xff\n");
}

#[test]
fn readonly_nonregular_and_removed_targets_fail_without_fallback() {
    let fixture = Fixture::new();
    let path = fixture.path("private-payload.log");
    let mut log = open(&path, options(1024, 128, 256, 512));
    log.append(b"private-payload\n").unwrap();
    let original = fs::metadata(&path).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    let error = log.append(b"new\n").unwrap_err();
    assert_io(error, Stage::Open, io::ErrorKind::PermissionDenied);
    let wrapped = log.write(b"new\n").unwrap_err();
    assert_eq!(wrapped.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(wrapped.raw_os_error(), None);
    for text in [
        format!("{log:?}"),
        format!("{error:?}"),
        error.to_string(),
        format!("{wrapped:?}"),
        wrapped.to_string(),
    ] {
        assert!(!text.contains("private-payload"));
        assert!(!text.contains(fixture.root().to_str().unwrap()));
    }
    assert_eq!(fs::read(&path).unwrap(), b"private-payload\n");
    fs::set_permissions(&path, original).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(log.append(b"new\n").is_err());
    assert!(log.sync().is_err());
    fs::remove_dir(&path).unwrap();
    fs::remove_file(log.paths().lock_path()).unwrap();
    fs::create_dir(log.paths().lock_path()).unwrap();
    assert!(log.append(b"new\n").is_err());
    assert!(!path.exists());
}

#[test]
fn managed_creation_rejects_non_directory_and_preserves_existing_directories() {
    let fixture = Fixture::new();
    let project = fixture.project();
    fs::create_dir(project.brand_directory()).unwrap();
    let original = fs::metadata(project.brand_directory())
        .unwrap()
        .permissions();
    let runtime = project
        .project_runtime("01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(), &[])
        .unwrap();
    fs::create_dir_all(runtime.parent().unwrap()).unwrap();
    fs::write(&runtime, b"file").unwrap();
    assert!(
        LogFile::open(
            LogPaths::project(
                &project,
                "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
                "ast"
            )
            .unwrap(),
            options(1024, 128, 256, 512),
            Cancellation::default()
        )
        .is_err()
    );
    assert_eq!(fs::read(&runtime).unwrap(), b"file");
    fs::remove_file(&runtime).unwrap();
    #[cfg(unix)]
    {
        let outside = fixture.path("outside");
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, &runtime).unwrap();
        assert!(
            LogFile::open(
                LogPaths::project(
                    &project,
                    "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
                    "ast"
                )
                .unwrap(),
                options(1024, 128, 256, 512),
                Cancellation::default()
            )
            .is_err()
        );
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        fs::remove_file(&runtime).unwrap();
    }
    LogFile::open(
        LogPaths::project(
            &project,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            "ast",
        )
        .unwrap(),
        options(1024, 128, 256, 512),
        Cancellation::default(),
    )
    .unwrap();
    assert_eq!(
        fs::metadata(project.brand_directory())
            .unwrap()
            .permissions(),
        original
    );
}

#[test]
fn write_and_flush_report_replaced_target_io_errors() {
    let fixture = Fixture::new();
    let path = fixture.path("replaced.log");
    let mut log = open(&path, options(65536, 4096, 4096, 32768));
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(log.write_all(b"safe dto\n").is_err());
    assert!(log.flush().is_err());
    assert!(log.sync().is_err());
    assert!(path.is_dir());
    assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn unix_permissions_symlinks_parent_aliases_and_native_long_names() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let fixture = Fixture::new();
    let path = fixture.path("permissions.log");
    fs::write(&path, b"old\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
    let log = open(&path, options(9, 3, 3, 6));
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(log.paths().lock_path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let alias = fixture.path("alias");
    symlink(fixture.root(), &alias).unwrap();
    assert_eq!(
        LogPaths::explicit(alias.join("permissions.log"))
            .unwrap()
            .lock_path(),
        log.paths().lock_path()
    );
    let link = fixture.path("link.log");
    symlink(&path, &link).unwrap();
    assert!(
        LogFile::open(
            LogPaths::explicit(&link).unwrap(),
            options(9, 3, 3, 6),
            Cancellation::default()
        )
        .is_err()
    );
    fs::remove_file(&link).unwrap();
    let lock_link = log.paths().lock_path();
    fs::remove_file(&lock_link).unwrap();
    symlink(&path, &lock_link).unwrap();
    assert!(log.append(b"aa\n").is_err());
    fs::remove_file(&lock_link).unwrap();
    symlink(fixture.root(), fixture.path(".graphit")).unwrap();
    assert!(ProjectPaths::at(fixture.root(), &fixture.installation()).is_err());
    fs::remove_file(fixture.path(".graphit")).unwrap();
    #[cfg(target_os = "linux")]
    let native = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec([vec![b'x'; 240], vec![0xff]].concat())
    };
    #[cfg(not(target_os = "linux"))]
    let native = format!("{}é", "x".repeat(240));
    let long = open(&fixture.root().join(native), options(9, 3, 3, 6));
    assert_eq!(long.paths().lock_path().file_name().unwrap().len(), 74);
    long.append(b"aa\nbb\ncc\n").unwrap_err(); // Batch is limited independently of the file.
    for bytes in [b"aa\n", b"bb\n", b"cc\n", b"dd\n"] {
        long.append(bytes).unwrap();
    }
    assert_eq!(fs::read(long.paths().path()).unwrap(), b"bb\ncc\ndd\n");
    assert_eq!(
        fs::metadata(long.paths().path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn child(path: &Path, ready: &Path, mode: &str, worker: usize) -> OwnedChild {
    OwnedChild(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "log_child", "--nocapture"])
            .env("GRAPHIT_RUST_LOG_FIXTURE_MODE", mode)
            .env("GRAPHIT_RUST_LOG_FIXTURE_PATH", path)
            .env("GRAPHIT_RUST_LOG_FIXTURE_READY", ready)
            .env("GRAPHIT_RUST_LOG_FIXTURE_WORKER", worker.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    )
}
fn wait_ready(child: &mut OwnedChild, ready: &Path) {
    let start = Instant::now();
    while !ready.exists() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "child exited before ready"
        );
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "child readiness deadline"
        );
        thread::sleep(Duration::from_millis(5));
    }
}
fn wait_exit(child: &mut OwnedChild) {
    let start = Instant::now();
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "child exit deadline"
        );
        thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn four_processes_append_four_hundred_complete_unique_records() {
    let fixture = Fixture::new();
    let path = fixture.path("process.log");
    let log = open(&path, options(1024 * 1024, 256, 256, 512 * 1024));
    fs::write(log.paths().lock_path(), b"permanent lock identity").unwrap();
    let mut children: Vec<_> = (0..4)
        .map(|worker| {
            let ready = fixture.path(&format!("ready-{worker}"));
            (child(&path, &ready, "append", worker), ready)
        })
        .collect();
    for (child, ready) in &mut children {
        wait_ready(child, ready);
    }
    for (child, _) in &mut children {
        child.0.stdin.as_mut().unwrap().write_all(b"g").unwrap();
    }
    for (child, _) in &mut children {
        wait_exit(child);
    }
    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.ends_with('\n'));
    let values: Vec<Value> = contents
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(values.len(), 400);
    let ids: HashSet<_> = values
        .iter()
        .map(|v| {
            (
                v["worker"].as_u64().unwrap(),
                v["sequence"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(ids.len(), 400);
    for worker in 0..4 {
        for sequence in 0..100 {
            assert!(ids.contains(&(worker, sequence)));
        }
    }
    assert_eq!(
        fs::read(log.paths().lock_path()).unwrap(),
        b"permanent lock identity"
    );
}

#[test]
fn killed_writer_releases_lock_and_next_append_repairs_partial_eof() {
    let fixture = Fixture::new();
    let path = fixture.path("crash.log");
    let ready = fixture.path("ready");
    let log = open(&path, options(1024 * 1024, 256, 256, 512 * 1024));
    let mut holder = child(&path, &ready, "crash", 0);
    wait_ready(&mut holder, &ready);
    assert!(FileLock::is_locked(log.paths().lock_path()).unwrap());
    let start = Instant::now();
    let short = Options {
        lock_timeout: Duration::from_millis(40),
        ..options(1024 * 1024, 256, 256, 512 * 1024)
    };
    assert_io(
        LogFile::open(log.paths().clone(), short, Cancellation::default()).unwrap_err(),
        Stage::Lock,
        io::ErrorKind::TimedOut,
    );
    assert!(start.elapsed() < Duration::from_secs(2));
    let token = Cancellation::default();
    let cancelled_log = LogFile::open(
        LogPaths::explicit(fixture.path("cancelled.log")).unwrap(),
        options(1024, 128, 128, 512),
        token.clone(),
    )
    .unwrap();
    let cancel_lock = FileLock::try_acquire(cancelled_log.paths().lock_path()).unwrap();
    thread::scope(|scope| {
        let waiter = scope.spawn(|| cancelled_log.append(b"not written\n"));
        thread::sleep(Duration::from_millis(30));
        token.cancel();
        assert_io(
            waiter.join().unwrap().unwrap_err(),
            Stage::Lock,
            io::ErrorKind::Interrupted,
        );
    });
    drop(cancel_lock);
    assert_eq!(fs::read(cancelled_log.paths().path()).unwrap(), b"");
    holder.0.kill().unwrap();
    holder.0.wait().unwrap();
    assert!(!FileLock::is_locked(log.paths().lock_path()).unwrap());
    let result = log.append(b"new\n").unwrap();
    assert_eq!(result.recovered_bytes, 7);
    assert_eq!(fs::read(&path).unwrap(), b"complete\nnew\n");
}

#[test]
fn orphan_cleanup_is_exclusive_and_does_not_scan_on_each_append() {
    let fixture = Fixture::new();
    let path = fixture.path("orphan.log");
    let log = open(&path, options(9, 3, 3, 6));
    let lock_name = log
        .paths()
        .lock_path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let prefix = format!("{}.compact-", lock_name.strip_suffix(".lock").unwrap());
    let owned = fixture.path(&format!("{prefix}Ab0123456789CdEf"));
    assert_eq!(
        LogPaths::explicit(log.paths().lock_path()).unwrap_err(),
        LogError::InvalidPath
    );
    assert_eq!(
        LogPaths::explicit(&owned).unwrap_err(),
        LogError::InvalidPath
    );
    assert_eq!(
        LogPaths::explicit(fixture.path(".LOG-override")).unwrap_err(),
        LogError::InvalidPath
    );
    let user = fixture.path(&format!("{prefix}user-data"));
    let other = fixture.path(".log-other.compact-Ab0123456789CdEf");
    fs::write(&owned, b"orphan").unwrap();
    fs::write(&user, b"user").unwrap();
    fs::write(&other, b"other").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&user, fixture.path(&format!("{prefix}Sy0123456789Link")))
            .unwrap();
    }
    log.append(b"aa\n").unwrap();
    assert!(owned.exists());
    let reopened = open(&path, options(9, 3, 3, 6));
    assert!(!owned.exists());
    fs::write(&owned, b"new orphan").unwrap();
    reopened.append(b"bb\n").unwrap();
    reopened.append(b"cc\n").unwrap();
    reopened.append(b"dd\n").unwrap();
    assert!(!owned.exists());
    assert_eq!(fs::read(&user).unwrap(), b"user");
    assert_eq!(fs::read(other).unwrap(), b"other");
    #[cfg(unix)]
    assert!(
        fs::symlink_metadata(fixture.path(&format!("{prefix}Sy0123456789Link")))
            .unwrap()
            .is_symlink()
    );
    assert!(reopened.paths().lock_path().exists());
}

#[cfg(unix)]
#[test]
fn a_preexisting_reader_observes_its_snapshot_after_atomic_compaction() {
    let fixture = Fixture::new();
    let path = fixture.path("snapshot.log");
    let log = open(&path, options(9, 3, 3, 6));
    for bytes in [b"aa\n", b"bb\n", b"cc\n"] {
        log.append(bytes).unwrap();
    }
    let mut old_reader = File::open(&path).unwrap();
    log.append(b"dd\n").unwrap();
    let mut old = String::new();
    old_reader.read_to_string(&mut old).unwrap();
    assert_eq!(old, "aa\nbb\ncc\n");
    assert_eq!(fs::read_to_string(&path).unwrap(), "bb\ncc\ndd\n");
}

#[test]
fn log_child() {
    let Some(mode) = std::env::var_os("GRAPHIT_RUST_LOG_FIXTURE_MODE") else {
        return;
    };
    let path = std::env::var_os("GRAPHIT_RUST_LOG_FIXTURE_PATH").unwrap();
    let ready = std::env::var_os("GRAPHIT_RUST_LOG_FIXTURE_READY").unwrap();
    let log = open(Path::new(&path), options(1024 * 1024, 256, 256, 512 * 1024));
    if mode == "append" {
        fs::write(ready, b"ready").unwrap();
        let mut start = [0];
        io::stdin().read_exact(&mut start).unwrap();
        let worker: usize = std::env::var("GRAPHIT_RUST_LOG_FIXTURE_WORKER")
            .unwrap()
            .parse()
            .unwrap();
        for sequence in 0..100 {
            log.append(format!("{{\"worker\":{worker},\"sequence\":{sequence}}}\n").as_bytes())
                .unwrap();
        }
    } else {
        assert_eq!(mode, "crash");
        let _guard = FileLock::try_acquire(log.paths().lock_path()).unwrap();
        let mut file = OpenOptions::new()
            .append(true)
            .open(Path::new(&path))
            .unwrap();
        file.write_all(b"complete\npartial").unwrap();
        file.sync_data().unwrap();
        fs::write(ready, b"ready").unwrap();
        let mut input = [0];
        let _ = io::stdin().read(&mut input);
    }
}

// Scope birth performs metadata work only; payload requests keep their own original context.
#[cfg(unix)]
fn project_birth_request() -> std::sync::Arc<graphit_core::file_io::SnapshotContext> {
    std::sync::Arc::new(graphit_core::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_secs(5),
        Cancellation::default(),
    ))
}
