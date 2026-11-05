//! Actual foreground Raw -> native model -> authenticated M -> SDK NativeNN publication.
use arrow_array::{
    Array, FixedSizeBinaryArray, FixedSizeListArray, Float32Array, RecordBatch, StringArray,
    UInt64Array,
};
use futures_util::{FutureExt, TryStreamExt};
use graphit_app::{
    ast_semantic::expected_native_nn_model, foreground_source::ForegroundSource,
    models::DEFAULT_SLOT,
};
use graphit_ast::{
    indexed_source::read_branch_source,
    native_nn::validate_native_nn,
    native_nn_index_locator::{NativeNnIndexPresence, open_local_native_nn_index},
};
use graphit_auth::{ProjectCapture, ScopeCapture};
use graphit_auth::{
    profile::{
        AiService, LocalCredentials, LocalDevice, LocalExecution, ProfileCredentials, ProfileData,
        ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source,
        Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits},
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    config::{ConfigKey, ConfigSnapshot, ConfigStore},
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationIdentityStore,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
    version_context::{Checkout, VersionContext},
};
use graphit_daemon::{
    model_service as model_api, model_service::ExpectedModel,
    project_bootstrap::ProjectBootstrapInput,
};
use graphit_data::lance_branch_store::BranchSnapshot;
use graphit_hub::{NativeBinding, policy::HubResourcePolicy};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::Path,
    time::{Duration, Instant},
};

#[allow(dead_code)]
#[path = "../../graphit-daemon/tests/support/model_fixture.rs"]
mod model_fixture;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const RUST: &str = "pub fn alpha() -> u32 { 42 }";
const JSON: &str = r#"{"answer":42}"#;

async fn assert_current_source(
    raw: &BranchSnapshot,
    path: &str,
    observed: &Value,
    source: &str,
    original: &std::sync::Arc<graphit_core::file_io::SnapshotContext>,
) -> TestResult {
    let version: u64 = observed.as_str().ok_or("FILE Raw pin missing")?.parse()?;
    assert!(
        version > 0 && version <= raw.version(),
        "invalid Raw pin for {path}"
    );
    let read = read_branch_source(raw, path, original).await?;
    assert!(
        matches!(read.source(original)?, graphit_ast::source_inspection::IndexedSource::Available(text) if text == source),
        "{path} must resolve to the expected FILE bytes in the captured current Source"
    );
    Ok(())
}

async fn file_is_current(
    raw: &BranchSnapshot,
    path: &str,
    version: u64,
    digest: &str,
    source: &str,
    original: &std::sync::Arc<graphit_core::file_io::SnapshotContext>,
) -> TestResult<bool> {
    assert!(
        version > 0 && version <= raw.version(),
        "invalid Raw pin for {path}"
    );
    let observed = read_branch_source(raw, path, original).await?;
    let graphit_ast::source_inspection::IndexedSource::Available(observed) =
        observed.source(original)?
    else {
        return Ok(false);
    };
    Ok(observed == source
        && Sha256::digest(observed.as_bytes()).as_slice()
            == gix::hash::ObjectId::from_hex(digest.as_bytes())?.as_bytes())
}

struct Fixture {
    _directory: tempfile::TempDir,
    installation: InstallationPaths,
    identity: graphit_core::installation_identity::InstallationIdentity,
    project: ProjectPaths,
    config: ConfigSnapshot,
    policy: HubResourcePolicy,
    embedding: bool,
    _model: model_fixture::Fixture,
    expected: ExpectedModel,
}

impl Fixture {
    fn new() -> TestResult<Self> {
        Self::with_embedding(true)
    }

    fn with_embedding(embedding: bool) -> TestResult<Self> {
        let directory = tempfile::tempdir()?;
        let installation = InstallationPaths::resolve(
            Brand::default(),
            directory.path(),
            None,
            Some(&directory.path().join("installation")),
        )?;
        let cancellation = Cancellation::default();
        let empty = ConfigSnapshot::new(installation.brand(), [])?;
        let policy = HubResourcePolicy::resolve(&empty)?;
        let request = policy.request_context(Instant::now(), &cancellation)?;
        let private = installation_bootstrap::prepare(
            &installation,
            BootstrapLimits::new(65536, 256)?,
            request.snapshot(),
        )
        .into_installation()?;
        let identity = InstallationIdentityStore::new(&installation)?
            .ensure(Duration::from_secs(5), &cancellation)?;
        private.revalidate(request.snapshot())?;
        let capture = CapturedPublicationScope::installation(
            &installation,
            identity.unit_id().clone(),
            [installation.root().to_owned()],
            ScopeLimits::new(16, 65536, 256)?,
            request.snapshot(),
        )?;
        let auth = AuthStore::new(
            AuthDocumentStore::capture(
                capture,
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
        let observed = auth.load(&request)?;
        let outcome = auth.mutate(observed, &request, |document, request| {
            document.add_provider(
                "cpu".into(),
                ProviderConfig {
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
                },
                document.revision(),
                1,
                request,
            )?;
            let expected = ProfileExpectation {
                document_revision: document.revision(),
                provider_revision: document.provider("cpu").unwrap().revision(),
                profile_revision: None,
            };
            document.replace_profile_data(
                ProfileData {
                    name: "local".into(),
                    provider: "cpu".into(),
                    username: "foreground AST fixture".into(),
                    organization: None,
                    teams: vec![],
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: Some(Secret::new(secrecy::SecretString::from(
                            "actual-foreground-ast-fixture",
                        ))),
                        broker_key: None,
                        embedding_api_key: None,
                        rerank_api_key: None,
                        s3: None,
                    }),
                },
                expected,
                1,
                request,
            )?;
            document.activate("local", document.revision(), request)?;
            Ok(())
        })?;
        assert!(
            outcome.failure().is_none(),
            "actual provider write: {:?}",
            outcome.failure()
        );
        assert!(outcome.cleanup_failure().is_none());

        let model = if embedding {
            model_fixture::Fixture::actual(model_api::TaskKind::Embedding)
        } else {
            // AST-only consumers resolve model metadata without loading weights.
            model_fixture::Fixture::metadata(model_api::TaskKind::Embedding)
        };
        let resolved = model.config(&[DEFAULT_SLOT]);
        let expected = ExpectedModel::new(&resolved, DEFAULT_SLOT, "cpu")?;
        let global = ConfigStore::global(&installation)?;
        for (key, value) in [
            ("modules.ast", json!(true)),
            ("modules.embedding", json!(embedding)),
            ("modules.memory", json!(false)),
            ("modules.knowledge", json!(false)),
            ("modules.dream", json!(false)),
            ("modules.task", json!(false)),
            (
                "daemon.models",
                serde_json::to_value(model.settings(&[DEFAULT_SLOT]))?,
            ),
        ] {
            global.set(
                ConfigKey::new(key)?,
                value,
                Duration::from_secs(5),
                &cancellation,
            )?;
        }
        let config = ConfigSnapshot::new(installation.brand(), [global.load()?])?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let root = directory.path().join("project");
        fs::create_dir(&root)?;
        let project = ProjectPaths::at(&root, &installation)?;
        ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &cancellation)?
            .ensure(&ProjectMetadata::new("actual-AST-NN")?)?;
        fs::write(root.join("alpha.rs"), RUST)?;
        fs::write(root.join("answer.json"), JSON)?;
        Ok(Self {
            _directory: directory,
            installation,
            identity,
            project,
            config,
            policy,
            embedding,
            _model: model,
            expected,
        })
    }

    fn environment(&self) -> TestResult<Vec<(std::ffi::OsString, std::ffi::OsString)>> {
        if !self.embedding {
            return Ok(vec![(
                "PATH".into(),
                self._directory.path().as_os_str().to_owned(),
            )]);
        }
        let executable = std::env::var_os("GRAPHIT_TEST_MODEL_SERVER")
            .ok_or("GRAPHIT_TEST_MODEL_SERVER must name the compiled model child")?;
        let executable = Path::new(&executable);
        assert!(executable.is_absolute() && executable.is_file());
        Ok(vec![(
            "PATH".into(),
            executable.parent().unwrap().as_os_str().to_owned(),
        )])
    }

    async fn assert_published(&self) -> TestResult {
        let id = ProjectIdentityStore::at(&self.project)
            .load()?
            .unwrap()
            .id();
        let root = self
            .installation
            .root()
            .join("projects")
            .join(id.to_string())
            .join("ast");
        let model = expected_native_nn_model(&self.expected)?;
        let expires = Instant::now() + Duration::from_secs(30);
        let nn = loop {
            match graphit_data::lance_branch_store::BranchSnapshot::capture_observed(
                root.join("corpus.lance").to_str().unwrap(),
                None,
                None,
            )
            .await
            {
                Ok(store) => break store,
                Err(error) => {
                    if Instant::now() >= expires {
                        return Err(io::Error::other(format!(
                            "productive NativeNN Main not published: {error}"
                        ))
                        .into());
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        };
        let cancel = Cancellation::default();
        let request = self.policy.request_context(Instant::now(), &cancel)?;
        let raw = BranchSnapshot::capture_ast_corpus(
            &root.join("source.lance"),
            None,
            request.snapshot(),
        )
        .await?;
        graphit_ast::corpus::validate_schema(nn.schema())?;
        let snapshot = nn;
        let mut stream = snapshot.scan().try_into_stream().await?;
        let mut rust_found = false;
        let mut json_found = false;
        while let Some(batch) = stream.try_next().await? {
            let batch = graphit_ast::corpus::vectors(&batch, &model, request.snapshot())?;
            validate_native_nn(&batch, &model, request.snapshot())?;
            let strings = |name: &str| {
                batch
                    .column_by_name(name)
                    .unwrap()
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap()
            };
            let versions = batch
                .column_by_name("raw_version")
                .unwrap()
                .as_any()
                .downcast_ref::<UInt64Array>()
                .unwrap();
            let hashes = batch
                .column_by_name("full_source_sha256")
                .unwrap()
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .unwrap();
            let vectors = batch
                .column_by_name("vector")
                .unwrap()
                .as_any()
                .downcast_ref::<FixedSizeListArray>()
                .unwrap();
            for row in 0..batch.num_rows() {
                let path = strings("path").value(row);
                let source = match path {
                    "alpha.rs" => RUST,
                    "answer.json" => JSON,
                    _ => return Err(format!("unexpected published FILE {path}").into()),
                };
                assert_current_source(
                    &raw,
                    path,
                    &json!(versions.value(row).to_string()),
                    source,
                    request.snapshot(),
                )
                .await?;
                assert_eq!(
                    hashes.value(row),
                    Sha256::digest(source.as_bytes()).as_slice()
                );
                assert!(source.contains(strings("projection").value(row)));
                let vector = vectors.value(row);
                let vector = vector.as_any().downcast_ref::<Float32Array>().unwrap();
                assert_eq!(vector.len(), 768);
                assert!(vector.values().iter().all(|x| x.is_finite()));
                rust_found |= path == "alpha.rs"
                    && strings("kind").value(row) == "Function"
                    && strings("name").value(row) == "alpha"
                    && strings("language").value(row) == "rust"
                    && strings("data_key").value(row) == "functions";
                json_found |= path == "answer.json"
                    && strings("kind").value(row) == "Pair"
                    && strings("name").value(row) == "answer"
                    && strings("language").value(row) == "json";
            }
        }
        assert!(rust_found, "real stock Rust Function was not published");
        assert!(json_found, "real stock JSON Pair was not published");
        for (path, expected) in [("alpha.rs", RUST), ("answer.json", JSON)] {
            let read = read_branch_source(&raw, path, request.snapshot()).await?;
            assert!(
                matches!(read.source(request.snapshot())?, graphit_ast::source_inspection::IndexedSource::Available(text) if text==expected)
            );
        }
        Ok(())
    }

    async fn rows(&self) -> TestResult<(u64, Vec<RecordBatch>)> {
        self.rows_for(&self.project).await
    }

    async fn rows_for(&self, project: &ProjectPaths) -> TestResult<(u64, Vec<RecordBatch>)> {
        self.rows_for_version(project, "").await
    }

    async fn rows_for_version(
        &self,
        project: &ProjectPaths,
        selected_version: &str,
    ) -> TestResult<(u64, Vec<RecordBatch>)> {
        let version =
            VersionContext::select(project, Checkout::NonGit, selected_version, None, "")?;
        let batches = self
            .native_nn_rows(project, &version)
            .await?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "NativeNN not published"))?;
        let raw = self.raw_for_version(project, selected_version).await?;
        Ok((raw.version(), batches))
    }

    async fn raw_for_version(
        &self,
        project: &ProjectPaths,
        selected_version: &str,
    ) -> TestResult<BranchSnapshot> {
        let id = ProjectIdentityStore::at(project).load()?.unwrap().id();
        let cancel = Cancellation::default();
        let original = self.policy.request_context(Instant::now(), &cancel)?;
        let scope = CapturedPublicationScope::project(
            project,
            id,
            ScopeLimits::new(16, 65536, 256)?,
            original.snapshot(),
        )?;
        let version =
            VersionContext::select(project, Checkout::NonGit, selected_version, None, "")?;
        let content = self.policy.content();
        let limits = graphit_core::native_document_file::NativeDocumentFileLimits::new(
            content.max_input_bytes,
            graphit_core::native_path::MAX_PATH_BYTES,
            content.max_depth,
            content.max_scalar_bytes,
        )?;
        let relative = graphit_core::local_corpus::resolve(
            &scope,
            version.version(),
            "ast",
            "source.lance",
            limits,
            original.snapshot(),
        )?;
        let path = scope.runtime_root().join(relative);
        let branch = version.lance_branch();
        Ok(BranchSnapshot::capture_ast_corpus(
            &path,
            (!branch.is_empty()).then_some(branch.as_str()),
            original.snapshot(),
        )
        .await?)
    }

    async fn nn_rows_are_current(
        &self,
        project: &ProjectPaths,
        selected_version: &str,
        batches: &[RecordBatch],
    ) -> TestResult<bool> {
        let raw = self.raw_for_version(project, selected_version).await?;
        let cancel = Cancellation::default();
        let original = self.policy.request_context(Instant::now(), &cancel)?;
        let model = expected_native_nn_model(&self.expected)?;
        let mut files = std::collections::BTreeMap::new();
        for batch in batches {
            validate_native_nn(batch, &model, original.snapshot())?;
            let paths = batch
                .column_by_name("path")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let versions = batch
                .column_by_name("raw_version")
                .unwrap()
                .as_any()
                .downcast_ref::<UInt64Array>()
                .unwrap();
            let hashes = batch
                .column_by_name("full_source_sha256")
                .unwrap()
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .unwrap();
            for row in 0..batch.num_rows() {
                let hash = hashes.value(row);
                if let Some(previous) = files.insert((paths.value(row), versions.value(row)), hash)
                {
                    assert_eq!(
                        previous, hash,
                        "one FILE/Raw pin must have one Source digest"
                    );
                }
            }
        }
        let mut current = true;
        for ((path, version), hash) in files {
            let source = match fs::read_to_string(project.root().join(path)) {
                Ok(source) => source,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
                Err(error) => return Err(error.into()),
            };
            let digest = hash
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            current &=
                file_is_current(&raw, path, version, &digest, &source, original.snapshot()).await?;
        }
        Ok(current)
    }

    async fn native_nn_rows(
        &self,
        project: &ProjectPaths,
        version: &VersionContext,
    ) -> TestResult<Option<Vec<RecordBatch>>> {
        let id = ProjectIdentityStore::at(project).load()?.unwrap().id();
        let cancellation = Cancellation::default();
        let original = self.policy.request_context(Instant::now(), &cancellation)?;
        let installation = CapturedPublicationScope::installation(
            &self.installation,
            self.identity.unit_id().clone(),
            [
                self.installation.root().to_owned(),
                project.root().to_owned(),
            ],
            ScopeLimits::new(16, 65536, 256)?,
            original.snapshot(),
        )?;
        let installation_record = installation.binding()?;
        let captured = CapturedPublicationScope::project(
            project,
            id,
            ScopeLimits::new(16, 65536, 256)?,
            original.snapshot(),
        )?;
        let project_record = captured.binding()?;
        let content = self.policy.content();
        let native = graphit_core::native_document_file::NativeDocumentFileLimits::new(
            content.max_input_bytes,
            graphit_core::native_path::MAX_PATH_BYTES,
            content.max_depth,
            content.max_scalar_bytes,
        )?;
        let nn = open_local_native_nn_index(
            ScopeCapture {
                live: &installation,
                record: &installation_record,
            },
            ProjectCapture {
                scope: ScopeCapture {
                    live: &captured,
                    record: &project_record,
                },
                version,
            },
            original.snapshot(),
            native,
        )
        .await?;
        let NativeNnIndexPresence::Available(nn) = nn else {
            return Ok(None);
        };
        let snapshot = nn.snapshot(original.snapshot())?;
        let batches = snapshot
            .scan()
            .try_into_stream()
            .await?
            .try_collect::<Vec<_>>()
            .await?;
        nn.revalidate(original.snapshot())?;
        original.check()?;
        Ok(Some(batches))
    }

    async fn wait_rows_for(
        &self,
        project: &ProjectPaths,
        after: u64,
        matches: impl Fn(&[RecordBatch]) -> bool,
    ) -> TestResult<(u64, Vec<RecordBatch>)> {
        self.wait_rows_for_version(project, "", after, matches)
            .await
    }

    async fn wait_rows_for_version(
        &self,
        project: &ProjectPaths,
        selected_version: &str,
        after: u64,
        matches: impl Fn(&[RecordBatch]) -> bool,
    ) -> TestResult<(u64, Vec<RecordBatch>)> {
        let expires = Instant::now() + Duration::from_secs(30);
        loop {
            match self.rows_for_version(project, selected_version).await {
                Ok((raw, batches)) => {
                    let current = self
                        .nn_rows_are_current(project, selected_version, &batches)
                        .await?;
                    if raw > after && current && matches(&batches) {
                        return Ok((raw, batches));
                    }
                    assert!(
                        Instant::now() < expires,
                        "NativeNN did not publish the required current project image; project={}, raw={raw}, previous={after}, all_current={current}",
                        project.root().display()
                    );
                }
                Err(error) => {
                    if Instant::now() >= expires {
                        return Err(error);
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn wait_current_rows(&self, after: u64, pair_key: &str) -> TestResult<Vec<RecordBatch>> {
        Ok(self
            .wait_rows_for(&self.project, after, |rows| {
                has_named_row(rows, "answer.json", "answer", pair_key)
            })
            .await?
            .1)
    }

    async fn run(&self, scenario: impl std::future::Future<Output = TestResult>) -> TestResult {
        self.run_projects(&[&self.project], |_| scenario).await
    }

    async fn run_projects<F>(
        &self,
        projects: &[&ProjectPaths],
        scenario: impl FnOnce(std::net::SocketAddr) -> F,
    ) -> TestResult
    where
        F: std::future::Future<Output = TestResult>,
    {
        let startup_cancel = Cancellation::default();
        let startup = self
            .policy
            .request_context(Instant::now(), &startup_cancel)?;
        let inputs = projects
            .iter()
            .map(|project| ProjectBootstrapInput {
                project,
                explicit_version: "",
                pin: None,
                detached_base: "",
            })
            .collect::<Vec<_>>();
        let owner = match ForegroundSource::start_and_publish(
            &self.installation,
            &inputs,
            0,
            &self.config,
            NativeBinding::Codex,
            self.environment()?,
            &startup,
        )
        .await
        {
            Ok(owner) => owner,
            Err(mut failure) => {
                if let Some(owner) = failure.owner.take()
                    && let Err(mut cleanup) = owner.shutdown().await
                {
                    failure.owner = cleanup.owner.take();
                    failure.runtime = cleanup.runtime.take();
                    failure.service = Some(cleanup.error);
                }
                return Err(failure.into());
            }
        };
        let address = owner.address().ok_or("foreground listener absent")?;
        let scenario = scenario(address);
        let signal = Cancellation::default();
        let worker_signal = signal.clone();
        let running = tokio::spawn(async move { owner.run(&worker_signal).await });
        let outcome = std::panic::AssertUnwindSafe(scenario).catch_unwind().await;
        signal.cancel();
        let shutdown = running.await?;
        match outcome {
            Ok(result) => result?,
            Err(panic) => std::panic::resume_unwind(panic),
        }
        shutdown?;
        Ok(())
    }

    async fn graph(
        &self,
        address: std::net::SocketAddr,
        route: &str,
        body: &serde_json::Value,
    ) -> TestResult<(reqwest::StatusCode, serde_json::Value)> {
        let cancellation = Cancellation::default();
        let original = self.policy.request_context(Instant::now(), &cancellation)?;
        let content = self.policy.content();
        let limits = graphit_core::native_document_file::NativeDocumentFileLimits::new(
            content.max_input_bytes,
            graphit_core::native_path::MAX_PATH_BYTES,
            content.max_depth,
            content.max_scalar_bytes,
        )?;
        let headers =
            graphit_app::ast_source::runtime_headers(&self.installation, limits, &original).await?;
        let response = reqwest::Client::builder()
            .no_proxy()
            .build()?
            .post(format!("http://{address}/api/graphs/project/{route}"))
            .timeout(
                original
                    .remaining()?
                    .ok_or("finite graph request deadline")?,
            )
            .headers(headers.into_iter().collect())
            .json(body)
            .send()
            .await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        original.check()?;
        let value = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            serde_json::Value::String(String::from_utf8_lossy(&bytes).into_owned())
        });
        Ok((status, value))
    }

    async fn graph_ready(
        &self,
        address: std::net::SocketAddr,
        selection: &serde_json::Value,
        required: &[(&str, u64)],
    ) -> TestResult<serde_json::Value> {
        let expires = Instant::now() + Duration::from_secs(30);
        loop {
            let (status, result) = self.graph(address, "schema", selection).await?;
            if status == reqwest::StatusCode::NOT_FOUND && Instant::now() < expires {
                tokio::time::sleep(Duration::from_millis(20)).await;
                continue;
            }
            assert_eq!(status, reqwest::StatusCode::OK, "Graph schema: {result}");
            let nodes = result["nodes"]
                .as_array()
                .ok_or("Graph node schema missing")?;
            let ready = required.iter().all(|&(label, minimum)| {
                nodes.iter().any(|node| {
                    node["label"] == label
                        && node["rows"]
                            .as_str()
                            .and_then(|count| count.parse::<u64>().ok())
                            .is_some_and(|count| count >= minimum)
                })
            });
            if ready {
                return Ok(result);
            }
            if Instant::now() >= expires {
                return Err(format!(
                    "Graph did not publish required FILEs/tables {required:?}: {result}"
                )
                .into());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn control(
        &self,
        command: graphit_app::foreground_project_control::client::Command,
    ) -> TestResult<serde_json::Value> {
        let cancellation = Cancellation::default();
        let original = self.policy.request_context(Instant::now(), &cancellation)?;
        let mut bytes = Vec::new();
        let command = graphit_app::foreground_project_control::client::prepare(
            graphit_app::foreground_project_control::client::Arguments { command },
        )?;
        let status = graphit_app::foreground_project_control::client::run(
            command,
            &self.installation,
            &self.policy,
            &original,
            &mut bytes,
        )
        .await?;
        let response: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(
            status,
            std::process::ExitCode::SUCCESS,
            "control: {response}"
        );
        assert_eq!(response["phase"], "complete", "control: {response}");
        Ok(response)
    }

    async fn graph_observations(
        &self,
        address: std::net::SocketAddr,
        project: &ProjectPaths,
        version: Option<&str>,
        paths: &[&str],
        raw: u64,
    ) -> TestResult {
        let input = json!({"selection":{
            "project_dir":graphit_core::native_path::NativePath::new(project.root().to_owned())?,
            "project_version":version
        }, "query":{"query":"MATCH (n:AstFileObservation) RETURN n.path,n.raw_version,n.source_sha256 ORDER BY n.path"}});
        let expires = Instant::now() + Duration::from_secs(30);
        loop {
            let (status, result) = self.graph(address, "query", &input).await?;
            if status == reqwest::StatusCode::NOT_FOUND && Instant::now() < expires {
                tokio::time::sleep(Duration::from_millis(20)).await;
                continue;
            }
            assert_eq!(
                status,
                reqwest::StatusCode::OK,
                "Graph observation: {result}"
            );
            let rows = result["rows"]
                .as_array()
                .ok_or("Graph observation rows missing")?;
            if rows.len() == paths.len()
                && rows
                    .iter()
                    .zip(paths)
                    .all(|(row, path)| row[0]["value"] == *path)
            {
                let current_raw = self.raw_for_version(project, version.unwrap_or("")).await?;
                assert!(
                    current_raw.version() >= raw,
                    "Raw branch must retain the observed version"
                );
                let cancel = Cancellation::default();
                let original = self.policy.request_context(Instant::now(), &cancel)?;
                let mut current = true;
                for (row, path) in rows.iter().zip(paths) {
                    let version = row[1]["value"]
                        .as_str()
                        .ok_or("FILE Raw pin missing")?
                        .parse()?;
                    let digest = row[2]["value"]
                        .as_str()
                        .ok_or("FILE Source digest missing")?;
                    let source = fs::read_to_string(project.root().join(path))?;
                    current &= file_is_current(
                        &current_raw,
                        path,
                        version,
                        digest,
                        &source,
                        original.snapshot(),
                    )
                    .await?;
                }
                if current {
                    return Ok(());
                }
            }
            assert!(
                Instant::now() < expires,
                "Graph is not the current FILE image: raw={raw}, paths={paths:?}, result={result}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn graph_pair_data_key(
        &self,
        address: std::net::SocketAddr,
        project: &ProjectPaths,
        data_key: &str,
        raw: u64,
    ) -> TestResult {
        let input = json!({"selection":{"project_dir":
            graphit_core::native_path::NativePath::new(project.root().to_owned())?
        },"query":{"query":"MATCH (n:Pair) WHERE n.name = 'answer' RETURN n.data_key,n.raw_version,n.source_sha256"}});
        let expires = Instant::now() + Duration::from_secs(30);
        loop {
            let (status, result) = self.graph(address, "query", &input).await?;
            assert_eq!(status, reqwest::StatusCode::OK, "Pair query: {result}");
            let rows = result["rows"].as_array().ok_or("Pair rows missing")?;
            if rows.len() == 1 && rows[0][0]["value"] == data_key {
                let current_raw = self.raw_for_version(project, "").await?;
                assert!(
                    current_raw.version() >= raw,
                    "Raw branch must retain the observed version"
                );
                let cancel = Cancellation::default();
                let original = self.policy.request_context(Instant::now(), &cancel)?;
                let version = rows[0][1]["value"]
                    .as_str()
                    .ok_or("Pair Raw pin missing")?
                    .parse()?;
                let digest = rows[0][2]["value"]
                    .as_str()
                    .ok_or("Pair Source digest missing")?;
                let source = fs::read_to_string(project.root().join("answer.json"))?;
                if file_is_current(
                    &current_raw,
                    "answer.json",
                    version,
                    digest,
                    &source,
                    original.snapshot(),
                )
                .await?
                {
                    return Ok(());
                }
            }
            assert!(
                Instant::now() < expires,
                "profile event did not publish the current Graph Pair: {result}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a model-server executable and the pinned EmbeddingGemma GGUF for native AST inference"]
async fn initial_rust_json_callback_publishes_real_main_and_current_raw() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.run(fixture.assert_published()).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a model-server executable and the pinned EmbeddingGemma GGUF for native AST inference"]
async fn project_graph_http_queries_real_models_neighborhood_and_current_raw() -> TestResult {
    let fixture = Fixture::new()?;
    fs::write(
        fixture.project.root().join("relations.go"),
        "package demo\nfunc Gamma() int { return Delta() }\nfunc Delta() int { return 7 }\n",
    )?;
    fixture
        .run_projects(&[&fixture.project], |address| { let fixture = &fixture; async move {
            let (raw, _) = fixture
                .wait_rows_for(&fixture.project, 0, |rows| {
                    has_named_row(rows, "relations.go", "Gamma", "functions")
                        && has_named_row(rows, "relations.go", "Delta", "functions")
                })
                .await?;
            let selection = json!({"selection":{
                "project_dir": graphit_core::native_path::NativePath::new(fixture.project.root().to_owned())?
            }});
            let schema = fixture.graph_ready(address, &selection, &[("Function", 3), ("Pair", 1), ("AstFileObservation", 3)]).await?;
            let tables = schema["nodes"].as_array().ok_or("Graph node schema missing")?;
            for label in ["Function", "Pair", "AstFileObservation"] {
                assert!(tables.iter().any(|table| table["label"] == label));
            }
            assert!(tables.iter().all(|table| table["primary_key"] == "entity_key"));
            let unauthenticated = reqwest::Client::new()
                .post(format!("http://{address}/api/graphs/project/schema"))
                .json(&selection)
                .send()
                .await?;
            assert_eq!(unauthenticated.status(), reqwest::StatusCode::UNAUTHORIZED);

            let mut query = selection.clone();
            query["query"] = json!({"query":"MATCH (n:AstFileObservation) RETURN n.path,n.raw_version ORDER BY n.path"});
            let (status, observations) = fixture.graph(address, "query", &query).await?;
            assert_eq!(status, reqwest::StatusCode::OK, "{observations}");
            let rows = observations["rows"].as_array().ok_or("Graph query rows missing")?;
            assert_eq!(rows.len(), 3, "exactly one observation for each real FILE");
            fixture.graph_observations(address, &fixture.project, None, &["alpha.rs", "answer.json", "relations.go"], raw).await?;
            query["query"] = json!({"query":"MATCH (n:Function) WHERE n.name = 'Gamma' RETURN n.entity_key,n.raw_version"});
            let (status, function_result) = fixture.graph(address, "query", &query).await?;
            assert_eq!(status, reqwest::StatusCode::OK, "{function_result}");
            let functions = function_result["rows"].as_array().ok_or("Function rows missing")?;
            assert_eq!(functions.len(), 1);
            let gamma = functions[0][0]["value"].as_str().ok_or("Function primary key missing")?;
            let original_cancel = Cancellation::default();
            let original = fixture.policy.request_context(Instant::now(), &original_cancel)?;
            let retained_raw = fixture.raw_for_version(&fixture.project, "").await?;
            assert_current_source(&retained_raw, "relations.go", &functions[0][1]["value"], &fs::read_to_string(fixture.project.root().join("relations.go"))?, original.snapshot()).await?;

            let (status, sample) = fixture.graph(address, "sample", &selection).await?;
            assert_eq!(status, reqwest::StatusCode::OK, "{sample}");
            assert_eq!(sample["generation"], schema["generation"]);
            let delta = sample["nodes"].as_array().ok_or("sample nodes missing")?
                .iter().find(|node| node["label"] == "Function" && node["name"] == "Delta")
                .ok_or("real Delta absent from sample")?;
            assert!(sample["edges"].as_array().ok_or("sample edges missing")?
                .iter().any(|edge| edge["relationship"] == "CALLS" && edge["to"] == delta["id"]));
            let mut neighborhood = selection.clone();
            neighborhood["neighborhood"] = json!({
                "anchor_label":"Function", "anchor_key":{"type":"string","value":gamma},
                "relationship":"CALLS", "other_label":"Function", "direction":"outgoing", "limit":10
            });
            let (status, page) = fixture.graph(address, "neighborhood", &neighborhood).await?;
            assert_eq!(status, reqwest::StatusCode::OK, "{page}");
            assert_eq!(page["generation"], schema["generation"]);
            assert_eq!(page["edges"].as_array().ok_or("neighborhood edges missing")?.len(), 1);
            assert!(page["nodes"].as_array().ok_or("neighborhood nodes missing")?
                .iter().any(|node| node["id"] == delta["id"]));
            let cancellation = Cancellation::default();
            let original = fixture.policy.request_context(Instant::now(), &cancellation)?;
            let content = fixture.policy.content();
            let limits = graphit_core::native_document_file::NativeDocumentFileLimits::new(
                content.max_input_bytes, graphit_core::native_path::MAX_PATH_BYTES,
                content.max_depth, content.max_scalar_bytes,
            )?;
            use graphit_app::graph_entry::{Command, Direction, KeyType, Neighborhood, Query, Selection};
            let anchor_key = gamma.to_owned();
            let commands = [
                (Command::Schema(Selection::default()), schema),
                (Command::Sample(Selection::default()), sample),
                (Command::Query(Query {
                    selection: Selection::default(),
                    query: "MATCH (n:Function) WHERE n.name = 'Gamma' RETURN n.entity_key,n.raw_version".into(),
                    parameters: Vec::new(), string_parameters: Vec::new(),
                }), function_result),
                (Command::Neighborhood(Neighborhood {
                    selection: Selection::default(), anchor_label: "Function".into(), anchor_key,
                    key_type: KeyType::String, relationship: "CALLS".into(), other_label: "Function".into(),
                    direction: Direction::Outgoing, limit: 10, cursor: None, cursor_file: None,
                }), page),
            ];
            for (command, expected) in commands {
                let mut cli_output = Vec::new();
                graphit_app::graph_entry::run(
                    &fixture.installation, &reqwest::Client::new(), command.prepare(content)?,
                    (limits, content), &original, &mut cli_output,
                ).await?;
                let mut result: serde_json::Value = serde_json::from_slice(&cli_output)?;
                let mut expected = expected;
                if expected.get("generation").is_some() {
                    // Separate HTTP/MCP operations capture their own real publication.
                    // A watcher refresh may republish identical content between them.
                    let pin: [u8; 32] = serde_json::from_value(result["generation"].clone())?;
                    if let Some(cursor) = result.get("next_cursor").filter(|value| !value.is_null()) {
                        assert_eq!(cursor["generation"], json!(pin));
                    }
                    result.as_object_mut().ok_or("graph response object missing")?.remove("generation");
                    expected.as_object_mut().ok_or("HTTP graph object missing")?.remove("generation");
                }
                assert_eq!(result, expected, "compiled CLI uses actual Source MCP/current PROJECT content");
            }
            Ok(())
        }})
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ast_only_publishes_current_graph_without_native_nn() -> TestResult {
    let fixture = Fixture::with_embedding(false)?;
    let fallback_source = "class Fallback { static int value() { return 7; } }\n";
    fs::write(
        fixture.project.root().join("fallback.java"),
        fallback_source,
    )?;
    fixture
        .run_projects(&[&fixture.project], |address| {
            let fixture = &fixture;
            async move {
                let selection = json!({"selection":{"project_dir":
                    graphit_core::native_path::NativePath::new(fixture.project.root().to_owned())?
                }});
                fixture.graph_ready(address, &selection, &[("Function", 2), ("AstFileObservation", 3)]).await?;
                let mut query = selection;
                query["query"] = json!({"query":"MATCH (n:Function) RETURN n.name,n.path,n.raw_version,n.source_sha256,n.raw_name,n.extent_status ORDER BY n.path,n.name"});
                let (status, result) = fixture.graph(address, "query", &query).await?;
                assert_eq!(status, reqwest::StatusCode::OK, "{result}");
                let rows = result["rows"].as_array().ok_or("Graph rows missing")?;
                assert_eq!(rows.len(), 2, "both specialized Rust and Java fallback must publish functions");
                let id = ProjectIdentityStore::at(&fixture.project)
                    .load()?
                    .unwrap()
                    .id();
                let root = fixture.project.project_runtime(id, &["ast"])?;
                let cancel = Cancellation::default();
                let original = fixture.policy.request_context(Instant::now(), &cancel)?;
                let raw = BranchSnapshot::capture_ast_corpus(&root.join("source.lance"), None, original.snapshot()).await?;
                for (row, (name, path, source)) in rows.iter().zip([
                    ("alpha", "alpha.rs", RUST),
                    ("value", "fallback.java", fallback_source),
                ]) {
                    assert_eq!(row[0]["value"], name);
                    assert_eq!(row[1]["value"], path);
                    assert_current_source(&raw, path, &row[2]["value"], source, original.snapshot()).await?;
                    assert_eq!(row[3]["value"], Sha256::digest(source.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect::<String>());
                    assert_eq!(row[4]["value"], name);
                    assert_eq!(row[5]["value"], "Exact");
                }
                // Available models do not emit model_unavailable_reason. Verify
                // their actual capability records rather than querying an absent property.
                query["query"] = json!({"query":"MATCH (n:AstFileObservation) WHERE n.path = 'fallback.java' RETURN n.model_available,n.capabilities_json,n.diagnostics_json,n.raw_version,n.source_sha256,n.source_stored,n.source_empty,n.lang,n.grammar"});
                let (status, observation) = fixture.graph(address, "query", &query).await?;
                assert_eq!(status, reqwest::StatusCode::OK, "{observation}");
                let observations = observation["rows"].as_array().ok_or("Graph observation missing")?;
                assert_eq!(observations.len(), 1);
                let available = &observations[0];
                assert_eq!(available[0]["value"], true);
                let capabilities: Value = serde_json::from_str(available[1]["value"].as_str().ok_or("Capabilities JSON missing")?)?;
                let capabilities = capabilities.as_array().ok_or("Capabilities array missing")?;
                assert!(!capabilities.is_empty());
                for (index, capability) in capabilities.iter().enumerate() {
                    assert_eq!(capability["query_index"], index);
                    assert_eq!(capability["adapter"], "GenericSemanticDocument");
                    assert!(matches!(capability["query_kind"].as_str(), Some("Entity" | "Relation")));
                    assert!(capability["matches"].is_u64());
                    assert!(capability["policy"].is_object());
                    assert!(capability.get("reason").is_none());
                }
                for label in ["Class", "Function"] {
                    let matching: Vec<_> = capabilities.iter().filter(|capability| capability["policy"]["graph_label"] == label).collect();
                    assert_eq!(matching.len(), 1, "expected one {label} extraction query");
                    assert_eq!(matching[0]["query_kind"], "Entity");
                    assert_eq!(matching[0]["matches"], 1, "Java fallback must execute {label} extraction");
                }
                let diagnostics: Value = serde_json::from_str(available[2]["value"].as_str().ok_or("Diagnostics JSON missing")?)?;
                assert_eq!(diagnostics, json!([]), "valid Java fallback must not publish unavailable-model or recovery diagnostics");
                assert_eq!(available[3]["value"], rows[1][2]["value"], "Java entity and FILE must agree on the Raw pin");
                assert_eq!(available[4]["value"], Sha256::digest(fallback_source.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect::<String>());
                assert_eq!(available[5]["value"], true);
                assert_eq!(available[6]["value"], false);
                assert_eq!(available[7]["value"], "java");
                assert_eq!(available[8]["value"], "tree-sitter-java");
                fs::write(fixture.project.root().join("later.rs"), "pub fn later() {}")?;
                fixture.graph_observations(address, &fixture.project, None,
                    &["alpha.rs", "answer.json", "fallback.java", "later.rs"], raw.version()).await?;
                let updated_raw = fixture.raw_for_version(&fixture.project, "").await?;
                assert!(updated_raw.version() > raw.version());
                assert!(file_is_current(&updated_raw, "alpha.rs",
                    rows[0][2]["value"].as_str().ok_or("Rust Raw pin missing")?.parse()?,
                    rows[0][3]["value"].as_str().ok_or("Rust Source digest missing")?,
                    RUST, original.snapshot()).await?,
                    "an unchanged FILE keeps a valid historical pin after another FILE advances Raw");
                let corpus = BranchSnapshot::capture_observed(root.join("corpus.lance").to_str().unwrap(), None, None).await?;
                graphit_ast::corpus::validate_schema(corpus.schema())?;
                assert!(graphit_ast::corpus::model(corpus.schema())?.is_none(),
                    "the shared lexical corpus must not contain a model or vector column when embedding is disabled");
                Ok(())
            }
        })
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a model-server executable and the pinned EmbeddingGemma GGUF for native AST inference"]
async fn generic_unavailable_projections_preserve_graph_and_publish_only_eligible_rows()
-> TestResult {
    let fixture = Fixture::new()?;
    let queries = fixture.project.brand_directory().join("ast/queries");
    fs::create_dir_all(&queries)?;
    fs::write(
        queries.join("yaml.yaml"),
        r#"language: yaml
grammar: tree-sitter-yaml
extensions: [".yaml", ".yml"]
embed_labels: [Mapping]
queries:
  - graph_label: Mapping
    data_key: mappings
    pattern: '(block_mapping_pair key: (flow_node (double_quote_scalar) @name)) @unit'
    span_capture: unit
    name_is_data: true
  - graph_label: Mapping
    data_key: mappings
    pattern: '(block_mapping_pair key: (flow_node (plain_scalar) @name) value: (flow_node) @value)'
    value_capture: value
    value_label: Value
  - graph_label: Mapping
    data_key: mappings
    pattern: '(block_mapping_pair key: (flow_node (plain_scalar) @name))'
"#,
    )?;
    fixture
        .run_projects(&[&fixture.project], |address| {
            let fixture = &fixture;
            async move {
                fixture.assert_published().await?;
                let (before, _) = fixture.rows().await?;
                let path = "mixed.yaml";
                fs::write(
                    fixture.project.root().join(path),
                    "\"keep\": 1\nmissing:\n  nested: 2\nplain: 3\n",
                )?;
                let (raw, rows) = fixture
                    .wait_rows_for(&fixture.project, before, |rows| {
                        has_named_row(rows, path, "\"keep\"", "mappings")
                            && has_named_row(rows, "alpha.rs", "alpha", "functions")
                    })
                    .await?;
                let generic_rows = rows
                    .iter()
                    .map(|batch| {
                        let paths = batch
                            .column_by_name("path")
                            .unwrap()
                            .as_any()
                            .downcast_ref::<StringArray>()
                            .unwrap();
                        (0..batch.num_rows())
                            .filter(|&row| paths.value(row) == path)
                            .count()
                    })
                    .sum::<usize>();
                assert_eq!(generic_rows, 1, "only the proven native extent is eligible");
                fixture
                    .graph_observations(
                        address,
                        &fixture.project,
                        None,
                        &["alpha.rs", "answer.json", path],
                        raw,
                    )
                    .await?;
                let query = json!({
                    "selection": {"project_dir": graphit_core::native_path::NativePath::new(fixture.project.root().to_owned())?},
                    "query": {"query": "MATCH (n:Mapping) WHERE n.path = 'mixed.yaml' RETURN n.name,n.available,n.extent_status,n.metadata_json"}
                });
                let (status, graph) = fixture.graph(address, "query", &query).await?;
                assert_eq!(status, reqwest::StatusCode::OK, "{graph}");
                let mappings = graph["rows"].as_array().ok_or("missing Graph mappings")?;
                assert_eq!(mappings.len(), 4, "NN eligibility must not truncate Graph");
                for (name, available, extent) in [
                    ("keep", true, "Exact"),
                    ("missing", false, "Unavailable"),
                    ("plain", false, "Conflict"),
                ] {
                    let row = mappings
                        .iter()
                        .find(|row| row[0]["value"] == name)
                        .ok_or_else(|| format!("missing Graph entity {name}: {graph}"))?;
                    assert_eq!(row[1]["value"], available, "{name}");
                    assert_eq!(row[2]["value"], extent, "{name}");
                    let metadata: Value = serde_json::from_str(
                        row[3]["value"].as_str().ok_or("missing native metadata")?,
                    )?;
                    assert!(metadata["contributions"].as_array().is_some_and(|v| !v.is_empty()));
                    if name == "plain" {
                        assert_eq!(metadata["conflicts"], "[ConflictingName]");
                    }
                }
                fs::write(
                    fixture.project.root().join(path),
                    "missing:\n  nested: 2\nplain: 3\n",
                )?;
                let (next_raw, _) = fixture
                    .wait_rows_for(&fixture.project, raw, |rows| {
                        !contains_path(rows, path)
                            && has_named_row(rows, "alpha.rs", "alpha", "functions")
                            && has_named_row(rows, "answer.json", "answer", "pairs")
                    })
                    .await?;
                fixture
                    .graph_observations(
                        address,
                        &fixture.project,
                        None,
                        &["alpha.rs", "answer.json", path],
                        next_raw,
                    )
                    .await?;
                Ok(())
            }
        })
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a model-server executable and the pinned EmbeddingGemma GGUF for native AST inference"]
async fn corrupt_model_asset_keeps_complete_graph_and_empty_vector_snapshot() -> TestResult {
    let fixture = Fixture::new()?;
    let f = &fixture;
    fixture.run_projects(&[&fixture.project], |address| {
        // Corrupt the owned GGUF before the worker starts so this failure has no race.
        // The graph must remain available when model integrity validation fails.
        let damaged = fs::write(f._model.directory.path().join("model.gguf"), b"invalid GGUF asset");
        async move {
            damaged?;
            let selection = json!({"selection":{"project_dir":
                graphit_core::native_path::NativePath::new(f.project.root().to_owned())?
            }});
            let schema = f.graph_ready(address, &selection, &[("Function", 1), ("AstFileObservation", 2)]).await?;
            assert!(schema["nodes"].as_array().ok_or("Graph tables missing")?
                .iter().any(|table| table["label"] == "Function"));
            let mut query = selection;
            query["query"] = json!({"query":"MATCH (n:AstFileObservation) RETURN n.path,n.raw_version ORDER BY n.path"});
            let (status, result) = f.graph(address, "query", &query).await?;
            assert_eq!(status, reqwest::StatusCode::OK, "{result}");
            let rows = result["rows"].as_array().ok_or("Graph observations missing")?;
            assert_eq!(rows.len(), 2, "failure of NN must not truncate Graph's FILE stream");
            assert_eq!(rows[0][0]["value"], "alpha.rs");
            assert_eq!(rows[1][0]["value"], "answer.json");
            let id = ProjectIdentityStore::at(&f.project).load()?.unwrap().id();
            let root = f.project.project_runtime(id, &["ast"])?;
            let cancel = Cancellation::default();
            let original = f.policy.request_context(Instant::now(), &cancel)?;
            let raw = BranchSnapshot::capture_ast_corpus(&root.join("source.lance"), None, original.snapshot()).await?;
            for (row, (path, text)) in rows.iter().zip([("alpha.rs", RUST), ("answer.json", JSON)]) {
                assert_current_source(&raw, path, &row[1]["value"], text, original.snapshot()).await?;
            }
            let corpus = BranchSnapshot::capture_observed(root.join("corpus.lance").to_str().unwrap(), None, None).await?;
            graphit_ast::corpus::validate_schema(corpus.schema())?;
            if corpus.schema().field_with_name(graphit_ast::corpus::VECTOR).is_ok() {
                let mut scan = corpus.scan();
                scan.filter_expr(graphit_data::lance_branch_store::col(graphit_ast::corpus::VECTOR).is_not_null());
                assert_eq!(scan.count_rows().await?, 0, "the graph-ready snapshot must contain no vectors for the corrupted asset");
            }
            Ok(())
        }
    }).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a model-server executable and the pinned EmbeddingGemma GGUF for native AST inference"]
async fn maintain_fresh_project_switch_version_and_retire_sibling_keeps_real_jobs() -> TestResult {
    use graphit_app::foreground_project_control::client::Command;
    let fixture = Fixture::new()?;
    let root = fixture._directory.path().join("maintained-later");
    fs::create_dir(&root)?;
    let sibling = ProjectPaths::at(&root, &fixture.installation)?;
    let sibling_id = ProjectIdentityStore::at(&sibling)
        .writer(Duration::from_secs(5), &Cancellation::default())?
        .ensure(&ProjectMetadata::new("maintained-after-startup")?)?
        .id();
    fs::write(root.join("beta.rs"), "pub fn Beta() {}")?;
    fixture
        .run_projects(&[&fixture.project], |address| {
            let fixture = &fixture;
            async move {
                fixture.assert_published().await?;
                let (a_raw, a_rows) = fixture.rows().await?;
                let a_vectors = vectors(&a_rows);
                fixture
                    .control(Command::MaintainCheckout {
                        project_id: sibling_id.to_string(),
                        project_dir: root.clone(),
                        version: String::new(),
                        detached_base: String::new(),
                        select: true,
                    })
                    .await?;
                let (main_raw, main_rows) = fixture
                    .wait_rows_for(&sibling, 0, |rows| {
                        has_named_row(rows, "beta.rs", "Beta", "functions")
                    })
                    .await?;
                fixture
                    .graph_observations(address, &sibling, None, &["beta.rs"], main_raw)
                    .await?;
                fixture
                    .graph_observations(
                        address,
                        &fixture.project,
                        None,
                        &["alpha.rs", "answer.json"],
                        a_raw,
                    )
                    .await?;
                let main_vectors = vectors(&main_rows);
                let version_name = "feature/graph";
                fixture
                    .control(Command::MaintainCheckout {
                        project_id: sibling_id.to_string(),
                        project_dir: root.clone(),
                        version: version_name.into(),
                        detached_base: String::new(),
                        select: true,
                    })
                    .await?;
                fs::write(root.join("versioned.rs"), "pub fn Versioned() {}")?;
                let (branch_raw, _) = fixture
                    .wait_rows_for_version(&sibling, version_name, 0, |rows| {
                        has_named_row(rows, "beta.rs", "Beta", "functions")
                            && has_named_row(rows, "versioned.rs", "Versioned", "functions")
                    })
                    .await?;
                fixture
                    .graph_observations(
                        address,
                        &sibling,
                        Some(version_name),
                        &["beta.rs", "versioned.rs"],
                        branch_raw,
                    )
                    .await?;
                let (retained_main_raw, retained_main_rows) = fixture.rows_for(&sibling).await?;
                assert_eq!(
                    retained_main_raw, main_raw,
                    "named checkout must not overwrite Raw Main"
                );
                assert_eq!(
                    vectors(&retained_main_rows),
                    main_vectors,
                    "named checkout must not overwrite NN Main"
                );
                assert!(!contains_path(&retained_main_rows, "versioned.rs"));
                let a_id = ProjectIdentityStore::at(&fixture.project)
                    .load()?
                    .unwrap()
                    .id();
                let retired = fixture
                    .control(Command::RetireCheckout {
                        project_id: a_id.to_string(),
                        project_dir: fixture.project.root().to_owned(),
                    })
                    .await?;
                assert_eq!(retired["checkout_removed"], true);
                fs::write(
                    fixture.project.root().join("ignored.rs"),
                    "pub fn Ignored() {}",
                )?;
                fs::write(root.join("survivor.rs"), "pub fn Survivor() {}")?;
                let (current, _) = fixture
                    .wait_rows_for_version(&sibling, version_name, branch_raw, |rows| {
                        has_named_row(rows, "survivor.rs", "Survivor", "functions")
                            && has_named_row(rows, "versioned.rs", "Versioned", "functions")
                    })
                    .await?;
                fixture
                    .graph_observations(
                        address,
                        &sibling,
                        Some(version_name),
                        &["beta.rs", "survivor.rs", "versioned.rs"],
                        current,
                    )
                    .await?;
                let (retained_a_raw, retained_a_rows) = fixture.rows().await?;
                assert_eq!(
                    retained_a_raw, a_raw,
                    "retired A cannot admit new Raw while B runs"
                );
                assert_eq!(vectors(&retained_a_rows), a_vectors);
                assert!(!contains_path(&retained_a_rows, "ignored.rs"));
                Ok(())
            }
        })
        .await
}

fn vectors(batches: &[RecordBatch]) -> std::collections::BTreeMap<(String, String), Vec<f32>> {
    let mut result = std::collections::BTreeMap::new();
    for batch in batches {
        let ids = batch
            .column_by_name("uid")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let keys = batch
            .column_by_name("data_key")
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
        for i in 0..batch.num_rows() {
            let vector = values.value(i);
            let vector = vector.as_any().downcast_ref::<Float32Array>().unwrap();
            assert!(
                result
                    .insert(
                        (ids.value(i).into(), keys.value(i).into()),
                        vector.values().to_vec()
                    )
                    .is_none(),
                "one observed NN row per actual UID/data_key"
            );
        }
    }
    result
}

fn has_named_row(batches: &[RecordBatch], path: &str, name: &str, key: &str) -> bool {
    batches.iter().any(|batch| {
        let strings = |column: &str| {
            batch
                .column_by_name(column)
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
        };
        (0..batch.num_rows()).any(|row| {
            strings("path").value(row) == path
                && strings("name").value(row) == name
                && strings("data_key").value(row) == key
        })
    })
}

fn contains_path(batches: &[RecordBatch], path: &str) -> bool {
    batches.iter().any(|batch| {
        let paths = batch
            .column_by_name("path")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        (0..batch.num_rows()).any(|row| paths.value(row) == path)
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a model-server executable and the pinned EmbeddingGemma GGUF for native AST inference"]
async fn current_raw_preserves_vectors_then_profile_control_refreshes_real_nn() -> TestResult {
    let fixture = Fixture::new()?;
    fixture
        .run_projects(&[&fixture.project], |address| {
            let fixture = &fixture;
            async move {
                fixture.assert_published().await?;
                let (before, old) = fixture.rows().await?;
                let old_vectors = vectors(&old);

                fs::write(fixture.project.root().join("empty.json"), "{}")?;
                let reassociated = fixture.wait_current_rows(before, "pairs").await?;
                assert_eq!(
                    vectors(&reassociated),
                    old_vectors,
                    "only Source/Raw association changes for unchanged entities"
                );

                let directory = fixture.project.brand_directory().join("ast/queries");
                fs::create_dir_all(&directory)?;
                let stock = include_str!("../../../crates/graphit-ast/profiles/queries/json.yaml");
                let profile = directory.join("json.yaml");
                fs::write(
                    &profile,
                    stock.replace("data_key: pairs", "data_key: refreshed_pairs"),
                )?;
                fs::write(fixture.project.root().join("beta.rs"), "pub fn beta() {}")?;
                let changed = r#"{"answer":43}"#;
                fs::write(fixture.project.root().join("answer.json"), changed)?;
                let current = fixture.wait_current_rows(before, "refreshed_pairs").await?;
                let (raw, _) = fixture.rows().await?;
                fixture
                    .graph_observations(
                        address,
                        &fixture.project,
                        None,
                        &["alpha.rs", "answer.json", "beta.rs", "empty.json"],
                        raw,
                    )
                    .await?;
                fixture
                    .graph_pair_data_key(address, &fixture.project, "refreshed_pairs", raw)
                    .await?;
                assert!(
                    current.iter().any(|batch| {
                        let names = batch
                            .column_by_name("name")
                            .unwrap()
                            .as_any()
                            .downcast_ref::<StringArray>()
                            .unwrap();
                        let paths = batch
                            .column_by_name("path")
                            .unwrap()
                            .as_any()
                            .downcast_ref::<StringArray>()
                            .unwrap();
                        (0..batch.num_rows())
                            .any(|i| names.value(i) == "beta" && paths.value(i) == "beta.rs")
                    }),
                    "the same watcher applies the profile control and publishes the new FILE"
                );
                Ok(())
            }
        })
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a model-server executable and the pinned EmbeddingGemma GGUF for native AST inference"]
async fn incremental_go_callback_publishes_real_model_vectors_and_current_raw() -> TestResult {
    let fixture = Fixture::new()?;
    fixture
        .run_projects(&[&fixture.project], |address| { let fixture = &fixture; async move {
            fixture.assert_published().await?;
            let (before, _) = fixture.rows().await?;
            let source = "package demo\ntype Runner interface { Run() int }\ntype Counter int\nconst Limit = 7\nvar Enabled = true\nfunc Gamma() int { return int(Limit) }\n";
            let expected = [
                ("Function", "Gamma", "functions"),
                ("Interface", "Runner", "interfaces"),
                ("Type", "Counter", "types"),
                ("Constant", "Limit", "constants"),
                ("Variable", "Enabled", "variables"),
            ];
            fs::write(fixture.project.root().join("gamma.go"), source)?;
            let model = expected_native_nn_model(&fixture.expected)?;
            let expires = Instant::now() + Duration::from_secs(30);
            let raw = loop {
                let (raw, batches) = fixture.rows().await?;
                let cancel = Cancellation::default();
                let request = fixture.policy.request_context(Instant::now(), &cancel)?;
                let mut found = std::collections::BTreeSet::new();
                let current = fixture.nn_rows_are_current(&fixture.project, "", &batches).await?;
                for batch in &batches {
                    validate_native_nn(batch, &model, request.snapshot())?;
                    let strings = |name: &str| {
                        batch.column_by_name(name).unwrap().as_any().downcast_ref::<StringArray>().unwrap()
                    };
                    let vectors = batch.column_by_name("vector").unwrap().as_any().downcast_ref::<FixedSizeListArray>().unwrap();
                    let hashes = batch.column_by_name("full_source_sha256").unwrap().as_any().downcast_ref::<FixedSizeBinaryArray>().unwrap();
                    for row in 0..batch.num_rows() {
                        if strings("path").value(row) != "gamma.go" { continue; }
                        assert_eq!(strings("language").value(row), "go");
                        assert!(source.contains(strings("projection").value(row)));
                        assert_eq!(hashes.value(row), Sha256::digest(source.as_bytes()).as_slice());
                        let vector = vectors.value(row);
                        let vector = vector.as_any().downcast_ref::<Float32Array>().unwrap();
                        assert_eq!(vector.len(), 768);
                        assert!(vector.values().iter().all(|value| value.is_finite()));
                        found.insert((strings("kind").value(row).to_owned(), strings("name").value(row).to_owned(), strings("data_key").value(row).to_owned()));
                    }
                }
                if raw > before && current && expected.iter().all(|(kind, name, key)| found.contains(&(kind.to_string(), name.to_string(), key.to_string()))) { break raw; }
                assert!(Instant::now() < expires, "incremental Go callback did not publish its genuine model and actual M vectors; raw={raw}, previous={before}, all_current={current}, found={found:?}");
                tokio::time::sleep(Duration::from_millis(20)).await;
            };
            fixture.graph_observations(address, &fixture.project, None, &["alpha.rs", "answer.json", "gamma.go"], raw).await?;
            Ok(())
        }})
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a model-server executable and the pinned EmbeddingGemma GGUF for native AST inference"]
async fn incremental_ecma_callbacks_publish_graphs_and_real_model_vectors() -> TestResult {
    let fixture = Fixture::new()?;
    fixture
        .run_projects(&[&fixture.project], |address| {
            let fixture = &fixture;
            async move {
                fixture.assert_published().await?;
                let (before, _) = fixture.rows().await?;
                let sources = [
                    ("script.js", "javascript", "ScriptFn", "function ScriptFn() { return 7; }\n"),
                    ("typed.ts", "typescript", "TypedFn", "function TypedFn(): number { return 8; }\n"),
                    ("view.tsx", "tsx", "ViewFn", "function ViewFn() { return <div>ready</div>; }\n"),
                ];
                for (path, _, _, source) in sources {
                    fs::write(fixture.project.root().join(path), source)?;
                }
                let (raw, batches) = fixture
                    .wait_rows_for(&fixture.project, before, |rows| {
                        sources.iter().all(|(path, _, name, _)| {
                            has_named_row(rows, path, name, "functions")
                        })
                    })
                    .await?;
                for batch in &batches {
                    let strings = |name: &str| {
                        batch.column_by_name(name).unwrap().as_any()
                            .downcast_ref::<StringArray>().unwrap()
                    };
                    let vectors = batch.column_by_name("vector").unwrap().as_any()
                        .downcast_ref::<FixedSizeListArray>().unwrap();
                    let hashes = batch.column_by_name("full_source_sha256").unwrap().as_any()
                        .downcast_ref::<FixedSizeBinaryArray>().unwrap();
                    for row in 0..batch.num_rows() {
                        let Some((_, language, _, source)) = sources.iter()
                            .find(|(path, _, _, _)| *path == strings("path").value(row))
                        else { continue; };
                        assert_eq!(strings("language").value(row), *language);
                        assert_eq!(strings("name_kind").value(row), "Identifier");
                        assert!(source.contains(strings("projection").value(row)));
                        assert_eq!(hashes.value(row), Sha256::digest(source.as_bytes()).as_slice());
                        let vector = vectors.value(row);
                        let vector = vector.as_any().downcast_ref::<Float32Array>().unwrap();
                        assert_eq!(vector.len(), 768);
                        assert!(vector.values().iter().all(|value| value.is_finite()));
                    }
                }
                fixture.graph_observations(address, &fixture.project, None,
                    &["alpha.rs", "answer.json", "script.js", "typed.ts", "view.tsx"], raw).await?;
                let selection = json!({
                    "selection":{"project_dir": graphit_core::native_path::NativePath::new(fixture.project.root().to_owned())?},
                    "query":{"query":"MATCH (n:Function) RETURN n.name,n.path"}
                });
                let (status, result) = fixture.graph(address, "query", &selection).await?;
                assert_eq!(status, reqwest::StatusCode::OK, "{result}");
                for (_, _, name, _) in sources {
                    assert!(result["rows"].as_array().unwrap().iter()
                        .any(|row| row[0]["value"] == name));
                }
                fs::write(fixture.project.root().join("script.js"),
                    "function UpdatedScriptFn() { return 9; }\n")?;
                let (updated, _) = fixture.wait_rows_for(&fixture.project, raw, |rows| {
                    has_named_row(rows, "script.js", "UpdatedScriptFn", "functions")
                        && !has_named_row(rows, "script.js", "ScriptFn", "functions")
                }).await?;
                fixture.graph_observations(address, &fixture.project, None,
                    &["alpha.rs", "answer.json", "script.js", "typed.ts", "view.tsx"], updated).await?;
                Ok(())
            }
        })
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a model-server executable and the pinned EmbeddingGemma GGUF for native AST inference"]
async fn two_projects_refresh_real_nn_and_publish_rename_delete() -> TestResult {
    let fixture = Fixture::new()?;
    let root = fixture._directory.path().join("sibling");
    fs::create_dir(&root)?;
    let sibling = ProjectPaths::at(&root, &fixture.installation)?;
    ProjectIdentityStore::at(&sibling)
        .writer(Duration::from_secs(5), &Cancellation::default())?
        .ensure(&ProjectMetadata::new("actual-AST-NN-sibling")?)?;
    fs::write(root.join("alpha.rs"), RUST)?;
    fs::write(root.join("answer.json"), JSON)?;
    fixture
        .run_projects(&[&fixture.project, &sibling], |address| {
            let fixture = &fixture;
            let sibling = &sibling;
            async move {
                fixture.assert_published().await?;
                let (before_a, _) = fixture.rows().await?;
                let (before_b, _) = fixture
                    .wait_rows_for(sibling, 0, |rows| {
                        has_named_row(rows, "answer.json", "answer", "pairs")
                            && has_named_row(rows, "alpha.rs", "alpha", "functions")
                    })
                    .await?;
                let directory = fixture.project.brand_directory().join("ast/queries");
                fs::create_dir_all(&directory)?;
                let stock = include_str!("../../../crates/graphit-ast/profiles/queries/json.yaml");
                let profile = directory.join("json.yaml");
                fs::write(
                    &profile,
                    stock.replace(
                        "data_key: pairs",
                        "data_key: staged_pairs",
                    ),
                )?;
                fs::write(fixture.project.root().join("beta.rs"), "pub fn beta() {}")?;
                let changed = r#"{"answer":77}"#;
                fs::write(fixture.project.root().join("answer.json"), changed)?;

                let bravo = "pub fn Bravo() -> u32 { 9 }";
                fs::write(sibling.root().join("bravo.rs"), bravo)?;
                let (updated_b, rows) = fixture
                    .wait_rows_for(sibling, before_b, |rows| {
                        has_named_row(rows, "bravo.rs", "Bravo", "functions")
                            && has_named_row(rows, "answer.json", "answer", "pairs")
                    })
                    .await?;
                for batch in &rows {
                    let paths = batch
                        .column_by_name("path")
                        .unwrap()
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .unwrap();
                    let hashes = batch
                        .column_by_name("full_source_sha256")
                        .unwrap()
                        .as_any()
                        .downcast_ref::<FixedSizeBinaryArray>()
                        .unwrap();
                    let projections = batch
                        .column_by_name("projection")
                        .unwrap()
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .unwrap();
                    for row in 0..batch.num_rows() {
                        if paths.value(row) == "bravo.rs" {
                            assert_eq!(
                                hashes.value(row),
                                Sha256::digest(bravo.as_bytes()).as_slice()
                            );
                            assert!(bravo.contains(projections.value(row)));
                        }
                    }
                }

                fixture
                    .graph_observations(
                        address,
                        sibling,
                        None,
                        &["alpha.rs", "answer.json", "bravo.rs"],
                        updated_b,
                    )
                    .await?;
                fixture.wait_rows_for(&fixture.project, before_a, |rows| {
                    has_named_row(rows, "answer.json", "answer", "staged_pairs")
                        && has_named_row(rows, "beta.rs", "beta", "functions")
                }).await?;
                fs::write(
                    &profile,
                    stock.replace("data_key: pairs", "data_key: refreshed_pairs"),
                )?;
                fixture
                    .wait_rows_for(&fixture.project, before_a, |rows| {
                        has_named_row(rows, "answer.json", "answer", "refreshed_pairs")
                            && has_named_row(rows, "beta.rs", "beta", "functions")
                    })
                    .await?;
                let (refreshed_a, _) = fixture.rows().await?;
                fixture
                    .graph_observations(
                        address,
                        &fixture.project,
                        None,
                        &["alpha.rs", "answer.json", "beta.rs"],
                        refreshed_a,
                    )
                    .await?;
                fixture
                    .graph_pair_data_key(address, &fixture.project, "refreshed_pairs", refreshed_a)
                    .await?;

                fs::rename(
                    sibling.root().join("bravo.rs"),
                    sibling.root().join("renamed.rs"),
                )?;
                let (renamed_b, _) = fixture
                    .wait_rows_for(sibling, updated_b, |rows| {
                        has_named_row(rows, "renamed.rs", "Bravo", "functions")
                            && !contains_path(rows, "bravo.rs")
                    })
                    .await?;
                fixture
                    .graph_observations(
                        address,
                        sibling,
                        None,
                        &["alpha.rs", "answer.json", "renamed.rs"],
                        renamed_b,
                    )
                    .await?;
                fs::remove_file(sibling.root().join("renamed.rs"))?;
                let (deleted_b, _) = fixture
                    .wait_rows_for(sibling, renamed_b, |rows| {
                        !contains_path(rows, "renamed.rs")
                            && !contains_path(rows, "bravo.rs")
                            && has_named_row(rows, "alpha.rs", "alpha", "functions")
                            && has_named_row(rows, "answer.json", "answer", "pairs")
                    })
                    .await?;

                fixture
                    .graph_observations(
                        address,
                        sibling,
                        None,
                        &["alpha.rs", "answer.json"],
                        deleted_b,
                    )
                    .await?;
                let (before_retirement, rows_a) = fixture.rows().await?;
                let vectors_a = vectors(&rows_a);
                let project_id = ProjectIdentityStore::at(&fixture.project)
                    .load()?
                    .unwrap()
                    .id();
                let cancel = Cancellation::default();
                let original = fixture.policy.request_context(Instant::now(), &cancel)?;
                let mut response = Vec::new();
                let command = graphit_app::foreground_project_control::client::prepare(
                    graphit_app::foreground_project_control::client::Arguments {
                        command:
                            graphit_app::foreground_project_control::client::Command::RetireCheckout {
                                project_id: project_id.to_string(),
                                project_dir: fixture.project.root().to_owned(),
                            },
                    },
                )?;
                let status = graphit_app::foreground_project_control::client::run(
                    command,
                    &fixture.installation,
                    &fixture.policy,
                    &original,
                    &mut response,
                )
                .await?;
                let response: serde_json::Value = serde_json::from_slice(&response)?;
                assert_eq!(
                    status,
                    std::process::ExitCode::SUCCESS,
                    "retirement response: {response}"
                );
                assert_eq!(response["phase"], "complete");
                assert_eq!(response["checkout_removed"], true);

                fs::write(
                    fixture.project.root().join("ignored.rs"),
                    "pub fn retired() {}",
                )?;
                fs::write(sibling.root().join("survivor.rs"), "pub fn survivor() {}")?;
                fixture
                    .wait_rows_for(sibling, deleted_b, |rows| {
                        has_named_row(rows, "survivor.rs", "survivor", "functions")
                            && has_named_row(rows, "answer.json", "answer", "pairs")
                    })
                    .await?;
                let (after_retirement, rows_a) = fixture.rows().await?;
                assert_eq!(
                    after_retirement, before_retirement,
                    "retired A must stop Raw ingress while B continues"
                );
                assert_eq!(
                    vectors(&rows_a),
                    vectors_a,
                    "retired A must stop NN ingress while B continues"
                );
                assert!(!contains_path(&rows_a, "ignored.rs"));
                Ok(())
            }
        })
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a model-server executable and the pinned EmbeddingGemma GGUF for native AST inference"]
async fn foreground_python_publishes_native_graph_real_vectors_and_current_source() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.run_projects(&[&fixture.project], |address| {
        let fixture = &fixture;
        async move {
            fixture.assert_published().await?;
            let (before, _) = fixture.rows().await?;
            let path = "native.py";
            let source = "BASE = 1\nclass PythonClass:\n    def member(self, arg: int = 2): return helper(arg)\ndef helper(value): return value\ndef caller(arg): return helper(arg)\n";
            fs::write(fixture.project.root().join(path), source)?;
            let (raw, batches) = fixture.wait_rows_for(&fixture.project, before, |rows| {
                has_named_row(rows, path, "PythonClass", "classes")
                    && has_named_row(rows, path, "helper", "functions")
                    && has_named_row(rows, path, "caller", "functions")
                    && has_named_row(rows, path, "BASE", "variables")
            }).await?;
            let mut python_rows = 0;
            for batch in &batches {
                let strings = |name: &str| batch.column_by_name(name).unwrap().as_any().downcast_ref::<StringArray>().unwrap();
                let hashes = batch.column_by_name("full_source_sha256").unwrap().as_any().downcast_ref::<FixedSizeBinaryArray>().unwrap();
                let vectors = batch.column_by_name("vector").unwrap().as_any().downcast_ref::<FixedSizeListArray>().unwrap();
                for row in 0..batch.num_rows() {
                    if strings("path").value(row) != path { continue; }
                    python_rows += 1;
                    assert_eq!(strings("language").value(row), "python");
                    assert_eq!(strings("grammar").value(row), "tree-sitter-python");
                    assert_eq!(strings("name_kind").value(row), "Identifier");
                    assert!(source.contains(strings("projection").value(row)));
                    assert_eq!(hashes.value(row), Sha256::digest(source.as_bytes()).as_slice());
                    let vector = vectors.value(row);
                    let vector = vector.as_any().downcast_ref::<Float32Array>().unwrap();
                    assert_eq!(vector.len(), 768);
                    assert!(vector.values().iter().all(|v| v.is_finite()));
                }
            }
            assert!(python_rows >= 5);
            fixture.graph_observations(address, &fixture.project, None, &["alpha.rs", "answer.json", path], raw).await?;
            let selection = json!({"selection":{"project_dir":graphit_core::native_path::NativePath::new(fixture.project.root().to_owned())?}});
            for (label, name) in [("Class", "PythonClass"), ("Function", "helper"), ("Parameter", "arg"), ("Variable", "BASE"), ("Value", "1")] {
                let mut input = selection.clone();
                input["query"] = json!({"query":format!("MATCH (n:{label}) WHERE n.path = 'native.py' RETURN n.name,n.start_byte,n.end_byte,n.metadata_json")});
                let (status, result) = fixture.graph(address, "query", &input).await?;
                assert_eq!(status, reqwest::StatusCode::OK, "{result}");
                let row = result["rows"].as_array().unwrap().iter().find(|r| r[0]["value"] == name).unwrap_or_else(|| panic!("missing {label}/{name}: {result}"));
                let start = row[1]["value"].as_str().unwrap().parse::<usize>()?;
                let end = row[2]["value"].as_str().unwrap().parse::<usize>()?;
                assert!(start < end && end <= source.len());
                let metadata: Value = serde_json::from_str(row[3]["value"].as_str().unwrap())?;
                assert!(!metadata["contributions"].as_array().unwrap().is_empty());
            }
            let mut input = selection.clone();
            input["query"] = json!({"query":"MATCH (a:Function)-[:CALLS]->(b:Function) WHERE a.path = 'native.py' RETURN a.name,b.name"});
            let (status, result) = fixture.graph(address, "query", &input).await?;
            assert_eq!(status, reqwest::StatusCode::OK, "{result}");
            assert!(result["rows"].as_array().unwrap().iter().any(|r| r[0]["value"] == "caller" && r[1]["value"] == "helper"));
            input["query"] = json!({"query":"MATCH (n:AstFileObservation) WHERE n.path = 'native.py' RETURN n.model_available,n.source_stored,n.source_empty,n.source_sha256"});
            let (status, result) = fixture.graph(address, "query", &input).await?;
            assert_eq!(status, reqwest::StatusCode::OK, "{result}");
            let row = &result["rows"][0];
            assert_eq!(row[0]["value"], true);
            assert_eq!(row[1]["value"], true);
            assert_eq!(row[2]["value"], false);
            assert_eq!(row[3]["value"], Sha256::digest(source.as_bytes()).iter().map(|b| format!("{b:02x}")).collect::<String>());
            Ok(())
        }
    }).await
}
