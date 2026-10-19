use graphit_data::knowledge_document_lifecycle::{
    KnowledgeSourceIdentity as Identity, KnowledgeSourceIdentityError as IdentityError,
    KnowledgeSourcePathComponents as Components, decode_managed_knowledge_source_path as decode,
};
use std::{num::NonZeroUsize, path::Path};

fn cap(bytes: usize) -> NonZeroUsize {
    NonZeroUsize::new(bytes).unwrap()
}
fn identity(path: &str, id: Option<&str>) -> Identity {
    Identity::from_relative_path(Path::new(path), id, cap(4096)).unwrap()
}

#[test]
fn portable_components_have_independent_golden_origins_and_roundtrip() {
    for (path, origin, slug, parts) in [
        (
            "docs/Guide.md",
            "knowledge-source/t/646f6373/47756964652e6d64",
            "source-t-646f6373-47756964652e6d64",
            vec!["docs", "Guide.md"],
        ),
        (
            "docs/ação.md",
            "knowledge-source/t/646f6373/61c3a7c3a36f2e6d64",
            "source-t-646f6373-61c3a7c3a36f2e6d64",
            vec!["docs", "ação.md"],
        ),
    ] {
        let actual = identity(path, None);
        assert_eq!(actual.source_path(), origin);
        assert_eq!(actual.slug(), slug);
        assert_eq!(
            decode(origin, cap(4096)).unwrap(),
            Some(Components::Portable(
                parts.into_iter().map(String::from).collect()
            ))
        );
    }
    assert_ne!(
        identity("ação.md", None).source_path(),
        identity("ac\u{327}a\u{303}o.md", None).source_path()
    );
}

#[test]
fn literal_identity_survives_rename_while_default_identity_changes() {
    let before = identity("docs/a.md", Some(" Manual Ω "));
    let after = identity("docs/b.md", Some(" Manual Ω "));
    assert_eq!(before.slug(), " Manual Ω ");
    assert_eq!(after.slug(), before.slug());
    assert_ne!(before.source_path(), after.source_path());
    let default_before = identity("docs/a.md", None);
    let default_after = identity("docs/b.md", None);
    assert_ne!(default_before.slug(), default_after.slug());
    assert_ne!(default_before.source_path(), default_after.source_path());
    for id in ["", "..name", "a/b", "a\\b", "a\nb", "\0"] {
        assert!(
            matches!(
                Identity::from_relative_path(Path::new("a"), Some(id), cap(4096)),
                Err(IdentityError::InvalidAuthoredId)
            ),
            "{id:?}"
        );
    }
    for path in ["", ".", "..", "../a", "a/../b", "/a"] {
        assert!(
            matches!(
                Identity::from_relative_path(Path::new(path), None, cap(4096)),
                Err(IdentityError::InvalidRelativePath)
            ),
            "{path:?}"
        );
    }
}

#[test]
fn reserved_boundary_and_canonicality_prevent_external_adoption() {
    for outside in [
        "docs/a.md",
        "knowledge-source-old/t/61",
        "knowledge-sources/t/61",
    ] {
        assert_eq!(decode(outside, cap(4096)).unwrap(), None);
    }
    for invalid in [
        "knowledge-source",
        "knowledge-source/",
        "knowledge-source/t",
        "knowledge-source/x/61",
        "knowledge-source/t/",
        "knowledge-source/t/61/",
        "knowledge-source/t//61",
        "knowledge-source/t/6A",
        "knowledge-source/t/6",
        "knowledge-source/t/gg",
        "knowledge-source/t/00",
        "knowledge-source/t/2e",
        "knowledge-source/t/2e2e",
        "knowledge-source/t/2f",
        "knowledge-source/t/ff",
        "knowledge-source/u/61",
        "knowledge-source/u/ff/2f",
        "knowledge-source/w/6100",
        "knowledge-source/w/00d8/5c00",
        "knowledge-source/w/00d8/2f00",
        "knowledge-source/w/00d8/0000",
        "knowledge-source/w/00d8/2e00",
        "knowledge-source/w/00d8/61",
    ] {
        assert!(
            matches!(
                decode(invalid, cap(4096)),
                Err(IdentityError::InvalidManagedSourcePath)
            ),
            "{invalid}"
        );
    }
    for removed_encoding in [
        "knowledge-source/u/646f6373/ff",
        "knowledge-source/w/64006f0063007300/00d8",
    ] {
        assert!(matches!(
            decode(removed_encoding, cap(4096)),
            Err(IdentityError::InvalidManagedSourcePath)
        ));
    }
}

#[test]
fn exact_field_bytes_accept_and_one_more_reject_without_namespace_fallback() {
    let initial = identity("a", None);
    let exact = cap(initial.source_path().len());
    assert_eq!(
        Identity::from_relative_path(Path::new("a"), None, exact).unwrap(),
        initial
    );
    assert!(matches!(
        Identity::from_relative_path(Path::new("a"), None, cap(exact.get() - 1)),
        Err(IdentityError::LimitExceeded("source_path"))
    ));
    assert!(decode(initial.source_path(), exact).unwrap().is_some());
    assert!(matches!(
        decode(initial.source_path(), cap(exact.get() - 1)),
        Err(IdentityError::LimitExceeded("source_path"))
    ));
    assert!(matches!(
        decode("knowledge-source-old", cap(1)),
        Err(IdentityError::LimitExceeded("source_path"))
    ));
    let id = "Ω".repeat(exact.get() / 2 + 1);
    assert_eq!(
        Identity::from_relative_path(Path::new("a"), Some(&id), cap(id.len()))
            .unwrap()
            .slug(),
        id
    );
    assert!(matches!(
        Identity::from_relative_path(Path::new("a"), Some(&id), cap(id.len() - 1)),
        Err(IdentityError::LimitExceeded("slug"))
    ));
}

#[cfg(unix)]
#[test]
fn actual_unix_non_utf8_filename_is_rejected_without_replacement() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};
    let path = PathBuf::from(OsString::from_vec(b"docs/a-\xff.md".to_vec()));
    for id in [None, Some("Explicit")] {
        assert!(matches!(
            Identity::from_relative_path(&path, id, cap(4096)),
            Err(IdentityError::NonUnicodePath)
        ));
    }
}

#[cfg(unix)]
#[test]
fn unicode_unix_backslash_is_a_component_character_not_a_windows_separator() {
    let actual = identity("a\\b.md", None);
    assert_eq!(actual.source_path(), "knowledge-source/t/615c622e6d64");
    assert_eq!(
        decode(actual.source_path(), cap(4096)).unwrap(),
        Some(Components::Portable(vec!["a\\b.md".into()]))
    );
}

#[cfg(windows)]
#[test]
fn actual_windows_unpaired_utf16_is_rejected_without_replacement() {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};
    let path = PathBuf::from(OsString::from_wide(&[0xd800]));
    assert!(matches!(
        Identity::from_relative_path(&path, None, cap(4096)),
        Err(IdentityError::NonUnicodePath)
    ));
}
