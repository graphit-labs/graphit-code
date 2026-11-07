use super::*;
#[path = "knowledge_transport_tests.rs"]
mod knowledge_transport;
use graphit_core::artifact_store::{
    CapturedSourceReader, CopyPolicy, RetainedBytesLimits, RetainedFileSet, SnapshotPolicy,
    SourceReadLimits,
};
use graphit_core::file_lock::Cancellation;
use graphit_fs::NodeKind;
use std::{
    fs::{self, File},
    io::{Seek, SeekFrom},
    sync::Arc,
    time::{Duration, Instant},
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn limits() -> CopyLimits {
    CopyLimits::new(1024 * 1024, 100, 64 * 1024, 16).unwrap()
}
fn context() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ))
}
fn capture(
    path: &Path,
    kind: NodeKind,
    limits: CopyLimits,
    context: &Arc<SnapshotContext>,
) -> CapturedSourceReader {
    let retained = RetainedBytesLimits::new(1024 * 1024, 100, 64 * 1024).unwrap();
    let policy = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        limits,
        RetainedFileSet::new([], retained).unwrap(),
        retained,
    )
    .unwrap();
    CapturedSourceReader::capture(
        path,
        kind,
        &policy,
        SourceReadLimits::new(64 * 1024, 64 * 1024, limits.total_file_bytes_limit()).unwrap(),
        Arc::clone(context),
    )
    .unwrap()
}
fn payload(root: &Path, kind: PackageKind) {
    let bundle = match kind {
        PackageKind::Ast => "graph.icebug",
        PackageKind::Knowledge => "index.lance",
    };
    fs::create_dir_all(root.join(bundle).join("empty")).unwrap();
    fs::write(
        root.join(bundle).join(if kind == PackageKind::Ast {
            "schema.cypher"
        } else {
            "fragment.bin"
        }),
        b"physical bytes; not a semantic native dataset",
    )
    .unwrap();
}
fn sdk_archive(path: &Path, envelope: &str, files: &[(&str, &[u8])], method: CompressionMethod) {
    let mut writer = ZipWriter::new(File::create(path).unwrap());
    let options = SimpleFileOptions::default()
        .compression_method(method)
        .system(zip::System::Unix)
        .unix_permissions(0o640);
    writer.start_file("package.json", options).unwrap();
    writer.write_all(envelope.as_bytes()).unwrap();
    for &(name, bytes) in files {
        if name.ends_with('/') {
            writer
                .add_directory(name, options.unix_permissions(0o750))
                .unwrap();
        } else {
            writer.start_file(name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
    }
    writer.finish().unwrap();
}
const AST_ENVELOPE: &str = r#"{"format":"graphit-native-package","v":1,"kind":"ast"}"#;
#[test]
fn dos_members_require_attributes_and_preserve_readonly_without_host_conversion() {
    // Real DOS entries on every host: missing, NORMAL, and conflicting DIRECTORY.
    for (attributes, valid) in [(0, false), (0x80, true), (0x10, false)] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("dos.zip");
        let mut archive = ZipWriter::new(File::create(&path).unwrap());
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        archive
            .start_file("package.json", options.system(zip::System::Unix))
            .unwrap();
        archive.write_all(AST_ENVELOPE.as_bytes()).unwrap();
        let dos = options.system(zip::System::Dos);
        archive
            .add_directory("graph.icebug/", dos.external_attributes(0x10))
            .unwrap();
        archive
            .start_file(
                "graph.icebug/schema.cypher",
                dos.external_attributes(attributes),
            )
            .unwrap();
        archive.write_all(b"schema").unwrap();
        archive
            .start_file("graph.icebug/readonly.bin", dos.external_attributes(0x01))
            .unwrap();
        archive.write_all(b"readonly").unwrap();
        archive.finish().unwrap();

        let context = context();
        let source = capture(&path, NodeKind::File, limits(), &context);
        let destination = temp.path().join("installed");
        if !valid {
            assert!(matches!(
                inspect(&source, None, limits(), &context),
                Err(PackageError::Invalid(_))
            ));
            assert!(stage_import(&source, None, &destination, limits(), &context).is_err());
            assert!(!destination.exists());
            continue;
        }
        inspect(&source, Some(PackageKind::Ast), limits(), &context).unwrap();
        let staged = stage_import(
            &source,
            Some(PackageKind::Ast),
            &destination,
            limits(),
            &context,
        )
        .unwrap();
        let writable = staged.path().join("graph.icebug/schema.cypher");
        let readonly = staged.path().join("graph.icebug/readonly.bin");
        assert_eq!(fs::read(&writable).unwrap(), b"schema");
        assert_eq!(fs::read(&readonly).unwrap(), b"readonly");
        assert!(!fs::metadata(&writable).unwrap().permissions().readonly());
        assert!(fs::metadata(&readonly).unwrap().permissions().readonly());
        staged.publish().unwrap();
        let readonly = destination.join("graph.icebug/readonly.bin");
        assert_eq!(fs::read(&readonly).unwrap(), b"readonly");
        #[cfg(windows)]
        {
            let mut permissions = fs::metadata(&readonly).unwrap().permissions();
            #[expect(
                clippy::permissions_set_readonly_false,
                reason = "Windows fixture cleanup clears FILE_ATTRIBUTE_READONLY, not Unix write permissions"
            )]
            permissions.set_readonly(false);
            fs::set_permissions(&readonly, permissions).unwrap();
        }
    }
}

#[test]
fn sdk_symlink_member_is_rejected_on_every_host() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("symlink.zip");
    let mut archive = ZipWriter::new(File::create(&path).unwrap());
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .system(zip::System::Unix);
    archive.start_file("package.json", options).unwrap();
    archive.write_all(AST_ENVELOPE.as_bytes()).unwrap();
    archive.add_directory("graph.icebug/", options).unwrap();
    archive
        .add_symlink("graph.icebug/schema.cypher", "elsewhere", options)
        .unwrap();
    archive.finish().unwrap();
    let context = context();
    let source = capture(&path, NodeKind::File, limits(), &context);
    let destination = temp.path().join("installed");
    assert!(matches!(
        inspect(&source, None, limits(), &context),
        Err(PackageError::Invalid(_))
    ));
    assert!(stage_import(&source, None, &destination, limits(), &context).is_err());
    assert!(!destination.exists());
}

#[test]
fn staged_payload_is_readable_before_exclusive_publication_for_both_kinds() {
    for kind in [PackageKind::Ast, PackageKind::Knowledge] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        payload(&source, kind);
        let context = context();
        let reader = capture(&source, NodeKind::Directory, limits(), &context);
        let archive = temp.path().join("payload.zip");
        export(&reader, kind, &archive, &context).unwrap();
        let input = capture(&archive, NodeKind::File, limits(), &context);
        let destination = temp.path().join("installed");
        let staged = stage_import(&input, Some(kind), &destination, limits(), &context).unwrap();
        let stage_path = staged.path().to_owned();
        let relative = if kind == PackageKind::Ast {
            "graph.icebug/schema.cypher"
        } else {
            "index.lance/fragment.bin"
        };
        let expected = fs::read(source.join(relative)).unwrap();
        assert_eq!(staged.kind(), kind);
        assert_eq!(fs::read(staged.path().join(relative)).unwrap(), expected);
        assert!(!destination.exists());
        assert_eq!(staged.publish().unwrap(), kind);
        assert!(!stage_path.exists());
        assert_eq!(fs::read(destination.join(relative)).unwrap(), expected);
        assert!(!destination.join("package.json").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(destination.join(relative))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                fs::metadata(source.join(relative))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777
            );
        }
        // The consumed TempDir cannot clean up a subsequent tree at its former path.
        fs::create_dir(&stage_path).unwrap();
        fs::write(stage_path.join("other-owner"), b"keep").unwrap();
        assert_eq!(fs::read(stage_path.join("other-owner")).unwrap(), b"keep");
    }
}
#[test]
fn dropping_staged_readonly_payload_cleans_only_the_owned_tree() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("payload.zip");
    sdk_archive(
        &archive,
        AST_ENVELOPE,
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"schema"),
        ],
        CompressionMethod::Stored,
    );
    let context = context();
    let input = capture(&archive, NodeKind::File, limits(), &context);
    let destination = temp.path().join("installed");
    let sibling = temp.path().join("sibling");
    fs::create_dir(&sibling).unwrap();
    fs::write(sibling.join("keep"), b"unrelated").unwrap();
    let staged = stage_import(&input, None, &destination, limits(), &context).unwrap();
    let stage_path = staged.path().to_owned();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            stage_path.join("graph.icebug"),
            fs::Permissions::from_mode(0o500),
        )
        .unwrap();
        fs::set_permissions(
            stage_path.join("graph.icebug/schema.cypher"),
            fs::Permissions::from_mode(0o400),
        )
        .unwrap();
    }
    #[cfg(windows)]
    {
        let path = stage_path.join("graph.icebug/schema.cypher");
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(path, permissions).unwrap();
    }
    drop(staged);
    assert!(!stage_path.exists());
    assert!(!destination.exists());
    assert_eq!(fs::read(sibling.join("keep")).unwrap(), b"unrelated");
    assert!(archive.is_file());
}
#[test]
fn staged_publication_rechecks_cancellation_source_and_destination_races() {
    for case in ["cancel", "source", "destination"] {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("payload.zip");
        sdk_archive(
            &archive,
            AST_ENVELOPE,
            &[
                ("graph.icebug/", b""),
                ("graph.icebug/schema.cypher", b"schema"),
            ],
            CompressionMethod::Stored,
        );
        let context = context();
        let input = capture(&archive, NodeKind::File, limits(), &context);
        let destination = temp.path().join("installed");
        let staged = stage_import(&input, None, &destination, limits(), &context).unwrap();
        let stage_path = staged.path().to_owned();
        let expected_kind = match case {
            "cancel" => {
                context.cancellation().cancel();
                input.revalidate_content().unwrap_err().kind()
            }
            "source" => {
                fs::write(&archive, b"changed source").unwrap();
                input.revalidate_content().unwrap_err().kind()
            }
            "destination" => {
                fs::create_dir(&destination).unwrap();
                fs::write(destination.join("other-owner"), b"keep").unwrap();
                io::ErrorKind::AlreadyExists
            }
            _ => unreachable!(),
        };
        let error = staged.publish().unwrap_err();
        assert!(
            matches!(error, PackageError::Io(ref cause) if cause.kind() == expected_kind),
            "{case}: {error}"
        );
        assert!(!stage_path.exists());
        if case == "destination" {
            assert_eq!(fs::read(destination.join("other-owner")).unwrap(), b"keep");
            assert!(!destination.join("graph.icebug").exists());
        } else {
            assert!(!destination.exists());
        }
    }
}
#[test]
fn staged_publication_rechecks_the_original_deadline() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("payload.zip");
    sdk_archive(
        &archive,
        AST_ENVELOPE,
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"schema"),
        ],
        CompressionMethod::Stored,
    );
    let deadline = Instant::now() + Duration::from_secs(1);
    let context = Arc::new(SnapshotContext::new(deadline, Cancellation::default()));
    let input = capture(&archive, NodeKind::File, limits(), &context);
    let destination = temp.path().join("installed");
    let staged = stage_import(&input, None, &destination, limits(), &context).unwrap();
    let stage_path = staged.path().to_owned();
    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
    let expected_kind = input.revalidate_content().unwrap_err().kind();
    let error = staged.publish().unwrap_err();
    assert!(matches!(error, PackageError::Io(ref cause) if cause.kind() == expected_kind));
    assert!(!stage_path.exists());
    assert!(!destination.exists());
}
#[test]
fn captured_graph_export_prefixes_the_same_tree_without_recapturing() {
    let temp = tempfile::tempdir().unwrap();
    let payload_root = temp.path().join("payload");
    payload(&payload_root, PackageKind::Ast);
    let graph = payload_root.join("graph.icebug");
    for sidecar in ["source.lance", "corpus.lance"] {
        fs::create_dir(graph.join(sidecar)).unwrap();
        fs::write(graph.join(sidecar).join("fragment.bin"), sidecar.as_bytes()).unwrap();
    }
    let context = context();
    let reader = capture(&graph, NodeKind::Directory, limits(), &context);
    let archive = temp.path().join("graph.zip");
    let foreign = self::context();
    assert!(export_ast_graph(&reader, &archive, &foreign).is_err());
    assert!(!archive.exists());
    export_ast_graph(&reader, &archive, &context).unwrap();
    let captured = capture(&archive, NodeKind::File, limits(), &context);
    let destination = temp.path().join("installed");
    import(
        &captured,
        Some(PackageKind::Ast),
        &destination,
        limits(),
        &context,
    )
    .unwrap();
    for sidecar in ["source.lance", "corpus.lance"] {
        assert_eq!(
            fs::read(
                destination
                    .join("graph.icebug")
                    .join(sidecar)
                    .join("fragment.bin")
            )
            .unwrap(),
            sidecar.as_bytes()
        );
        assert!(!destination.join(sidecar).exists());
    }
    assert!(destination.join("graph.icebug/schema.cypher").is_file());
}

#[test]
fn ast_transport_rejects_separate_text_or_nn_datasets_before_publication() {
    for split in ["search.lance", "native_nn.lance"] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        payload(&source, PackageKind::Ast);
        let directory = source.join("graph.icebug").join(split);
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("fragment.bin"), b"split data").unwrap();
        let context = context();
        let captured = capture(&source, NodeKind::Directory, limits(), &context);
        assert!(
            export(
                &captured,
                PackageKind::Ast,
                &temp.path().join("export.zip"),
                &context
            )
            .is_err()
        );
        let archive = temp.path().join("input.zip");
        let directory = format!("graph.icebug/{split}/");
        let fragment = format!("graph.icebug/{split}/fragment.bin");
        sdk_archive(
            &archive,
            AST_ENVELOPE,
            &[
                ("graph.icebug/", b""),
                ("graph.icebug/schema.cypher", b"schema"),
                (&directory, b""),
                (&fragment, b"split data"),
            ],
            CompressionMethod::Stored,
        );
        let input = capture(&archive, NodeKind::File, limits(), &context);
        let destination = temp.path().join("installed");
        assert!(
            stage_import(
                &input,
                Some(PackageKind::Ast),
                &destination,
                limits(),
                &context
            )
            .is_err()
        );
        assert!(!destination.exists());
    }
}

#[test]
fn physical_roundtrip_preserves_bytes_and_empty_dirs_for_both_kinds() {
    for kind in [PackageKind::Ast, PackageKind::Knowledge] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        payload(&source, kind);
        if kind == PackageKind::Ast {
            fs::create_dir(source.join("graph.icebug/corpus.lance")).unwrap();
            fs::write(
                source.join("graph.icebug/corpus.lance/fragment.bin"),
                b"shared text and nullable NN bytes",
            )
            .unwrap();
            fs::create_dir(source.join("graph.icebug/source.lance")).unwrap();
            fs::write(
                source.join("graph.icebug/source.lance/fragment.bin"),
                b"optional raw Source bytes\0\r\n",
            )
            .unwrap();
        }
        let context = context();
        let reader = capture(&source, NodeKind::Directory, limits(), &context);
        let archive = temp.path().join("payload.zip");
        assert_eq!(export(&reader, kind, &archive, &context).unwrap(), kind);
        let input = capture(&archive, NodeKind::File, limits(), &context);
        assert_eq!(
            inspect(&input, Some(kind), limits(), &context).unwrap(),
            kind
        );
        let destination = temp.path().join("installed");
        assert_eq!(
            import(&input, Some(kind), &destination, limits(), &context).unwrap(),
            kind
        );
        let bundle = if kind == PackageKind::Ast {
            "graph.icebug"
        } else {
            "index.lance"
        };
        let file = if kind == PackageKind::Ast {
            "schema.cypher"
        } else {
            "fragment.bin"
        };
        assert_eq!(
            fs::read(source.join(bundle).join(file)).unwrap(),
            fs::read(destination.join(bundle).join(file)).unwrap()
        );
        assert!(destination.join(bundle).join("empty").is_dir());
        assert!(!destination.join("package.json").exists());
        if kind == PackageKind::Ast {
            assert_eq!(
                fs::read(destination.join("graph.icebug/corpus.lance/fragment.bin")).unwrap(),
                b"shared text and nullable NN bytes"
            );
            assert_eq!(
                fs::read(destination.join("graph.icebug/source.lance/fragment.bin")).unwrap(),
                b"optional raw Source bytes\0\r\n"
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(source.join(bundle))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                fs::metadata(destination.join(bundle))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777
            );
        }
        assert!(import(&input, Some(kind), &destination, limits(), &context).is_err());
        assert!(
            inspect(
                &input,
                Some(if kind == PackageKind::Ast {
                    PackageKind::Knowledge
                } else {
                    PackageKind::Ast
                }),
                limits(),
                &context
            )
            .is_err()
        );
    }
}
#[test]
fn deflated_bodies_validate_and_optional_bundles_absence_is_accepted() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("payload.zip");
    sdk_archive(
        &archive,
        AST_ENVELOPE,
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"schema"),
        ],
        CompressionMethod::Deflated,
    );
    let context = context();
    let source = capture(&archive, NodeKind::File, limits(), &context);
    assert_eq!(
        inspect(&source, None, limits(), &context).unwrap(),
        PackageKind::Ast
    );
}
#[test]
fn strict_envelope_rejects_unknown_duplicate_null_and_legacy_fields() {
    for envelope in [
        r#"{"format":"graphit-native-package","v":2,"kind":"ast"}"#,
        r#"{"format":"graphit-native-package","v":1,"kind":"other"}"#,
        r#"{"format":"graphit-native-package","v":1,"kind":"ast","extra":0}"#,
        r#"{"format":"graphit-native-package","v":1,"v":1,"kind":"ast"}"#,
        r#"{"format":null,"v":1,"kind":"ast"}"#,
        r#"{"format":"graphit-native-package","v":1,"kind":"ast"} {}"#,
        r#"{"type":"ast","version":1}"#,
    ] {
        assert!(envelope::read(envelope.as_bytes()).is_err());
    }
}
#[test]
fn canonical_paths_reject_traversal_and_reserved_names() {
    for path in [
        "../schema",
        "graph.icebug/../schema",
        "/schema",
        "C:/schema",
        "//host/schema",
        "graph.icebug\\schema",
        "graph.icebug//schema",
        "graph.icebug/./schema",
        "graph.icebug/CON",
        "graph.icebug/a.",
        "graph.icebug/a\n",
    ] {
        assert!(canonical_name(path, false).is_err(), "{path:?}");
    }
}
#[test]
fn invalid_members_do_not_publish_or_leave_a_stage() {
    let cases: &[&[(&str, &[u8])]] = &[
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"x"),
            ("source.lance/", b""),
            ("source.lance/fragment", b"x"),
        ],
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"x"),
            ("search.lance/", b""),
            ("search.lance/fragment", b"x"),
        ],
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"x"),
            ("graph.icebug/native_nn.lance/", b""),
        ],
        &[("index.lance/", b""), ("index.lance/x", b"x")],
        &[("graph.icebug/", b"")],
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"x"),
            ("graph.icebug/search.lance/", b""),
        ],
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"x"),
            ("graph.icebug/source.lance/", b""),
        ],
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"x"),
            ("graph.icebug/source.lance", b"not a directory"),
        ],
        &[("graph.icebug/schema.cypher", b"x")],
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"x"),
            ("graph.icebug/../escape", b"x"),
        ],
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"x"),
            ("graph.icebug/SCHEMA.cypher", b"x"),
        ],
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"x"),
            ("graph.icebug/schema.cypher/child", b"x"),
        ],
    ];
    for files in cases {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bad.zip");
        sdk_archive(&path, AST_ENVELOPE, files, CompressionMethod::Stored);
        let context = context();
        let reader = capture(&path, NodeKind::File, limits(), &context);
        let destination = temp.path().join("installed");
        assert!(import(&reader, None, &destination, limits(), &context).is_err());
        assert!(!destination.exists());
        assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".graphit-package-")
        }));
    }
}
#[test]
fn changed_source_cancelled_and_wrong_request_preserve_existing_archive() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    payload(&source, PackageKind::Ast);
    let context = context();
    let reader = capture(&source, NodeKind::Directory, limits(), &context);
    let destination = temp.path().join("output.zip");
    fs::write(&destination, b"existing").unwrap();
    let different =
        SnapshotContext::new(context.deadline().unwrap(), context.cancellation().clone());
    assert!(export(&reader, PackageKind::Ast, &destination, &different).is_err());
    fs::write(source.join("graph.icebug/schema.cypher"), b"changed").unwrap();
    assert!(export(&reader, PackageKind::Ast, &destination, &context).is_err());
    assert_eq!(fs::read(&destination).unwrap(), b"existing");
    context.cancellation().cancel();
    assert!(export(&reader, PackageKind::Ast, &destination, &context).is_err());
    assert_eq!(fs::read(&destination).unwrap(), b"existing");
}
#[test]
fn generated_zip_overhead_is_not_an_import_size_limit() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("graph.icebug")).unwrap();
    fs::write(source.join("graph.icebug/schema.cypher"), b"x").unwrap();
    let context = context();
    let tiny = CopyLimits::new(1, 10, 4096, 16).unwrap();
    let reader = capture(&source, NodeKind::Directory, tiny, &context);
    let output = temp.path().join("output.zip");
    fs::write(&output, b"keep").unwrap();
    export(&reader, PackageKind::Ast, &output, &context).unwrap();
    assert!(fs::metadata(&output).unwrap().len() > tiny.total_file_bytes_limit());
    let input = capture(&output, NodeKind::File, limits(), &context);
    assert!(inspect(&input, None, tiny, &context).is_err());
}
#[test]
fn crc_failure_is_detected_in_inspect_and_import() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("bad.zip");
    sdk_archive(
        &path,
        AST_ENVELOPE,
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"original"),
        ],
        CompressionMethod::Stored,
    );
    let offset = {
        let mut archive = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
        archive
            .by_name("graph.icebug/schema.cypher")
            .unwrap()
            .data_start()
            .unwrap()
    };
    let mut file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.seek(SeekFrom::Start(offset)).unwrap();
    file.write_all(b"X").unwrap();
    drop(file);
    let context = context();
    let reader = capture(&path, NodeKind::File, limits(), &context);
    assert!(inspect(&reader, None, limits(), &context).is_err());
    let output = temp.path().join("installed");
    assert!(import(&reader, None, &output, limits(), &context).is_err());
    assert!(!output.exists());
    assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".graphit-package-")
    }));
}
#[test]
fn actual_total_path_nodes_and_depth_bounds_are_enforced() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("valid.zip");
    sdk_archive(
        &path,
        AST_ENVELOPE,
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"schema"),
        ],
        CompressionMethod::Stored,
    );
    let context = context();
    let reader = capture(&path, NodeKind::File, limits(), &context);
    for constrained in [
        CopyLimits::new(1024 * 1024, 2, 4096, 16).unwrap(),
        CopyLimits::new(1024 * 1024, 10, 24, 16).unwrap(),
        CopyLimits::new(1024 * 1024, 10, 4096, 1).unwrap(),
    ] {
        assert!(inspect(&reader, None, constrained, &context).is_err());
    }
}
#[cfg(unix)]
#[test]
fn modes_are_preserved_after_children_are_written() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    payload(&source, PackageKind::Ast);
    fs::set_permissions(
        source.join("graph.icebug/schema.cypher"),
        fs::Permissions::from_mode(0o440),
    )
    .unwrap();
    fs::set_permissions(
        source.join("graph.icebug"),
        fs::Permissions::from_mode(0o550),
    )
    .unwrap();
    let context = context();
    let reader = capture(&source, NodeKind::Directory, limits(), &context);
    let archive = temp.path().join("payload.zip");
    export(&reader, PackageKind::Ast, &archive, &context).unwrap();
    let input = capture(&archive, NodeKind::File, limits(), &context);
    let output = temp.path().join("installed");
    import(&input, None, &output, limits(), &context).unwrap();
    assert_eq!(
        fs::metadata(output.join("graph.icebug"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o550
    );
    assert_eq!(
        fs::metadata(output.join("graph.icebug/schema.cypher"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o440
    );
    fs::set_permissions(
        source.join("graph.icebug"),
        fs::Permissions::from_mode(0o750),
    )
    .unwrap();
    fs::set_permissions(
        output.join("graph.icebug"),
        fs::Permissions::from_mode(0o750),
    )
    .unwrap();
}

#[cfg(unix)]
#[test]
fn special_unix_member_types_are_rejected() {
    #[allow(clippy::unnecessary_cast)]
    let special_modes = [
        libc::S_IFLNK as u32,
        libc::S_IFIFO as u32,
        libc::S_IFSOCK as u32,
        libc::S_IFCHR as u32,
        libc::S_IFBLK as u32,
    ];
    for mode in special_modes {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("special.zip");
        let mut writer = ZipWriter::new(File::create(&path).unwrap());
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .system(zip::System::Unix);
        writer.start_file("package.json", options).unwrap();
        writer.write_all(AST_ENVELOPE.as_bytes()).unwrap();
        writer.add_directory("graph.icebug/", options).unwrap();
        writer
            .start_file(
                "graph.icebug/schema.cypher",
                options.external_attributes((mode | 0o640) << 16),
            )
            .unwrap();
        writer.write_all(b"x").unwrap();
        writer.finish().unwrap();
        let context = context();
        let source = capture(&path, NodeKind::File, limits(), &context);
        let output = temp.path().join("installed");
        assert!(inspect(&source, None, limits(), &context).is_err());
        assert!(import(&source, None, &output, limits(), &context).is_err());
        assert!(!output.exists());
    }
}
#[test]
fn expanded_deflated_bytes_and_truncated_archives_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("compressed.zip");
    let body = vec![b'x'; 64 * 1024];
    sdk_archive(
        &path,
        AST_ENVELOPE,
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", &body),
        ],
        CompressionMethod::Deflated,
    );
    let constrained = CopyLimits::new(4096, 100, 64 * 1024, 16).unwrap();
    assert!(fs::metadata(&path).unwrap().len() < constrained.total_file_bytes_limit());
    let context = context();
    let source = capture(&path, NodeKind::File, constrained, &context);
    let output = temp.path().join("installed");
    assert!(inspect(&source, None, constrained, &context).is_err());
    assert!(import(&source, None, &output, constrained, &context).is_err());
    assert!(!output.exists());
    drop(source);
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(fs::metadata(&path).unwrap().len() - 12)
        .unwrap();
    drop(file);
    let source = capture(&path, NodeKind::File, limits(), &context);
    assert!(inspect(&source, None, limits(), &context).is_err());
    assert!(import(&source, None, &output, limits(), &context).is_err());
    assert!(!output.exists());
}
#[test]
fn expired_original_request_preserves_existing_export() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    payload(&source, PackageKind::Ast);
    let deadline = Instant::now() + Duration::from_secs(1);
    let context = Arc::new(SnapshotContext::new(deadline, Cancellation::default()));
    let reader = capture(&source, NodeKind::Directory, limits(), &context);
    let output = temp.path().join("output.zip");
    fs::write(&output, b"keep").unwrap();
    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
    assert!(export(&reader, PackageKind::Ast, &output, &context).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"keep");
}

fn central_header(path: &Path, member: &str) -> u64 {
    let mut archive = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    let file = archive.by_name(member).unwrap();
    file.central_header_start()
}
fn patch_central(path: &Path, member: &str, field_offset: usize, bytes: &[u8]) {
    let offset = central_header(path, member);
    let mut file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.seek(SeekFrom::Start(
        offset + u64::try_from(field_offset).unwrap(),
    ))
    .unwrap();
    file.write_all(bytes).unwrap();
}
#[test]
fn raw_duplicate_names_are_not_hidden_by_the_sdk_index() {
    use zip::unstable::format::blocks::ZipCentralEntryBlock;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("duplicate.zip");
    sdk_archive(
        &path,
        AST_ENVELOPE,
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"first"),
            ("graph.icebug/schemb.cypher", b"second"),
        ],
        CompressionMethod::Stored,
    );
    // Mutate only a fixture's central name at the SDK-provided header offset.
    // The SDK treats that directory as authoritative and otherwise deduplicates it.
    patch_central(
        &path,
        "graph.icebug/schemb.cypher",
        size_of::<u32>() + size_of::<ZipCentralEntryBlock>(),
        b"graph.icebug/schema.cypher",
    );
    assert_eq!(
        zip::ZipArchive::new(File::open(&path).unwrap())
            .unwrap()
            .len(),
        3
    );
    let context = context();
    let reader = capture(&path, NodeKind::File, limits(), &context);
    let output = temp.path().join("installed");
    assert!(inspect(&reader, None, limits(), &context).is_err());
    assert!(import(&reader, None, &output, limits(), &context).is_err());
    assert!(!output.exists());
}
#[test]
fn unsupported_methods_and_non_utf8_names_are_rejected() {
    use zip::unstable::format::blocks::ZipCentralEntryBlock;
    for invalid_name in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("invalid.zip");
        sdk_archive(
            &path,
            AST_ENVELOPE,
            &[
                ("graph.icebug/", b""),
                ("graph.icebug/schema.cypher", b"schema"),
            ],
            CompressionMethod::Stored,
        );
        if invalid_name {
            let mut name = b"graph.icebug/schema.cypher".to_vec();
            name[13] = 0xff;
            patch_central(
                &path,
                "graph.icebug/schema.cypher",
                size_of::<u32>() + size_of::<ZipCentralEntryBlock>(),
                &name,
            );
        } else {
            // Bzip2 is intentionally outside the package's Stored/Deflated contract.
            patch_central(
                &path,
                "graph.icebug/schema.cypher",
                size_of::<u32>() + std::mem::offset_of!(ZipCentralEntryBlock, compression_method),
                &12u16.to_le_bytes(),
            );
        }
        let context = context();
        let reader = capture(&path, NodeKind::File, limits(), &context);
        let output = temp.path().join("installed");
        assert!(inspect(&reader, None, limits(), &context).is_err());
        assert!(import(&reader, None, &output, limits(), &context).is_err());
        assert!(!output.exists());
    }
}
#[test]
fn encrypted_sdk_member_is_rejected_without_password_prompt() {
    use zip::unstable::write::FileOptionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("encrypted.zip");
    let mut writer = ZipWriter::new(File::create(&path).unwrap());
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .system(zip::System::Unix);
    writer.start_file("package.json", options).unwrap();
    writer.write_all(AST_ENVELOPE.as_bytes()).unwrap();
    writer.add_directory("graph.icebug/", options).unwrap();
    writer
        .start_file(
            "graph.icebug/schema.cypher",
            options.with_deprecated_encryption(b"fixture only").unwrap(),
        )
        .unwrap();
    writer.write_all(b"schema").unwrap();
    writer.finish().unwrap();
    let context = context();
    let reader = capture(&path, NodeKind::File, limits(), &context);
    let output = temp.path().join("installed");
    assert!(inspect(&reader, None, limits(), &context).is_err());
    assert!(import(&reader, None, &output, limits(), &context).is_err());
    assert!(!output.exists());
}

#[test]
fn cancelled_import_does_not_publish_or_leave_staging() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("payload.zip");
    sdk_archive(
        &path,
        AST_ENVELOPE,
        &[
            ("graph.icebug/", b""),
            ("graph.icebug/schema.cypher", b"schema"),
        ],
        CompressionMethod::Stored,
    );
    let context = context();
    let reader = capture(&path, NodeKind::File, limits(), &context);
    context.cancellation().cancel();
    let output = temp.path().join("installed");
    assert!(import(&reader, None, &output, limits(), &context).is_err());
    assert!(!output.exists());
    assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".graphit-package-")
    }));
}
