use super::*;
use native_document::{Operation, Outcome, State};

fn operation(fixture: &Fixture) -> Operation {
    Operation {
        id: ID.into(),
        key: fixture.key(),
        destination: NativePath(fixture.destination.clone()),
        parent: crate::native_file_identity::physical_id(
            fixture.destination.parent().unwrap(),
            &fs::metadata(fixture.destination.parent().unwrap()).unwrap(),
        )
        .unwrap(),
        previous: None,
        state: State::Intent {},
    }
}

fn seed(fixture: &Fixture, writer: &ArtifactWriter) {
    writer
        .save(
            &Document::empty(&fixture.store).unwrap(),
            ArtifactStage::Commit,
        )
        .unwrap();
}

fn assert_no_scratch(fixture: &Fixture) {
    assert!(
        fs::read_dir(fixture.store.runtime_root())
            .unwrap()
            .all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .as_encoded_bytes()
                    .starts_with(b".artifact-store-")
            })
    );
}

#[test]
fn loaded_absence_never_adopts_an_appearing_catalogue() {
    let fixture = Fixture::new();
    let writer = fixture.writer();
    let loaded = writer.loaded_catalog().unwrap();
    seed(&fixture, &writer);
    let foreign = fs::read(fixture.store.manifest_path()).unwrap();
    let error = writer
        .save_native_projection(loaded, Some(&operation(&fixture)), ArtifactStage::Commit)
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(fs::read(fixture.store.manifest_path()).unwrap(), foreign);

    assert_no_scratch(&fixture);
}

#[cfg(unix)]
#[test]
fn loaded_same_identity_changed_content_denies_before_scratch_birth() {
    let fixture = Fixture::new();
    let writer = fixture.writer();
    seed(&fixture, &writer);
    let loaded = writer.loaded_catalog().unwrap();
    let path = fixture.store.manifest_path();
    let before =
        crate::native_file_identity::physical_id(&path, &fs::metadata(&path).unwrap()).unwrap();
    let mut changed = fs::read(&path).unwrap();
    changed[0] = b'[';
    fs::write(&path, &changed).unwrap();
    assert!(
        before
            == crate::native_file_identity::physical_id(&path, &fs::metadata(&path).unwrap())
                .unwrap()
    );
    let error = writer
        .save_native_projection(loaded, Some(&operation(&fixture)), ArtifactStage::Commit)
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(fs::read(path).unwrap(), changed);

    assert_no_scratch(&fixture);
}

#[cfg(unix)]
#[test]
fn loaded_path_replacement_never_adopts_the_replacement() {
    let fixture = Fixture::new();
    let writer = fixture.writer();
    seed(&fixture, &writer);
    let loaded = writer.loaded_catalog().unwrap();
    let path = fixture.store.manifest_path();
    let replaced = path.with_extension("retained");
    let bytes = fs::read(&path).unwrap();
    fs::rename(&path, &replaced).unwrap();
    let mut file = graphit_fs::create_private_file(&path).unwrap();
    std::io::Write::write_all(&mut file, &bytes).unwrap();
    drop(file);
    let error = writer
        .save_native_projection(loaded, Some(&operation(&fixture)), ArtifactStage::Commit)
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(fs::read(path).unwrap(), bytes);

    assert_no_scratch(&fixture);
}

#[test]
fn projection_preserves_closed_wire_for_every_phase_and_outcome() {
    use crate::{native_document_file::NativeDocumentReceipt, native_file_identity::PhysicalId};
    let fixture = Fixture::new();
    let writer = fixture.writer();
    let base = writer.loaded_catalog().unwrap();
    let scope = fixture.store.scope.binding().unwrap();
    let largest = PhysicalId::largest();
    let stage = NativeDocumentReceipt::largest(17);
    let mut operation = operation(&fixture);
    let mut states = vec![
        State::Intent {},
        State::AbortedBeforeBirth {},
        State::Directory {
            directory: largest.clone(),
        },
        State::File {
            directory: largest.clone(),
            file: largest.clone(),
        },
        State::Prepared {
            directory: largest.clone(),
            stage: stage.clone(),
        },
        State::Publishing {
            directory: largest.clone(),
            stage: stage.clone(),
        },
    ];
    for outcome in [
        Outcome::Applied {
            destination: stage.clone(),
        },
        Outcome::Unchanged {},
        Outcome::NotPublished {},
    ] {
        states.extend([
            State::Resolved {
                directory: largest.clone(),
                stage: stage.clone(),
                outcome: outcome.clone(),
            },
            State::Acknowledged {
                directory: largest.clone(),
                stage: stage.clone(),
                outcome: outcome.clone(),
            },
            State::Cleaned {
                directory: largest.clone(),
                stage: stage.clone(),
                outcome,
            },
        ]);
    }
    for state in states {
        operation.state = state;
        let projection = codec::DocumentProjection::new(&base.document, &scope, Some(&operation));
        let mut expected = base.document.clone();
        expected.scope = scope.clone();
        expected.pending = Some(Pending::NativeDocument(Box::new(operation.clone())));
        let bytes = codec::encode(&projection, fixture.store.limits.max_document_bytes).unwrap();
        assert_eq!(
            bytes,
            codec::encode(&expected, fixture.store.limits.max_document_bytes).unwrap()
        );
        assert_eq!(
            codec::encode(&projection, bytes.len()).unwrap().len(),
            bytes.len()
        );
        assert_eq!(
            codec::encode(&projection, bytes.len() - 1)
                .unwrap_err()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
    }
    let projection = codec::DocumentProjection::<Operation>::new(&base.document, &scope, None);
    assert_eq!(
        codec::encode(&projection, fixture.store.limits.max_document_bytes).unwrap(),
        codec::encode(&base.document, fixture.store.limits.max_document_bytes).unwrap()
    );
}
