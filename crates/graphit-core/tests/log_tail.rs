use graphit_core::{
    brand::Brand,
    file_lock::Cancellation,
    log_file::{Durability, Limits as FileLimits, LogError, LogFile, LogPaths, Options, Stage},
    log_tail::{self, Limits},
    project_paths::{InstallationPaths, ProjectPaths},
};
use std::{
    fs, io,
    sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
mod support;
use support::Fixture;

fn limits(bytes: usize, lines: usize) -> Limits {
    Limits {
        max_read_bytes: bytes,
        max_lines: lines,
    }
}
fn read(path: &std::path::Path, limits: Limits) -> Result<log_tail::Tail, LogError> {
    log_tail::read(
        &LogPaths::explicit(path).unwrap(),
        limits,
        &Cancellation::default(),
    )
}

#[test]
fn absence_empty_and_zero_are_distinct_without_creating_any_state() {
    let fixture = Fixture::new();
    let path = fixture.path("missing.log");
    assert!(matches!(
        read(&path, limits(64, 10)),
        Err(LogError::Io {
            kind: io::ErrorKind::NotFound,
            ..
        })
    ));
    assert_eq!(fs::read_dir(fixture.root()).unwrap().count(), 0);
    for (bytes, lines) in [(0, 10), (64, 0), (0, usize::MAX)] {
        let tail = read(&path, limits(bytes, lines)).unwrap();
        assert!(tail.lines.is_empty());
        assert_eq!(tail.read_bytes, 0);
        assert_eq!(fs::read_dir(fixture.root()).unwrap().count(), 0);
    }
    fs::write(&path, b"").unwrap();
    let empty = read(&path, limits(64, 10)).unwrap();
    assert!(empty.lines.is_empty());
    assert_eq!(empty.read_bytes, 0);
    assert!(!empty.byte_windowed);
    assert!(!empty.final_unterminated);
    assert!(!empty.replaced_utf8);
    assert_eq!(fs::read_dir(fixture.root()).unwrap().count(), 1);
}

#[test]
fn captured_scopes_and_brands_do_not_bootstrap_or_change_permissions() {
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
        for paths in [
            LogPaths::project(
                &project,
                "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
                "ast",
            )
            .unwrap(),
            LogPaths::installation(&install, "daemon").unwrap(),
        ] {
            assert!(matches!(
                log_tail::read(&paths, limits(64, 10), &Cancellation::default()),
                Err(LogError::Io {
                    kind: io::ErrorKind::NotFound,
                    ..
                })
            ));
            assert!(!paths.path().exists());
            assert!(!paths.lock_path().exists());
            fs::create_dir_all(paths.path().parent().unwrap()).unwrap();
            fs::write(paths.path(), b"one\ntwo\n").unwrap();
            let original = fs::metadata(paths.path()).unwrap().permissions();
            assert_eq!(
                log_tail::read(&paths, limits(64, 10), &Cancellation::default())
                    .unwrap()
                    .lines,
                ["one", "two"]
            );
            assert_eq!(fs::metadata(paths.path()).unwrap().permissions(), original);
            assert!(!paths.lock_path().exists());
        }
    }
    let override_path = fixture.path("override.log");
    fs::write(&override_path, b"selected\n").unwrap();
    assert_eq!(
        read(&override_path, limits(64, 10)).unwrap().lines,
        ["selected"]
    );
    assert!(!fixture.path("default.log").exists());
}

#[test]
fn invalid_limits_and_cancellation_precede_io_but_zero_is_quiet() {
    let fixture = Fixture::new();
    let path = fixture.path("private-payload.log");
    let paths = LogPaths::explicit(&path).unwrap();
    let token = Cancellation::default();
    token.cancel();
    assert_eq!(
        log_tail::read(&paths, limits(usize::MAX, 0), &token).unwrap_err(),
        LogError::InvalidOptions
    );
    assert!(matches!(
        log_tail::read(&paths, limits(64, 10), &token),
        Err(LogError::Io {
            stage: Stage::Read,
            kind: io::ErrorKind::Interrupted,
            ..
        })
    ));
    assert!(
        log_tail::read(&paths, limits(64, 0), &token)
            .unwrap()
            .lines
            .is_empty()
    );
    assert!(!path.exists());
    assert!(!paths.lock_path().exists());
    fs::write(&path, b"private-payload\n").unwrap();
    let tail = read(&path, limits(64, usize::MAX)).unwrap();
    assert_eq!(tail.lines, ["private-payload"]);
    assert!(!format!("{tail:?}").contains("private-payload"));
    let error = read(&fixture.path("private-payload-missing"), limits(64, 10)).unwrap_err();
    assert!(!error.to_string().contains("private-payload"));
    assert!(!format!("{error:?}").contains("private-payload"));
}

#[test]
fn uniform_lines_preserve_blanks_normalize_crlf_and_report_final_fragments() {
    let fixture = Fixture::new();
    let path = fixture.path("text.log");
    for (source, expected, fragment) in [
        ("", vec![], false),
        ("\n", vec![""], false),
        ("\n\n", vec!["", ""], false),
        ("one\n\nthree\n", vec!["one", "", "three"], false),
        ("one\r\n\r\nthree\r\n", vec!["one", "", "three"], false),
        ("one\rinside\nlast\r", vec!["one\rinside", "last"], true),
        ("one\nlast", vec!["one", "last"], true),
        ("ação\n🦀\n", vec!["ação", "🦀"], false),
    ] {
        fs::write(&path, source).unwrap();
        let tail = read(&path, limits(128, 50)).unwrap();
        assert_eq!(tail.lines, expected, "{source:?}");
        assert_eq!(tail.final_unterminated, fragment);
        assert_eq!(tail.read_bytes, source.len());
        assert!(!tail.byte_windowed);
        assert!(!tail.replaced_utf8);
    }
    let text: String = (0..100).map(|n| format!("line {n}\n")).collect();
    fs::write(&path, text).unwrap();
    for count in [10, 50] {
        let tail = read(&path, limits(4096, count)).unwrap();
        assert_eq!(tail.lines.len(), count);
        assert_eq!(tail.lines[0], format!("line {}", 100 - count));
        assert_eq!(tail.lines.last().unwrap(), "line 99");
    }
}

#[test]
fn the_boundary_peek_is_inside_the_raw_byte_budget() {
    let fixture = Fixture::new();
    let path = fixture.path("boundary.log");
    for (source, budget, expected, windowed) in [
        ("aa\nbb\n", 6, vec!["aa", "bb"], false),
        ("aa\nbb\n", 5, vec!["bb"], true),
        ("aa\nbb\n", 4, vec!["bb"], true), // Peek is the previous LF.
        ("aa\nbb\n", 3, vec![], true),
        ("aa\nbb\n", 1, vec![], true),
        ("\n", 1, vec![""], false),
        ("xxx\nend", 5, vec!["end"], true),
        ("中中中\nfim", 8, vec!["fim"], true),
    ] {
        fs::write(&path, source).unwrap();
        let tail = read(&path, limits(budget, 50)).unwrap();
        assert_eq!(tail.lines, expected, "{source:?} budget={budget}");
        assert_eq!(tail.read_bytes, source.len().min(budget));
        assert_eq!(tail.byte_windowed, windowed);
    }
    fs::write(&path, "x".repeat(100_000)).unwrap();
    let tail = read(&path, limits(64, 50)).unwrap();
    assert_eq!(tail.read_bytes, 64);
    assert!(tail.lines.is_empty());
    assert!(!tail.final_unterminated);
}

#[test]
fn only_returned_lines_are_decoded_and_invalid_utf8_is_bounded_text() {
    let fixture = Fixture::new();
    let path = fixture.path("utf8.log");
    fs::write(&path, b"\xff\nvalid\nlast\xf0\x9f").unwrap();
    let tail = read(&path, limits(64, 2)).unwrap();
    assert_eq!(tail.lines, ["valid", "last�"]);
    assert!(tail.replaced_utf8);
    assert!(tail.final_unterminated);
    let last_valid = fixture.path("omitted.log");
    fs::write(&last_valid, b"\xff\nvalid\n").unwrap();
    let tail = read(&last_valid, limits(64, 1)).unwrap();
    assert_eq!(tail.lines, ["valid"]);
    assert!(!tail.replaced_utf8);
    fs::write(&path, [vec![0xff; 1024], b"\n".to_vec()].concat()).unwrap();
    let tail = read(&path, limits(4096, 50)).unwrap();
    assert_eq!(tail.lines.len(), 1);
    assert_eq!(tail.lines[0].len(), 3072);
    assert!(tail.lines.iter().map(String::len).sum::<usize>() <= 3 * tail.read_bytes);
}

#[test]
fn readonly_readable_file_and_nonregular_targets_do_not_get_modified() {
    let fixture = Fixture::new();
    let path = fixture.path("readonly.log");
    fs::write(&path, b"safe\n").unwrap();
    let original = fs::metadata(&path).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly.clone()).unwrap();
    assert_eq!(read(&path, limits(64, 10)).unwrap().lines, ["safe"]);
    assert_eq!(fs::metadata(&path).unwrap().permissions(), readonly);
    fs::set_permissions(&path, original).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(read(&path, limits(64, 10)).is_err());
    assert!(path.is_dir());
    assert_eq!(fs::read_dir(fixture.root()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn unix_permission_failures_and_observed_links_are_reported_without_state() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let fixture = Fixture::new();
    let path = fixture.path("private.log");
    fs::write(&path, b"safe\n").unwrap();
    let original = fs::metadata(&path).unwrap().permissions();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o0)).unwrap();
    let result = read(&path, limits(64, 10));
    let privileged = fs::File::open(&path).is_ok();
    fs::set_permissions(&path, original).unwrap();
    eprintln!(
        "real read permission-denied branch exercised={}",
        !privileged
    );
    if !privileged {
        assert!(matches!(
            result,
            Err(LogError::Io {
                kind: io::ErrorKind::PermissionDenied,
                ..
            })
        ));
    }
    let link = fixture.path("link.log");
    symlink(&path, &link).unwrap();
    assert!(read(&link, limits(64, 10)).is_err());
    let project = fixture.project();
    let paths = LogPaths::project(
        &project,
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        "ast",
    )
    .unwrap();
    fs::create_dir_all(
        project
            .project_runtime("01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(), &[])
            .unwrap(),
    )
    .unwrap();
    symlink(fixture.root(), paths.path().parent().unwrap()).unwrap();
    assert!(log_tail::read(&paths, limits(64, 10), &Cancellation::default()).is_err());
    assert!(!paths.lock_path().exists());
    assert!(!fixture.path("ast.log").exists());
}

#[test]
fn concurrent_cooperative_append_and_compaction_are_bounded_observations() {
    let fixture = Fixture::new();
    let path = fixture.path("concurrent.log");
    let paths = LogPaths::explicit(&path).unwrap();
    let writer = LogFile::open(
        paths.clone(),
        Options {
            limits: FileLimits {
                max_file_bytes: 4096,
                max_record_bytes: 512,
                max_batch_bytes: 1024,
                retain_bytes: 2048,
            },
            durability: Durability::Buffered,
            lock_timeout: Duration::from_secs(3),
        },
        Cancellation::default(),
    )
    .unwrap();
    let start_barrier = Arc::new(Barrier::new(2));
    let done = Arc::new(AtomicBool::new(false));
    thread::scope(|scope| {
        let barrier = start_barrier.clone();
        let finished = done.clone();
        let producer = scope.spawn(move || {
            barrier.wait();
            for id in 0..1000 {
                writer
                    .append(
                        format!("{{\"id\":{id},\"message\":\"{}\"}}\n", "x".repeat(128)).as_bytes(),
                    )
                    .unwrap();
            }
            finished.store(true, Ordering::Release);
        });
        start_barrier.wait();
        let start = Instant::now();
        let mut polls = 0;
        loop {
            assert!(start.elapsed() < Duration::from_secs(5));
            let tail = log_tail::read(&paths, limits(1024, 20), &Cancellation::default()).unwrap();
            assert!(tail.read_bytes <= 1024);
            assert!(tail.lines.len() <= 20);
            let complete = tail
                .lines
                .len()
                .saturating_sub(usize::from(tail.final_unterminated));
            let ids: Vec<u64> = tail.lines[..complete]
                .iter()
                .map(|line| {
                    serde_json::from_str::<serde_json::Value>(line).unwrap()["id"]
                        .as_u64()
                        .unwrap()
                })
                .collect();
            assert!(ids.windows(2).all(|pair| pair[1] == pair[0] + 1));
            polls += 1;
            if done.load(Ordering::Acquire) {
                break;
            }
        }
        producer.join().unwrap();
        assert!(polls > 0);
    });
    assert_eq!(
        log_tail::read(&paths, limits(1024, 20), &Cancellation::default())
            .unwrap()
            .lines
            .last()
            .map(
                |s| serde_json::from_str::<serde_json::Value>(s).unwrap()["id"]
                    .as_u64()
                    .unwrap()
            ),
        Some(999)
    );
    assert!(paths.lock_path().exists()); // Created by the producer, never by the reader.
}
