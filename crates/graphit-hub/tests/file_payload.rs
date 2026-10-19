use graphit_core::{
    artifact_store::{CopyLimits, SnapshotContext},
    file_lock::Cancellation,
};
use graphit_hub::file_payload::stage_file_payload;
use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
    time::{Duration, Instant},
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

fn limits() -> CopyLimits {
    CopyLimits::new(1024 * 1024, 100, 64 * 1024, 16).unwrap()
}

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}

fn archive(path: &Path, entries: &[(&str, &[u8])], method: CompressionMethod) {
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    let options = SimpleFileOptions::default()
        .compression_method(method)
        .unix_permissions(0o440);
    for &(name, bytes) in entries {
        if name.ends_with('/') {
            zip.add_directory(name, options.unix_permissions(0o550))
                .unwrap();
        } else {
            zip.start_file(name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
    }
    zip.finish().unwrap();
}

#[test]
fn ordinary_payload_has_no_native_envelope_and_stage_drop_removes_readonly_members() {
    for method in [CompressionMethod::Stored, CompressionMethod::Deflated] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("payload.zip");
        let staging = root.path().join("stages");
        fs::create_dir(&staging).unwrap();
        archive(
            &source,
            &[
                ("docs/empty/", b""),
                ("docs/ação.md", b"source bytes"),
                ("SKILL.md", b"instructions"),
            ],
            method,
        );
        let stage = stage_file_payload(
            &mut File::open(source).unwrap(),
            &staging,
            limits(),
            &context(),
        )
        .unwrap();
        assert_eq!(
            fs::read(stage.path().join("SKILL.md")).unwrap(),
            b"instructions"
        );
        assert_eq!(
            fs::read(stage.path().join("docs/ação.md")).unwrap(),
            b"source bytes"
        );
        assert!(stage.path().join("docs/empty").is_dir());
        assert!(!stage.path().join("package.json").exists());
        drop(stage);
        assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
    }
}

#[test]
fn traversal_case_collisions_and_file_parent_conflicts_never_escape_or_leave_stage() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("payload.zip");
    let staging = root.path().join("stages");
    fs::create_dir(&staging).unwrap();
    for names in [
        vec!["../escape"],
        vec!["/absolute"],
        vec!["dir\\escape"],
        vec!["CON.txt"],
        vec!["a", "A"],
        vec!["dir", "dir/file"],
        vec!["dir/file", "dir"],
    ] {
        let entries: Vec<_> = names
            .iter()
            .map(|name| (*name, b"data".as_slice()))
            .collect();
        archive(&source, &entries, CompressionMethod::Stored);
        let error = stage_file_payload(
            &mut File::open(&source).unwrap(),
            &staging,
            limits(),
            &context(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData, "{names:?}");
        assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
        assert!(!root.path().join("escape").exists());
    }
}

#[test]
fn crc_truncation_duplicate_headers_and_symlinks_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("payload.zip");
    let staging = root.path().join("stages");
    fs::create_dir(&staging).unwrap();
    archive(
        &source,
        &[("x.md", b"one"), ("y.md", b"two")],
        CompressionMethod::Stored,
    );
    let good = fs::read(&source).unwrap();
    let offset = ZipArchive::new(File::open(&source).unwrap())
        .unwrap()
        .by_index(0)
        .unwrap()
        .data_start()
        .and_then(|offset| usize::try_from(offset).ok())
        .expect("stored fixture member has a representable data offset");
    let mut corrupt = good.clone();
    corrupt[offset] ^= 1;
    let mut duplicate = good.clone();
    for position in 0..duplicate.len().saturating_sub(3) {
        if &duplicate[position..position + 4] == b"y.md" {
            duplicate[position] = b'x';
        }
    }
    for bytes in [corrupt, duplicate, good[..good.len() / 2].to_vec()] {
        fs::write(&source, bytes).unwrap();
        assert!(
            stage_file_payload(
                &mut File::open(&source).unwrap(),
                &staging,
                limits(),
                &context()
            )
            .is_err()
        );
        assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
    }
    let mut zip = ZipWriter::new(File::create(&source).unwrap());
    zip.add_symlink("link", "../outside", SimpleFileOptions::default())
        .unwrap();
    zip.finish().unwrap();
    assert!(
        stage_file_payload(
            &mut File::open(source).unwrap(),
            &staging,
            limits(),
            &context()
        )
        .is_err()
    );
    assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
}

#[test]
fn decoded_file_limits_and_original_cancellation_preserve_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("payload.zip");
    let staging = root.path().join("stages");
    fs::create_dir(&staging).unwrap();
    archive(
        &source,
        &[("body", &[b'a'; 4096])],
        CompressionMethod::Deflated,
    );
    let small = CopyLimits::new(1024, 100, 4096, 16).unwrap();
    let error = stage_file_payload(
        &mut File::open(&source).unwrap(),
        &staging,
        small,
        &context(),
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let cancelled = SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancellation);
    assert_eq!(
        stage_file_payload(
            &mut File::open(&source).unwrap(),
            &staging,
            limits(),
            &cancelled
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::Interrupted
    );
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    assert_eq!(
        stage_file_payload(
            &mut File::open(&source).unwrap(),
            &staging,
            limits(),
            &expired
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
}
