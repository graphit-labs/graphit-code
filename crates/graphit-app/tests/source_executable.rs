#![cfg(target_os = "linux")]
//! Current compiled executable, private installation, real Git/Lance and MCP peers.
use arrow_array::{ArrayRef, RecordBatch, StringArray};
use graphit_app::foreground_source::ForegroundSource;
use graphit_core::{
    brand::Brand,
    config::{ConfigKey, ConfigSnapshot, ConfigStore},
    file_lock::Cancellation,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_daemon::project_bootstrap::ProjectBootstrapInput;
use graphit_hub::{
    NativeBinding,
    policy::{HubResourcePolicy, ResourceSetting},
};
use rmcp::{ServiceExt, model::CallToolRequestParams};
use serde_json::json;
use std::{
    fs, io,
    process::{Child, Command, Output, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

#[allow(dead_code)]
#[path = "support/native_install_auth.rs"]
mod memory_index_auth;

fn json_arguments(
    mut arguments: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    arguments.insert("ai_optimized".into(), serde_json::Value::Bool(false));
    arguments
}

struct Fixture {
    _temp: tempfile::TempDir,
    paths: InstallationPaths,
    projects: Vec<ProjectPaths>,
}
impl Fixture {
    async fn imported(&self, id: &str, text: &str) -> Result<u64> {
        use graphit_core::{
            installation_identity::InstallationIdentityStore, native_path::NativePath,
        };
        use graphit_hub::{
            ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
            registry::{
                HubRegistry, Installation, InstalledArtifact, InstalledBackend, InstalledPayload,
            },
        };
        let config = ConfigSnapshot::new(self.paths.brand(), [])?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let limits = policy.hub();
        let cancel = Cancellation::default();
        let original = policy.request_context(Instant::now(), &cancel)?;
        let identity = InstallationIdentityStore::new(&self.paths)?.load()?;
        let registry = HubRegistry::global(
            &self.paths,
            identity
                .unit_id()
                .ok_or("missing installation unit identity")?,
            limits,
        )?;
        let projection = registry.limits().projection();
        let producer = ProjectIdentityStore::at(&self.projects[0])
            .load()?
            .unwrap()
            .id();
        let source = SourceRef::local_link(
            NativePath::new(self._temp.path().join("imported").join(id))?,
            producer,
            ArtifactType::Ast,
            ArtifactId::new(id, projection)?,
            ConcreteVersion::new("release-1", projection)?,
            projection,
        )?;
        fs::create_dir_all(source.directory().as_path())?;
        use arrow_array::UInt64Array;
        use graphit_ast::entity_search;
        use graphit_data::{
            graph_search_sidecar::GraphSearchSpool,
            graph_writer::{GraphEdge, GraphNode, GraphValue, IcebugWriter},
        };
        use sha2::{Digest, Sha256};
        use std::collections::BTreeMap;
        let output = IcebugWriter::new(Arc::clone(original.snapshot()))?
            .push_file(
                [GraphNode {
                    label: "File".into(),
                    key: "same".into(),
                    properties: BTreeMap::from([
                        ("name".into(), GraphValue::String("same".into())),
                        ("path".into(), GraphValue::String("same".into())),
                        ("uid".into(), GraphValue::String("same".into())),
                        ("data_key".into(), GraphValue::String("raw".into())),
                        ("line_number".into(), GraphValue::Int64(1)),
                        ("end_line".into(), GraphValue::Int64(1)),
                    ]),
                }],
                std::iter::empty::<GraphEdge>(),
            )?
            .finish()?;
        let columns: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(vec!["same", "empty", "null"])),
            Arc::new(StringArray::from(vec![Some(text), Some(""), None])),
        ];
        let schema = graphit_ast::indexed_source::source_schema();
        let raw = graphit_data::lance_module_store::LanceModuleStore::create(
            output.directory().join("source.lance").to_str().unwrap(),
            Arc::clone(&schema),
            vec![RecordBatch::try_new(schema, columns)?],
        )
        .await?;
        let raw_version = raw.snapshot().version();
        drop(raw);
        let digest = Sha256::digest(text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let mut columns: Vec<ArrayRef> = [
            "same",
            "same",
            "raw",
            "same",
            "File",
            "same",
            "text",
            "fixture",
            digest.as_str(),
            "absent",
        ]
        .into_iter()
        .map(|value| Arc::new(StringArray::from(vec![value])) as ArrayRef)
        .collect();
        columns.push(Arc::new(StringArray::from(vec![Some(text)])));
        columns.push(Arc::new(UInt64Array::from(vec![raw_version])));
        columns.extend((0..4).map(|_| Arc::new(UInt64Array::from(vec![None::<u64>])) as ArrayRef));
        let mut search = GraphSearchSpool::new(
            graphit_ast::corpus::schema(),
            Arc::clone(original.snapshot()),
        )?;
        search.push(&graphit_ast::corpus::encode_file(
            graphit_ast::corpus::schema(),
            RecordBatch::try_new(entity_search::schema(), columns)?,
            RecordBatch::new_empty(graphit_ast::native_nn::native_nn_projection_schema()),
            None,
            original.snapshot(),
        )?)?;
        search.finish(&output, entity_search::TEXT_COLUMNS).await?;
        fs::rename(
            output.directory(),
            source.directory().as_path().join("graph.icebug"),
        )?;
        registry
            .writer(Duration::from_secs(5), &Cancellation::default())?
            .put_metadata(InstalledArtifact {
                consumer: graphit_core::artifact_store::PublicationSlot::Scope,
                artifact_type: ArtifactType::Ast,
                id: source.id().clone(),
                installations: vec![Installation {
                    target: InstallationTarget::GlobalNative,
                    original_source: source,
                    alias: None,
                    entry_name: None,
                    display: None,
                    backend: InstalledBackend::Native,
                    payload: InstalledPayload::Original {
                        selection_binding: None,
                    },
                }],
            })?;
        Ok(raw_version)
    }
    async fn new() -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
        let paths = InstallationPaths::resolve(
            brand.clone(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )?;
        let cancel = Cancellation::default();
        let mut projects = Vec::new();
        for name in ["current", "sibling"] {
            let root = temp.path().join(name);
            fs::create_dir(&root)?;
            let project = ProjectPaths::at(&root, &paths)?;
            ProjectIdentityStore::at(&project)
                .writer(Duration::from_secs(5), &cancel)?
                .ensure(&ProjectMetadata::new(name)?)?;
            projects.push(project);
        }
        for args in [
            vec!["init", "-b", "main"],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "initial",
            ],
            vec!["branch", "feature"],
        ] {
            let output = Command::new("git")
                .args(args)
                .current_dir(projects[0].root())
                .output()?;
            if !output.status.success() {
                return Err(io::Error::other(String::from_utf8_lossy(&output.stderr)).into());
            }
        }
        let config = ConfigSnapshot::new(&brand, [])?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let original = policy.request_context(Instant::now(), &cancel)?;
        let inputs = [
            (&projects[0], "main"),
            (&projects[0], "feature"),
            (&projects[1], ""),
        ]
        .map(|(project, explicit_version)| ProjectBootstrapInput {
            project,
            explicit_version,
            pin: None,
            detached_base: "",
        });
        let owner =
            ForegroundSource::start(&paths, &inputs, 0, &config, NativeBinding::Codex, &original)?;
        owner.shutdown().await?;
        ConfigStore::global(&paths)?.set(
            ConfigKey::new("agent")?,
            json!("codex"),
            Duration::from_secs(5),
            &cancel,
        )?;
        fs::write(
            projects[0].root().join("raw.unknown"),
            b"\xef\xbb\xbfalpha\r\nbeta\0\nlast",
        )?;
        fs::write(projects[0].root().join("empty"), b"")?;
        fs::write(
            projects[0].root().join("indexed_empty"),
            b"physical must not replace empty indexed text",
        )?;
        fs::write(
            projects[0].root().join("indexed_null"),
            b"physical null fallback",
        )?;
        fs::write(
            projects[0].root().join("selected"),
            b"physical wrong version",
        )?;
        fs::write(projects[1].root().join("sibling"), b"sibling raw text")?;
        let id = ProjectIdentityStore::at(&projects[0]).load()?.unwrap().id();
        let corpus = projects[0].project_runtime(id, &["ast", "source.lance"])?;
        // The productive Raw writer reopens this root with private_handle().
        // Match its staged 0700 creation rather than SDK default directory modes.
        for directory in [
            paths.root().join("projects"),
            projects[0].project_runtime(id, &[])?,
            projects[0].project_runtime(id, &["ast"])?,
            corpus.clone(),
        ] {
            if !directory.try_exists()? {
                drop(graphit_fs::create_private_directory(&directory)?);
            }
        }
        let schema = graphit_ast::indexed_source::source_schema();
        let batch = |text| {
            let columns: Vec<ArrayRef> = vec![
                Arc::new(StringArray::from(vec![
                    "selected",
                    "indexed_empty",
                    "indexed_null",
                ])),
                Arc::new(StringArray::from(vec![Some(text), Some(""), None])),
            ];
            RecordBatch::try_new(schema.clone(), columns)
        };
        let _main = graphit_data::lance_module_store::LanceModuleStore::create(
            corpus.to_str().unwrap(),
            schema.clone(),
            vec![batch("main indexed")?],
        )
        .await?;
        let store = graphit_data::lance_branch_store::LanceBranchStore::open(
            corpus.to_str().unwrap(),
            schema.clone(),
            None,
        )
        .await?;
        store
            .capture_target(Some("feature"))
            .await?
            .materialize()
            .await?
            .prepare_merge(vec![batch("feature indexed")?], vec!["path".into()])
            .await?
            .commit()
            .await?;
        Ok(Self {
            _temp: temp,
            paths,
            projects,
        })
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .current_dir(self.projects[0].root())
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self._temp.path())
            .env("GRAPHIT_MODULES__AST", "false");
        command.env(
            self.paths
                .brand()
                .environment_variable("GLOBAL_DIR")
                .unwrap(),
            self.paths.root(),
        );
        command
    }
    fn source(&self, args: &[&str]) -> Output {
        self.command()
            .args(["ast", "source"])
            .args(args)
            .output()
            .unwrap()
    }
    async fn daemon(&self, args: &[&str]) -> Result<Daemon> {
        let mut command = self.command();
        command.args(["daemon", "foreground"]).args(args);
        self.daemon_command(command).await
    }
    async fn daemon_command(&self, mut command: Command) -> Result<Daemon> {
        // This fixture changes agent/module flags, not the request budget. Resolve
        // the same distribution/global/PROJECT policy layers used by Main.
        let config = ConfigSnapshot::new(
            self.paths.brand(),
            [
                HubResourcePolicy::distribution_defaults(self.paths.brand())?,
                ConfigStore::global(&self.paths)?.load()?,
                ConfigStore::project(&self.projects[0])?.load()?,
            ],
        )?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let until = Instant::now()
            .checked_add(Duration::from_millis(
                policy.limit(ResourceSetting::RequestDeadlineMs).value,
            ))
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        let log = self._temp.path().join("daemon.log");
        let child = command
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log)?)
            .spawn()?;
        let mut daemon = Daemon { child, log };
        loop {
            if let Some(status) = daemon.child.try_wait()? {
                return Err(io::Error::other(format!(
                    "daemon {status}: {}",
                    fs::read_to_string(&daemon.log)?
                ))
                .into());
            }
            if self.paths.root().join("daemon/mcp.json").exists() {
                break;
            }
            if Instant::now() >= until {
                return Err(io::Error::other(format!(
                    "daemon discovery did not become ready; child status {:?}; stderr: {}",
                    daemon.child.try_wait()?,
                    fs::read_to_string(&daemon.log)?,
                ))
                .into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(daemon)
    }
    fn assert_source(&self, args: &[&str], text: &[u8]) {
        let output = self.source(args);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        assert_eq!(output.stdout, text);
    }
}
struct Daemon {
    child: Child,
    log: std::path::PathBuf,
}
impl Daemon {
    async fn stop(mut self, signal: &str) -> Result {
        signal_child(&self.child, signal)?;
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self.child.try_wait()? {
                if status.code() != Some(130) {
                    return Err(io::Error::other(format!(
                        "daemon exited with {status}, expected exit130 after {signal}: {}",
                        fs::read_to_string(&self.log)?
                    ))
                    .into());
                }
                break;
            }
            if Instant::now() >= until {
                return Err(io::Error::other("daemon did not join on signal").into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    }
}
fn signal_child(child: &Child, signal: &str) -> io::Result<()> {
    let status = Command::new("kill")
        .args([signal, &child.id().to_string()])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("owned child signal failed"))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_source_closed_stdout_keeps_service_usable() -> Result {
    let f = Fixture::new().await?;
    let daemon = f.daemon(&[]).await?;
    let key = fs::read(f.paths.root().join("daemon/mcp.key"))?;
    let mut command = tokio::process::Command::from(f.command());
    command
        .args(["ast", "source", "selected"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    drop(child.stdout.take());
    let output = tokio::time::timeout(Duration::from_secs(20), child.wait_with_output()).await??;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    assert_eq!(fs::read(f.paths.root().join("daemon/mcp.key"))?, key);
    f.assert_source(&["selected"], b"main indexed");
    daemon.stop("-TERM").await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_source_imported_context_preserves_namespace_without_checkout_fallback() -> Result
{
    let f = Fixture::new().await?;
    let source = |args: &[&str]| {
        f.command()
            .current_dir(f.paths.root())
            .args(["ast", "source"])
            .args(args)
            .output()
            .unwrap()
    };
    let assert_source = |args: &[&str], text: &[u8]| {
        let output = source(args);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        assert_eq!(output.stdout, text);
    };
    let a_source_version = f.imported("vendorA/context", "A raw").await?;
    let b_source_version = f.imported("vendorB/context", "B raw").await?;
    let daemon = f.daemon(&["--project-version", "main"]).await?;
    let imports = f
        .command()
        .current_dir(f.paths.root())
        .args(["ast", "imports"])
        .output()?;
    assert_eq!(
        imports.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&imports.stderr)
    );
    assert!(imports.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&imports.stdout)?,
        json!({"imports":[
            {"id":"vendorA/context","version":"release-1","context":"vendorA/context@release-1","backend":"native"},
            {"id":"vendorB/context","version":"release-1","context":"vendorB/context@release-1","backend":"native"}
        ]})
    );
    for (id, text, raw_version) in [
        ("vendorA/context", b"A raw".as_slice(), a_source_version),
        ("vendorB/context", b"B raw".as_slice(), b_source_version),
    ] {
        let context = format!("{id}@release-1");
        assert_source(&["same", "--context", &context], text);
        assert_source(&["empty", "--context", &context], b"");
        let search = f
            .command()
            .current_dir(f.paths.root())
            .args([
                "ast",
                "search",
                "raw",
                "--context",
                &context,
                "--mode",
                "fts",
                "--k",
                "8",
            ])
            .output()?;
        assert_eq!(
            search.status.code(),
            Some(0),
            "{context}: {}",
            String::from_utf8_lossy(&search.stderr)
        );
        assert!(search.stderr.is_empty());
        let search: serde_json::Value = serde_json::from_slice(&search.stdout)?;
        assert_eq!(search["status"], "available");
        assert_eq!(search["mode"], "fts");
        for field in [
            "search_generation",
            "graph_generation",
            "raw_generation",
            "source_original_generation",
            "nn_generation",
            "query_model_generation",
        ] {
            assert!(
                search.get(field).is_none(),
                "obsolete field {field}: {search}"
            );
        }
        let results = search["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["path"], "same");
        assert_eq!(results[0]["raw_version"], raw_version);
        assert!(results[0]["score"].as_f64().unwrap().is_finite());
        use sha2::{Digest, Sha256};
        assert_eq!(
            results[0]["source_sha256"],
            Sha256::digest(text)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
    }
    // Export through Main from the installation, then consume the physical import
    // through a new genuine LocalLink rather than rewriting the original artifact.
    let archive = f._temp.path().join("ast-roundtrip.zip");
    let exported = f
        .command()
        .current_dir(f.paths.root())
        .args([
            "ast",
            "export",
            "--context",
            "vendorA/context@release-1",
            "--output",
        ])
        .arg(&archive)
        .output()?;
    assert_eq!(
        exported.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    assert!(exported.stderr.is_empty());
    let exported: serde_json::Value = serde_json::from_slice(&exported.stdout)?;
    assert_eq!(exported["status"], "exported");
    assert_eq!(exported["artifact_type"], "ast");
    assert!(exported.get("graph_generation").is_none());
    let imported = f._temp.path().join("ast-roundtrip");
    let extracted = f
        .command()
        .current_dir(f.paths.root())
        .args(["package", "import", "--kind", "ast", "--input"])
        .arg(&archive)
        .arg("--output")
        .arg(&imported)
        .output()?;
    assert_eq!(
        extracted.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&extracted.stderr)
    );
    assert!(extracted.stderr.is_empty());
    assert_eq!(extracted.stdout, b"ast\n");
    assert!(!imported.join("graph.icebug/source_bindings.json").exists());
    {
        use graphit_core::{
            installation_identity::InstallationIdentityStore, native_path::NativePath,
        };
        use graphit_hub::{
            ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
            registry::{
                HubRegistry, Installation, InstalledArtifact, InstalledBackend, InstalledPayload,
            },
        };
        let config = ConfigSnapshot::new(f.paths.brand(), [])?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let identity = InstallationIdentityStore::new(&f.paths)?.load()?;
        let registry = HubRegistry::global(
            &f.paths,
            identity.unit_id().ok_or("missing installation unit")?,
            policy.hub(),
        )?;
        let projection = registry.limits().projection();
        let source = SourceRef::local_link(
            NativePath::new(imported)?,
            ProjectIdentityStore::at(&f.projects[0])
                .load()?
                .unwrap()
                .id(),
            ArtifactType::Ast,
            ArtifactId::new("vendor/roundtrip", projection)?,
            ConcreteVersion::new("release-1", projection)?,
            projection,
        )?;
        registry
            .writer(Duration::from_secs(5), &Cancellation::default())?
            .put_metadata(InstalledArtifact {
                consumer: graphit_core::artifact_store::PublicationSlot::Scope,
                artifact_type: ArtifactType::Ast,
                id: source.id().clone(),
                installations: vec![Installation {
                    target: InstallationTarget::GlobalNative,
                    original_source: source,
                    alias: None,
                    entry_name: None,
                    display: None,
                    backend: InstalledBackend::Native,
                    payload: InstalledPayload::Original {
                        selection_binding: None,
                    },
                }],
            })?;
    }
    assert_source(
        &["same", "--context", "vendor/roundtrip@release-1"],
        b"A raw",
    );
    assert_source(&["empty", "--context", "vendor/roundtrip@release-1"], b"");
    let search = f
        .command()
        .current_dir(f.paths.root())
        .args([
            "ast",
            "search",
            "raw",
            "--context",
            "vendor/roundtrip@release-1",
            "--mode",
            "fts",
            "--k",
            "8",
        ])
        .output()?;
    assert_eq!(
        search.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&search.stderr)
    );
    assert!(search.stderr.is_empty());
    let search: serde_json::Value = serde_json::from_slice(&search.stdout)?;
    assert_eq!(search["status"], "available");
    assert_eq!(search["mode"], "fts");
    for field in [
        "search_generation",
        "graph_generation",
        "raw_generation",
        "source_original_generation",
        "nn_generation",
        "query_model_generation",
    ] {
        assert!(
            search.get(field).is_none(),
            "obsolete field {field}: {search}"
        );
    }
    let hits = search["results"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["path"], "same");
    assert_eq!(hits[0]["uid"], "same");
    assert_eq!(hits[0]["data_key"], "raw");
    assert_eq!(hits[0]["raw_version"], a_source_version);
    assert!(hits[0]["score"].as_f64().unwrap().is_finite());
    let root = f.projects[0].root().to_str().unwrap();
    fs::write(
        f.projects[0].root().join("null"),
        "explicit producer physical",
    )?;
    assert_source(
        &[
            "null",
            "--context",
            "vendorA/context@release-1",
            "--project-dir",
            root,
            "--project-version",
            "feature",
        ],
        b"explicit producer physical",
    );
    let unavailable = source(&["null", "--context", "vendorA/context@release-1"]);
    assert_eq!(unavailable.status.code(), Some(1));
    assert!(unavailable.stdout.is_empty());
    let explicit = source(&[
        "same",
        "--context",
        "vendorA/context@release-1",
        "--project-dir",
        root,
        "--project-version",
        "missing",
    ]);
    assert_eq!(explicit.status.code(), Some(1));
    assert!(explicit.stdout.is_empty());
    // The Main CLI runs from the installation, without PROJECT discovery.
    // Imported Source also survives invalidation of the retained current PROJECT.
    fs::remove_dir_all(f.projects[0].brand_directory())?;
    assert_source(
        &["same", "--context", "vendorA/context@release-1"],
        b"A raw",
    );
    for context in ["vendorA/context@", "vendorA/context@wrong"] {
        let output = source(&["same", "--context", context]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
    }
    daemon.stop("-INT").await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_source_inflight_signal_keeps_service_usable() -> Result {
    use axum::{body::Body, extract::Request, response::Response};
    let f = Fixture::new().await?;
    let daemon = f.daemon(&[]).await?;
    let discovery_path = f.paths.root().join("daemon/mcp.json");
    let discovery = fs::read(&discovery_path)?;
    let mut record: serde_json::Value = serde_json::from_slice(&discovery)?;
    let upstream = record["address"].as_str().unwrap().to_owned();
    let key = fs::read(f.paths.root().join("daemon/mcp.key"))?;
    let ready = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    record["address"] = json!(listener.local_addr()?.to_string());
    // Forward real authenticated SDK traffic, holding only a completed tool response.
    let client = reqwest::Client::builder().no_proxy().build()?;
    let router = axum::Router::new().fallback({
        let ready = ready.clone();
        let release = release.clone();
        move |request: Request| {
            let client = client.clone();
            let upstream = upstream.clone();
            let ready = ready.clone();
            let release = release.clone();
            async move {
                let (parts, body) = request.into_parts();
                let body = axum::body::to_bytes(body, 2 * 1024 * 1024).await.unwrap();
                let tool_call = parts.uri.path() == "/mcp"
                    && serde_json::from_slice::<serde_json::Value>(&body).is_ok_and(|value| {
                        value["method"] == "tools/call"
                            && value["params"]["name"] == "graphit_ast_source"
                    });
                let path = parts.uri.path_and_query().map_or("/", |path| path.as_str());
                let response = client
                    .request(parts.method, format!("http://{upstream}{path}"))
                    .headers(parts.headers)
                    .body(body)
                    .send()
                    .await
                    .unwrap();
                if tool_call {
                    ready.notify_one();
                    release.notified().await;
                }
                let status = response.status();
                let headers = response.headers().clone();
                let mut result = Response::new(Body::from_stream(response.bytes_stream()));
                *result.status_mut() = status;
                *result.headers_mut() = headers;
                result
            }
        }
    });
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let proxy = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
    });
    fs::write(&discovery_path, serde_json::to_vec(&record)?)?;
    let mut command = tokio::process::Command::from(f.command());
    command
        .args(["ast", "source", "selected"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = command.spawn()?;
    let reached = tokio::time::timeout(Duration::from_secs(20), ready.notified()).await;
    let output = match reached {
        Ok(()) => {
            let status = Command::new("kill")
                .args(["-INT", &child.id().unwrap().to_string()])
                .status()?;
            assert!(status.success());
            tokio::time::timeout(Duration::from_secs(20), child.wait_with_output()).await
        }
        Err(error) => {
            drop(child);
            Err(error)
        }
    };
    release.notify_one();
    let _ = stop.send(());
    fs::write(&discovery_path, &discovery)?;
    tokio::time::timeout(Duration::from_secs(20), proxy).await???;
    let output = output??;
    assert_eq!(output.status.code(), Some(130));
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(f.paths.root().join("daemon/mcp.key"))?, key);
    f.assert_source(&["selected"], b"main indexed");
    daemon.stop("-TERM").await?;
    Ok(())
}
impl Drop for Daemon {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_setup_preserves_unified_identity_metadata_settings_and_git_hooks() -> Result {
    let f = Fixture::new().await?;
    let project = &f.projects[0];
    let cancel = Cancellation::default();
    let config = graphit_core::config::ConfigStore::project(project)?;
    let hooks = json!({"pre-commit":["git diff --cached --check"]});
    config.set(
        graphit_core::config::ConfigKey::new("githooks")?,
        hooks.clone(),
        Duration::from_secs(5),
        &cancel,
    )?;
    config.set(
        graphit_core::config::ConfigKey::new("custom.retained")?,
        json!("ação, λ"),
        Duration::from_secs(5),
        &cancel,
    )?;
    let store = ProjectIdentityStore::at(project);
    let identity = store.load()?.unwrap();
    let mut metadata = identity.metadata().clone();
    metadata.description = "preserved by setup".into();
    store
        .writer(Duration::from_secs(5), &cancel)?
        .update(identity.id(), &metadata)?;
    let daemon = f.daemon(&["--project-version", "main"]).await?;
    f.assert_source(&["selected"], b"main indexed");
    for name in ["replacement-one", "replacement-two"] {
        let output = f
            .command()
            .args(["setup", "--agent", "codex", "--name", name, "--project-dir"])
            .arg(project.root())
            .output()?;
        assert!(
            output.status.success(),
            "setup failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let document: serde_json::Value =
            serde_json::from_slice(&fs::read(project.identity_file())?)?;
        assert_eq!(document["id"], identity.id().to_string());
        assert_eq!(document["name"], metadata.name);
        assert_eq!(document["description"], metadata.description);
        assert_eq!(document["githooks"], hooks);
        assert_eq!(document["settings"]["custom.retained"], "ação, λ");
        assert!(document.get("metadata").is_none());
        assert!(!project.brand_directory().join("config.json").exists());
        f.assert_source(&["selected"], b"main indexed");
    }
    let registered = Command::new("git")
        .current_dir(project.root())
        .args([
            "config",
            "--local",
            "--get",
            &format!("hook.{}--pre-commit.command", f.paths.brand().key()),
        ])
        .output()?;
    assert!(registered.status.success());
    assert_eq!(
        String::from_utf8(registered.stdout)?.trim(),
        format!("{} git hook pre-commit --", f.paths.brand().binary_name())
    );
    daemon.stop("-TERM").await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_requested_read_version_does_not_register_a_maintained_version() -> Result {
    let f = Fixture::new().await?;
    let daemon = f.daemon(&["--project-version", "main"]).await?;
    f.assert_source(&["selected"], b"main indexed");
    let inventory = f.paths.root().join("project-inventory.json");
    let before = fs::read(&inventory)?;
    let key = fs::read(f.paths.root().join("daemon/mcp.key"))?;
    let missing = "unmaintained-read-version";

    let setup = f
        .command()
        .args([
            "--project-version",
            missing,
            "setup",
            "--agent",
            "codex",
            "--project-dir",
        ])
        .arg(f.projects[0].root())
        .output()?;
    assert_eq!(setup.status.code(), Some(1));
    assert!(setup.stdout.is_empty());
    assert!(String::from_utf8_lossy(&setup.stderr).contains("setup maintains the actual checkout"));
    assert_eq!(fs::read(&inventory)?, before);

    let source = f.source(&["selected", "--project-version", missing]);
    assert_eq!(source.status.code(), Some(1));
    assert!(source.stdout.is_empty());
    assert!(String::from_utf8_lossy(&source.stderr).contains("Source project/version unavailable"));
    assert_eq!(fs::read(&inventory)?, before);
    let schema = f
        .command()
        .args(["--project-version", missing, "ast", "schema"])
        .output()?;
    assert_eq!(schema.status.code(), Some(1));
    assert!(schema.stdout.is_empty());
    assert_eq!(fs::read(&inventory)?, before);

    let command = f.command();
    let mut command = tokio::process::Command::from(command);
    let mut child = command
        .args([
            "--project-version",
            missing,
            "mcp",
            "--stdio",
            "--non-interactive",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let sdk = ().serve((child.stdout.take().unwrap(), child.stdin.take().unwrap())).await?;
    for (arguments, expected) in [
        (
            json!({"path":"selected"}),
            Err("Source project/version unavailable"),
        ),
        (
            json!({"path":"selected","project_version":"main"}),
            Ok("main indexed"),
        ),
    ] {
        let result = sdk
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_ast_source")
                    .with_arguments(arguments.as_object().unwrap().clone()),
            )
            .await?;
        assert_eq!(result.is_error == Some(true), expected.is_err());
        assert_eq!(
            result.content[0].as_text().unwrap().text,
            expected.unwrap_or_else(|error| error)
        );
        assert_eq!(fs::read(&inventory)?, before);
    }
    sdk.cancel().await?;
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait()).await??;
    assert!(status.success());
    assert_eq!(fs::read(&inventory)?, before);
    assert_eq!(fs::read(f.paths.root().join("daemon/mcp.key"))?, key);
    f.assert_source(&["selected"], b"main indexed");
    daemon.stop("-TERM").await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_source_versions_raw_selectors_and_owned_signal_cleanup() -> Result {
    let f = Fixture::new().await?;
    let daemon = f
        .daemon(&["--project-version", "main", "--non-interactive"])
        .await?;
    let key = fs::read(f.paths.root().join("daemon/mcp.key"))?;
    f.assert_source(&["selected"], b"main indexed");
    f.assert_source(
        &["selected", "--project-version", "feature"],
        b"feature indexed",
    );
    f.assert_source(&["indexed_empty"], b"");
    f.assert_source(&["indexed_null"], b"physical null fallback");
    f.assert_source(&["empty"], b"");
    f.assert_source(&["raw.unknown"], b"\xef\xbb\xbfalpha\r\nbeta\0\nlast");
    f.assert_source(&["raw.unknown", "--head", "1"], b"\xef\xbb\xbfalpha\r");
    f.assert_source(&["raw.unknown", "--tail", "1"], b"last");
    f.assert_source(&["raw.unknown", "--start", "2", "--end", "2"], b"beta\0");
    f.assert_source(
        &[
            "raw.unknown",
            "--pattern",
            "^beta",
            "--regex",
            "--line-numbers",
        ],
        b">    2: beta\0",
    );
    let sibling = f.projects[1].root().to_str().unwrap();
    f.assert_source(&["sibling", "--project-dir", sibling], b"sibling raw text");
    let second = f
        .command()
        .args(["daemon", "foreground", "--project-version", "main"])
        .output()?;
    assert_eq!(second.status.code(), Some(1));
    assert_eq!(fs::read(f.paths.root().join("daemon/mcp.key"))?, key);
    daemon.stop("-INT").await?;
    assert!(!f.paths.root().join("daemon/mcp.json").exists());
    assert!(!f.paths.root().join("daemon/mcp.key").exists());
    f.daemon(&["--project-version", "feature"])
        .await?
        .stop("-TERM")
        .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_config_globals_and_discovery_failures_remain_causal() -> Result {
    let f = Fixture::new().await?;
    let cancel = Cancellation::default();
    ConfigStore::global(&f.paths)?.set(
        ConfigKey::new("agent")?,
        json!("invalid-global"),
        Duration::from_secs(5),
        &cancel,
    )?;
    ConfigStore::project(&f.projects[0])?.set(
        ConfigKey::new("agent")?,
        json!("codex"),
        Duration::from_secs(5),
        &cancel,
    )?;
    // Each higher layer is observed through the real executable's startup.
    f.daemon(&["--project-version", "main"])
        .await?
        .stop("-TERM")
        .await?;
    let agent_variable = f.paths.brand().environment_variable("AGENT")?;
    let denied = f
        .command()
        .env(&agent_variable, "invalid-environment")
        .args(["daemon", "foreground", "--project-version", "main"])
        .output()?;
    assert_eq!(denied.status.code(), Some(1));
    assert!(denied.stdout.is_empty());
    ConfigStore::project(&f.projects[0])?.set(
        ConfigKey::new("agent")?,
        json!("invalid-project"),
        Duration::from_secs(5),
        &cancel,
    )?;
    let mut environment_command = f.command();
    environment_command.env(&agent_variable, "codex").args([
        "daemon",
        "foreground",
        "--project-version",
        "main",
    ]);
    f.daemon_command(environment_command)
        .await?
        .stop("-TERM")
        .await?;
    let mut inline_command = f.command();
    inline_command
        .env(&agent_variable, "invalid-environment")
        .args([
            "daemon",
            "foreground",
            "-cagent=invalid-command",
            "-cagent=codex",
            "--project-version",
            "main",
        ]);
    let daemon = f.daemon_command(inline_command).await?;
    f.assert_source(
        &[
            "selected",
            "--global-dir",
            f.paths.root().to_str().unwrap(),
            "--non-interactive",
        ],
        b"main indexed",
    );
    let output = f
        .command()
        .env(
            f.paths.brand().environment_variable("GLOBAL_DIR")?,
            f._temp.path().join("unused-installation"),
        )
        .args([
            "--global-dir",
            f.paths.root().to_str().unwrap(),
            "ast",
            "source",
            "selected",
        ])
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"main indexed");
    let subdir = f.projects[0].root().join("subdir");
    fs::create_dir(&subdir)?;
    let output = f
        .command()
        .current_dir(&subdir)
        .args(["ast", "source", "selected"])
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"main indexed");
    let output = f
        .command()
        .env(
            f.paths.brand().environment_variable("PROJECT_VERSION")?,
            "feature",
        )
        .args(["ast", "source", "selected"])
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"feature indexed");
    let output = f
        .command()
        .env(
            f.paths.brand().environment_variable("PROJECT_VERSION")?,
            "feature",
        )
        .args(["--project-version", "main", "ast", "source", "selected"])
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"main indexed");
    for args in [
        vec!["-c", "missing", "ast", "source", "selected"],
        vec!["ast", "source", "selected", "-c", "agent=null"],
        vec!["--version", "ast", "source", "selected"],
    ] {
        let output = f.command().args(args).output()?;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    let record = f.paths.root().join("daemon/mcp.json");
    daemon.stop("-TERM").await?;
    // Ordinary CLI startup is exercised by daemon_autostart; malformed
    // discovery remains an integrity failure rather than a startup signal.
    fs::write(&record, b"{ malformed")?;
    let malformed = f.source(&["selected"]);
    assert_eq!(malformed.status.code(), Some(1));
    assert!(malformed.stdout.is_empty());
    fs::remove_file(record)?;
    let invalid_type = f
        .command()
        .args([
            "daemon",
            "foreground",
            "-cagent=codex",
            "-cmcp.port=invalid",
        ])
        .output()?;
    assert_eq!(invalid_type.status.code(), Some(1));
    assert!(invalid_type.stdout.is_empty());
    ConfigStore::global(&f.paths)?.unset(
        ConfigKey::new("agent")?,
        Duration::from_secs(5),
        &cancel,
    )?;
    ConfigStore::project(&f.projects[0])?.unset(
        ConfigKey::new("agent")?,
        Duration::from_secs(5),
        &cancel,
    )?;
    let missing_agent = f.command().args(["daemon", "foreground"]).output()?;
    assert_eq!(missing_agent.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing_agent.stderr).contains("requires configured agent"));
    assert!(!f.paths.root().join("daemon/mcp.key").exists());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_mcp_stdio_current_explicit_sibling_and_eof_leave_daemon_alive() -> Result {
    let f = Fixture::new().await?;
    let daemon = f.daemon(&["--project-version", "main"]).await?;
    let key = fs::read(f.paths.root().join("daemon/mcp.key"))?;
    for (version, selected_text) in [(Some("feature"), "feature indexed"), (None, "main indexed")] {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .env_clear()
            .env("HOME", f._temp.path())
            .env(
                f.paths.brand().environment_variable("GLOBAL_DIR")?,
                f.paths.root(),
            )
            .current_dir(f.projects[0].root())
            .args(
                version
                    .into_iter()
                    .flat_map(|version| ["--project-version", version]),
            )
            .args(["mcp", "--stdio", "--non-interactive"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn()?;
        let sdk = match ().serve((child.stdout.take().unwrap(), child.stdin.take().unwrap())).await
        {
            Ok(sdk) => sdk,
            Err(error) => {
                let output = child.wait_with_output().await?;
                return Err(io::Error::other(format!(
                    "{error}: {}",
                    String::from_utf8_lossy(&output.stderr)
                ))
                .into());
            }
        };
        let tools = sdk.peer().list_tools(Default::default()).await?;
        assert!(
            tools
                .tools
                .iter()
                .any(|tool| tool.name == "graphit_ast_source")
        );
        assert!(
            tools
                .tools
                .iter()
                .any(|tool| tool.name == "graphit_hub_publish_native")
        );
        assert!(
            tools
                .tools
                .iter()
                .any(|tool| tool.name == "graphit_hub_install_native")
        );
        let producer = ProjectIdentityStore::at(&f.projects[0])
            .load()?
            .ok_or("missing project identity")?
            .id();
        let denied_install =
            sdk.peer()
                .call_tool(
                    CallToolRequestParams::new("graphit_hub_install_native")
                        .with_arguments(json_arguments(
                        json!({"installation":{
                            "provider":"unconfigured-publication-provider","producer":producer,
                            "artifact_type":"ast","id":"namespace/native","version":"release/one"
                        }})
                        .as_object()
                        .unwrap()
                        .clone(),
                    )),
                )
                .await?;
        assert_eq!(denied_install.is_error, Some(true));
        assert_eq!(
            denied_install.structured_content.as_ref().unwrap()["code"],
            "authentication_or_permission_denied"
        );
        let denied=sdk.peer().call_tool(CallToolRequestParams::new("graphit_hub_publish_native").with_arguments(json_arguments(json!({
            "project_dir":graphit_core::native_path::NativePath::new(f.projects[0].root().to_owned())?,
            "project_version":version.unwrap_or("main"),
            "publication":{"provider":"unconfigured-publication-provider","artifact_type":"ast","id":"namespace/native","version":"release/one","name":"Native","description":null,"author":null,"tags":[]}
        }).as_object().unwrap().clone()))).await?;
        assert_eq!(denied.is_error, Some(true));
        assert_eq!(
            denied.structured_content.as_ref().unwrap()["code"],
            "authentication_or_permission_denied"
        );
        for (args, text) in [
            (json!({"path":"selected"}), selected_text),
            (
                json!({"path":"selected","project_version":"main"}),
                "main indexed",
            ),
            (
                json!({"path":"sibling","project_dir":f.projects[1].root()}),
                "sibling raw text",
            ),
        ] {
            let result = sdk
                .peer()
                .call_tool(
                    CallToolRequestParams::new("graphit_ast_source")
                        .with_arguments(args.as_object().unwrap().clone()),
                )
                .await?;
            assert_ne!(result.is_error, Some(true), "{result:?}");
            let actual: String = result
                .content
                .iter()
                .filter_map(|c| c.as_text())
                .map(|t| t.text.as_str())
                .collect();
            assert_eq!(actual, text);
        }
        sdk.cancel().await?;
        let status = tokio::time::timeout(Duration::from_secs(10), child.wait()).await??;
        assert!(status.success());
    }
    assert_eq!(fs::read(f.paths.root().join("daemon/mcp.key"))?, key);
    f.assert_source(&["selected"], b"main indexed");
    daemon.stop("-TERM").await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_private_credentials_and_late_startup_failure_cleanup() -> Result {
    let primary = Fixture::new().await?;
    let daemon = primary.daemon(&["--project-version", "main"]).await?;
    let record = fs::read(primary.paths.root().join("daemon/mcp.json"))?;
    let key = fs::read(primary.paths.root().join("daemon/mcp.key"))?;
    let other = Fixture::new().await?;
    let other_record = other.paths.root().join("daemon/mcp.json");
    // Metadata can select an endpoint; it does not convey the endpoint's private credential.
    fs::write(&other_record, &record)?;
    let denied = other.source(&[
        "selected",
        "--project-dir",
        primary.projects[0].root().to_str().unwrap(),
    ]);
    assert_eq!(denied.status.code(), Some(1));
    assert!(denied.stdout.is_empty());
    fs::remove_file(&other_record)?;
    let other_daemon = other.daemon(&["--project-version", "main"]).await?;
    let own_record = fs::read(&other_record)?;
    let other_key = fs::read(other.paths.root().join("daemon/mcp.key"))?;
    assert_ne!(key, other_key);
    fs::write(&other_record, &record)?;
    let denied = other.source(&[
        "selected",
        "--project-dir",
        primary.projects[0].root().to_str().unwrap(),
    ]);
    assert_eq!(denied.status.code(), Some(1));
    assert!(denied.stdout.is_empty());
    assert_eq!(fs::read(primary.paths.root().join("daemon/mcp.key"))?, key);
    assert_eq!(
        fs::read(other.paths.root().join("daemon/mcp.key"))?,
        other_key
    );
    // Repair only this fixture's discovery data; no key or authority is overwritten/restored.
    fs::write(&other_record, own_record)?;
    other_daemon.stop("-TERM").await?;
    daemon.stop("-TERM").await?;
    // Occupy a real socket; bind failure occurs after key startup and must clean the owner.
    let occupied = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = format!("mcp.port={}", occupied.local_addr()?.port());
    let failure = primary
        .command()
        .args([
            "daemon",
            "foreground",
            "--project-version",
            "main",
            "-c",
            &port,
        ])
        .output()?;
    assert_eq!(failure.status.code(), Some(1));
    assert!(failure.stdout.is_empty());
    assert!(!primary.paths.root().join("daemon/mcp.json").exists());
    assert!(!primary.paths.root().join("daemon/mcp.key").exists());
    drop(occupied);
    primary.daemon(&[]).await?.stop("-INT").await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_knowledge_watcher_export_import_and_installed_mcp_query_without_model() -> Result
{
    use graphit_core::{installation_identity::InstallationIdentityStore, native_path::NativePath};
    use graphit_hub::{
        ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
        registry::{
            HubRegistry, Installation, InstalledArtifact, InstalledBackend, InstalledPayload,
        },
    };
    let f = Fixture::new().await?;
    let cancel = Cancellation::default();
    let store = ConfigStore::global(&f.paths)?;
    for (key, enabled) in [
        ("modules.ast", false),
        ("modules.memory", false),
        ("modules.knowledge", true),
        ("modules.embedding", true),
        ("modules.dream", false),
    ] {
        store.set(
            ConfigKey::new(key)?,
            json!(enabled),
            Duration::from_secs(5),
            &cancel,
        )?;
    }
    fs::create_dir(f.projects[0].root().join("docs"))?;
    fs::write(
        f.projects[0].root().join("docs/a.md"),
        "---\nid: exported-note\ntitle: Native export\n---\n# Native export\nexportableword literal body.\n",
    )?;
    fs::write(
        f.projects[0].root().join("docs/b.md"),
        "---\nid: exported-second\ntitle: Second export\n---\n# Second export\nliteral second document.\n",
    )?;
    let project_id = ProjectIdentityStore::at(&f.projects[0])
        .load()?
        .ok_or("missing Knowledge fixture project identity")?
        .id();
    let mut daemon = f.daemon(&["--project-version", "main"]).await?;
    // Discovery only means MCP can accept connections. Wait for the watcher
    // to finish the document pass so FTS and export observe the same corpus.
    let publication =
        format!("Graphit Knowledge job for {project_id} version main: document generation ");
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let log = fs::read_to_string(&daemon.log)?;
        if let Some(status) = daemon.child.try_wait()? {
            return Err(
                io::Error::other(format!("Knowledge daemon exited with {status}: {log}")).into(),
            );
        }
        if log.lines().any(|line| line.starts_with(&publication)) {
            break;
        }
        if Instant::now() >= until {
            return Err(io::Error::other(format!(
                "Knowledge watcher did not finish its initial publication: {log}"
            ))
            .into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut command = tokio::process::Command::from(f.command());
    command
        .current_dir(f.paths.root())
        .args(["mcp", "--stdio", "--non-interactive"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let sdk = match ().serve((child.stdout.take().unwrap(), child.stdin.take().unwrap())).await {
        Ok(sdk) => sdk,
        Err(error) => {
            let output = child.wait_with_output().await?;
            return Err(io::Error::other(format!(
                "{error}: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
            .into());
        }
    };
    let project_query = json!({
        "search":{"query":"literal","mode":"fts","preview":true},
        "project_dir":NativePath::new(f.projects[0].root().to_owned())?,
        "project_version":"main"
    });
    let response = sdk
        .peer()
        .call_tool(
            CallToolRequestParams::new("graphit_knowledge_search")
                .with_arguments(json_arguments(project_query.as_object().unwrap().clone())),
        )
        .await?;
    assert_ne!(
        response.is_error,
        Some(true),
        "{response:?}: {}",
        fs::read_to_string(&daemon.log)?
    );
    let page = response
        .structured_content
        .ok_or("missing Knowledge search result")?;
    assert_eq!(page["native"]["state"], "hits", "{page}");
    assert_eq!(page["source"], json!({"kind":"project_knowledge"}));
    assert_eq!(page["native"]["total"], 2);
    let project_hit = page["native"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|hit| {
            hit["preview"]["text"]
                .as_str()
                .unwrap()
                .contains("exportableword")
        })
        .ok_or("missing original exported document")?;
    assert!(
        project_hit["preview"]["text"]
            .as_str()
            .unwrap()
            .contains("exportableword")
    );
    assert!(project_hit["lexical_score"].as_f64().unwrap().is_finite());
    let archive = f._temp.path().join("knowledge.zip");
    let exported = f
        .command()
        .current_dir(f.paths.root())
        .args([
            "--project-version",
            "main",
            "knowledge",
            "export",
            "--project-dir",
        ])
        .arg(f.projects[0].root())
        .arg("--output")
        .arg(&archive)
        .output()?;
    assert_eq!(
        exported.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    assert!(exported.stderr.is_empty());
    let exported: serde_json::Value = serde_json::from_slice(&exported.stdout)?;
    assert_eq!(exported["status"], "exported");
    let generation = exported["generation"]["version"]
        .as_u64()
        .ok_or("native corpus version")?;
    assert_eq!(page["native"]["generation"]["version"], generation);
    // No Models service was started. The actual producer exports its lexical variant.
    assert!(exported["generation"].get("corpus_uri").is_none());
    // Exercise the same captured PROJECT/version through Main's Markdown selector.
    // The real corpus has no cross-document references; SDK fixtures cover those.
    let markdown = f._temp.path().join("knowledge-markdown");
    assert!(!markdown.starts_with(f.projects[0].root()));
    assert!(!markdown.starts_with(f.paths.root()));
    let output = f
        .command()
        .current_dir(f.paths.root())
        .args([
            "--project-version",
            "main",
            "knowledge",
            "export",
            "--format",
            "markdown",
            "--project-dir",
        ])
        .arg(f.projects[0].root())
        .arg("--output")
        .arg(&markdown)
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(result["status"], "exported");
    assert_eq!(result["format"], "markdown");
    assert_eq!(result["output"], json!(NativePath::new(markdown.clone())?));
    assert_eq!(result["generation"], page["native"]["generation"]);
    assert_eq!(result["cleanup"]["status"], "complete");
    assert_eq!(result["communities"]["groups"], json!([]));
    let fingerprint = result["fingerprint"].as_str().unwrap();
    assert_eq!(fingerprint.len(), 64);
    assert!(
        fingerprint
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    );
    let index = fs::read_to_string(markdown.join("index.md"))?;
    assert!(
        index.starts_with("# Knowledge index\n\n2 pages\n"),
        "{index}"
    );
    assert_eq!(fs::read_dir(markdown.join("pages"))?.count(), 2);
    for hit in page["native"]["hits"].as_array().unwrap() {
        use sha2::{Digest, Sha256};
        let slug = hit["focused_source"]["slug"].as_str().unwrap();
        let digest = Sha256::digest(slug.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let name = format!("pages/p-{digest}.md");
        let text = fs::read_to_string(markdown.join(&name))?;
        let (title, body) = if hit["preview"]["text"]
            .as_str()
            .unwrap()
            .contains("exportableword")
        {
            (
                "Native export",
                "# Native export\nexportableword literal body.\n",
            )
        } else {
            (
                "Second export",
                "# Second export\nliteral second document.\n",
            )
        };
        assert!(index.contains(&format!("[{title}]({name})")), "{index}");
        let (header, content) = text
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n")
            .unwrap();
        let fields: Vec<_> = header
            .lines()
            .filter(|line| !line.starts_with(char::is_whitespace))
            .map(|line| line.split_once(':').unwrap().0)
            .collect();
        assert_eq!(
            fields,
            [
                "id",
                "title",
                "type",
                "summary",
                "description",
                "summary_truncated",
                "source_path",
                "physical_digest",
                "compilation_digest",
                "recipe",
                "recipe_parameters",
                "updated_at",
                "stale_since",
                "stale_after_ns",
                "resolved_wiki_targets",
            ]
        );
        assert!(
            header.starts_with(&format!("id: '{slug}'\ntitle: '{title}'\n")),
            "{header}"
        );
        assert!(content.starts_with(&format!("\n# {title}\n")), "{content}");
        assert!(content.contains("## Content\n"), "{content}");
        assert!(content.ends_with(body), "{content}");
    }
    for (version, format, code) in [("main", "invalid", 2), ("missing", "markdown", 1)] {
        let destination = f._temp.path().join(format!("rejected-{version}-{format}"));
        let output = f
            .command()
            .current_dir(f.paths.root())
            .args([
                "--project-version",
                version,
                "knowledge",
                "export",
                "--format",
                format,
                "--project-dir",
            ])
            .arg(f.projects[0].root())
            .arg("--output")
            .arg(&destination)
            .output()?;
        assert_eq!(output.status.code(), Some(code));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
        assert!(!destination.exists());
    }
    let imported = f._temp.path().join("installed-knowledge");
    let extracted = f
        .command()
        .current_dir(f.paths.root())
        .args(["package", "import", "--kind", "knowledge", "--input"])
        .arg(&archive)
        .arg("--output")
        .arg(&imported)
        .output()?;
    assert_eq!(
        extracted.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&extracted.stderr)
    );
    assert!(extracted.stderr.is_empty());
    assert_eq!(extracted.stdout, b"knowledge\n");
    assert!(imported.join("index.lance").is_dir());
    let cloned_version = knowledge_dataset_version(&imported).await?;
    assert_eq!(fs::read_dir(&imported)?.count(), 1);

    let config = ConfigSnapshot::new(f.paths.brand(), [])?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let identity = InstallationIdentityStore::new(&f.paths)?.load()?;
    let registry = HubRegistry::global(
        &f.paths,
        identity.unit_id().ok_or("missing installation unit")?,
        policy.hub(),
    )?;
    let projection = registry.limits().projection();
    let producer = ProjectIdentityStore::at(&f.projects[0])
        .load()?
        .unwrap()
        .id();
    let source = SourceRef::local_link(
        NativePath::new(imported)?,
        producer,
        ArtifactType::Knowledge,
        ArtifactId::new("vendor/knowledge", projection)?,
        ConcreteVersion::new("release-1", projection)?,
        projection,
    )?;
    registry
        .writer(Duration::from_secs(5), &cancel)?
        .put_metadata(InstalledArtifact {
            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
            artifact_type: ArtifactType::Knowledge,
            id: source.id().clone(),
            installations: vec![Installation {
                target: InstallationTarget::GlobalNative,
                original_source: source,
                alias: None,
                entry_name: None,
                display: None,
                backend: InstalledBackend::Native,
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        })?;
    let imported_source = json!({"kind":"imported_knowledge","artifact":{"id":"vendor/knowledge","version":"release-1"}});
    let response = sdk.peer().call_tool(CallToolRequestParams::new("graphit_knowledge_search")
        .with_arguments(json_arguments(json!({"search":{"query":"exportableword","mode":"fts","preview":true},"source":imported_source})
            .as_object().unwrap().clone()))).await?;
    assert_ne!(response.is_error, Some(true), "{response:?}");
    let payload = response
        .structured_content
        .ok_or("missing installed Knowledge result")?;
    assert_eq!(payload["source"], imported_source);
    assert_eq!(payload["native"]["state"], "hits");
    assert_eq!(payload["native"]["total"], 1);
    assert_eq!(payload["native"]["generation"]["version"], cloned_version);
    let imported_hit = &payload["native"]["hits"][0];
    assert_eq!(
        imported_hit["focused_source"]["slug"],
        project_hit["focused_source"]["slug"]
    );
    assert_eq!(
        imported_hit["preview"]["text"],
        project_hit["preview"]["text"]
    );
    assert!(imported_hit["lexical_score"].as_f64().unwrap().is_finite());
    // Reexport the installed documentary/vector corpus, then query a fresh physical installation.
    let reexport_archive = f._temp.path().join("knowledge-reexport.zip");
    let output = f
        .command()
        .current_dir(f.paths.root())
        .args([
            "knowledge",
            "export",
            "--artifact-id",
            "vendor/knowledge",
            "--artifact-version",
            "release-1",
            "--context-alias",
            "installed-wiki",
            "--output",
        ])
        .arg(&reexport_archive)
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let reexported: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(reexported["status"], "exported");
    assert_eq!(reexported["generation"]["version"], cloned_version);
    let roundtrip = f._temp.path().join("reexported-knowledge");
    let output = f
        .command()
        .current_dir(f.paths.root())
        .args(["package", "import", "--kind", "knowledge", "--input"])
        .arg(&reexport_archive)
        .arg("--output")
        .arg(&roundtrip)
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout, b"knowledge\n");
    assert_eq!(knowledge_dataset_version(&roundtrip).await?, cloned_version);
    assert_eq!(fs::read_dir(&roundtrip)?.count(), 1);
    let source = SourceRef::local_link(
        NativePath::new(roundtrip.clone())?,
        ProjectIdentityStore::at(&f.projects[0])
            .load()?
            .unwrap()
            .id(),
        ArtifactType::Knowledge,
        ArtifactId::new("vendor/knowledge-roundtrip", projection)?,
        ConcreteVersion::new("release-2", projection)?,
        projection,
    )?;
    registry
        .writer(Duration::from_secs(5), &cancel)?
        .put_metadata(InstalledArtifact {
            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
            artifact_type: ArtifactType::Knowledge,
            id: source.id().clone(),
            installations: vec![Installation {
                target: InstallationTarget::GlobalNative,
                original_source: source,
                alias: None,
                entry_name: None,
                display: None,
                backend: InstalledBackend::Native,
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        })?;
    let roundtrip_source = json!({"kind":"imported_knowledge","artifact":{"id":"vendor/knowledge-roundtrip","version":"release-2","context_alias":"roundtrip-wiki"}});
    let response = sdk.peer().call_tool(CallToolRequestParams::new("graphit_knowledge_search")
        .with_arguments(json_arguments(json!({"search":{"query":"literal","mode":"fts","preview":true},"source":roundtrip_source})
            .as_object().unwrap().clone()))).await?;
    assert_ne!(response.is_error, Some(true), "{response:?}");
    let roundtrip_page = response
        .structured_content
        .ok_or("missing reexported Knowledge result")?;
    assert_eq!(roundtrip_page["source"], roundtrip_source);
    assert_eq!(roundtrip_page["native"]["state"], "hits");
    assert_eq!(roundtrip_page["native"]["total"], 2);
    assert_eq!(
        roundtrip_page["native"]["generation"]["version"].as_u64(),
        Some(cloned_version)
    );
    for hit in roundtrip_page["native"]["hits"].as_array().unwrap() {
        let original = page["native"]["hits"]
            .as_array()
            .unwrap()
            .iter()
            .find(|original| original["focused_source"]["slug"] == hit["focused_source"]["slug"])
            .ok_or("unexpected document after installed native reexport")?;
        assert_eq!(hit["preview"]["text"], original["preview"]["text"]);
        assert!(hit["lexical_score"].as_f64().unwrap().is_finite());
    }
    let imported_markdown = f._temp.path().join("installed-knowledge-markdown");
    let output = f
        .command()
        .current_dir(f.paths.root())
        .args([
            "knowledge",
            "export",
            "--format",
            "markdown",
            "--artifact-id",
            "vendor/knowledge-roundtrip",
            "--artifact-version",
            "release-2",
            "--context-alias",
            "roundtrip-wiki",
            "--output",
        ])
        .arg(&imported_markdown)
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let imported_result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(imported_result["status"], "exported");
    assert_eq!(imported_result["format"], "markdown");
    assert_eq!(
        imported_result["output"],
        json!(NativePath::new(imported_markdown.clone())?)
    );
    assert_eq!(
        imported_result["generation"],
        roundtrip_page["native"]["generation"]
    );
    assert_eq!(imported_result["fingerprint"], result["fingerprint"]);
    assert_eq!(imported_result["communities"], result["communities"]);
    assert_eq!(imported_result["cleanup"]["status"], "complete");
    // The PROJECT pages above already prove all fifteen native fields and exact authored suffixes.
    // Immutable installed export must preserve every byte, including those fields and bodies.
    assert_eq!(
        fs::read(imported_markdown.join("index.md"))?,
        fs::read(markdown.join("index.md"))?
    );
    assert_eq!(fs::read_dir(imported_markdown.join("pages"))?.count(), 2);
    for entry in fs::read_dir(markdown.join("pages"))? {
        let entry = entry?;
        assert_eq!(
            fs::read(imported_markdown.join("pages").join(entry.file_name()))?,
            fs::read(entry.path())?
        );
    }
    let missing_export = f._temp.path().join("missing-installed-markdown");
    let output = f
        .command()
        .current_dir(f.paths.root())
        .args([
            "knowledge",
            "export",
            "--format",
            "markdown",
            "--artifact-id",
            "vendor/knowledge-roundtrip",
            "--artifact-version",
            "missing",
            "--context-alias",
            "roundtrip-wiki",
            "--output",
        ])
        .arg(&missing_export)
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let missing: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(missing["status"], "registry_absent");
    assert_eq!(missing["format"], "markdown");
    assert!(missing.get("generation").is_none());
    assert!(missing.get("output").is_none());
    assert!(!missing_export.exists());
    // NOREPLACE preserves the directory and its literal bytes on a repeated export.
    let output = f
        .command()
        .current_dir(f.paths.root())
        .args([
            "knowledge",
            "export",
            "--format",
            "markdown",
            "--artifact-id",
            "vendor/knowledge-roundtrip",
            "--artifact-version",
            "release-2",
            "--context-alias",
            "roundtrip-wiki",
            "--output",
        ])
        .arg(&imported_markdown)
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr)?;
    let error: serde_json::Value = serde_json::from_str(
        stderr
            .lines()
            .find_map(|line| line.strip_prefix("graphit: "))
            .ok_or("missing publication error")?,
    )?;
    assert_eq!(
        error["code"], "knowledge_export_publication_failed",
        "{error}"
    );
    assert_eq!(error["publication"], "unconfirmed");
    assert_eq!(error["io_error_kind"], "AlreadyExists");
    assert_eq!(
        error["output"],
        json!(NativePath::new(imported_markdown.clone())?)
    );
    assert_eq!(error["cleanup"]["status"], "complete");
    assert_eq!(
        fs::read(imported_markdown.join("index.md"))?,
        fs::read(markdown.join("index.md"))?
    );
    assert_eq!(fs::read_dir(imported_markdown.join("pages"))?.count(), 2);
    for entry in fs::read_dir(markdown.join("pages"))? {
        let entry = entry?;
        assert_eq!(
            fs::read(imported_markdown.join("pages").join(entry.file_name()))?,
            fs::read(entry.path())?
        );
    }
    // A writable destination must never become part of the selected immutable payload.
    let inside_payload = roundtrip.join("rejected-export");
    let output = f
        .command()
        .current_dir(f.paths.root())
        .args([
            "knowledge",
            "export",
            "--format",
            "markdown",
            "--artifact-id",
            "vendor/knowledge-roundtrip",
            "--artifact-version",
            "release-2",
            "--context-alias",
            "roundtrip-wiki",
            "--output",
        ])
        .arg(&inside_payload)
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr)?;
    let error: serde_json::Value = serde_json::from_str(
        stderr
            .lines()
            .find_map(|line| line.strip_prefix("graphit: "))
            .ok_or("missing destination error")?,
    )?;
    assert_eq!(error["code"], "invalid_arguments", "{error}");
    assert!(!inside_payload.exists());
    let root = f.projects[0].root().to_str().unwrap();
    for (selectors, exit_code, cause) in [
        (
            vec!["--artifact-id", "vendor/knowledge"],
            2,
            "required arguments were not provided",
        ),
        (
            vec![
                "--artifact-id",
                "vendor/knowledge",
                "--artifact-version",
                "release-1",
                "--project-dir",
                root,
            ],
            2,
            "cannot be used with",
        ),
        (
            vec![
                "--project-version",
                "main",
                "--artifact-id",
                "vendor/knowledge",
                "--artifact-version",
                "release-1",
            ],
            1,
            "project version requires a project",
        ),
    ] {
        let rejected = f._temp.path().join("rejected-installed-export");
        let output = f
            .command()
            .current_dir(f.paths.root())
            .args(["knowledge", "export"])
            .args(selectors)
            .arg("--output")
            .arg(&rejected)
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(exit_code),
            "export selector error: {stderr}"
        );
        assert!(output.stdout.is_empty());
        assert!(stderr.contains(cause), "export selector error: {stderr}");
        assert!(!rejected.exists());
    }
    let cli_search =
        |selectors: &[&str], cursor: Option<&serde_json::Value>| -> Result<serde_json::Value> {
            let mut command = f.command();
            command
                .current_dir(f.paths.root())
                .args([
                    "knowledge",
                    "search",
                    "literal",
                    "--mode",
                    "fts",
                    "--page-size",
                    "1",
                    "--preview",
                ])
                .args(selectors);
            if let Some(cursor) = cursor {
                command.arg("--cursor").arg(serde_json::to_string(cursor)?);
            }
            let output = command.output()?;
            assert_eq!(
                output.status.code(),
                Some(0),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stderr.is_empty());
            Ok(serde_json::from_slice(&output.stdout)?)
        };
    let root = f.projects[0].root().to_str().unwrap();
    let mut expected_slugs: Vec<_> = page["native"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["focused_source"]["slug"].as_str().unwrap().to_owned())
        .collect();
    expected_slugs.sort();
    for (selectors, expected_source, version) in [
        (
            vec!["--project-dir", root],
            json!({"kind":"project_knowledge"}),
            generation,
        ),
        (
            vec![
                "--artifact-id",
                "vendor/knowledge",
                "--artifact-version",
                "release-1",
            ],
            imported_source.clone(),
            cloned_version,
        ),
    ] {
        let first = cli_search(&selectors, None)?;
        let cursor = &first["native"]["next_cursor"];
        assert!(cursor.is_object(), "{first}");
        let second = cli_search(&selectors, Some(cursor))?;
        let mut slugs = Vec::new();
        for result in [&first, &second] {
            assert_eq!(result["source"], expected_source);
            assert_eq!(result["native"]["state"], "hits");
            assert_eq!(
                result["native"]["generation"]["version"].as_u64(),
                Some(version)
            );
            let hits = result["native"]["hits"].as_array().unwrap();
            assert_eq!(hits.len(), 1);
            let hit = &hits[0];
            assert!(hit["preview"]["text"].as_str().unwrap().contains("literal"));
            assert!(hit["lexical_score"].as_f64().unwrap().is_finite());
            slugs.push(hit["focused_source"]["slug"].as_str().unwrap().to_owned());
        }
        assert!(second["native"]["next_cursor"].is_null());
        slugs.sort();
        assert_eq!(slugs, expected_slugs);
    }
    for args in [
        vec![
            "knowledge",
            "search",
            "literal",
            "--artifact-id",
            "vendor/knowledge",
        ],
        vec!["knowledge", "search", "literal", "--cursor", "null"],
        vec![
            "knowledge",
            "search",
            "literal",
            "--artifact-id",
            "vendor/knowledge",
            "--artifact-version",
            "release-1",
            "--project-dir",
            root,
        ],
    ] {
        let output = f
            .command()
            .current_dir(f.paths.root())
            .args(args)
            .output()?;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    let missing = f
        .command()
        .current_dir(f.paths.root())
        .args([
            "knowledge",
            "search",
            "literal",
            "--artifact-id",
            "vendor/knowledge",
            "--artifact-version",
            "missing",
        ])
        .output()?;
    assert_eq!(
        missing.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&missing.stderr)
    );
    assert!(missing.stderr.is_empty());
    let absent: serde_json::Value = serde_json::from_slice(&missing.stdout)?;
    assert_eq!(
        absent["source"],
        json!({"kind":"imported_knowledge","artifact":{"id":"vendor/knowledge","version":"missing"}})
    );
    assert_eq!(absent["native"], json!({"state":"registry_absent"}));
    assert!(absent["selection"].is_null());
    assert!(absent["focused_source"].is_null());
    let invalid_project = f
        .command()
        .current_dir(f.paths.root())
        .args([
            "--project-version",
            "missing",
            "knowledge",
            "search",
            "literal",
            "--project-dir",
            root,
        ])
        .output()?;
    assert_eq!(invalid_project.status.code(), Some(1));
    assert!(invalid_project.stdout.is_empty());
    assert!(!invalid_project.stderr.is_empty());
    sdk.cancel().await?;
    let output = tokio::time::timeout(Duration::from_secs(10), child.wait_with_output()).await??;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    daemon.stop("-INT").await?;
    Ok(())
}

#[tokio::test]
async fn compiled_okf_and_obsidian_export_project_and_installed_source() -> Result {
    use graphit_core::{installation_identity::InstallationIdentityStore, native_path::NativePath};
    use graphit_hub::{
        ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
        registry::{
            HubRegistry, Installation, InstalledArtifact, InstalledBackend, InstalledPayload,
        },
    };
    let f = Fixture::new().await?;
    let cancel = Cancellation::default();
    let store = ConfigStore::global(&f.paths)?;
    for (key, enabled) in [
        ("modules.ast", false),
        ("modules.memory", false),
        ("modules.knowledge", true),
        ("modules.embedding", true),
        ("modules.dream", false),
    ] {
        store.set(
            ConfigKey::new(key)?,
            json!(enabled),
            Duration::from_secs(5),
            &cancel,
        )?;
    }
    fs::create_dir(f.projects[0].root().join("docs"))?;
    fs::write(
        f.projects[0].root().join("docs/a.md"),
        "---\nid: exported-note\ntitle: Native export\ntype: guide\ndescription: exportableword Complete original description\n---\n# Native export\nexportableword literal body.\n[[exported-second]] [[exported-second#Second export|Leia segundo]] [[exported%2Dsecond]]\n```rust\nopen author fence [[exported-second]]\n",
    )?;
    fs::write(
        f.projects[0].root().join("docs/b.md"),
        "---\nid: exported-second\ntitle: Second export\ntype: reference\ndescription: ''\n---\n# Second export\nliteral second document.\n[[exported-note]]\n<div>\nopen author HTML [[exported-note]]\n",
    )?;
    let project_id = ProjectIdentityStore::at(&f.projects[0])
        .load()?
        .ok_or("missing Knowledge fixture project identity")?
        .id();
    let mut daemon = f.daemon(&["--project-version", "main"]).await?;
    // Discovery only means MCP can accept connections. Wait for the watcher
    // to finish the document pass so FTS and export observe the same corpus.
    let publication =
        format!("Graphit Knowledge job for {project_id} version main: document generation ");
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let log = fs::read_to_string(&daemon.log)?;
        if let Some(status) = daemon.child.try_wait()? {
            return Err(
                io::Error::other(format!("Knowledge daemon exited with {status}: {log}")).into(),
            );
        }
        if log.lines().any(|line| line.starts_with(&publication)) {
            break;
        }
        if Instant::now() >= until {
            return Err(io::Error::other(format!(
                "Knowledge watcher did not finish its initial publication: {log}"
            ))
            .into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut command = tokio::process::Command::from(f.command());
    command
        .current_dir(f.paths.root())
        .args(["mcp", "--stdio", "--non-interactive"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let sdk = match ().serve((child.stdout.take().unwrap(), child.stdin.take().unwrap())).await {
        Ok(sdk) => sdk,
        Err(error) => {
            let output = child.wait_with_output().await?;
            return Err(io::Error::other(format!(
                "{error}: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
            .into());
        }
    };
    let project_query = json!({
        "search":{"query":"literal","mode":"fts","preview":true},
        "project_dir":NativePath::new(f.projects[0].root().to_owned())?,
        "project_version":"main"
    });
    let response = sdk
        .peer()
        .call_tool(
            CallToolRequestParams::new("graphit_knowledge_search")
                .with_arguments(json_arguments(project_query.as_object().unwrap().clone())),
        )
        .await?;
    assert_ne!(
        response.is_error,
        Some(true),
        "{response:?}: {}",
        fs::read_to_string(&daemon.log)?
    );
    let page = response
        .structured_content
        .ok_or("missing Knowledge search result")?;
    assert_eq!(page["native"]["state"], "hits", "{page}");
    assert_eq!(page["source"], json!({"kind":"project_knowledge"}));
    assert_eq!(page["native"]["total"], 2);
    let project_hit = page["native"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|hit| {
            hit["preview"]["text"]
                .as_str()
                .unwrap()
                .contains("exportableword")
        })
        .ok_or("missing original exported document")?;
    assert!(
        project_hit["preview"]["text"]
            .as_str()
            .unwrap()
            .contains("exportableword")
    );
    assert!(project_hit["lexical_score"].as_f64().unwrap().is_finite());
    let archive = f._temp.path().join("knowledge.zip");
    let exported = f
        .command()
        .current_dir(f.paths.root())
        .args([
            "--project-version",
            "main",
            "knowledge",
            "export",
            "--project-dir",
        ])
        .arg(f.projects[0].root())
        .arg("--output")
        .arg(&archive)
        .output()?;
    assert_eq!(
        exported.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    assert!(exported.stderr.is_empty());
    let exported: serde_json::Value = serde_json::from_slice(&exported.stdout)?;
    assert_eq!(exported["status"], "exported");
    assert_eq!(exported["format"], "native");
    let generation = exported["generation"]["version"]
        .as_u64()
        .ok_or("native corpus version")?;
    assert_eq!(page["native"]["generation"]["version"], generation);
    // No Models service was started. The actual producer exports its lexical variant.
    assert!(exported["generation"].get("corpus_uri").is_none());
    let imported = f._temp.path().join("installed-knowledge");
    let extracted = f
        .command()
        .current_dir(f.paths.root())
        .args(["package", "import", "--kind", "knowledge", "--input"])
        .arg(&archive)
        .arg("--output")
        .arg(&imported)
        .output()?;
    assert_eq!(
        extracted.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&extracted.stderr)
    );
    assert!(extracted.stderr.is_empty());
    assert_eq!(extracted.stdout, b"knowledge\n");
    assert!(imported.join("index.lance").is_dir());
    let cloned_version = knowledge_dataset_version(&imported).await?;
    assert_eq!(fs::read_dir(&imported)?.count(), 1);

    let config = ConfigSnapshot::new(f.paths.brand(), [])?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let identity = InstallationIdentityStore::new(&f.paths)?.load()?;
    let registry = HubRegistry::global(
        &f.paths,
        identity.unit_id().ok_or("missing installation unit")?,
        policy.hub(),
    )?;
    let projection = registry.limits().projection();
    let producer = ProjectIdentityStore::at(&f.projects[0])
        .load()?
        .unwrap()
        .id();
    let source = SourceRef::local_link(
        NativePath::new(imported)?,
        producer,
        ArtifactType::Knowledge,
        ArtifactId::new("vendor/knowledge", projection)?,
        ConcreteVersion::new("release-1", projection)?,
        projection,
    )?;
    registry
        .writer(Duration::from_secs(5), &cancel)?
        .put_metadata(InstalledArtifact {
            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
            artifact_type: ArtifactType::Knowledge,
            id: source.id().clone(),
            installations: vec![Installation {
                target: InstallationTarget::GlobalNative,
                original_source: source,
                alias: None,
                entry_name: None,
                display: None,
                backend: InstalledBackend::Native,
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        })?;

    // Only the two newly integrated text profiles; existing Native/Markdown
    // consumer qualification remains in its own unchanged case.
    for format in ["okf", "obsidian"] {
        let project_output = f._temp.path().join(format!("project-{format}"));
        let output = f
            .command()
            .current_dir(f.paths.root())
            .args([
                "--project-version",
                "main",
                "knowledge",
                "export",
                "--format",
                format,
                "--project-dir",
            ])
            .arg(f.projects[0].root())
            .arg("--output")
            .arg(&project_output)
            .output()?;
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let project_result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(project_result["status"], "exported");
        assert_eq!(project_result["format"], format);
        assert_eq!(
            project_result["output"],
            json!(NativePath::new(project_output.clone())?)
        );
        assert_eq!(project_result["generation"], page["native"]["generation"]);
        assert_eq!(project_result["cleanup"]["status"], "complete");
        let installed_output = f._temp.path().join(format!("installed-{format}"));
        let output = f
            .command()
            .current_dir(f.paths.root())
            .args([
                "knowledge",
                "export",
                "--format",
                format,
                "--artifact-id",
                "vendor/knowledge",
                "--artifact-version",
                "release-1",
                "--context-alias",
                "text-wiki",
                "--output",
            ])
            .arg(&installed_output)
            .output()?;
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let installed_result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(installed_result["status"], "exported");
        assert_eq!(installed_result["format"], format);
        assert_eq!(
            installed_result["output"],
            json!(NativePath::new(installed_output.clone())?)
        );
        assert_eq!(
            installed_result["generation"]["version"].as_u64(),
            Some(cloned_version)
        );
        assert_eq!(
            installed_result["fingerprint"],
            project_result["fingerprint"]
        );
        assert_eq!(
            installed_result["communities"],
            project_result["communities"]
        );
        assert_eq!(installed_result["cleanup"]["status"], "complete");
        let index = fs::read_to_string(project_output.join("index.md"))?;
        assert_eq!(
            fs::read(installed_output.join("index.md"))?,
            index.as_bytes()
        );
        let (header, navigation) = index
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n")
            .unwrap();
        let header: serde_json::Value = serde_saphyr::from_str(header)?;
        assert_eq!(header, json!({"okf_version":"0.2"}));
        assert_eq!(fs::read_dir(project_output.join("pages"))?.count(), 2);
        assert_eq!(fs::read_dir(installed_output.join("pages"))?.count(), 2);
        for hit in page["native"]["hits"].as_array().unwrap() {
            let slug = hit["focused_source"]["slug"].as_str().unwrap();
            let (title, kind, description, body) = if hit["preview"]["text"]
                .as_str()
                .unwrap()
                .contains("exportableword")
            {
                (
                    "Native export",
                    "guide",
                    "exportableword Complete original description",
                    "# Native export\nexportableword literal body.\n[[exported-second]] [[exported-second#Second export|Leia segundo]] [[exported%2Dsecond]]\n```rust\nopen author fence [[exported-second]]\n",
                )
            } else {
                (
                    "Second export",
                    "reference",
                    "",
                    "# Second export\nliteral second document.\n[[exported-note]]\n<div>\nopen author HTML [[exported-note]]\n",
                )
            };
            let relative = navigation
                .split_once(&format!("[{title}]("))
                .unwrap()
                .1
                .split_once(')')
                .unwrap()
                .0;
            let text = fs::read_to_string(project_output.join(relative))?;
            assert_eq!(fs::read(installed_output.join(relative))?, text.as_bytes());
            assert!(
                navigation.contains(&format!("[{title}]({relative})")),
                "{navigation}"
            );
            let (metadata, content) = text
                .strip_prefix("---\n")
                .unwrap()
                .split_once("\n---\n")
                .unwrap();
            let metadata: serde_json::Value = serde_saphyr::from_str(metadata)?;
            assert_eq!(metadata["id"], slug);
            assert_eq!(metadata["title"], title);
            assert_eq!(metadata["type"], kind);
            assert_eq!(metadata["description"], description);
            assert_eq!(
                metadata["generated"]["by"],
                format!("process:{}-knowledge-wiki", f.paths.brand().key())
            );
            let native = &metadata["graphit"];
            assert_eq!(metadata["sources"][0]["resource"], native["source_path"]);
            assert_eq!(native.as_object().unwrap().len(), 15);
            assert_eq!(native["title"], title);
            assert_eq!(native["description"], description);
            // FileScan publishes its managed Source key, not the author's checkout path.
            assert_eq!(native["source_path"], hit["focused_source"]["source_path"]);
            let compilation_digest: String = hit["focused_source"]["compilation_digest"]
                .as_array()
                .unwrap()
                .iter()
                .map(|byte| format!("{:02x}", byte.as_u64().unwrap()))
                .collect();
            assert_eq!(native["compilation_digest"], compilation_digest);
            assert!(native["physical_digest"].is_string());
            assert!(native["compilation_digest"].is_string());
            assert!(native["recipe"].is_string());
            assert!(native["recipe_parameters"].is_object());
            if native["updated_at"].is_null() {
                assert!(metadata["generated"].get("at").is_none());
            } else {
                assert_eq!(metadata["generated"]["at"], native["updated_at"]);
            }
            assert!(metadata.get("stale_after").is_none());
            assert!(content.contains("Content"));
            let expected = if format == "obsidian" {
                let target_title = if title == "Native export" {
                    "Second export"
                } else {
                    "Native export"
                };
                let target = navigation
                    .split_once(&format!("[{target_title}]("))
                    .unwrap()
                    .1
                    .split_once(')')
                    .unwrap()
                    .0;
                assert!(project_output.join(target).is_file());
                assert!(installed_output.join(target).is_file());
                let target = target.strip_prefix("pages/").unwrap();
                if title == "Native export" {
                    format!(
                        "# Native export\nexportableword literal body.\n[[{target}|exported-second]] [[{target}#Second export|Leia segundo]] [[{target}|exported%2Dsecond]]\n```rust\nopen author fence [[exported-second]]\n"
                    )
                } else {
                    format!(
                        "# Second export\nliteral second document.\n[[{target}|exported-note]]\n<div>\nopen author HTML [[exported-note]]\n"
                    )
                }
            } else {
                body.to_owned()
            };
            assert!(content.ends_with(&expected), "{content}");
        }
    }
    sdk.cancel().await?;
    let output = tokio::time::timeout(Duration::from_secs(10), child.wait_with_output()).await??;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    daemon.stop("-INT").await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_memory_index_project_no_corpus_preserves_versions_and_user_boundary() -> Result {
    let f = Fixture::new().await?;
    let cancel = Cancellation::default();
    let config = ConfigStore::global(&f.paths)?;
    for (key, enabled) in [
        ("modules.ast", false),
        ("modules.memory", true),
        ("modules.knowledge", false),
        ("modules.embedding", false),
        ("modules.dream", false),
        ("broker.allow_loopback_http", true),
    ] {
        config.set(
            ConfigKey::new(key)?,
            json!(enabled),
            Duration::from_secs(5),
            &cancel,
        )?;
    }
    let project_id = ProjectIdentityStore::at(&f.projects[0])
        .load()?
        .ok_or("missing project identity")?
        .id();
    let corpus = f.projects[0].project_runtime(project_id, &["memory", "index.lance"])?;
    assert!(!corpus.try_exists()?);
    let mut daemon = f.daemon(&["--project-version", "main"]).await?;
    for (version, refresh) in [
        ("main", false),
        ("main", true),
        ("feature", false),
        ("feature", true),
    ] {
        let mut command = f.command();
        command.args([
            "--project-version",
            version,
            "memory",
            "index",
            "--project-dir",
        ]);
        command.arg(f.projects[0].root());
        if refresh {
            command.arg("--refresh");
        }
        let output = command.output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|error| {
            io::Error::other(format!(
                "Memory index {version} refresh={refresh} returned invalid JSON: {error}; stdout={:?}; stderr={:?}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            ))
        })?;
        assert_eq!(result["scope"], "project");
        assert_eq!(result["project"]["id"], json!(project_id));
        assert_eq!(result["project"]["version"], version);
        assert_eq!(result["lexical"]["state"], "no_corpus");
        assert!(result["lexical"].get("generation").is_none());
        assert_eq!(result["semantic"]["state"], "no_corpus");
        assert!(result["semantic"].get("generation").is_none());
        assert!(!corpus.try_exists()?);
    }

    let discovery_path = f.paths.root().join("daemon/mcp.json");
    let key_path = f.paths.root().join("daemon/mcp.key");
    let discovery = fs::read(&discovery_path)?;
    let key = fs::read(&key_path)?;
    let invalid_selection = f
        .command()
        .current_dir(f.paths.root())
        .args(["memory", "index", "--scope", "user", "--project-dir"])
        .arg(f.projects[0].root())
        .output()?;
    assert_eq!(invalid_selection.status.code(), Some(1));
    assert!(invalid_selection.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&invalid_selection.stderr)
            .contains("USER Memory does not accept PROJECT selectors")
    );

    let denied_user = f
        .command()
        .current_dir(f.paths.root())
        .args(["memory", "index", "--scope", "user"])
        .output()?;
    assert_eq!(denied_user.status.code(), Some(1));
    assert!(denied_user.stdout.is_empty());
    let denied_message = String::from_utf8_lossy(&denied_user.stderr);
    assert!(
        denied_message.contains("no active auth profile"),
        "{denied_message}"
    );
    assert_eq!(fs::read(&discovery_path)?, discovery);
    assert_eq!(fs::read(&key_path)?, key);
    assert!(!corpus.try_exists()?);

    let broker = memory_index_auth::Broker::start().await;
    daemon.stop("-TERM").await?;
    activate_memory_index_native_profile(&f, &broker)?;
    // Startup captures the configured provider's service authority. Publishing a
    // new private profile directly is not the product login/configuration lifecycle.
    daemon = f.daemon(&["--project-version", "main"]).await?;
    // Existence alone can still observe the previous owner's discovery during
    // restart. Wait for this privately joined/restarted child to publish its PID.
    let ready_until = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = daemon.child.try_wait()? {
            return Err(io::Error::other(format!(
                "restarted daemon exited with {status}: {}",
                fs::read_to_string(&daemon.log)?
            ))
            .into());
        }
        let published = fs::read(&discovery_path).ok().and_then(|bytes| {
            serde_json::from_slice::<graphit_daemon::source_discovery::DiscoveryRecord>(&bytes).ok()
        });
        if published
            .as_ref()
            .is_some_and(|record| record.pid() == daemon.child.id())
        {
            break;
        }
        if Instant::now() >= ready_until {
            return Err(
                io::Error::other("restarted daemon did not publish its owned discovery").into(),
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let discovery = fs::read(&discovery_path)?;
    let key = fs::read(&key_path)?;
    let mut warm_requests = None;
    for refresh in [false, true] {
        let mut command = f.command();
        command
            .current_dir(f.paths.root())
            .args(["memory", "index", "--scope", "user"]);
        if refresh {
            command.arg("--refresh");
        }
        let output = command.output()?;
        assert!(
            output.status.success(),
            "USER Memory index refresh={refresh}: {}; owned daemon status {:?}; daemon log: {}",
            String::from_utf8_lossy(&output.stderr),
            daemon.child.try_wait()?,
            fs::read_to_string(&daemon.log)?,
        );
        assert!(output.stderr.is_empty());
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|error| {
            io::Error::other(format!(
                "USER Memory index refresh={refresh} returned invalid JSON: {error}; stdout={:?}",
                String::from_utf8_lossy(&output.stdout),
            ))
        })?;
        assert_eq!(
            result,
            json!({
                "scope":"user",
                "lexical":{"state":"no_corpus"},
                "semantic":{"state":"no_corpus"}
            })
        );
        let requests = broker.requests();
        if let Some(warm) = warm_requests {
            assert_eq!(requests, warm);
        } else {
            assert!(requests.iter().all(|count| *count > 0));
            warm_requests = Some(requests);
        }
    }
    assert_eq!(fs::read(&discovery_path)?, discovery);
    assert_eq!(fs::read(&key_path)?, key);
    assert!(!corpus.try_exists()?);
    broker.shutdown().await;

    assert!(daemon.child.try_wait()?.is_none());
    let status = f.command().args(["daemon", "start"]).output()?;
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).map_err(|error| {
        io::Error::other(format!(
            "daemon start returned invalid JSON: {error}; stdout={:?}; stderr={:?}",
            String::from_utf8_lossy(&status.stdout),
            String::from_utf8_lossy(&status.stderr),
        ))
    })?;
    assert_eq!(status["pid"], daemon.child.id());
    daemon.stop("-TERM").await?;
    Ok(())
}

fn activate_memory_index_native_profile(f: &Fixture, broker: &memory_index_auth::Broker) -> Result {
    use graphit_auth::{
        profile::{
            AiService, BrokerService, OidcTokens, ProfileCredentials, ProfileData,
            ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source,
            Secret,
        },
        store::AuthStore,
    };
    use graphit_core::{
        artifact_store::CapturedPublicationScope,
        auth_document::{
            AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect,
        },
        installation_identity::InstallationIdentityStore,
    };
    let config = ConfigSnapshot::new(
        f.paths.brand(),
        [
            HubResourcePolicy::distribution_defaults(f.paths.brand())?,
            ConfigStore::global(&f.paths)?.load()?,
        ],
    )?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let cancellation = Cancellation::default();
    let request = policy.request_context(Instant::now(), &cancellation)?;
    let identity = InstallationIdentityStore::new(&f.paths)?.load()?;
    let installation_id = identity.unit_id().ok_or("missing installation identity")?;
    let live = CapturedPublicationScope::installation(
        &f.paths,
        installation_id.clone(),
        std::iter::once(f.paths.root().to_owned())
            .chain(f.projects.iter().map(|project| project.root().to_owned())),
        memory_index_auth::scopes(),
        request.snapshot(),
    )?;
    let store = AuthStore::new(
        AuthDocumentStore::capture(
            live,
            AuthDocumentLimits::new(1048576, 65536, 256, 65536)?,
            request.snapshot(),
        )?,
        ProfileLimits {
            bytes: 1048576,
            nodes: 65536,
            scalar_bytes: 65536,
            collection_entries: 1024,
            providers: 16,
            profiles: 16,
        },
        &request,
    )?;
    let provider = ProviderConfig {
        kind: ProviderKind::Broker(BrokerService {
            endpoint: broker.origin.clone(),
            mcp_resource: None,
            allow_anonymous: false,
        }),
        embedding: AiService::Disabled,
        rerank: AiService::Disabled,
        s3: S3Location {
            bucket: None,
            region: None,
            endpoint: None,
            prefix: None,
            source: S3Source::BrokerScope,
        },
    };
    let expiry = memory_index_auth::now() + 600;
    let data = ProfileData {
        name: "native".into(),
        provider: "service".into(),
        username: "display-not-principal".into(),
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Broker {
            issuer: broker.origin.clone(),
            subject: "native-fixture-sub".into(),
            s3_disabled: true,
            tokens: OidcTokens {
                access_token: Secret::new(broker.token(expiry, "broker-api").into()),
                refresh_token: None,
                id_token: Secret::new("historical-fixture-id-token".into()),
                // Renewable OIDC profiles persist UTC microseconds; JWT exp is seconds.
                expires_at: expiry
                    .checked_mul(1_000_000)
                    .ok_or("fixture OIDC expiry overflow")?
                    .try_into()?,
            },
        },
    };
    let published = store.mutate(store.load(&request)?, &request, |doc, request| {
        if doc.provider("service").is_none() {
            doc.add_provider("service".into(), provider, doc.revision(), 1, request)?;
        }
        let expected = ProfileExpectation {
            document_revision: doc.revision(),
            provider_revision: doc
                .provider("service")
                .expect("provider just published")
                .revision(),
            profile_revision: doc.profile("native").map(|profile| profile.revision()),
        };
        let updated = doc
            .profile("native")
            .map_or(1, |profile| profile.updated_at() + 1);
        doc.replace_profile_data(data, expected, updated, request)?;
        doc.activate("native", doc.revision(), request)
    })?;
    assert_eq!(published.effect(), AuthPublicationEffect::Applied);
    assert!(published.failure().is_none());
    assert!(published.cleanup_failure().is_none());
    store.load(&request).map_err(|error| {
        io::Error::other(format!(
            "Memory profile fixture SDK roundtrip failed: {error}"
        ))
    })?;
    Ok(())
}

#[path = "source_executable/native_publication.rs"]
mod native_publication;

async fn knowledge_dataset_version(root: &std::path::Path) -> Result<u64> {
    let uri =
        url::Url::from_file_path(root.join("index.lance")).map_err(|_| "absolute corpus path")?;
    let pin = graphit_data::lance_branch_store::BranchSnapshot::capture_observed(
        uri.as_str(),
        None,
        None,
    )
    .await?;
    graphit_data::knowledge_embeddings::native_knowledge_embedding_dimensions(pin.schema())?;
    Ok(pin.version())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_hub_sync_admits_local_native_without_importing_again() -> Result {
    let f = Fixture::new().await?;
    f.imported("fixture/native-sync", "retained native Source")
        .await?;
    let daemon = f.daemon(&["--project-version", "main"]).await?;
    let result: Result = async {
        let output = f.command().args(["hub", "sync", "--global"]).output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(value["operation"], "sync");
        assert_eq!(value["items"][0]["outcome"]["admission"], "verified");
        assert_eq!(value["items"][0]["outcome"]["publication"], "unchanged");
        let directory = f
            ._temp
            .path()
            .join("imported/fixture/native-sync/graph.icebug");
        let mut before: Vec<_> = fs::read_dir(directory.join("source.lance/_versions"))?
            .map(|entry| entry.map(|e| e.file_name()))
            .collect::<std::io::Result<_>>()?;
        let output = f.command().args(["hub", "sync", "--global"]).output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut after: Vec<_> = fs::read_dir(directory.join("source.lance/_versions"))?
            .map(|entry| entry.map(|e| e.file_name()))
            .collect::<std::io::Result<_>>()?;
        before.sort();
        after.sort();
        assert_eq!(before, after);
        fs::rename(&directory, directory.with_extension("unavailable"))?;
        let rejected = f.command().args(["hub", "sync", "--global"]).output()?;
        assert!(!rejected.status.success());
        assert!(!directory.exists());
        Ok(())
    }
    .await;
    let stopped = daemon.stop("-TERM").await;
    match (result, stopped) {
        (Err(primary), _) => Err(primary),
        (Ok(()), stopped) => stopped,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_general_sync_stdio_uses_launch_project_and_keeps_global_explicit() -> Result {
    let f = Fixture::new().await?;
    let init = Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(f.projects[1].root())
        .output()?;
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let daemon = f.daemon(&["--project-version", "main"]).await?;
    let result: Result = async {
        let root = f.projects[1].root();
        let mut command = tokio::process::Command::from(f.command());
        command
            .current_dir(root)
            .args(["mcp", "--stdio", "--project-dir"])
            .arg(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn()?;
        let client = ()
            .serve((
                child.stdout.take().ok_or("MCP stdout")?,
                child.stdin.take().ok_or("MCP stdin")?,
            ))
            .await?;
        let calls: Result = async {
            let catalog = client.peer().list_all_tools().await?;
            assert!(catalog.iter().any(|tool| tool.name == "graphit_sync"));
            let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../templates/agent-instructions/coordinator")
                .canonicalize()?;
            let synced = client
                .peer()
                .call_tool(
                    CallToolRequestParams::new("graphit_sync").with_arguments(json_arguments(
                        json!({
                            "coordinator_source":source,"project_dir":null,"project_version":null
                        })
                        .as_object()
                        .unwrap()
                        .clone(),
                    )),
                )
                .await?;
            assert_ne!(synced.is_error, Some(true), "{synced:?}");
            let native = synced
                .structured_content
                .as_ref()
                .ok_or("project sync result")?;
            assert_eq!(
                native["steps"]["native"]["git_hooks"]["state"],
                "reconciled"
            );
            let registration = std::process::Command::new("git")
                .args([
                    "config",
                    "--local",
                    "--get",
                    "hook.graphit--post-commit.command",
                ])
                .current_dir(root)
                .output()?;
            assert_eq!(registration.stdout, b"graphit git hook post-commit --\n");
            assert!(root.join(".codex/agents").exists());
            assert!(!f.projects[0].root().join(".codex/agents").exists());
            let global = client
                .peer()
                .call_tool(CallToolRequestParams::new("graphit_sync").with_arguments(
                    json_arguments(json!({"global":true}).as_object().unwrap().clone()),
                ))
                .await?;
            assert_ne!(global.is_error, Some(true), "{global:?}");
            let global = global.structured_content.ok_or("global sync result")?;
            assert!(global["steps"]["native"]["coordinator_hooks_mcp"].is_null());
            assert!(global["steps"]["native"]["git_hooks"].is_null());
            Ok(())
        }
        .await;
        client.cancel().await?;
        let status = tokio::time::timeout(Duration::from_secs(30), child.wait()).await??;
        assert!(status.success());
        calls
    }
    .await;
    let stopped = daemon.stop("-TERM").await;
    match (result, stopped) {
        (Err(primary), _) => Err(primary),
        (Ok(()), stopped) => stopped,
    }
}
