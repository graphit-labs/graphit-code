//! Actual authorized foreground Oracle target-rule closure and incremental publication.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreground_oracle_target_rules_cross_file_and_ambiguous()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::foreground_ast_sources::{ForegroundAstSourceConfig, ForegroundAstSources};
    use graphit_daemon::supervisor::ProjectContext;
    let (_storage, owner, tasks, credential, _) =
        super::super::foreground_memory_jobs_tests::browser_search_fixture().await;
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let cancellation = Cancellation::default();
    let config = ConfigStore::project(&selected.0)?;
    config.set(
        ConfigKey::new("modules.ast")?,
        json!(true),
        Duration::from_secs(5),
        &cancellation,
    )?;
    config.set(
        ConfigKey::new("ast.grammar")?,
        json!(".pkb=antlr-plsql"),
        Duration::from_secs(5),
        &cancellation,
    )?;
    const TARGET: &str = "CREATE TABLE target_tab(id NUMBER);";
    const CALLER: &str =
        "CREATE PROCEDURE caller AS v NUMBER; BEGIN SELECT id INTO v FROM target_tab; END; /";
    std::fs::write(selected.0.root().join("target.pkb"), TARGET)?;
    std::fs::write(selected.0.root().join("caller.pkb"), CALLER)?;
    let source = owner.server.as_ref().unwrap().clone();
    let original = owner
        .policy
        .request_context(Instant::now(), &cancellation)?;
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
            environment: Vec::new(),
        },
        original.snapshot(),
    )?);
    let selected_context = ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    };
    let rescan = || graphit_daemon::watcher::ChangeBatch {
        rescan: true,
        ..Default::default()
    };
    let live = source.live_projects();
    let base = ConfigSnapshot::new(source.policy().installation.brand(), [])?;
    let runtime = tokio::runtime::Handle::current();
    let read_graph = |cycle: usize, ambiguous: bool| {
        tokio::task::block_in_place(|| {
            crate::project_graph::with_exported_graph(
                &source.graph_server(),
                crate::project_graph::ReadInput {
                    auth: &owner.auth,
                    policy: source.policy(),
                    base: &base,
                    live: &live,
                    selected: (&selected.0, selected.1, &selected.2),
                    credential: &credential,
                    local: false,
                    request: &original,
                },
                |graph, _| {
                    let schema = graph.schema().map_err(io::Error::other)?;
                    let selects = schema
                        .relationships()
                        .filter(|member| member.kind() == "SELECTS")
                        .map(|member| {
                            (
                                member.from_label().to_owned(),
                                member.to_label().to_owned(),
                                member.rows(),
                            )
                        })
                        .collect::<Vec<_>>();
                    let execute = |cypher| {
                        graphit_ast::icebug_query::execute(
                            graph,
                            original.snapshot(),
                            &runtime,
                            cypher,
                            &Default::default(),
                            source.policy().graph_query,
                        )
                    };
                    let files = execute(FILES)?;
                    eprintln!(
                        "Oracle publication cycle={cycle} ambiguous={ambiguous} generation={} SELECTS={selects:?} FILE={:?}",
                        schema.manifest_generation(),
                        files.rows,
                    );
                    let edges = if ambiguous {
                        assert!(
                            selects.is_empty(),
                            "two genuine tables cannot resolve by spelling; cycle={cycle}"
                        );
                        None
                    } else {
                        assert_eq!(
                            selects,
                            vec![("Procedure".to_owned(), "Table".to_owned(), 1)],
                            "unique publication must contain the actual SELECTS member; cycle={cycle}"
                        );
                        Some(execute(EDGES)?)
                    };
                    Ok::<_, io::Error>((edges, files))
                },
            )
        })
    };
    const EDGES: &str = "MATCH (a:Procedure)-[:SELECTS]->(t:`Table`) RETURN a.entity_key,a.path,t.entity_key,t.path";
    const FILES: &str = "MATCH (f:AstFileObservation) WHERE f.is_file_observation=true RETURN f.path,f.resolution_outcomes_json";
    for (cycle, ambiguous) in [false, true, false].into_iter().enumerate() {
        if ambiguous {
            std::fs::write(selected.0.root().join("duplicate.pkb"), TARGET)?;
        } else if selected.0.root().join("duplicate.pkb").exists() {
            std::fs::remove_file(selected.0.root().join("duplicate.pkb"))?;
        }
        assert!(
            ast.index(&selected.0, &selected_context, rescan(), &cancellation)
                .await?
                .is_some()
        );
        let progress = ast
            .index_semantic(
                selected.0.clone(),
                selected_context.clone(),
                None,
                None,
                std::collections::BTreeSet::new(),
                false,
                cancellation.clone(),
                cancellation.clone(),
            )
            .await?;
        assert!(
            progress.pending_files.is_empty(),
            "Graph-only publication finishes its source work; graph and Source are asserted below"
        );
        let (edges, files) = read_graph(cycle, ambiguous).map_err(io::Error::other)??;
        if !ambiguous {
            let edges = edges
                .as_ref()
                .expect("unique publication ran the actual SELECTS query");
            assert_eq!(
                edges.rows.len(),
                1,
                "duplicate stock selectors must not duplicate one native occurrence edge"
            );
            assert_eq!(edges.rows[0][1]["value"], "caller.pkb");
            assert_eq!(edges.rows[0][3]["value"], "target.pkb");
        }
        let caller = files
            .rows
            .iter()
            .find(|r| r[0]["value"] == "caller.pkb")
            .unwrap();
        let outcomes: serde_json::Value =
            serde_json::from_str(caller[1]["value"].as_str().unwrap())?;
        assert!(!outcomes.as_array().unwrap().is_empty());
        for outcome in outcomes.as_array().unwrap() {
            assert_eq!(
                outcome["target"]["state"],
                if ambiguous { "Ambiguous" } else { "Resolved" }
            );
            if !ambiguous {
                assert_eq!(
                    outcome["target"]["key"],
                    edges.as_ref().unwrap().rows[0][2]["value"]
                );
            }
        }
        let raw_uri = selected
            .0
            .project_runtime(selected.1, &["ast", "source.lance"])?;
        let branch = selected.2.lance_branch();
        assert_eq!(
            observed_branch_text(
                &raw_uri,
                (!branch.is_empty()).then_some(branch.as_str()),
                "caller.pkb",
                original.snapshot()
            )
            .await?
            .as_deref(),
            Some(CALLER)
        );
    }
    source.shutdown().await?;
    tasks.shutdown().await?;
    drop(ast);
    drop(source);
    owner.shutdown().await?;
    Ok(())
}
