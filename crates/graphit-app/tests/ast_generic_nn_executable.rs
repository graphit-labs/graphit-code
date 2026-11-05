#![cfg(target_os = "linux")]
//! Main, a custom extraction profile over linked TOML, and productive Raw/Graph/FTS/NN.
use graphit_app::foreground_source::ForegroundSource;
use graphit_core::{
    brand::Brand,
    config::{ConfigKey, ConfigSnapshot, ConfigStore},
    file_lock::Cancellation,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_daemon::model_service as model_api;
use graphit_daemon::project_bootstrap::ProjectBootstrapInput;
use graphit_hub::{
    NativeBinding,
    policy::{HubResourcePolicy, ResourceSetting},
};
use serde_json::{Value, json};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};
#[path = "../../graphit-daemon/tests/support/model_fixture.rs"]
pub mod model_fixture;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Fixture {
    temp: tempfile::TempDir,
    paths: InstallationPaths,
    project: ProjectPaths,
    model_server: Option<PathBuf>,
    expected_model: Option<model_api::ExpectedModel>,
}
#[derive(Debug)]
struct NnCapture {
    version: u64,
    batches: Vec<arrow_array::RecordBatch>,
}

struct NnState {
    version: u64,
    generation: u64,
    projection: [u8; 32],
    vector: Vec<f32>,
    vector_id: String,
}

struct Daemon {
    child: Child,
    log: PathBuf,
    joined: bool,
}

fn checked(output: Output) -> Result<Output> {
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "command {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(output)
}
fn require(condition: bool, message: impl Into<String>) -> Result {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message.into()).into())
    }
}

impl Fixture {
    fn git(home: &Path, directory: &Path) -> Command {
        let mut command = Command::new("git");
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", home.join("gitconfig-empty"))
            .args(["-c", "init.templateDir="])
            .current_dir(directory);
        command
    }

    async fn new() -> Result<Self> {
        let temp = tempfile::tempdir()?;
        fs::write(temp.path().join("gitconfig-empty"), "")?;
        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
        let paths = InstallationPaths::resolve(
            brand.clone(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )?;
        let root = temp.path().join("project");
        fs::create_dir(&root)?;
        let project = ProjectPaths::at(&root, &paths)?;
        let cancel = Cancellation::default();
        ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &cancel)?
            .ensure(&ProjectMetadata::new("generic-native-nn")?)?;
        checked(
            Self::git(temp.path(), &root)
                .args(["init", "-b", "main"])
                .output()?,
        )?;
        checked(
            Self::git(temp.path(), &root)
                .args([
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                    "commit",
                    "--allow-empty",
                    "-m",
                    "initial",
                ])
                .output()?,
        )?;
        let config = ConfigSnapshot::new(&brand, [])?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let original = policy.request_context(Instant::now(), &cancel)?;
        let inputs = [ProjectBootstrapInput {
            project: &project,
            explicit_version: "main",
            pin: None,
            detached_base: "",
        }];
        let owner =
            ForegroundSource::start(&paths, &inputs, 0, &config, NativeBinding::Codex, &original)?;
        owner.shutdown().await?;
        let store = ConfigStore::global(&paths)?;
        for (key, value) in [("agent", json!("codex")), ("modules.ast", json!(true))] {
            store.set(ConfigKey::new(key)?, value, Duration::from_secs(5), &cancel)?;
        }
        for module in [
            "agents",
            "hooks",
            "memory",
            "knowledge",
            "embedding",
            "dream",
        ] {
            store.set(
                ConfigKey::new(&format!("modules.{module}"))?,
                json!(false),
                Duration::from_secs(5),
                &cancel,
            )?;
        }
        // Private parent for the global extraction-profile sources.
        let ast_root = paths.global_path(&["ast"])?;
        if !ast_root.try_exists()? {
            graphit_fs::create_private_directory(&ast_root)?;
        }
        let queries = paths.global_path(&["ast", "queries"])?;
        fs::create_dir_all(&queries)?;
        fs::write(
            queries.join("toml.yaml"),
            r#"language: toml
grammar: tree-sitter-toml
extensions: [.native]
context_types: {pair: Pair}
context_name_queries: {pair: '(pair (quoted_key) @name) @context'}
anon_func_types: []
declaration_types: []
embed_labels: [Pair, Value]
queries:
  - data_key: pairs
    graph_label: Pair
    pattern: '(pair (quoted_key) @name (string) @value) @unit'
    span_capture: unit
    name_is_data: true
    value_capture: value
    value_label: Value
exports: {strategy: none}
"#,
        )?;
        Ok(Self {
            temp,
            paths,
            project,
            model_server: None,
            expected_model: None,
        })
    }

    fn install_active_cpu_profile(&self) -> Result {
        use graphit_auth::{
            profile::{
                AiService, LocalCredentials, LocalDevice, LocalExecution, ProfileCredentials,
                ProfileData, ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind,
                S3Location, S3Source,
            },
            store::AuthStore,
        };
        use graphit_core::{
            artifact_store::{CapturedPublicationScope, ScopeLimits},
            auth_document::{
                AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect,
            },
            installation_identity::InstallationIdentityStore,
        };
        let config = ConfigSnapshot::new(
            self.paths.brand(),
            [HubResourcePolicy::distribution_defaults(
                self.paths.brand(),
            )?],
        )?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let cancel = Cancellation::default();
        let request = policy.request_context(Instant::now(), &cancel)?;
        let identity = InstallationIdentityStore::new(&self.paths)?.load()?;
        let scope = CapturedPublicationScope::installation(
            &self.paths,
            identity.unit_id().ok_or("missing installation")?.clone(),
            [self.paths.root().to_owned()],
            ScopeLimits::new(16, 65536, 256)?,
            request.snapshot(),
        )?;
        let store = AuthStore::new(
            AuthDocumentStore::capture(
                scope,
                AuthDocumentLimits::new(65536, 65536, 256, 65536)?,
                request.snapshot(),
            )?,
            ProfileLimits {
                bytes: 65536,
                nodes: 4096,
                scalar_bytes: 4096,
                collection_entries: 64,
                providers: 16,
                profiles: 16,
            },
            &request,
        )?;
        let publication = store.mutate(store.load(&request)?, &request, |doc, ctx| {
            let config = ProviderConfig {
                kind: ProviderKind::Local {
                    allow_aws_credential_chain: false,
                    broker: None,
                },
                embedding: AiService::Local(LocalExecution {
                    device: LocalDevice::Cpu,
                    device_id: 0,
                }),
                rerank: AiService::Disabled,
                s3: S3Location {
                    bucket: None,
                    region: None,
                    endpoint: None,
                    prefix: None,
                    source: S3Source::LocalCredentials,
                },
            };
            if let Some(provider) = doc.provider("native-cpu") {
                let (revision, updated) = (provider.revision(), provider.updated_at() + 1);
                doc.update_provider("native-cpu", config, doc.revision(), revision, updated, ctx)?;
            } else {
                doc.add_provider("native-cpu".into(), config, doc.revision(), 1, ctx)?;
            }
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc
                    .provider("native-cpu")
                    .expect("provider just published")
                    .revision(),
                profile_revision: doc.profile("native-cpu").map(|profile| profile.revision()),
            };
            let updated = doc
                .profile("native-cpu")
                .map_or(1, |profile| profile.updated_at() + 1);
            doc.replace_profile_data(
                ProfileData {
                    name: "native-cpu".into(),
                    provider: "native-cpu".into(),
                    username: "native-cpu-fixture".into(),
                    organization: None,
                    teams: vec![],
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: None,
                        broker_key: None,
                        embedding_api_key: None,
                        rerank_api_key: None,
                        s3: None,
                    }),
                },
                expected,
                updated,
                ctx,
            )?;
            doc.activate("native-cpu", doc.revision(), ctx)
        })?;
        require(
            publication.effect() == AuthPublicationEffect::Applied
                && publication.failure().is_none()
                && publication.cleanup_failure().is_none(),
            "native CPU active profile publication failed",
        )?;
        Ok(())
    }

    async fn install_cpu_model(&mut self, dimensions: u64) -> Result {
        use graphit_app::model_provision::{
            InstallSource, ModelProvisioner, PinnedModel, ProvisionLimits,
        };
        use graphit_core::artifact_store::CapturedSourceReader;
        let server = PathBuf::from(
            std::env::var_os("GRAPHIT_TEST_MODEL_SERVER")
                .ok_or("missing GRAPHIT_TEST_MODEL_SERVER")?,
        );
        require(
            server.is_absolute() && server.is_file(),
            "real model server path is invalid",
        )?;
        let source = PathBuf::from(
            std::env::var_os("GRAPHIT_EMBEDDINGGEMMA2_MODEL")
                .ok_or("missing GRAPHIT_EMBEDDINGGEMMA2_MODEL")?,
        );
        let original = std::sync::Arc::new(graphit_core::file_io::SnapshotContext::new(
            Instant::now() + Duration::from_secs(60),
            Cancellation::default(),
        ));
        let producer =
            ModelProvisioner::capture(&self.paths, original.clone(), ProvisionLimits::product()?)?;
        let reader = CapturedSourceReader::capture(
            &source,
            graphit_fs::NodeKind::File,
            &producer.limits().snapshot,
            producer.limits().read,
            original.clone(),
        )?;
        let relative = reader
            .nodes()
            .next()
            .ok_or("missing captured GGUF")?
            .relative()
            .to_owned();
        let mut gate = || original.check();
        let installed = producer
            .install(
                PinnedModel::EmbeddingGemma2,
                InstallSource::Captured {
                    reader: &reader,
                    relative: &relative,
                },
                &mut gate,
            )
            .await?;
        let name = installed
            .bundle
            .file_name()
            .ok_or("installed bundle name")?;
        let relative =
            PathBuf::from(name).join(graphit_app::model_provision::manifest_name(dimensions));
        producer.select(&relative, None, &mut gate)?;
        let settings: model_api::ModelSettings = producer
            .config()
            .load()?
            .get(&ConfigKey::new("daemon.models")?)?
            .ok_or("missing selected model")?;
        let captured = model_api::CapturedModelInput::capture(
            graphit_app::models::DEFAULT_SLOT,
            &installed.bundle,
            &producer.limits().snapshot,
            producer.limits().read,
            original.clone(),
        )?;
        let resolved = model_api::ResolvedConfig::resolve(
            settings,
            model_api::LocalExecutionSelection {
                rerank: None,
                ..model_fixture::cpu_selection()
            },
            [captured],
            &original,
            &producer.limits().manifest,
        )?;
        self.expected_model = Some(model_api::ExpectedModel::new(
            &resolved,
            graphit_app::models::DEFAULT_SLOT,
            "ast-main-fixture",
        )?);
        require(
            self.expected_model
                .as_ref()
                .and_then(|model| model.dimensions())
                == Some(dimensions),
            "selected Gemma recipe dimensions differ",
        )?;
        ConfigStore::global(&self.paths)?.set(
            ConfigKey::new("modules.embedding")?,
            json!(true),
            Duration::from_secs(5),
            &Cancellation::default(),
        )?;
        self.install_active_cpu_profile()?;
        self.model_server = Some(server);
        Ok(())
    }

    async fn nn_batches(&self) -> Result<NnCapture> {
        use graphit_ast::native_nn_index_locator::{
            NativeNnIndexPresence, open_local_native_nn_index,
        };
        use graphit_auth::{ProjectCapture, ScopeCapture};
        use graphit_core::{
            artifact_store::{CapturedPublicationScope, ScopeLimits},
            git_checkout::GitCheckout,
            installation_identity::InstallationIdentityStore,
            native_document_file::NativeDocumentFileLimits,
            native_path::MAX_PATH_BYTES,
            version_context::VersionContext,
        };
        use tokio_stream::StreamExt;
        let expected = self
            .expected_model
            .as_ref()
            .ok_or("missing captured model")?;
        let model = graphit_app::ast_semantic::expected_native_nn_model(expected)?;
        let config = ConfigSnapshot::new(
            self.paths.brand(),
            [
                HubResourcePolicy::distribution_defaults(self.paths.brand())?,
                ConfigStore::global(&self.paths)?.load()?,
                ConfigStore::project(&self.project)?.load()?,
            ],
        )?;
        let hub = HubResourcePolicy::resolve(&config)?;
        let cancel = Cancellation::default();
        let request = hub.request_context(Instant::now(), &cancel)?;
        let original = std::sync::Arc::clone(request.snapshot());
        let number = |setting| usize::try_from(hub.limit(setting).value);
        let depth = number(ResourceSetting::SnapshotDepth)?;
        let native = NativeDocumentFileLimits::new(
            number(ResourceSetting::ContentInputBytes)?,
            MAX_PATH_BYTES,
            depth,
            number(ResourceSetting::ContentScalarBytes)?,
        )?;
        let entries = depth.checked_mul(2).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "snapshot entry bound overflow")
        })?;
        let scope_limits = ScopeLimits::new(2, MAX_PATH_BYTES, entries)?;
        let identity = InstallationIdentityStore::new(&self.paths)?.load()?;
        let installation = CapturedPublicationScope::installation(
            &self.paths,
            identity.unit_id().ok_or("installation identity")?.clone(),
            [self.project.root().to_owned()],
            scope_limits,
            &original,
        )?;
        let installation_record = installation.binding()?;
        let installation = ScopeCapture {
            live: &installation,
            record: &installation_record,
        };
        let identity = ProjectIdentityStore::at(&self.project)
            .load()?
            .ok_or("PROJECT identity")?;
        let scope = CapturedPublicationScope::project(
            &self.project,
            identity.id(),
            scope_limits,
            &original,
        )?;
        let record = scope.binding()?;
        let checkout = GitCheckout::inspect(
            self.project.root(),
            Duration::from_secs(5),
            original.cancellation(),
        )?;
        let version =
            VersionContext::select(&self.project, checkout.as_checkout(), "main", None, "")?;
        let project = ProjectCapture {
            scope: ScopeCapture {
                live: &scope,
                record: &record,
            },
            version: &version,
        };
        let NativeNnIndexPresence::Available(nn) =
            open_local_native_nn_index(installation, project, &original, native).await?
        else {
            return Err("published NN corpus is absent".into());
        };
        graphit_ast::corpus::validate_schema(nn.schema())?;
        require(
            nn.schema().field_with_name("vector").ok()
                == Some(graphit_ast::corpus::vector_fields(&model)?.field(0)),
            "pinned corpus model mismatch",
        )?;
        let version = nn.version();
        let scanner = nn.snapshot(&original)?.scan();
        let mut stream = scanner.try_into_stream().await?;
        let mut batches = Vec::new();
        while let Some(batch) = stream.next().await {
            let batch = graphit_ast::corpus::vectors(&batch?, &model, &original)?;
            graphit_ast::native_nn::validate_native_nn(&batch, &model, &original)?;
            batches.push(batch);
        }
        nn.revalidate(&original)?;
        original.check()?;
        Ok(NnCapture { version, batches })
    }

    async fn nn_state(
        &self,
        name: &str,
        uid: &str,
        source: &str,
        raw_version: u64,
    ) -> Result<NnState> {
        use sha2::Digest;
        let deadline = self.deadline()?;
        loop {
            let observation: Result<NnState> = async {
                let capture = self.nn_batches().await?;
                let version = capture.version;
                for batch in capture.batches {
                    let strings = |column: &str| -> Result<&arrow_array::StringArray> {
                        Ok(batch
                            .column_by_name(column)
                            .ok_or("NN string column")?
                            .as_any()
                            .downcast_ref()
                            .ok_or("NN string type")?)
                    };
                    for row in 0..batch.num_rows() {
                        if strings("path")?.value(row) != "model.native" {
                            continue;
                        }
                        let hashes = batch
                            .column_by_name("full_source_sha256")
                            .ok_or("NN SourceSHA")?
                            .as_any()
                            .downcast_ref::<arrow_array::FixedSizeBinaryArray>()
                            .ok_or("SourceSHA type")?;
                        let versions = batch
                            .column_by_name("raw_version")
                            .ok_or("NN Raw version")?
                            .as_any()
                            .downcast_ref::<arrow_array::UInt64Array>()
                            .ok_or("Raw version type")?;
                        require(
                            hashes.value(row) == sha2::Sha256::digest(source.as_bytes()).as_slice()
                                && versions.value(row) == raw_version,
                            "NN Source SHA/version is stale",
                        )?;
                        if strings("data_key")?.value(row) != "pairs"
                            || strings("uid")?.value(row) != uid
                        {
                            continue;
                        }
                        require(
                            strings("name")?.value(row) == serde_json::to_string(name)?
                                && strings("name_kind")?.value(row) == "NativeSource",
                            "NN name lost raw original quotes",
                        )?;
                        let generations = batch
                            .column_by_name("model_generation")
                            .ok_or("model generation")?
                            .as_any()
                            .downcast_ref::<arrow_array::UInt64Array>()
                            .ok_or("generation type")?;
                        let projections = batch
                            .column_by_name("projection_sha256")
                            .ok_or("projection SHA")?
                            .as_any()
                            .downcast_ref::<arrow_array::FixedSizeBinaryArray>()
                            .ok_or("projection SHA type")?;
                        let vectors = batch
                            .column_by_name("vector")
                            .ok_or("vector")?
                            .as_any()
                            .downcast_ref::<arrow_array::FixedSizeListArray>()
                            .ok_or("vector type")?
                            .value(row);
                        let vector = vectors
                            .as_any()
                            .downcast_ref::<arrow_array::Float32Array>()
                            .ok_or("vector values")?
                            .values()
                            .to_vec();
                        require(
                            Some(vector.len() as u64)
                                == self
                                    .expected_model
                                    .as_ref()
                                    .and_then(|model| model.dimensions())
                                && vector.iter().all(|v| v.is_finite())
                                && generations.value(row) > 0,
                            "real CPU NN vector/generation invalid",
                        )?;
                        return Ok(NnState {
                            version,
                            generation: generations.value(row),
                            projection: projections.value(row).try_into()?,
                            vector,
                            vector_id: batch.schema().metadata()["graphit.ast.nn.vector_id"]
                                .clone(),
                        });
                    }
                }
                Err("current native NN tuple is absent".into())
            }
            .await;
            match observation {
                Ok(state) => return Ok(state),
                Err(cause) if Instant::now() >= deadline => {
                    return Err(io::Error::other(format!(
                        "NN publication {name}: {cause}; Main {}",
                        fs::read_to_string(self.temp.path().join("daemon.log"))
                            .unwrap_or_else(|e| format!("log read failed: {e}"))
                    ))
                    .into());
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
    }

    fn verify_semantic_modes(&self, name: &str, uid: &str) -> Result {
        for mode in ["semantic", "hybrid"] {
            let payload = self.search_mode(name, mode)?;
            require(
                payload["status"] == "available",
                format!("{mode} unavailable: {payload}"),
            )?;
            let hit = payload["results"]
                .as_array()
                .and_then(|hits| {
                    hits.iter().find(|hit| {
                        hit["path"] == "model.native"
                            && hit["uid"] == uid
                            && hit["data_key"] == "pairs"
                    })
                })
                .ok_or_else(|| {
                    io::Error::other(format!("{mode} current tuple missing: {payload}"))
                })?;
            require(
                hit["distance"].as_f64().is_some_and(f64::is_finite),
                format!("{mode} distance not finite: {hit}"),
            )?;
            for field in [
                "search_generation",
                "graph_generation",
                "raw_generation",
                "source_original_generation",
                "nn_generation",
                "query_model_generation",
            ] {
                require(
                    payload.get(field).is_none(),
                    format!("obsolete field {field}: {payload}"),
                )?;
            }
            require(
                payload.get("vector_status").is_none(),
                format!("{mode} did not use available vectors: {payload}"),
            )?;
            if mode == "hybrid" {
                require(
                    hit["score"].as_f64().is_some_and(f64::is_finite),
                    format!("hybrid RRF score not finite: {hit}"),
                )?;
            }
        }
        Ok(())
    }

    fn command(&self) -> Result<Command> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
        let inherited_path = std::env::var_os("PATH").unwrap_or_default();
        let path = if let Some(model) = &self.model_server {
            let mut entries = vec![model.parent().ok_or("model server parent")?.to_owned()];
            entries.extend(std::env::split_paths(&inherited_path));
            std::env::join_paths(entries)?
        } else {
            inherited_path
        };
        command
            .current_dir(self.project.root())
            .env_clear()
            .env("PATH", path)
            .env("HOME", self.temp.path())
            .env(
                self.paths.brand().environment_variable("GLOBAL_DIR")?,
                self.paths.root(),
            )
            .env("GRAPHIT_MODULES__AST", "true");
        Ok(command)
    }
    fn deadline(&self) -> Result<Instant> {
        let config = ConfigSnapshot::new(
            self.paths.brand(),
            [
                HubResourcePolicy::distribution_defaults(self.paths.brand())?,
                ConfigStore::global(&self.paths)?.load()?,
                ConfigStore::project(&self.project)?.load()?,
            ],
        )?;
        let milliseconds = HubResourcePolicy::resolve(&config)?
            .limit(ResourceSetting::RequestDeadlineMs)
            .value;
        Instant::now()
            .checked_add(Duration::from_millis(milliseconds))
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput).into())
    }
    async fn daemon(&self) -> Result<Daemon> {
        let log = self.temp.path().join("daemon.log");
        let child = self
            .command()?
            .args(["daemon", "foreground", "--project-version", "main"])
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log)?)
            .spawn()?;
        let mut daemon = Daemon {
            child,
            log,
            joined: false,
        };
        let until = self.deadline()?;
        loop {
            if let Some(status) = daemon.child.try_wait()? {
                daemon.joined = true;
                return Err(io::Error::other(format!(
                    "Main exited {status}: {}",
                    fs::read_to_string(&daemon.log)?
                ))
                .into());
            }
            if self.paths.root().join("daemon/mcp.json").exists() {
                return Ok(daemon);
            }
            if Instant::now() >= until {
                return Err(io::Error::other(format!(
                    "Main discovery unavailable: {}",
                    fs::read_to_string(&daemon.log)?
                ))
                .into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn file_observation(&self) -> Result<Value> {
        let query = "MATCH (f:AstFileObservation) WHERE f.path = $path RETURN f.model_available AS model_available, f.source_empty AS source_empty, f.raw_version AS raw_version";
        let output = checked(
            self.command()?
                .args([
                    "ast",
                    "query",
                    query,
                    "--project-version",
                    "main",
                    "--param-string",
                ])
                .arg("path=model.native")
                .output()?,
        )?;
        Ok(serde_json::from_slice(&output.stdout)?)
    }
    async fn empty_file(&self) -> Result<Value> {
        let until = self.deadline()?;
        loop {
            let observation = self.file_observation();
            if let Ok(payload) = &observation
                && let Some(rows) = payload["rows"].as_array()
                && rows
                    .iter()
                    .any(|row| row[0]["value"] == true && row[1]["value"] == true)
            {
                let old = self.search("próximo")?;
                let output = self
                    .command()?
                    .args(["ast", "source", "model.native", "--project-version", "main"])
                    .output()?;
                if old["results"]
                    .as_array()
                    .is_some_and(|hits| hits.iter().all(|hit| hit["path"] != "model.native"))
                    && output.status.success()
                    && output.stdout.is_empty()
                {
                    return Ok(payload.clone());
                }
            }
            if Instant::now() >= until {
                return Err(io::Error::other(format!(
                    "FILE empty not evaluated by native model: {observation:?}"
                ))
                .into());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn search(&self, name: &str) -> Result<Value> {
        self.search_mode(name, "fts")
    }

    fn search_mode(&self, name: &str, mode: &str) -> Result<Value> {
        let output = checked(
            self.command()?
                .args([
                    "ast",
                    "search",
                    name,
                    "--mode",
                    mode,
                    "--project-version",
                    "main",
                ])
                .output()?,
        )?;
        Ok(serde_json::from_slice(&output.stdout)?)
    }
    async fn hit(&self, name: &str, expected_source: &str) -> Result<Value> {
        use sha2::Digest;
        let expected_sha: String = sha2::Sha256::digest(expected_source.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let until = self.deadline()?;
        loop {
            let last = match self.search(name) {
                Ok(payload) => {
                    if let Some(hit) = payload["results"].as_array().and_then(|hits| {
                        hits.iter().find(|hit| {
                            hit["name"] == name
                                && hit["kind"] == "Pair"
                                && hit["path"] == "model.native"
                                && hit["source_sha256"] == expected_sha
                        })
                    }) {
                        let output = self
                            .command()?
                            .args(["ast", "source", "model.native", "--project-version", "main"])
                            .output()?;
                        if output.status.success() && output.stdout == expected_source.as_bytes() {
                            return Ok(hit.clone());
                        }
                    }
                    payload.to_string()
                }
                Err(cause) => cause.to_string(),
            };
            if Instant::now() >= until {
                return Err(io::Error::other(format!(
                    "productive Graph/FTS hit {name} unavailable: {last}; Main {}",
                    fs::read_to_string(self.temp.path().join("daemon.log"))?
                ))
                .into());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

impl Daemon {
    async fn stop(mut self) -> Result {
        checked(
            Command::new("kill")
                .args(["-TERM", &self.child.id().to_string()])
                .output()?,
        )?;
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self.child.try_wait()? {
                self.joined = true;
                return require(
                    status.code() == Some(130),
                    format!(
                        "Main exit {status}, expected130: {}",
                        fs::read_to_string(&self.log)?
                    ),
                );
            }
            if Instant::now() >= until {
                return Err(io::Error::other("Main did not join after TERM").into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
impl Drop for Daemon {
    fn drop(&mut self) {
        if !self.joined {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires built normal model server and real Gemma BF16 via GRAPHIT_TEST_MODEL_SERVER/GRAPHIT_EMBEDDINGGEMMA2_MODEL"]
async fn generic_main_native_nn() -> Result {
    let mut fixture = Fixture::new().await?;
    fixture.install_cpu_model(768).await?;
    let first = "\"café 日本\" = \"ação\"";
    fs::write(fixture.project.root().join("model.native"), first)?;
    let mut daemon = Some(fixture.daemon().await?);
    let result: Result = async {
        let first_hit = fixture.hit("café 日本", first).await?;
        require(
            first_hit["language"] == "toml"
                && first_hit["grammar"] == "tree-sitter-toml"
                && first_hit["data_key"] == "pairs",
            format!("linked TOML extraction profile lost: {first_hit}"),
        )?;
        let uid = first_hit["uid"].as_str().ok_or("Graph UID")?.to_owned();
        let raw = first_hit["raw_version"].as_u64().ok_or("Raw version")?;
        let initial = fixture.nn_state("café 日本", &uid, first, raw).await?;
        fixture.verify_semantic_modes("café 日本", &uid)?;
        require(initial.version > 0, "corpus publication pin is absent")?;
        let same_projection = format!("{first}\n");
        fs::write(
            fixture.project.root().join("model.native"),
            &same_projection,
        )?;
        let hit = fixture.hit("café 日本", &same_projection).await?;
        let raw2 = hit["raw_version"]
            .as_u64()
            .ok_or("incremental Raw version")?;
        let increment_observation=json!({"phase":"unchanged_projection_increment","path":"model.native","previous":{"uid":uid,"data_key":first_hit["data_key"],"raw_version":raw,"source_sha256":first_hit["source_sha256"],"nn_version":initial.version,"model_generation":initial.generation},"current":{"uid":hit["uid"],"data_key":hit["data_key"],"raw_version":raw2,"source_sha256":hit["source_sha256"]}});
        eprintln!("{increment_observation}");
        require(
            raw2 > raw && hit["uid"] == uid,
            format!("incremental Source version/tuple failed: {increment_observation}"),
        )?;
        let reused = fixture
            .nn_state("café 日本", &uid, &same_projection, raw2)
            .await?;
        require(
            reused.version > initial.version
                && reused.projection == initial.projection
                && reused.vector == initial.vector
                && reused.generation == initial.generation,
            "unchanged native projection was not preserved on Source update",
        )?;
        fixture.verify_semantic_modes("café 日本", &uid)?;
        let next = "\"próximo\" = \"日本\"";
        fs::write(fixture.project.root().join("model.native"), next)?;
        let hit = fixture.hit("próximo", next).await?;
        let next_uid = hit["uid"].as_str().ok_or("next UID")?.to_owned();
        let next_raw = hit["raw_version"].as_u64().ok_or("next Raw version")?;
        require(next_raw > raw2, "projection change did not advance Raw")?;
        let changed = fixture
            .nn_state("próximo", &next_uid, next, next_raw)
            .await?;
        require(
            changed.projection != reused.projection,
            "changed projection reused the old projection hash",
        )?;
        fixture.verify_semantic_modes("próximo", &next_uid)?;
        daemon.take().ok_or("Main handle")?.stop().await?;
        fixture.install_cpu_model(128).await?;
        daemon = Some(fixture.daemon().await?);
        let model_changed = fixture
            .nn_state("próximo", &next_uid, next, next_raw)
            .await?;
        require(
            model_changed.vector_id != changed.vector_id
                && model_changed.projection == changed.projection,
            "validated Gemma MRL recipe identity did not invalidate NN reuse",
        )?;
        fixture.verify_semantic_modes("próximo", &next_uid)?;
        fs::write(fixture.project.root().join("model.native"), "")?;
        fixture.empty_file().await?;
        let until = fixture.deadline()?;
        loop {
            match fixture.nn_batches().await {
                Ok(capture) if capture.batches.iter().all(|batch| batch.num_rows() == 0) => break,
                result if Instant::now() >= until => {
                    return Err(io::Error::other(format!(
                        "empty FILE NN not published: {result:?}"
                    ))
                    .into());
                }
                _ => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
        Ok(())
    }
    .await;
    let cleanup = match daemon.take() {
        Some(daemon) => daemon.stop().await,
        None => Ok(()),
    };
    match (result, cleanup) {
        (Err(primary), Err(cleanup)) => Err(io::Error::other(format!(
            "native NN phase: {primary}; cleanup: {cleanup}; Main {}",
            fs::read_to_string(fixture.temp.path().join("daemon.log"))
                .unwrap_or_else(|e| format!("log read failed: {e}"))
        ))
        .into()),
        (Err(primary), _) => Err(io::Error::other(format!(
            "native NN phase: {primary}; Main {}",
            fs::read_to_string(fixture.temp.path().join("daemon.log"))
                .unwrap_or_else(|e| format!("log read failed: {e}"))
        ))
        .into()),
        (Ok(()), cleanup) => cleanup,
    }
}
