//! A newly built structural cohort must never expose unrelated historical NN hits.
use super::foreground_memory_jobs_tests as support;
use super::*;
use arrow_array::{Array, FixedSizeListArray, Float32Array, StringArray};
use axum::http::{HeaderMap, HeaderValue, header};
use futures_util::{FutureExt, TryStreamExt};
use graphit_ast::native_nn_index_locator::{NativeNnIndexPresence, open_local_native_nn_index};
use graphit_auth::{ProjectCapture, ScopeCapture};
use graphit_core::{config::ConfigStore, native_path::NativePath};
use graphit_daemon::{supervisor::ProjectContext, watcher::ChangeBatch};
use graphit_data::lance_branch_store::BranchSnapshot;
use rmcp::model::CallToolRequestParams;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
async fn vectors(snapshot: &BranchSnapshot) -> TestResult<BTreeMap<String, Vec<f32>>> {
    let mut scan = snapshot.scan();
    scan.project(&["uid", "vector"])?;
    let mut stream = scan.try_into_stream().await?;
    let mut vectors = BTreeMap::new();
    while let Some(batch) = stream.try_next().await? {
        let uids = batch
            .column_by_name("uid")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let values = batch
            .column_by_name("vector")
            .unwrap()
            .as_any()
            .downcast_ref::<FixedSizeListArray>()
            .unwrap();
        for row in 0..batch.num_rows() {
            if values.is_null(row) {
                continue;
            }
            vectors.insert(
                uids.value(row).to_owned(),
                values
                    .value(row)
                    .as_any()
                    .downcast_ref::<Float32Array>()
                    .unwrap()
                    .values()
                    .to_vec(),
            );
        }
    }
    Ok(vectors)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn changed_file_withdraws_its_vectors_and_preserves_the_intact_file() -> TestResult {
    let (_fixture, mut owner, tasks, credential, _) = support::browser_search_fixture().await;
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let resources = owner.memory_job_resources.upgrade().unwrap();
    owner
        .server
        .as_mut()
        .unwrap()
        .attach_semantic_model(Some(Arc::clone(&resources)))?;
    let source = owner.server.as_ref().unwrap().clone();
    let cancel = Cancellation::default();
    ConfigStore::project(&selected.0)?.set(
        ConfigKey::new("modules.ast")?,
        json!(true),
        Duration::from_secs(5),
        &cancel,
    )?;
    for (path, text) in [
        ("a.rs", "pub fn alpha() -> i32 { 7 }\n"),
        ("b.rs", "pub fn beta() -> i32 { 3 }\n"),
    ] {
        fs::write(selected.0.root().join(path), text)?;
    }
    let no_docker = tempfile::tempdir()?;
    let original = owner.policy.request_context(Instant::now(), &cancel)?;
    let ast = Arc::new(ForegroundAstSources::new(
        Arc::clone(&owner.auth),
        ForegroundAstSourceConfig {
            live: source.graph_server().live_ast(),
            installation: source.policy().installation.clone(),
            base: ConfigSnapshot::new(source.policy().installation.brand(), [])?,
            policy: owner.policy.clone(),
            scope: owner.scope,
            native: owner.native,
            binding: NativeBinding::KiroV3,
            environment: vec![("PATH".into(), no_docker.path().as_os_str().to_owned())],
        },
        original.snapshot(),
    )?);
    let context = ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    };
    let result=std::panic::AssertUnwindSafe(async {
        ast.index(&selected.0,&context,ChangeBatch{rescan:true,..Default::default()},&cancel).await?;
        ast.index_semantic(selected.0.clone(),context.clone(),Some(Arc::clone(&resources)),None, std::collections::BTreeSet::new(), false,cancel.clone(), cancel.clone()).await?;
        let capture=CapturedPublicationScope::project(&selected.0,selected.1,owner.scope,original.snapshot())?;
        let record=capture.binding()?;
        let project=ProjectCapture{scope:ScopeCapture{live:&capture,record:&record},version:&selected.2};
        let NativeNnIndexPresence::Available(index)=open_local_native_nn_index(owner.auth.installation(),project,original.snapshot(),owner.native).await? else {return Err("initial NN index missing".into());};
        let old=index.snapshot(original.snapshot())?.clone();
        let before=vectors(&old).await?;
        assert_eq!(before.len(),2,"both genuine parser projections were inferred by the CPU child");
        assert_eq!(old.corpus_uri(),capture.runtime_root().join("ast/corpus.lance").to_str().unwrap(),"writer and reader share the canonical corpus");
        assert!(capture.runtime_root().join("ast/source.lance").try_exists()?);
        fs::write(selected.0.root().join("a.rs"),"pub fn changed_alpha() -> i32 { 11 }\n")?;
        let (first,release)=ast.hold_after_file_publication("a.rs");
        let producer=Arc::clone(&ast);let project_paths=selected.0.clone();
        let cancellation=cancel.clone();
        let job=tokio::spawn(async move{producer.index_productive(&project_paths,&context,
            ChangeBatch{paths: BTreeSet::from([project_paths.root().join("a.rs")]),..Default::default()},None,
            Some(BTreeSet::from(["a.rs".to_owned()])),false,&cancellation,&cancellation,&cancellation).await});
        tokio::time::timeout(Duration::from_secs(30),first).await??;
        let NativeNnIndexPresence::Available(current)=open_local_native_nn_index(owner.auth.installation(),project,original.snapshot(),owner.native).await? else {return Err("corpus missing".into());};
        let during=vectors(current.snapshot(original.snapshot())?).await?;
        assert_eq!(during.len(),1,"only changed FILE loses its vectors before inference");
        for (uid,vector) in &during { assert_eq!(before.get(uid),Some(vector)); }
        let headers=HeaderMap::from_iter([(header::AUTHORIZATION,HeaderValue::from_str(&format!("Bearer {credential}"))?),
            (header::HeaderName::from_static("x-graphit-auth-origin"),HeaderValue::from_static("runtime"))]);
        let search=source.graph_server().call_search_mcp(&source,CallToolRequestParams::new("graphit_ast_search").with_arguments(json!({
            "project_dir":NativePath::new(selected.0.root().to_owned())?,"project_version":selected.2.version(),
            "query":"beta","mode":"semantic","rerank":false,
        }).as_object().unwrap().clone()),headers.clone(),CancellationToken::new()).await;
        assert_ne!(search.is_error,Some(true),"the first FILE must be queryable without a 409 from unrelated historical NN hits: {search:?}");
        let search:Value=search.structured_content.unwrap();assert_eq!(search["status"],"available", "intact FILE vectors remain immediately queryable");
        assert!(!search["results"].as_array().unwrap().is_empty());
        release.send(()).unwrap();job.await??;
        ast.index_semantic(selected.0.clone(),ProjectContext{id:selected.1,version:selected.2.clone()},
            Some(Arc::clone(&resources)),Some(std::collections::BTreeSet::new()), std::collections::BTreeSet::new(), false,cancel.clone(), cancel.clone()).await?;
        let NativeNnIndexPresence::Available(current)=open_local_native_nn_index(owner.auth.installation(),project,original.snapshot(),owner.native).await? else {return Err("completed NN cohort missing".into());};
        let after=vectors(current.snapshot(original.snapshot())?).await?;
        assert_eq!(after.len(),2);
        for (uid,vector) in during { assert_eq!(after.get(&uid),Some(&vector)); }
        Ok::<(),Box<dyn std::error::Error>>(())
    }).catch_unwind().await;
    let compiler_cleanup = ast.shutdown().await;
    tasks.shutdown().await?;
    owner.shutdown().await.map_err(|failure| failure.error)?;
    compiler_cleanup?;
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn first_file_is_queryable_before_raw_full_eof_and_delta_preempts_tail() -> TestResult {
    let (_fixture, mut owner, tasks, credential, _) = support::browser_search_fixture().await;
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let resources = owner.memory_job_resources.upgrade().unwrap();
    owner
        .server
        .as_mut()
        .unwrap()
        .attach_semantic_model(Some(Arc::clone(&resources)))?;
    let source = owner.server.as_ref().unwrap().clone();
    let cancel = Cancellation::default();
    ConfigStore::project(&selected.0)?.set(
        ConfigKey::new("modules.ast")?,
        json!(true),
        Duration::from_secs(5),
        &cancel,
    )?;
    for (path, text) in [
        ("a.rs", "pub fn alpha() -> i32 { 7 }\n"),
        ("b.rs", "pub fn beta() -> i32 { 3 }\n"),
    ] {
        fs::write(selected.0.root().join(path), text)?;
    }
    let no_docker = tempfile::tempdir()?;
    let original = owner.policy.request_context(Instant::now(), &cancel)?;
    let ast = Arc::new(ForegroundAstSources::new(
        Arc::clone(&owner.auth),
        ForegroundAstSourceConfig {
            live: source.graph_server().live_ast(),
            installation: source.policy().installation.clone(),
            base: ConfigSnapshot::new(source.policy().installation.brand(), [])?,
            policy: owner.policy.clone(),
            scope: owner.scope,
            native: owner.native,
            binding: NativeBinding::KiroV3,
            environment: vec![("PATH".into(), no_docker.path().as_os_str().to_owned())],
        },
        original.snapshot(),
    )?);
    let context = ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    };
    let result = std::panic::AssertUnwindSafe(async {
        let capture = CapturedPublicationScope::project(&selected.0, selected.1, owner.scope, original.snapshot())?;
        let record = capture.binding()?;
        let project = ProjectCapture { scope: ScopeCapture { live: &capture, record: &record }, version: &selected.2 };
        let (first, release) = ast.hold_after_file_publication("a.rs");
        let producer = Arc::clone(&ast);
        let paths = selected.0.clone();
        let context_copy = context.clone();
        let execution = Arc::clone(&resources);
        let lifetime = cancel.clone();
        let background = Cancellation::default();
        let ingress = background.clone();
        let job = tokio::spawn(async move { producer.index_productive(&paths, &context_copy,
            ChangeBatch { rescan: true, ..Default::default() }, Some(execution), Some(BTreeSet::new()), false, &ingress, &ingress, &lifetime).await });
        tokio::time::timeout(Duration::from_secs(30), first).await??;
        assert!(!job.is_finished());
        let raw = graphit_ast::source_index_locator::open_local_source_index(owner.auth.installation(), project,
            original.snapshot(), owner.native).await?;
        let graphit_ast::source_index_locator::SourceIndexPresence::Available(raw) = raw else {
            return Err("first Raw FILE is not available".into());
        };
        assert_eq!(raw.paths(original.snapshot()).await?, ["a.rs"], "the tail has not been read or committed to Raw");
        let headers = HeaderMap::from_iter([
            (header::AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {credential}"))?),
            (header::HeaderName::from_static("x-graphit-auth-origin"), HeaderValue::from_static("runtime")),
        ]);
        let syntax = source.graph_server().call_mcp(CallToolRequestParams::new("graphit_ast_query").with_arguments(json!({
            "project_dir": NativePath::new(selected.0.root().to_owned())?, "project_version": selected.2.version(),
            "query": "MATCH (n:Function) WHERE n.name='alpha' RETURN n.uid,n.raw_version,n.source_sha256",
        }).as_object().unwrap().clone()), headers.clone(), CancellationToken::new()).await;
        assert_ne!(syntax.is_error, Some(true), "{syntax:?}");
        let syntax = syntax.structured_content.unwrap();
        assert_eq!(syntax["rows"].as_array().unwrap().len(), 1);
        let search = source.graph_server().call_search_mcp(&source, CallToolRequestParams::new("graphit_ast_search").with_arguments(json!({
            "project_dir": NativePath::new(selected.0.root().to_owned())?, "project_version": selected.2.version(),
            "query": "alpha", "mode": "fts", "rerank": false,
        }).as_object().unwrap().clone()), headers.clone(), CancellationToken::new()).await;
        assert_ne!(search.is_error, Some(true), "{search:?}");
        assert!(!search.structured_content.unwrap()["results"].as_array().unwrap().is_empty());
        let before = tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                if let NativeNnIndexPresence::Available(index) = open_local_native_nn_index(owner.auth.installation(), project, original.snapshot(), owner.native).await? {
                    let values = vectors(index.snapshot(original.snapshot())?).await?;
                    if !values.is_empty() { break TestResult::Ok(values); }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await??;
        assert_eq!(before.len(), 1, "embedding progresses before Raw EOF");
        background.cancel();
        release.send(()).unwrap();
        assert_eq!(job.await?.unwrap_err().kind(), io::ErrorKind::Interrupted);
        fs::write(selected.0.root().join("b.rs"), "pub fn beta_changed() -> i32 { 9 }\n")?;
        ast.index_productive(&selected.0, &context, ChangeBatch {
            paths: BTreeSet::from([selected.0.root().join("b.rs")]), ..Default::default()
        }, Some(Arc::clone(&resources)), Some(BTreeSet::new()), false, &cancel, &cancel, &cancel).await?;
        // Resume the full metadata walk after the real delta. A is already
        // published and its graph revision, Source and vector must be reused.
        let resumed = ast.index_productive(&selected.0, &context, ChangeBatch { rescan: true, ..Default::default() },
            Some(Arc::clone(&resources)), Some(BTreeSet::new()), false, &cancel, &cancel, &cancel).await?;
        assert!(resumed.pending_files.is_empty());
        let current = source.graph_server().call_mcp(CallToolRequestParams::new("graphit_ast_query").with_arguments(json!({
            "project_dir": NativePath::new(selected.0.root().to_owned())?, "project_version": selected.2.version(),
            "query": "MATCH (n:Function) WHERE n.name='alpha' RETURN n.uid,n.raw_version,n.source_sha256",
        }).as_object().unwrap().clone()), headers.clone(), CancellationToken::new()).await;
        assert_ne!(current.is_error, Some(true), "{current:?}");
        assert_eq!(syntax["rows"], current.structured_content.unwrap()["rows"]);
        let NativeNnIndexPresence::Available(index) = open_local_native_nn_index(owner.auth.installation(), project, original.snapshot(), owner.native).await? else { return Err("completed NN missing".into()); };
        let after = vectors(index.snapshot(original.snapshot())?).await?;
        assert_eq!(after.len(), 2);
        for (uid, vector) in before { assert_eq!(after.get(&uid), Some(&vector)); }
        // Existing vectors, no pending projections: a refresh whose metadata
        // admission fails must remain pending for a later empty admission.
        let unavailable = Arc::new(crate::foreground_memory_jobs::MemoryJobResources::remote(
            Arc::clone(&resources.auth), resources.http.clone(), Arc::clone(&resources.brokers),
            resources.policy.clone(), resources.scope, resources.native, resources.binding)?);
        assert!(unavailable.local.is_none());
        let failed = ast.index_productive(&selected.0, &context, ChangeBatch::default(),
            Some(unavailable), Some(BTreeSet::new()), true, &cancel, &cancel, &cancel).await.unwrap_err();
        assert!(crate::foreground_ast_sources::semantic::only_inference_failed(&failed));
        let refresh_pending = !crate::foreground_ast_sources::semantic::inference_model_reconciled(&failed);
        assert!(refresh_pending, "metadata failure cannot consume the refresh request");
        let recovered = ast.index_productive(&selected.0, &context, ChangeBatch::default(),
            Some(Arc::clone(&resources)), Some(BTreeSet::new()), refresh_pending, &cancel, &cancel, &cancel).await?;
        assert!(recovered.model_reconciled, "the next empty admission reconciles existing vectors");
        let NativeNnIndexPresence::Available(index) = open_local_native_nn_index(owner.auth.installation(), project, original.snapshot(), owner.native).await? else { return Err("NN missing after refresh recovery".into()); };
        assert_eq!(after, vectors(index.snapshot(original.snapshot())?).await?, "compatible vectors are reused without parsing or inference");
        // B was absent at enumeration but becomes selected again while A is
        // held after its durable FILE commit. EOF must not delete B's old data.
        fs::remove_file(selected.0.root().join("b.rs"))?;
        fs::write(selected.0.root().join("a.rs"), "pub fn alpha() -> i32 { 11 }\n")?;
        let (first, release) = ast.hold_after_file_publication("a.rs");
        let producer = Arc::clone(&ast);
        let paths = selected.0.clone();
        let selected_context = context.clone();
        let lifetime = cancel.clone();
        let job = tokio::spawn(async move { producer.index_productive(&paths, &selected_context,
            ChangeBatch { rescan: true, ..Default::default() }, None, Some(BTreeSet::new()), false, &lifetime, &lifetime, &lifetime).await });
        tokio::time::timeout(Duration::from_secs(30), first).await??;
        fs::write(selected.0.root().join("b.rs"), "pub fn beta_reappeared() {}\n")?;
        release.send(()).unwrap();
        assert_eq!(job.await?.unwrap_err().kind(), io::ErrorKind::InvalidData);
        let preserved = source.graph_server().call_mcp(CallToolRequestParams::new("graphit_ast_query").with_arguments(json!({
            "project_dir": NativePath::new(selected.0.root().to_owned())?, "project_version": selected.2.version(),
            "query": "MATCH (n:Function) WHERE n.name='beta_changed' RETURN n.name",
        }).as_object().unwrap().clone()), headers.clone(), CancellationToken::new()).await;
        assert_ne!(preserved.is_error, Some(true), "{preserved:?}");
        assert_eq!(preserved.structured_content.unwrap()["rows"].as_array().unwrap().len(), 1, "a FILE cannot be deleted using stale selection coverage");
        // A full pass still pending must not postpone explicit FILE/subtree
        // tombstones until EOF, nor confuse a subtree with its similarly named sibling.
        for (path, name) in [("gone/item.lua", "removed_nested"), ("gone2/item.lua", "kept_sibling")] {
            fs::create_dir_all(selected.0.root().join(path).parent().unwrap())?;
            fs::write(selected.0.root().join(path), format!("function {name}() return 1 end\n"))?;
        }
        ast.index_productive(&selected.0, &context, ChangeBatch {
            paths: BTreeSet::from([selected.0.root().join("gone"), selected.0.root().join("gone2")]),
            ..Default::default()
        }, None, Some(BTreeSet::new()), false, &cancel, &cancel, &cancel).await?;
        fs::remove_file(selected.0.root().join("b.rs"))?;
        fs::remove_dir_all(selected.0.root().join("gone"))?;
        fs::write(selected.0.root().join("a.rs"), "pub fn alpha() -> i32 { 13 }\n")?;
        let (tail, release) = ast.hold_after_file_publication("a.rs");
        let producer = Arc::clone(&ast);
        let paths = selected.0.clone();
        let selected_context = context.clone();
        let lifetime = cancel.clone();
        let background = Cancellation::default();
        let ingress = background.clone();
        let job = tokio::spawn(async move { producer.index_productive(&paths, &selected_context,
            ChangeBatch { rescan: true, paths: BTreeSet::from([paths.root().join("b.rs"), paths.root().join("gone")]),
                ..Default::default() }, None, Some(BTreeSet::new()), false, &ingress, &ingress, &lifetime).await });
        tokio::time::timeout(Duration::from_secs(30), tail).await??;
        assert!(!job.is_finished(), "the full pass has not reached EOF");
        let raw = graphit_ast::source_index_locator::open_local_source_index(owner.auth.installation(), project,
            original.snapshot(), owner.native).await?;
        let graphit_ast::source_index_locator::SourceIndexPresence::Available(raw) = raw else {
            return Err("Raw unavailable after explicit tombstones".into());
        };
        let remaining = raw.paths(original.snapshot()).await?;
        assert!(!remaining.iter().any(|path| path == "b.rs" || path.starts_with("gone/")));
        assert!(remaining.iter().any(|path| path == "gone2/item.lua"));
        let remaining = source.graph_server().call_mcp(CallToolRequestParams::new("graphit_ast_query").with_arguments(json!({
            "project_dir": NativePath::new(selected.0.root().to_owned())?, "project_version": selected.2.version(),
            "query": "MATCH (n:Function) RETURN n.name,n.path",
        }).as_object().unwrap().clone()), headers.clone(), CancellationToken::new()).await;
        assert_ne!(remaining.is_error, Some(true), "{remaining:?}");
        let remaining = remaining.structured_content.unwrap();
        let remaining = remaining["rows"].as_array().unwrap();
        assert!(remaining.iter().any(|row| row[0]["value"] == "kept_sibling" && row[1]["value"] == "gone2/item.lua"));
        assert!(!remaining.iter().any(|row| row[0]["value"] == "beta_changed" || row[0]["value"] == "removed_nested"));
        let search = source.graph_server().call_search_mcp(&source, CallToolRequestParams::new("graphit_ast_search").with_arguments(json!({
            "project_dir": NativePath::new(selected.0.root().to_owned())?, "project_version": selected.2.version(),
            "query": "beta_changed removed_nested", "mode": "fts", "rerank": false,
        }).as_object().unwrap().clone()), headers.clone(), CancellationToken::new()).await;
        assert_ne!(search.is_error, Some(true), "{search:?}");
        assert!(!search.structured_content.unwrap()["results"].as_array().unwrap().iter()
            .any(|hit| hit["path"] == "b.rs" || hit["path"].as_str().is_some_and(|path| path.starts_with("gone/"))));
        let NativeNnIndexPresence::Available(index) = open_local_native_nn_index(owner.auth.installation(), project,
            original.snapshot(), owner.native).await? else { return Err("NN unavailable after tombstones".into()); };
        let mut scanner = index.snapshot(original.snapshot())?.scan();
        scanner.project(&["path"])?;
        let mut stream = scanner.try_into_stream().await?;
        while let Some(batch) = stream.try_next().await? {
            let paths = batch.column_by_name("path").unwrap().as_any().downcast_ref::<StringArray>().unwrap();
            assert!(paths.iter().flatten().all(|path| path != "b.rs" && !path.starts_with("gone/")));
        }
        background.cancel();
        release.send(()).unwrap();
        assert_eq!(job.await?.unwrap_err().kind(), io::ErrorKind::Interrupted);
        Ok::<(), Box<dyn std::error::Error>>(())
    }).catch_unwind().await;
    cancel.cancel();
    let compiler_cleanup = ast.shutdown().await;
    tasks.shutdown().await?;
    owner.shutdown().await.map_err(|failure| failure.error)?;
    compiler_cleanup?;
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[test]
fn partial_raw_batches_retain_unpublished_native_tail_and_broad_recovery() {
    use crate::foreground_ast_sources::semantic::merge_pending_selection;
    let mut pending = Some(BTreeSet::from(["z.vb".to_owned()]));
    merge_pending_selection(&mut pending, BTreeSet::new(), false);
    assert_eq!(pending, Some(BTreeSet::from(["z.vb".to_owned()])));
    merge_pending_selection(&mut pending, BTreeSet::from(["other.vb".to_owned()]), false);
    assert_eq!(pending.as_ref().unwrap().len(), 2);
    let mut broad = None;
    merge_pending_selection(&mut broad, BTreeSet::from(["z.vb".to_owned()]), false);
    assert!(
        broad.is_none(),
        "partial admission does not prove full structural recovery"
    );
    merge_pending_selection(&mut broad, BTreeSet::from(["z.vb".to_owned()]), true);
    assert_eq!(broad, Some(BTreeSet::from(["z.vb".to_owned()])));
    // A full Raw pass already reconciled its syntax tail. Replaying it in the
    // compiler phase would repeat parsing and preparation without new input.
    merge_pending_selection(&mut pending, BTreeSet::from(["z.vb".to_owned()]), true);
    assert_eq!(pending, Some(BTreeSet::from(["z.vb".to_owned()])));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn incremental_scopes_preserve_unchanged_files_and_repair_missing_native_files() -> TestResult
{
    let (_fixture, owner, tasks, _, _) = support::browser_search_fixture().await;
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let resources = owner.memory_job_resources.upgrade().unwrap();
    let cancellation = Cancellation::default();
    for key in ["modules.ast", "modules.embedding"] {
        ConfigStore::project(&selected.0)?.set(
            ConfigKey::new(key)?,
            json!(true),
            Duration::from_secs(5),
            &cancellation,
        )?;
    }
    for (path, text) in [
        ("src/a.lua", "function alpha() return 1 end\n"),
        ("src/b.lua", "function beta() return 2 end\n"),
        ("src2/c.lua", "function gamma() return 3 end\n"),
    ] {
        let path = selected.0.root().join(path);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, text)?;
    }
    let source = owner.server.as_ref().unwrap().clone();
    let original = owner
        .policy
        .request_context(Instant::now(), &cancellation)?;
    let no_docker = tempfile::tempdir()?;
    let registry = source.graph_server().live_ast();
    let ast = Arc::new(ForegroundAstSources::new(
        Arc::clone(&owner.auth),
        ForegroundAstSourceConfig {
            live: Arc::clone(&registry),
            installation: source.policy().installation.clone(),
            base: ConfigSnapshot::new(source.policy().installation.brand(), [])?,
            policy: owner.policy.clone(),
            scope: owner.scope,
            native: owner.native,
            binding: NativeBinding::KiroV3,
            environment: vec![("PATH".into(), no_docker.path().as_os_str().to_owned())],
        },
        original.snapshot(),
    )?);
    let context = ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    };
    let result = std::panic::AssertUnwindSafe(async {
        let capture = CapturedPublicationScope::project(
            &selected.0,
            selected.1,
            owner.scope,
            original.snapshot(),
        )?;
        let record = capture.binding()?;
        let project = ProjectCapture {
            scope: ScopeCapture {
                live: &capture,
                record: &record,
            },
            version: &selected.2,
        };
        let publish = |batch| {
            ast.index_productive(
                &selected.0,
                &context,
                batch,
                None,
                Some(BTreeSet::new()),
                false,
                &cancellation,
                &cancellation,
                &cancellation,
            )
        };
        publish(ChangeBatch {
            rescan: true,
            ..Default::default()
        })
        .await?;
        let open = || async {
            registry
                .capture(
                    owner.auth.installation(),
                    project,
                    owner.native,
                    original.snapshot(),
                    false,
                )
                .await?
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        };
        let (live, directory) = open().await?;
        let before = live
            .graph
            .files(original.snapshot())?
            .into_iter()
            .map(|file| (file.path.clone(), file))
            .collect::<BTreeMap<_, _>>();
        for path in ["src/a.lua", "src/b.lua", "src2/c.lua"] {
            assert!(before.contains_key(path));
        }
        drop((live, directory));
        publish(ChangeBatch {
            paths: BTreeSet::from([selected.0.root().join("src")]),
            ..Default::default()
        })
        .await?;
        let (live, directory) = open().await?;
        for (path, expected) in &before {
            assert_eq!(
                live.graph
                    .file_observation(path, original.snapshot())?
                    .as_ref(),
                Some(expected),
                "an unchanged subtree must preserve every FILE tag, including its sibling"
            );
        }
        {
            let _fence = live.write_fence(original.snapshot()).await?;
            live.graph.remove_file("src/a.lua", original.snapshot())?;
        }
        drop((live, directory));
        fs::remove_file(selected.0.root().join("src/b.lua"))?;
        fs::write(
            selected.0.root().join("src2/c.lua"),
            "function gamma() return 33 end\n",
        )?;
        publish(ChangeBatch {
            paths: BTreeSet::from([selected.0.root().join("src")]),
            ..Default::default()
        })
        .await?;
        let (live, directory) = open().await?;
        let repaired = live
            .graph
            .file_observation("src/a.lua", original.snapshot())?
            .ok_or("missing FILE not repaired")?;
        assert_eq!(repaired.source_sha256, before["src/a.lua"].source_sha256);
        assert!(repaired.raw_version >= before["src/a.lua"].raw_version);
        assert!(repaired.file_revision > before["src/a.lua"].file_revision);
        assert!(
            live.graph
                .file_observation("src/b.lua", original.snapshot())?
                .is_none()
        );
        assert_eq!(
            live.graph
                .file_observation("src2/c.lua", original.snapshot())?
                .as_ref(),
            Some(&before["src2/c.lua"]),
            "the sibling's changed source remains outside this subtree event"
        );
        drop((live, directory));
        publish(ChangeBatch {
            paths: BTreeSet::from([selected.0.root().join("src/a.lua")]),
            ..Default::default()
        })
        .await?;
        let (live, directory) = open().await?;
        assert_eq!(
            live.graph
                .file_observation("src/a.lua", original.snapshot())?
                .as_ref(),
            Some(&repaired),
            "an explicit unchanged FILE must preserve its repaired tag"
        );
        drop((live, directory));
        publish(ChangeBatch {
            paths: BTreeSet::from([selected.0.root().join("src2/c.lua")]),
            ..Default::default()
        })
        .await?;
        let (live, directory) = open().await?;
        let changed = live
            .graph
            .file_observation("src2/c.lua", original.snapshot())?
            .ok_or("changed FILE absent")?;
        assert_ne!(changed.source_sha256, before["src2/c.lua"].source_sha256);
        assert!(changed.file_revision > before["src2/c.lua"].file_revision);
        let (root, _) = crate::foreground_ast_sources::progressive::location(
            owner.auth.installation(),
            project,
        )?;
        let graph_path = root.join("graph");
        let retained_graph = root.join("retained-graph");
        #[cfg(unix)]
        {
            fs::rename(&graph_path, &retained_graph)?;
            let (shared, shared_directory) = open().await?;
            assert!(Arc::ptr_eq(&live.graph, &shared.graph));
            assert!(Arc::ptr_eq(&live.fence, &shared.fence));
            assert!(directory.directory_identity() == shared_directory.directory_identity());
            drop((shared, shared_directory));
            fs::rename(&retained_graph, &graph_path)?;
        }
        drop((live, directory));

        // Embedding work consumes the Lance corpus independently of the graph.
        fs::rename(&graph_path, &retained_graph)?;
        let stop = Cancellation::default();
        let (_sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let reconciled = std::cell::Cell::new(false);
        let waiting = ast.embed_pending_loop(
            &selected.0,
            &context,
            Some(&resources),
            receiver,
            false,
            BTreeSet::new(),
            &reconciled,
            &stop,
        );
        tokio::pin!(waiting);
        assert!(futures_util::poll!(waiting.as_mut()).is_pending());
        stop.cancel();
        assert_eq!(
            waiting.await.unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert!(!graph_path.try_exists()?);
        fs::rename(&retained_graph, &graph_path)?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .catch_unwind()
    .await;
    cancellation.cancel();
    let compiler_cleanup = ast.shutdown().await;
    tasks.shutdown().await?;
    owner.shutdown().await.map_err(|failure| failure.error)?;
    compiler_cleanup?;
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn idle_nn_releases_live_database_and_reopens_the_same_source_and_vectors()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::foreground_source::foreground_memory_jobs_tests as support;
    use futures_util::FutureExt;
    use graphit_core::config::ConfigStore;
    use graphit_daemon::watcher::ChangeBatch;
    use serde_json::json;
    use std::{
        future::Future,
        task::Poll,
        time::{Duration, Instant},
    };

    let (_fixture, owner, tasks, credential, _) = support::browser_search_fixture().await;
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let resources = owner.memory_job_resources.upgrade().unwrap();
    let cancellation = Cancellation::default();
    ConfigStore::project(&selected.0)?.set(
        ConfigKey::new("modules.ast")?,
        json!(true),
        Duration::from_secs(5),
        &cancellation,
    )?;
    std::fs::write(
        selected.0.root().join("idle.lua"),
        "function idle_source() return 7 end\n",
    )?;
    let source = owner.server.as_ref().unwrap().clone();
    let original = owner
        .policy
        .request_context(Instant::now(), &cancellation)?;
    let no_docker = tempfile::tempdir()?;
    let ast = Arc::new(ForegroundAstSources::new(
        Arc::clone(&owner.auth),
        ForegroundAstSourceConfig {
            live: source.graph_server().live_ast(),
            installation: source.policy().installation.clone(),
            base: ConfigSnapshot::new(source.policy().installation.brand(), [])?,
            policy: owner.policy.clone(),
            scope: owner.scope,
            native: owner.native,
            binding: graphit_hub::NativeBinding::KiroV3,
            environment: vec![("PATH".into(), no_docker.path().as_os_str().to_owned())],
        },
        original.snapshot(),
    )?);
    let context = ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    };
    let result = std::panic::AssertUnwindSafe(async {
        let capture = CapturedPublicationScope::project(
            &selected.0,
            selected.1,
            owner.scope,
            original.snapshot(),
        )?;
        let record = capture.binding()?;
        let project = ProjectCapture {
            scope: ScopeCapture {
                live: &capture,
                record: &record,
            },
            version: &selected.2,
        };
        let registry = source.graph_server().live_ast();
        let (native_root, _) = crate::foreground_ast_sources::progressive::location(
            owner.auth.installation(),
            project,
        )?;
        assert!(!native_root.join("graph").try_exists()?);
        let prepare = || async {
            crate::foreground_ast_sources::progressive::ProgressivePublication::open(
                owner.auth.installation(),
                project,
                owner.native,
                Arc::clone(original.snapshot()),
                Arc::clone(&registry),
                &|| original.check(),
            )
            .await
        };
        let mut prepared = prepare().await?;
        let corpus_root=graphit_ast::corpus::project_directory(owner.auth.installation(),project)?;
        assert!(corpus_root.join("corpus.lance").try_exists()?);
        assert!(!native_root.join("search.lance").try_exists()?);
        assert!(!native_root.join("projections.lance").try_exists()?);
        assert!(
            !native_root.join("graph").try_exists()?,
            "preparing the real Lance stores must not construct a native database"
        );
        prepared
            .retain_paths(&BTreeSet::new(), &|| original.check())
            .await?;
        assert!(
            !native_root.join("graph").try_exists()?,
            "retaining Lance pending paths must not construct a native database"
        );
        assert!(
            registry
                .capture(
                    owner.auth.installation(),
                    project,
                    owner.native,
                    original.snapshot(),
                    false,
                )
                .await?
                .is_none(),
            "a reader cannot initialize the prepared cohort"
        );
        ast.index_productive(
            &selected.0,
            &context,
            ChangeBatch {
                rescan: true,
                ..Default::default()
            },
            Some(Arc::clone(&resources)),
            Some(BTreeSet::new()),
            false,
            &cancellation,
            &cancellation,
            &cancellation,
        )
        .await?;
        drop(prepared);
        let query = source
            .graph_server()
            .call_mcp(
                CallToolRequestParams::new("graphit_ast_query").with_arguments(
                    json!({
                        "project_dir": NativePath::new(selected.0.root().to_owned())?,
                        "project_version": selected.2.version(),
                        "query": "MATCH (n:AstFileObservation) WHERE n.path='idle.lua' RETURN n.path",
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                ),
                HeaderMap::from_iter([
                    (
                        header::AUTHORIZATION,
                        HeaderValue::from_str(&format!("Bearer {credential}"))?,
                    ),
                    (
                        header::HeaderName::from_static("x-graphit-auth-origin"),
                        HeaderValue::from_static("runtime"),
                    ),
                ]),
                CancellationToken::new(),
            )
            .await;
        assert_ne!(query.is_error, Some(true), "{query:?}");
        let query = query.structured_content.ok_or("query result absent")?;
        assert_eq!(query["rows"].as_array().unwrap().len(), 1);
        assert_eq!(query["rows"][0].as_array().unwrap().len(), 1);
        assert_eq!(query["rows"][0][0]["value"], "idle.lua");
        let open = || async {
            registry
                .capture(
                    owner.auth.installation(),
                    project,
                    owner.native,
                    original.snapshot(),
                    false,
                )
                .await
                .and_then(|captured| {
                    captured.ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
                })
        };
        let (live, directory) = open().await?;
        let stable_fence = Arc::clone(&live.fence);
        let before_source = live
            .graph
            .file_observation("idle.lua", original.snapshot())?
            .ok_or("Source absent")?;
        // Search and role consumers can keep Arc<LiveGraph> after the lightweight
        // index wrapper drops. Captures must share that real database immediately.
        let escaped = Arc::clone(&live.graph);
        let mut closed = escaped.close_notification();
        drop((live, directory));
        let replacement = native_root.with_extension("live-replacement");
        fs::rename(&native_root, &replacement)?;
        drop(graphit_fs::create_private_directory(&native_root)?);
        drop(prepare().await?);
        assert_eq!(
            open()
                .await
                .err()
                .expect("graph capture cannot replace a live graph directory")
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert!(!native_root.join("search.lance").try_exists()?);
        fs::remove_dir(&native_root)?;
        fs::rename(&replacement, &native_root)?;
        let live_prepared = prepare().await?;
        let ((a, da), (b, db)) = tokio::time::timeout(Duration::from_secs(5), async {
            let (a, b) = tokio::join!(open(), open());
            Ok::<_, std::io::Error>((a?, b?))
        })
        .await??;
        assert!(Arc::ptr_eq(&a.graph, &escaped));
        assert!(Arc::ptr_eq(&a.graph, &b.graph));
        assert!(Arc::ptr_eq(&a.fence, &b.fence));
        assert!(Arc::ptr_eq(&a.fence, &stable_fence));
        drop((a, da, b, db));

        assert!(
            closed.has_changed().is_ok(),
            "observing closure does not retain or close a live graph"
        );
        drop(escaped);
        assert!(
            closed.changed().await.is_err(),
            "native destruction closes the notification"
        );
        drop(live_prepared);
        let graph_path = native_root.join("graph");
        let retained_graph = native_root.join("retained-graph");
        fs::rename(&graph_path, &retained_graph)?;
        for create in [false, true] {
            assert!(
                registry
                    .capture(
                        owner.auth.installation(),
                        project,
                        owner.native,
                        original.snapshot(),
                        create,
                    )
                    .await?
                    .is_none(),
                "a known cohort's removed graph must not be recreated"
            );
            assert!(!graph_path.try_exists()?);
        }
        fs::rename(&retained_graph, &graph_path)?;
        // Model the precise gap after Weak::upgrade starts failing and before
        // native destruction closes its signal. Exercise capture, not just the
        // notification primitive; no C++ hook or database contents are changed.
        let closing = registry.hold_native_close_for_test(&native_root);
        fs::rename(&native_root, &replacement)?;
        drop(graphit_fs::create_private_directory(&native_root)?);
        drop(prepare().await?);
        assert_eq!(
            open()
                .await
                .err()
                .expect("graph capture cannot replace a closing graph directory")
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert!(!native_root.join("projections.lance").try_exists()?);
        fs::remove_dir(&native_root)?;
        fs::rename(&replacement, &native_root)?;
        let first = open();
        let second = open();
        tokio::pin!(first, second);
        assert!(futures_util::poll!(first.as_mut()).is_pending());
        assert!(futures_util::poll!(second.as_mut()).is_pending());
        let stop = Cancellation::default();
        let waiting_context = Arc::new(graphit_core::file_io::SnapshotContext::service(
            stop.clone(),
        ));
        let waiting = registry.capture(
            owner.auth.installation(),
            project,
            owner.native,
            &waiting_context,
            false,
        );
        tokio::pin!(waiting);
        assert!(futures_util::poll!(waiting.as_mut()).is_pending());
        // Corpus preparation does not depend on the graph's close lifecycle.
        drop(prepare().await?);
        stop.cancel();
        assert_eq!(
            waiting
                .await
                .err()
                .expect("cancelled native close wait")
                .kind(),
            std::io::ErrorKind::Interrupted
        );
        let waiting_recheck = || waiting_context.check();
        assert_eq!(
            crate::foreground_ast_sources::progressive::ProgressivePublication::open(
                owner.auth.installation(),
                project,
                owner.native,
                Arc::clone(&waiting_context),
                Arc::clone(&registry),
                &waiting_recheck,
            )
            .await
            .err()
            .expect("cancelled preparation")
            .kind(),
            std::io::ErrorKind::Interrupted
        );
        let deadline_context = Arc::new(graphit_core::file_io::SnapshotContext::new(
            Instant::now() + Duration::from_millis(100),
            Cancellation::default(),
        ));
        let waiting_deadline = registry.capture(
            owner.auth.installation(),
            project,
            owner.native,
            &deadline_context,
            false,
        );
        tokio::pin!(waiting_deadline);
        assert!(futures_util::poll!(waiting_deadline.as_mut()).is_pending());
        assert_eq!(
            waiting_deadline
                .await
                .err()
                .expect("expired native close wait")
                .kind(),
            std::io::ErrorKind::TimedOut
        );
        let deadline_recheck = || deadline_context.check();
        assert_eq!(
            crate::foreground_ast_sources::progressive::ProgressivePublication::open(
                owner.auth.installation(),
                project,
                owner.native,
                Arc::clone(&deadline_context),
                Arc::clone(&registry),
                &deadline_recheck,
            )
            .await
            .err()
            .expect("expired preparation")
            .kind(),
            std::io::ErrorKind::TimedOut
        );
        drop(closing);
        let ((a, da), (b, db)) = tokio::time::timeout(Duration::from_secs(5), async {
            Ok::<_, std::io::Error>((first.await?, second.await?))
        })
        .await??;
        assert!(
            Arc::ptr_eq(&a.graph, &b.graph),
            "waiters share the published cold instance while its first reader remains alive"
        );
        assert!(Arc::ptr_eq(&a.fence, &b.fence));
        assert!(Arc::ptr_eq(&a.fence, &stable_fence));
        assert_eq!(
            a.graph
                .file_observation("idle.lua", original.snapshot())?
                .unwrap(),
            before_source
        );
        drop((a, da, b, db));

        // After complete native close, a new authorized operation may recapture
        // a different physical directory at the same logical PROJECT/version.
        // Active/closing graphs still reject replacement. Only this private
        // fixture is moved and restored; its original Source/NN remain intact.
        let retired = native_root.with_extension("retired");
        std::fs::rename(&native_root, &retired)?;
        drop(graphit_fs::create_private_directory(&native_root)?);
        let (fresh, fresh_directory) = registry
            .capture(
                owner.auth.installation(),
                project,
                owner.native,
                original.snapshot(),
                true,
            )
            .await?
            .ok_or("fresh authorized graph absent")?;
        assert!(fresh.graph.files(original.snapshot())?.is_empty());
        assert!(Arc::ptr_eq(&fresh.fence, &stable_fence));
        drop((fresh, fresh_directory));
        std::fs::remove_dir_all(&native_root)?;
        std::fs::rename(&retired, &native_root)?;
        let (restored, restored_directory) = open().await?;
        assert_eq!(
            restored
                .graph
                .file_observation("idle.lua", original.snapshot())?
                .unwrap(),
            before_source
        );
        drop((restored, restored_directory));
        ast.index_productive(
            &selected.0,
            &context,
            ChangeBatch {
                rescan: true,
                ..Default::default()
            },
            None,
            Some(BTreeSet::new()),
            false,
            &cancellation,
            &cancellation,
            &cancellation,
        )
        .await?;
        let (unchanged, unchanged_directory) = open().await?;
        assert_eq!(
            unchanged
                .graph
                .file_observation("idle.lua", original.snapshot())?
                .unwrap(),
            before_source,
            "a full unchanged pass must preserve the native FILE revision"
        );
        {
            let _fence = unchanged.write_fence(original.snapshot()).await?;
            unchanged.graph.remove_file("idle.lua", original.snapshot())?;
        }
        assert!(
            unchanged
                .graph
                .file_observation("idle.lua", original.snapshot())?
                .is_none()
        );
        drop((unchanged, unchanged_directory));
        ast.index_productive(
            &selected.0,
            &context,
            ChangeBatch {
                rescan: true,
                ..Default::default()
            },
            Some(Arc::clone(&resources)),
            Some(BTreeSet::new()),
            false,
            &cancellation,
            &cancellation,
            &cancellation,
        )
        .await?;
        let (repaired, repaired_directory) = open().await?;
        let repaired_source = repaired
            .graph
            .file_observation("idle.lua", original.snapshot())?
            .ok_or("unchanged Raw must repair a missing native FILE")?;
        assert_eq!(repaired_source.source_sha256, before_source.source_sha256);
        assert_eq!(repaired_source.raw_version, before_source.raw_version);
        assert!(repaired_source.file_revision > before_source.file_revision);
        let before_source = repaired_source;
        drop((repaired, repaired_directory));
        let NativeNnIndexPresence::Available(before) = open_local_native_nn_index(
            owner.auth.installation(),
            project,
            original.snapshot(),
            owner.native,
        )
        .await?
        else {
            return Err("NN absent".into());
        };
        let before_version = before.snapshot(original.snapshot())?.version();
        // Isolate this fixture's physical directory captures. Lance data files,
        // model mappings and unrelated process descriptors are not counted.
        let captured_directories = || {
            std::fs::read_dir("/proc/self/fd")
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| {
                    std::fs::read_link(entry.path()).is_ok_and(|path| {
                        (path.starts_with(selected.0.root())
                            || path.starts_with(source.policy().installation.root()))
                            && std::fs::metadata(entry.path())
                                .is_ok_and(|metadata| metadata.is_dir())
                    })
                })
                .count()
        };
        let idle_directories = captured_directories();
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let reconciled = std::cell::Cell::new(false);
        let mut consumer = Box::pin(ast.embed_pending_loop(
            &selected.0,
            &context,
            Some(&resources),
            receiver,
            false,
            BTreeSet::new(),
            &reconciled,
            &cancellation,
        ));
        tokio::time::timeout(Duration::from_secs(30), async {
            std::future::poll_fn(|cx| {
                assert!(
                    consumer.as_mut().poll(cx).is_pending(),
                    "the open command stream must remain live"
                );
                if reconciled.get() {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
        })
        .await?;
        assert_eq!(
            captured_directories(),
            idle_directories,
            "idle NN must release its PROJECT captures and admissions"
        );
        let (live, directory) = open().await?;
        assert_eq!(
            Arc::strong_count(&live.graph),
            1,
            "idle NN must not retain another database owner"
        );
        assert_eq!(
            live.graph
                .file_observation("idle.lua", original.snapshot())?
                .unwrap(),
            before_source
        );
        let idle_observer = Arc::clone(&live.graph);
        drop((live, directory));
        // Reconciliation describes the model, not a per-FILE acknowledgement.
        // Close this finite command stream after queuing its acknowledged FILE:
        // natural EOF proves that the receiver drained the event before the
        // same Source/vector owners are reopened and compared below.
        assert!(reconciled.get());
        sender.send("idle.lua".to_owned())?;
        drop(sender);
        tokio::time::timeout(Duration::from_secs(30), consumer)
            .await
            .map_err(|elapsed| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("acknowledged FILE command stream did not drain: {elapsed}"),
                )
            })??;
        assert_eq!(Arc::strong_count(&idle_observer), 1);
        assert_eq!(
            captured_directories(),
            idle_directories,
            "drained FILE commands must release their PROJECT captures and admissions"
        );
        drop(idle_observer);
        let (live, directory) = open().await?;
        assert_eq!(
            Arc::strong_count(&live.graph),
            1,
            "idle FILE must release its actual database owner"
        );
        assert_eq!(
            live.graph
                .file_observation("idle.lua", original.snapshot())?
                .unwrap(),
            before_source
        );
        let NativeNnIndexPresence::Available(after) = open_local_native_nn_index(
            owner.auth.installation(),
            project,
            original.snapshot(),
            owner.native,
        )
        .await?
        else {
            return Err("NN disappeared".into());
        };
        assert_eq!(
            after.snapshot(original.snapshot())?.version(),
            before_version,
            "idle/reopen must preserve the vector snapshot"
        );
        drop((live, directory));
        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .catch_unwind()
    .await;
    cancellation.cancel();
    let compiler_cleanup = ast.shutdown().await;
    tasks.shutdown().await?;
    owner.shutdown().await.map_err(|failure| failure.error)?;
    compiler_cleanup?;
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_versions_bootstrap_one_corpus_without_orphan_stages() -> TestResult {
    let (_fixture, owner, tasks, _, _) = support::browser_search_fixture().await;
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let cancellation = Cancellation::default();
    let original = owner
        .policy
        .request_context(Instant::now(), &cancellation)?;
    let capture = CapturedPublicationScope::project(
        &selected.0,
        selected.1,
        owner.scope,
        original.snapshot(),
    )?;
    let record = capture.binding()?;
    let checkout = graphit_core::git_checkout::GitCheckout::inspect(
        selected.0.root(),
        Duration::from_secs(5),
        &cancellation,
    )?;
    let feature = graphit_core::version_context::VersionContext::select(
        &selected.0,
        checkout.as_checkout(),
        "feature-corpus-birth",
        None,
        "",
    )?;
    let project = ProjectCapture {
        scope: ScopeCapture {
            live: &capture,
            record: &record,
        },
        version: &selected.2,
    };
    let feature_project = ProjectCapture {
        version: &feature,
        ..project
    };
    assert!(
        !graphit_ast::corpus::project_directory(owner.auth.installation(), project)?
            .join("corpus.lance")
            .try_exists()?
    );
    let registry = Arc::new(crate::foreground_ast_sources::progressive::LiveAstRegistry::default());
    *registry.1.lock().unwrap() = Some(Arc::new(tokio::sync::Barrier::new(2)));
    let recheck = || original.check();
    let open = |project| {
        crate::foreground_ast_sources::progressive::ProgressivePublication::open(
            owner.auth.installation(),
            project,
            owner.native,
            Arc::clone(original.snapshot()),
            Arc::clone(&registry),
            &recheck,
        )
    };
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let (main, feature) = tokio::join!(open(project), open(feature_project));
        let (main, feature) = (main?, feature?);
        let path = graphit_ast::corpus::project_directory(owner.auth.installation(), project)?;
        let main_pin = BranchSnapshot::capture_observed(
            path.join("corpus.lance").to_str().unwrap(),
            None,
            None,
        )
        .await?;
        graphit_ast::corpus::validate_schema(main_pin.schema())?;
        let branch = feature_project.version.lance_branch();
        let feature_pin = BranchSnapshot::capture_observed(
            path.join("corpus.lance").to_str().unwrap(),
            Some(&branch),
            None,
        )
        .await?;
        assert_eq!(feature_pin.physical_branch(), Some(branch.as_str()));
        assert!(feature_pin.inherited_target().is_none());
        assert_eq!(main_pin.scan().count_rows().await?, 0);
        assert_eq!(feature_pin.scan().count_rows().await?, 0);
        assert!(std::fs::read_dir(path)?.all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_str()
                .expect("fixture directory uses Unicode")
                .starts_with(".corpus-create-")
        }));
        drop((main, feature));
        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .await;
    *registry.1.lock().unwrap() = None;
    tasks.shutdown().await?;
    owner.shutdown().await.map_err(|failure| failure.error)?;
    result??;
    Ok(())
}
