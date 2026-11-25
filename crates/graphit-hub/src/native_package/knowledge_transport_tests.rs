//! Physical package transport is schema-opaque; the native producer validates the corpus.
use super::*;

#[test]
fn knowledge_transport_keeps_one_corpus_and_rejects_separate_semantic_roots() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    payload(&source, PackageKind::Knowledge);
    fs::create_dir(source.join("index.lance/_versions")).unwrap();
    fs::write(
        source.join("index.lance/_versions/7.manifest"),
        b"captured manifest bytes",
    )
    .unwrap();
    let context = context();
    let archive = temp.path().join("payload.zip");
    let reader = capture(&source, NodeKind::Directory, limits(), &context);
    export(&reader, PackageKind::Knowledge, &archive, &context).unwrap();
    let input = capture(&archive, NodeKind::File, limits(), &context);
    let destination = temp.path().join("installed");
    let staged = stage_import(
        &input,
        Some(PackageKind::Knowledge),
        &destination,
        limits(),
        &context,
    )
    .unwrap();
    assert_eq!(
        fs::read(staged.path().join("index.lance/_versions/7.manifest")).unwrap(),
        b"captured manifest bytes"
    );
    staged.publish().unwrap();
    assert_eq!(
        fs::read(destination.join("index.lance/fragment.bin")).unwrap(),
        fs::read(source.join("index.lance/fragment.bin")).unwrap()
    );
    assert!(!destination.join("embeddings.lance").exists());
    assert!(!destination.join("knowledge-pair.json").exists());
    fs::create_dir(source.join("embeddings.lance")).unwrap();
    fs::write(
        source.join("embeddings.lance/fragment.bin"),
        b"separate semantic data",
    )
    .unwrap();
    let split = capture(&source, NodeKind::Directory, limits(), &context);
    assert!(
        export(
            &split,
            PackageKind::Knowledge,
            &temp.path().join("split.zip"),
            &context
        )
        .is_err()
    );
    fs::remove_dir_all(source.join("embeddings.lance")).unwrap();
    fs::write(source.join("knowledge-pair.json"), b"{}").unwrap();
    let paired = capture(&source, NodeKind::Directory, limits(), &context);
    assert!(
        export(
            &paired,
            PackageKind::Knowledge,
            &temp.path().join("paired.zip"),
            &context
        )
        .is_err()
    );
}

#[test]
fn knowledge_import_rejects_a_second_dataset_or_pair_file_before_publication() {
    for extra in ["embeddings.lance/", "knowledge-pair.json"] {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("unexpected.zip");
        sdk_archive(
            &archive,
            r#"{"format":"graphit-native-package","v":1,"kind":"knowledge"}"#,
            &[
                ("index.lance/", b""),
                ("index.lance/fragment.bin", b"document bytes"),
                (extra, b"{}"),
            ],
            CompressionMethod::Stored,
        );
        let context = context();
        let input = capture(&archive, NodeKind::File, limits(), &context);
        let destination = temp.path().join("installed");
        assert!(
            stage_import(
                &input,
                Some(PackageKind::Knowledge),
                &destination,
                limits(),
                &context
            )
            .is_err()
        );
        assert!(!destination.exists());
    }
}
