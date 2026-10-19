use arrow_array::{RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_data::{
    lance_branch_store::{BranchError, BranchSnapshot, DeltaResolution},
    lance_module_store::LanceModuleStore,
};
use lance::dataset::cleanup::CleanupPolicyBuilder;
use std::{collections::BTreeSet, error::Error, path::Path, sync::Arc};

type TestResult = Result<(), Box<dyn Error>>;
const COMMIT_A: &str = "0123456789012345678901234567890123456789";
const COMMIT_B: &str = "abcdefabcdefabcdefabcdefabcdefabcdefabcd";
const COMMIT_256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn batch(value: &str) -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Utf8, false)]));
    RecordBatch::try_new(schema, vec![Arc::new(StringArray::from(vec![value]))]).unwrap()
}

async fn value(snapshot: &BranchSnapshot) -> Result<String, Box<dyn Error>> {
    let batches: Vec<RecordBatch> = snapshot
        .scan()
        .try_into_stream()
        .await?
        .try_collect()
        .await?;
    assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
    Ok(batches[0]
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap()
        .value(0)
        .to_owned())
}

fn data_files(root: &Path) -> BTreeSet<std::path::PathBuf> {
    let mut files = BTreeSet::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                directories.push(entry.path());
            } else if entry.path().extension().is_some_and(|ext| ext == "lance") {
                files.insert(entry.path());
            }
        }
    }
    files
}

async fn fixture() -> Result<(tempfile::TempDir, String, BranchSnapshot), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let uri = temp
        .path()
        .join("records.lance")
        .to_str()
        .unwrap()
        .to_owned();
    let image = batch("original");
    LanceModuleStore::create(&uri, image.schema(), vec![image]).await?;
    let snapshot = BranchSnapshot::capture_observed(&uri, None, None).await?;
    Ok((temp, uri, snapshot))
}

#[tokio::test]
async fn git_tags_are_immutable_and_branch_specific_without_data_copies() -> TestResult {
    let (temp, _uri, initial) = fixture().await?;
    assert!(initial.at_git_commit(COMMIT_A).await?.is_none());
    for invalid in [
        "",
        "abc",
        "../refs/heads/main",
        "g123456789012345678901234567890123456789",
    ] {
        assert!(matches!(
            initial.at_git_commit(invalid).await,
            Err(BranchError::InvalidGitObjectId)
        ));
        assert!(matches!(
            initial.tag_git_commit(invalid).await,
            Err(BranchError::InvalidGitObjectId)
        ));
    }
    let original_files = data_files(temp.path());
    initial.tag_git_commit(COMMIT_A).await?;
    initial.tag_git_commit(COMMIT_A).await?;
    initial.tag_git_commit(COMMIT_256).await?;
    let child = initial.materialize_git_branch("feature", COMMIT_A).await?;
    assert_eq!(data_files(temp.path()), original_files);
    assert_eq!(value(&child).await?, "original");
    assert_eq!(child.physical_branch(), Some("feature"));
    assert_eq!(child.version(), initial.version());
    assert!(matches!(
        child.tag_git_commit(COMMIT_A).await,
        Err(BranchError::GitSnapshotConflict)
    ));
    let edited = child
        .prepare_overwrite(vec![batch("child")])
        .await?
        .commit()
        .await?;
    edited.tag_git_commit(&COMMIT_B.to_uppercase()).await?;
    let captured = initial.at_git_commit(COMMIT_B).await?.unwrap();
    assert_eq!(captured.physical_branch(), Some("feature"));
    assert_eq!(captured.version(), edited.version());
    assert_eq!(value(&captured).await?, "child");
    // The native tag protects the entire named version chain from deletion too.
    assert!(captured.clone().retire_named().await.is_err());
    let original = edited.at_git_commit(COMMIT_A).await?.unwrap();
    assert_eq!(original.physical_branch(), None);
    assert_eq!(value(&original).await?, "original");
    assert!(matches!(
        edited.prepare_restore_git_commit(COMMIT_A).await,
        Err(BranchError::GitSnapshotBranchMismatch)
    ));
    assert!(
        initial
            .materialize_git_branch("feature", COMMIT_A)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn git_restore_keeps_exact_schema_and_fails_on_concurrent_commit() -> TestResult {
    let (_temp, uri, initial) = fixture().await?;
    initial.tag_git_commit(COMMIT_A).await?;
    let changed_schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("extra", DataType::Utf8, true),
    ]));
    let changed = RecordBatch::try_new(
        changed_schema.clone(),
        vec![
            Arc::new(StringArray::from(vec!["new"])),
            Arc::new(StringArray::from(vec![Some("metadata")])),
        ],
    )?;
    let latest = initial
        .prepare_overwrite(vec![changed])
        .await?
        .commit()
        .await?;
    let prepared = latest.prepare_restore_git_commit(COMMIT_A).await?;
    let restored_key = prepared.recovery_key().clone();
    let restored = prepared.commit().await?;
    assert_eq!(restored.schema(), initial.schema());
    assert_eq!(value(&restored).await?, "original");
    assert_eq!(
        restored.resolve(&restored_key).await?,
        DeltaResolution::Applied {
            version: restored.version()
        }
    );
    let pending = restored.prepare_restore_git_commit(COMMIT_A).await?;
    let competing = restored
        .prepare_overwrite(vec![batch("competing")])
        .await?
        .commit()
        .await?;
    assert!(pending.commit().await.is_err());
    let fresh = BranchSnapshot::capture_observed(&uri, None, None).await?;
    assert_eq!(fresh.version(), competing.version());
    assert_eq!(value(&fresh).await?, "competing");
    assert_eq!(
        value(&fresh.at_git_commit(COMMIT_A).await?.unwrap()).await?,
        "original"
    );
    Ok(())
}

#[tokio::test]
async fn git_native_tags_keep_default_and_named_snapshots_through_cleanup() -> TestResult {
    let (_temp, uri, initial) = fixture().await?;
    initial.tag_git_commit(COMMIT_A).await?;
    let child = initial.materialize_git_branch("feature", COMMIT_A).await?;
    let child = child
        .prepare_overwrite(vec![batch("named")])
        .await?
        .commit()
        .await?;
    child.tag_git_commit(COMMIT_B).await?;
    let child_latest = child
        .prepare_overwrite(vec![batch("named-new")])
        .await?
        .commit()
        .await?;
    let main_latest = initial
        .prepare_overwrite(vec![batch("main-new")])
        .await?
        .commit()
        .await?;
    for snapshot in [&main_latest, &child_latest] {
        let dataset = lance::Dataset::open(snapshot.corpus_uri()).await?;
        let dataset = if let Some(branch) = snapshot.physical_branch() {
            dataset.checkout_branch(branch).await?
        } else {
            dataset
        };
        dataset
            .cleanup_with_policy(
                CleanupPolicyBuilder::default()
                    .versions(vec![initial.version(), child.version()])?
                    .error_if_tagged_old_versions(false)
                    .build(),
            )
            .await?;
    }
    let fresh = BranchSnapshot::capture_observed(&uri, None, None).await?;
    assert_eq!(
        value(&fresh.at_git_commit(COMMIT_A).await?.unwrap()).await?,
        "original"
    );
    assert_eq!(
        value(&fresh.at_git_commit(COMMIT_B).await?.unwrap()).await?,
        "named"
    );
    Ok(())
}
