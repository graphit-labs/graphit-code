use graphit_core::{
    file_io::SnapshotContext,
    file_lock::Cancellation,
    native_document_file::{
        CapturedDocumentDirectory, NativeDocumentFileLimits, NativeDocumentName,
        NativeDocumentNameLimits,
    },
};
use std::{
    ffi::OsStr,
    fs, io,
    path::Path,
    time::{Duration, Instant},
};

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn capture(path: &Path, context: &SnapshotContext) -> CapturedDocumentDirectory {
    CapturedDocumentDirectory::capture(
        path,
        NativeDocumentFileLimits::new(1024, 65536, 256, 65536).unwrap(),
        context,
    )
    .unwrap()
}
fn names() -> NativeDocumentNameLimits {
    NativeDocumentNameLimits::new(256, 65536).unwrap()
}
fn wire(name: &str) -> String {
    serde_json::to_string(name).unwrap()
}

#[test]
fn existing_unicode_empty_file_and_missing_have_exact_data_and_one_content_read() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("configuração.json"), b"value").unwrap();
    fs::write(tmp.path().join("empty.json"), b"").unwrap();
    let ctx = context();
    let parent = capture(tmp.path(), &ctx);
    let value = parent
        .read_named(OsStr::new("configuração.json"), names(), &ctx)
        .unwrap();
    assert_eq!(
        value.native_name().unwrap().as_os_str(),
        OsStr::new("configuração.json")
    );
    assert_eq!(value.observation().bytes(), Some(b"value".as_slice()));

    let decoded: NativeDocumentName =
        serde_json::from_str(&serde_json::to_string(value.native_name().unwrap()).unwrap())
            .unwrap();
    assert!(decoded == *value.native_name().unwrap());
    parent.revalidate_named_observation(&value, &ctx).unwrap();
    let empty = parent
        .read_named(
            OsStr::new("empty.json"),
            NativeDocumentNameLimits::new(2, 64).unwrap(),
            &ctx,
        )
        .unwrap();
    assert_eq!(empty.observation().bytes(), Some(b"".as_slice()));
    assert_eq!(
        empty.native_name().unwrap().as_os_str(),
        OsStr::new("empty.json")
    );

    let missing = parent
        .read_named(
            OsStr::new("MISSING.json"),
            NativeDocumentNameLimits::new(1, 1).unwrap(),
            &ctx,
        )
        .unwrap();
    assert!(missing.observation().bytes().is_none());
    assert!(missing.native_name().is_none());

    assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 2);
}

#[test]
fn string_codec_rejects_non_strings_invalid_unicode_components_and_limits() {
    for name in [
        "",
        ".",
        "..",
        "../other",
        "absolute/other",
        "/absolute",
        "nul\0name",
        "name/",
    ] {
        assert!(serde_json::from_str::<NativeDocumentName>(&wire(name)).is_err());
    }
    for json in [
        "{}",
        "[]",
        "null",
        "true",
        "97",
        r#"{"os":"Unix","units":[97]}"#,
        r#""\ud800""#,
    ] {
        assert!(serde_json::from_str::<NativeDocumentName>(json).is_err());
    }
    assert!(serde_json::from_str::<NativeDocumentName>(&wire(&"a".repeat(65536))).is_ok());
    assert!(serde_json::from_str::<NativeDocumentName>(&wire(&"a".repeat(65537))).is_err());
    assert!(NativeDocumentNameLimits::new(0, 1).is_err());
    assert!(NativeDocumentNameLimits::new(1, 0).is_err());
    assert!(NativeDocumentNameLimits::new(usize::MAX, 1).is_err());
    assert!(NativeDocumentNameLimits::new(1, usize::MAX).is_err());
    #[cfg(windows)]
    for name in ["stream:secret", "folder\\name", "C:relative"] {
        assert!(serde_json::from_str::<NativeDocumentName>(&wire(name)).is_err());
    }
    #[cfg(unix)]
    for name in ["stream:secret", "literal\\name"] {
        assert!(serde_json::from_str::<NativeDocumentName>(&wire(name)).is_ok());
    }
}

#[test]
fn exact_scan_capacity_and_native_byte_cap_are_independent_from_regular_read() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("target.json"), b"{}").unwrap();
    let ctx = context();
    let parent = capture(tmp.path(), &ctx);
    let bytes = if cfg!(windows) { 22 } else { 11 };
    parent
        .read_named(
            OsStr::new("target.json"),
            NativeDocumentNameLimits::new(1, bytes).unwrap(),
            &ctx,
        )
        .unwrap();
    let error = parent
        .read_named(
            OsStr::new("target.json"),
            NativeDocumentNameLimits::new(1, bytes - 1).unwrap(),
            &ctx,
        )
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
    assert!(!error.to_string().contains("target.json"));
    assert_eq!(
        parent
            .read(OsStr::new("target.json"), &ctx)
            .unwrap()
            .bytes(),
        Some(b"{}".as_slice())
    );
    assert_eq!(fs::read(tmp.path().join("target.json")).unwrap(), b"{}");
}

#[test]
fn oversized_raw_component_preserves_regular_read_capacity_error_before_io() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = context();
    let parent = capture(tmp.path(), &ctx);
    let oversized = "a".repeat(65537);
    for error in [
        parent.read(OsStr::new(&oversized), &ctx).err().unwrap(),
        parent
            .read_named(OsStr::new(&oversized), names(), &ctx)
            .err()
            .unwrap(),
    ] {
        assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
    }

    assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0);
}

#[test]
fn replacement_rename_and_cross_request_revalidation_never_adopt_stored_names() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.json");
    fs::write(&path, b"same").unwrap();
    let ctx = context();
    let parent = capture(tmp.path(), &ctx);
    let named = parent
        .read_named(OsStr::new("config.json"), names(), &ctx)
        .unwrap();
    assert_eq!(
        parent
            .revalidate_named_observation(&named, &context())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    fs::rename(&path, tmp.path().join("old.json")).unwrap();
    fs::write(&path, b"same").unwrap();
    assert_eq!(
        parent
            .revalidate_named_observation(&named, &ctx)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read(&path).unwrap(), b"same");
    assert_eq!(fs::read(tmp.path().join("old.json")).unwrap(), b"same");
    let fresh = parent
        .read_named(OsStr::new("config.json"), names(), &ctx)
        .unwrap();
    fs::rename(&path, tmp.path().join("renamed.json")).unwrap();
    assert!(parent.revalidate_named_observation(&fresh, &ctx).is_err());
    let renamed = parent
        .read_named(OsStr::new("renamed.json"), names(), &ctx)
        .unwrap();
    assert_eq!(
        renamed.native_name().unwrap().as_os_str(),
        OsStr::new("renamed.json")
    );
    assert!(fresh.observation().receipt() == renamed.observation().receipt());
}

#[test]
fn cancellation_deadline_and_parent_alias_retarget_are_explicit_failures() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("parent")).unwrap();
    fs::write(tmp.path().join("parent/config"), b"{}").unwrap();
    let cancel = Cancellation::default();
    let ctx = SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel.clone());
    let parent = capture(&tmp.path().join("parent"), &ctx);
    let named = parent
        .read_named(OsStr::new("config"), names(), &ctx)
        .unwrap();
    cancel.cancel();
    assert_eq!(
        parent
            .revalidate_named_observation(&named, &ctx)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert_eq!(
        parent
            .read_named(OsStr::new("config"), names(), &expired)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );

    assert_eq!(fs::read(tmp.path().join("parent/config")).unwrap(), b"{}");
}

#[cfg(unix)]
#[test]
fn non_unicode_names_fail_and_unicode_colon_backslash_names_round_trip() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let tmp = tempfile::tempdir().unwrap();
    let ctx = context();
    let parent = capture(tmp.path(), &ctx);
    for byte in [0xff, 0xfe] {
        let filename = OsString::from_vec(vec![b'r', b'a', b'w', b'-', byte]);
        #[cfg(target_os = "linux")]
        fs::write(tmp.path().join(&filename), b"unchanged").unwrap();
        assert_eq!(
            parent
                .read_named(&filename, names(), &ctx)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        #[cfg(target_os = "linux")]
        assert_eq!(fs::read(tmp.path().join(&filename)).unwrap(), b"unchanged");
    }
    let filename = OsStr::new("colon:slash\\.json");
    fs::write(tmp.path().join(filename), b"unicode").unwrap();
    let named = parent.read_named(filename, names(), &ctx).unwrap();
    let encoded = serde_json::to_string(named.native_name().unwrap()).unwrap();
    assert_eq!(encoded, wire(filename.to_str().unwrap()));
    let decoded: NativeDocumentName = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded.as_os_str(), filename);
}

#[cfg(unix)]
#[test]
fn symlink_hardlink_and_retargeted_parent_preserve_foreign_contents() {
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("a")).unwrap();
    fs::create_dir(tmp.path().join("b")).unwrap();
    fs::write(tmp.path().join("a/config"), b"first").unwrap();
    fs::write(tmp.path().join("b/config"), b"second").unwrap();
    symlink(tmp.path().join("a"), tmp.path().join("alias")).unwrap();
    let ctx = context();
    let parent = capture(&tmp.path().join("alias"), &ctx);
    let named = parent
        .read_named(OsStr::new("config"), names(), &ctx)
        .unwrap();
    fs::remove_file(tmp.path().join("alias")).unwrap();
    symlink(tmp.path().join("b"), tmp.path().join("alias")).unwrap();
    assert!(parent.revalidate_named_observation(&named, &ctx).is_err());
    let direct = capture(&tmp.path().join("b"), &ctx);
    symlink(tmp.path().join("a/config"), tmp.path().join("b/link")).unwrap();
    assert!(
        direct
            .read_named(OsStr::new("link"), names(), &ctx)
            .is_err()
    );
    fs::hard_link(tmp.path().join("b/config"), tmp.path().join("b/hard")).unwrap();
    assert!(
        direct
            .read_named(OsStr::new("config"), names(), &ctx)
            .is_err()
    );
    assert_eq!(fs::read(tmp.path().join("a/config")).unwrap(), b"first");
    assert_eq!(fs::read(tmp.path().join("b/config")).unwrap(), b"second");
}
