//! Productive compiler through the actual daemon/Graph reader.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the configured Graphit SCIP Docker image; run explicitly in the native integration lane"]
async fn foreground_rust_scip_calls_and_physical_change_rejection()
-> Result<(), Box<dyn std::error::Error>> {
    use graphit_daemon::supervisor::ProjectContext;
    use sha2::{Digest, Sha256};
    let (_temp, installation, project, config, policy) = ast_fixture();
    std::fs::create_dir(project.root().join("src"))?;
    std::fs::write(
        project.root().join("Cargo.toml"),
        "[package]\nname=\"graphit-scip-daemon-fixture\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )?;
    std::fs::write(
        project.root().join("Cargo.lock"),
        "version=3\n[[package]]\nname=\"graphit-scip-daemon-fixture\"\nversion=\"0.1.0\"\n",
    )?;
    const SOURCE: &str = "pub fn target() -> u32 { 42 }\n";
    const CALLER_SOURCE: &str = "mod target;\npub fn caller() -> u32 { target::target() }\n";
    std::fs::write(project.root().join("src/lib.rs"), CALLER_SOURCE)?;
    std::fs::write(project.root().join("src/target.rs"), SOURCE)?;
    let cancellation = Cancellation::default();
    let startup = policy.request_context(Instant::now(), &cancellation)?;
    let mut owner = ForegroundSource::start_and_publish(
        &installation,
        &[ProjectBootstrapInput {
            project: &project,
            explicit_version: "",
            pin: None,
            detached_base: "",
        }],
        0,
        &config,
        NativeBinding::Codex,
        std::env::vars_os().collect(),
        &startup,
    )
    .await?;
    // Join the watcher before the explicit producer; no competing publication is hidden.
    if let Err(error) = owner.supervisors.shutdown().await {
        let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&error);
        let mut interrupted = false;
        while let Some(error) = cause {
            interrupted |= error
                .downcast_ref::<io::Error>()
                .is_some_and(|error| error.kind() == io::ErrorKind::Interrupted)
                || matches!(
                    error.downcast_ref::<graphit_auth::AuthError>(),
                    Some(graphit_auth::AuthError::Request)
                );
            cause = original_error_source(error);
        }
        assert!(interrupted, "real shutdown failure: {error}");
    }
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let context = ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    };
    let ast = Arc::clone(owner.ast_jobs.as_ref().unwrap());
    ast.index(
        &project,
        &context,
        graphit_daemon::watcher::ChangeBatch {
            rescan: true,
            ..Default::default()
        },
        &cancellation,
    )
    .await?;
    ast.index_semantic(
        project.clone(),
        context.clone(),
        None,
        None,
        std::collections::BTreeSet::new(),
        false,
        cancellation.clone(),
        cancellation.clone(),
    )
    .await?;
    {
        let source = owner.server.as_ref().unwrap().clone();
        let request = policy.request_context(Instant::now(), &cancellation)?;
        let runtime_key = owner.auth.runtime().check(request.snapshot())?;
        let credential = std::str::from_utf8(runtime_key.secret(request.snapshot())?)?;
        let live = source.live_projects();
        let base = ConfigSnapshot::new(installation.brand(), [])?;
        let runtime = tokio::runtime::Handle::current();
        let query = |cypher: &str| {
            tokio::task::block_in_place(|| {
                crate::project_graph::with_exported_graph(
                    &source.graph_server(),
                    crate::project_graph::ReadInput {
                        auth: &owner.auth,
                        policy: source.policy(),
                        base: &base,
                        live: &live,
                        selected: (&selected.0, selected.1, &selected.2),
                        credential,
                        local: false,
                        request: &request,
                    },
                    |graph, _| {
                        graphit_ast::icebug_query::execute(
                            graph,
                            request.snapshot(),
                            &runtime,
                            cypher,
                            &Default::default(),
                            source.policy().graph_query,
                        )
                    },
                )
            })
        };
        const CALLS: &str = "MATCH (a:Function)-[:CALLS]->(b:Function) RETURN a.entity_key, a.name, b.entity_key, b.name";
        let first = query(CALLS)??;
        let call = first
            .rows
            .iter()
            .find(|row| row[1]["value"] == "caller" && row[3]["value"] == "target")
            .unwrap_or_else(|| {
                panic!(
                    "actual Docker SCIP/Raw/TS/Graph cross-file CALLS: {:?}",
                    first.rows
                )
            });
        let caller_key = call[0]["value"].as_str().unwrap();
        let target_key = call[2]["value"].as_str().unwrap();
        assert!(!caller_key.is_empty());
        assert_ne!(caller_key, target_key);
        let files = query(
            "MATCH (f:AstFileObservation) WHERE f.is_file_observation = true RETURN f.path, f.raw_version, f.source_sha256, f.resolution_outcomes_json",
        )??;
        let caller_file = files
            .rows
            .iter()
            .find(|row| row[0]["value"] == "src/lib.rs")
            .unwrap_or_else(|| panic!("caller FILE observation missing: {:?}", files.rows));
        let raw_version: u64 = caller_file[1]["value"].as_str().unwrap().parse()?;
        assert_eq!(
            caller_file[2]["value"],
            Sha256::digest(CALLER_SOURCE.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let target_file = files
            .rows
            .iter()
            .find(|row| row[0]["value"] == "src/target.rs")
            .unwrap_or_else(|| panic!("target FILE observation missing: {:?}", files.rows));
        assert_eq!(target_file[1]["value"], caller_file[1]["value"]);
        assert_eq!(
            target_file[2]["value"],
            Sha256::digest(SOURCE.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let outcomes: serde_json::Value =
            serde_json::from_str(caller_file[3]["value"].as_str().unwrap())?;
        let resolved = outcomes
            .as_array()
            .unwrap()
            .iter()
            .find(|outcome| {
                outcome["target"]["state"] == "Resolved" && outcome["target"]["key"] == target_key
            })
            .unwrap_or_else(|| {
                panic!("FILE cross-file target must match the actual CALLS key: {outcomes}")
            });
        let endpoint = &resolved["observation"]["scip_graph_target"];
        assert_eq!(endpoint["key"], target_key);
        assert_eq!(endpoint["association"]["relative_path"], "src/target.rs");
        assert_eq!(endpoint["association"]["raw_version"], json!(raw_version));
        assert_eq!(
            endpoint["association"]["source_sha256"],
            json!(<[u8; 32]>::from(Sha256::digest(SOURCE.as_bytes())))
        );
        // Same captured Raw, changed physical compiler input: reject before replacing Graph.
        std::fs::write(
            project.root().join("src/target.rs"),
            SOURCE.replace("42", "43"),
        )?;
        let changed = ast
            .index_semantic(
                project.clone(),
                context,
                None,
                None,
                std::collections::BTreeSet::new(),
                false,
                cancellation.clone(),
                cancellation.clone(),
            )
            .await
            .unwrap_err();
        assert_eq!(changed.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            query(CALLS)??.rows,
            first.rows,
            "failed freshness cannot replace the published graph"
        );
    }
    owner.shutdown().await?;
    Ok(())
}
