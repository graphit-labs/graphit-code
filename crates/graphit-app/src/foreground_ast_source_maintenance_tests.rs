//! Source lookup maintenance through the actual Raw/FILE producer.
use super::*;
use lance::{
    Dataset,
    dataset::{
        CommitBuilder,
        transaction::{Operation, TransactionBuilder},
    },
    index::{CreateIndexBuilder, DatasetIndexExt},
};
use lance_index::{
    IndexType,
    scalar::{BuiltinIndexType, ScalarIndexParams},
};

fn data_files(dataset: &Dataset) -> BTreeSet<String> {
    dataset
        .fragments()
        .iter()
        .flat_map(|fragment| fragment.files.iter().map(|file| file.path.clone()))
        .collect()
}

async fn source_index(dataset: &Dataset) -> TestResult<(usize, BTreeSet<u32>)> {
    let metadata = dataset.load_indices().await?;
    let mut segments = 0;
    let mut covered = BTreeSet::new();
    for index in metadata.iter().filter(|index| index.name == "path_source") {
        segments += 1;
        covered.extend(
            index
                .fragment_bitmap
                .as_ref()
                .ok_or("missing Source coverage")?
                .iter(),
        );
    }
    Ok((segments, covered))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unchanged_rescan_consolidates_existing_source_index() -> TestResult {
    let fixture = Fixture::new()?;
    let result = std::panic::AssertUnwindSafe(async {
        let sources = [
            ("a.lua", "function alpha() return 1 end\n"),
            ("b.lua", "function beta() return 2 end\n"),
        ];
        for (path, body) in sources {
            std::fs::write(fixture.project.root().join(path), body)?;
        }
        fixture.index(&[], true).await?;
        let cancel = Cancellation::default();
        let original = fixture.policy.request_context(Instant::now(), &cancel)?;
        let capture = CapturedPublicationScope::project(
            &fixture.project,
            fixture.selected.id,
            fixture.owner.scope,
            original.snapshot(),
        )?;
        let record = capture.binding()?;
        let project = ProjectCapture {
            scope: ScopeCapture {
                live: &capture,
                record: &record,
            },
            version: &fixture.selected.version,
        };
        let registry = fixture.registry();
        let (index, _) = registry
            .capture(
                fixture.owner.auth.installation(),
                project,
                fixture.owner.native,
                original.snapshot(),
                false,
            )
            .await?
            .ok_or("graph missing")?;
        let observations = index.graph.files(original.snapshot())?;
        let source_path = fixture
            .project
            .project_runtime(fixture.selected.id, &["ast", "source.lance"])?;
        let uri = source_path.to_str().ok_or("Source URI is not Unicode")?;
        let mut dataset = Dataset::open(uri).await?;
        let files = data_files(&dataset);
        let fragments = dataset
            .fragments()
            .iter()
            .map(|fragment| u32::try_from(fragment.id))
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(fragments.len(), sources.len());
        let removed = dataset
            .load_indices()
            .await?
            .iter()
            .filter(|index| index.name == "path_source")
            .cloned()
            .collect();
        let params = ScalarIndexParams::for_builtin(BuiltinIndexType::BTree);
        let mut segments = Vec::new();
        for fragment in &fragments {
            segments.push(
                CreateIndexBuilder::new(&mut dataset, &["path"], IndexType::BTree, &params)
                    .name("path_source".into())
                    .replace(true)
                    .fragments(vec![*fragment])
                    .execute_uncommitted()
                    .await?,
            );
        }
        let tx = TransactionBuilder::new(
            dataset.version_id(),
            Operation::CreateIndex {
                new_indices: segments,
                removed_indices: removed,
            },
        )
        .build();
        let dataset = CommitBuilder::new(Arc::new(dataset)).execute(tx).await?;
        let old_version = dataset.version_id();
        assert_eq!(source_index(&dataset).await?.0, sources.len());
        drop(dataset);
        fixture.index(&[], true).await?;
        let current = Dataset::open(uri).await?;
        assert!(current.version_id() > old_version);
        assert_eq!(
            source_index(&current).await?,
            (1, fragments.into_iter().collect())
        );
        assert_eq!(
            data_files(&current),
            files,
            "index consolidation never rewrites Source data"
        );
        assert_eq!(
            index.graph.files(original.snapshot())?,
            observations,
            "unchanged FILE versions and hashes are never aligned to metadata HEAD"
        );
        for (path, body) in sources {
            assert_eq!(
                observed_text(&source_path, path, original.snapshot())
                    .await?
                    .as_deref(),
                Some(body)
            );
        }
        drop(current);
        let reopened = Dataset::open(uri).await?;
        assert_eq!(source_index(&reopened).await?.0, 1);
        assert_eq!(data_files(&reopened), files);
        Ok(())
    })
    .catch_unwind()
    .await;
    fixture.shutdown(result).await
}

#[cfg(target_os = "linux")]
fn function_rows(
    graph: &LiveGraph,
    original: &Arc<graphit_core::file_io::SnapshotContext>,
) -> io::Result<Vec<Vec<serde_json::Value>>> {
    Ok(graph.query("MATCH (n:Function) RETURN n.path, n.name, n.raw_version, n.source_sha256, n.start_byte, n.end_byte ORDER BY n.path",
        &BTreeMap::new(),graphit_ast::icebug_query::QueryOptions {
            buffer_pool_bytes: 64*1024*1024,query_threads:1,max_rows:64,max_bytes:65536,max_depth:16,
        },original)?.rows)
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_and_graph_are_readable_before_source_index_batch_and_after_reopen() -> TestResult {
    let fixture = Fixture::new()?;
    let lifetime = Cancellation::default();
    let mut job = None;
    let mut release = None;
    let result = std::panic::AssertUnwindSafe(async {
        let count = graphit_data::lance_branch_store::AST_INDEX_BATCH + 1;
        for ordinal in 0..count {
            std::fs::write(
                fixture.project.root().join(format!("f{ordinal:03}.lua")),
                format!("function fn_{ordinal:03}() return {ordinal} end\n"),
            )?;
        }
        let (observed, resume) = fixture.ast.hold_after_file_publication("f001.lua");
        release = Some(resume);
        let ast = Arc::clone(&fixture.ast);
        let paths = fixture.project.clone();
        let selected = fixture.selected.clone();
        let stop = lifetime.clone();
        job = Some(tokio::spawn(async move {
            ast.index_productive(
                &paths,
                &selected,
                graphit_daemon::watcher::ChangeBatch {
                    rescan: true,
                    ..Default::default()
                },
                None,
                Some(BTreeSet::new()),
                false,
                &Cancellation::default(),
                &Cancellation::default(),
                &stop,
            )
            .await
        }));
        tokio::time::timeout(Duration::from_secs(30), observed).await??;
        let cancel = Cancellation::default();
        let original = fixture.policy.request_context(Instant::now(), &cancel)?;
        let capture = CapturedPublicationScope::project(
            &fixture.project,
            fixture.selected.id,
            fixture.owner.scope,
            original.snapshot(),
        )?;
        let record = capture.binding()?;
        let project = ProjectCapture {
            scope: ScopeCapture {
                live: &capture,
                record: &record,
            },
            version: &fixture.selected.version,
        };
        let registry = fixture.registry();
        let (index, directory) = registry
            .capture(
                fixture.owner.auth.installation(),
                project,
                fixture.owner.native,
                original.snapshot(),
                false,
            )
            .await?
            .ok_or("graph missing")?;
        let source_path = fixture
            .project
            .project_runtime(fixture.selected.id, &["ast", "source.lance"])?;
        let uri = source_path.to_str().ok_or("Source URI is not Unicode")?;
        let raw = Dataset::open(uri).await?;
        let (_, coverage) = source_index(&raw).await?;
        assert!(
            raw.fragments()
                .iter()
                .any(|fragment| !coverage.contains(&u32::try_from(fragment.id).unwrap())),
            "FILE publication precedes scalar catch-up"
        );
        let body = "function fn_001() return 1 end\n";
        assert_eq!(
            observed_text(&source_path, "f001.lua", original.snapshot())
                .await?
                .as_deref(),
            Some(body)
        );
        let observation = index
            .graph
            .file_observation("f001.lua", original.snapshot())?
            .ok_or("FILE missing")?;
        assert_eq!(
            observation.source_sha256,
            <[u8; 32]>::from(Sha256::digest(body.as_bytes()))
        );
        let before = function_rows(&index.graph, original.snapshot())?;
        let row = before
            .iter()
            .find(|row| row[0]["value"] == "f001.lua")
            .ok_or("function missing")?;
        assert_eq!(row[2]["value"], observation.raw_version.to_string());
        assert_eq!(
            row[3]["value"],
            observation
                .source_sha256
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let start = row[4]["value"]
            .as_str()
            .ok_or("invalid start")?
            .parse::<usize>()?;
        let end = row[5]["value"]
            .as_str()
            .ok_or("invalid end")?
            .parse::<usize>()?;
        assert!(
            body.get(start..end)
                .ok_or("invalid Source span")?
                .contains("fn_001")
        );
        let saved = row.clone();
        drop(raw);
        release
            .take()
            .unwrap()
            .send(())
            .map_err(|_| "producer closed")?;
        tokio::time::timeout(Duration::from_secs(30), job.as_mut().unwrap()).await???;
        job.take();
        let raw = Dataset::open(uri).await?;
        let (_, coverage) = source_index(&raw).await?;
        assert!(
            raw.fragments()
                .iter()
                .all(|fragment| coverage.contains(&u32::try_from(fragment.id).unwrap()))
        );
        assert_eq!(
            index
                .graph
                .file_observation("f001.lua", original.snapshot())?,
            Some(observation.clone())
        );
        assert_ne!(
            observation.raw_version,
            raw.version_id(),
            "FILE origin is independent of maintenance HEAD"
        );
        assert_eq!(
            function_rows(&index.graph, original.snapshot())?
                .into_iter()
                .find(|row| row[0]["value"] == "f001.lua"),
            Some(saved.clone())
        );
        drop((raw, index, directory));
        let (reopened, _) = registry
            .capture(
                fixture.owner.auth.installation(),
                project,
                fixture.owner.native,
                original.snapshot(),
                false,
            )
            .await?
            .ok_or("cold graph missing")?;
        assert_eq!(
            reopened
                .graph
                .file_observation("f001.lua", original.snapshot())?,
            Some(observation.clone())
        );
        assert_eq!(
            function_rows(&reopened.graph, original.snapshot())?
                .into_iter()
                .find(|row| row[0]["value"] == "f001.lua"),
            Some(saved)
        );
        assert_eq!(
            observed_text(&source_path, "f001.lua", original.snapshot())
                .await?
                .as_deref(),
            Some(body)
        );
        let unaffected = reopened
            .graph
            .file_observation("f000.lua", original.snapshot())?;
        let updated = "function fn_001_new() return 777 end\n";
        std::fs::write(fixture.project.root().join("f001.lua"), updated)?;
        let (observed, resume) = fixture.ast.hold_after_file_publication("f001.lua");
        release = Some(resume);
        let ast = Arc::clone(&fixture.ast);
        let paths = fixture.project.clone();
        let selected = fixture.selected.clone();
        let stop = lifetime.clone();
        job = Some(tokio::spawn(async move {
            let changed = paths.root().join("f001.lua");
            ast.index_productive(
                &paths,
                &selected,
                graphit_daemon::watcher::ChangeBatch {
                    paths: BTreeSet::from([changed]),
                    ..Default::default()
                },
                None,
                Some(BTreeSet::new()),
                false,
                &Cancellation::default(),
                &Cancellation::default(),
                &stop,
            )
            .await
        }));
        tokio::time::timeout(Duration::from_secs(30), observed).await??;
        let raw = Dataset::open(uri).await?;
        let (_, coverage) = source_index(&raw).await?;
        assert!(
            raw.fragments()
                .iter()
                .any(|fragment| !coverage.contains(&u32::try_from(fragment.id).unwrap())),
            "an updated FILE is readable before scalar catch-up"
        );
        assert_eq!(
            observed_text(&source_path, "f001.lua", original.snapshot())
                .await?
                .as_deref(),
            Some(updated)
        );
        let changed = reopened
            .graph
            .file_observation("f001.lua", original.snapshot())?
            .ok_or("updated FILE missing")?;
        assert!(changed.raw_version > observation.raw_version);
        assert_eq!(changed.raw_version, raw.version_id());
        assert_eq!(
            changed.source_sha256,
            <[u8; 32]>::from(Sha256::digest(updated.as_bytes()))
        );
        assert_eq!(
            reopened
                .graph
                .file_observation("f000.lua", original.snapshot())?,
            unaffected
        );
        let changed_rows = function_rows(&reopened.graph, original.snapshot())?;
        let row = changed_rows
            .iter()
            .find(|row| row[0]["value"] == "f001.lua")
            .ok_or("updated function missing")?;
        assert_eq!(row[1]["value"], "fn_001_new");
        assert_eq!(row[2]["value"], changed.raw_version.to_string());
        let start = row[4]["value"]
            .as_str()
            .ok_or("invalid updated start")?
            .parse::<usize>()?;
        let end = row[5]["value"]
            .as_str()
            .ok_or("invalid updated end")?
            .parse::<usize>()?;
        assert!(
            updated
                .get(start..end)
                .ok_or("invalid updated span")?
                .contains("fn_001_new")
        );
        let saved = row.clone();
        drop(raw);
        release
            .take()
            .unwrap()
            .send(())
            .map_err(|_| "updated producer closed")?;
        tokio::time::timeout(Duration::from_secs(30), job.as_mut().unwrap()).await???;
        job.take();
        let raw = Dataset::open(uri).await?;
        let (_, coverage) = source_index(&raw).await?;
        assert!(
            raw.fragments()
                .iter()
                .all(|fragment| coverage.contains(&u32::try_from(fragment.id).unwrap()))
        );
        assert_ne!(changed.raw_version, raw.version_id());
        drop((raw, reopened));
        let (reopened, _) = registry
            .capture(
                fixture.owner.auth.installation(),
                project,
                fixture.owner.native,
                original.snapshot(),
                false,
            )
            .await?
            .ok_or("updated cold graph missing")?;
        assert_eq!(
            reopened
                .graph
                .file_observation("f001.lua", original.snapshot())?,
            Some(changed)
        );
        assert_eq!(
            reopened
                .graph
                .file_observation("f000.lua", original.snapshot())?,
            unaffected
        );
        assert_eq!(
            function_rows(&reopened.graph, original.snapshot())?
                .into_iter()
                .find(|row| row[0]["value"] == "f001.lua"),
            Some(saved)
        );
        assert_eq!(
            observed_text(&source_path, "f001.lua", original.snapshot())
                .await?
                .as_deref(),
            Some(updated)
        );
        Ok(())
    })
    .catch_unwind()
    .await;
    drop(release);
    lifetime.cancel();
    if let Some(job) = job {
        let _ = job.await;
    }
    fixture.shutdown(result).await
}
