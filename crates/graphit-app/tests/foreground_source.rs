//! Real foreground component; central executable journeys remain ff08.
use arrow_array::{ArrayRef, RecordBatch, StringArray};
use graphit_app::{ast_source::SourceArguments, foreground_source::ForegroundSource, source_entry};
use graphit_ast::indexed_source::source_schema;
use graphit_core::{
    brand::Brand,
    config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource},
    file_lock::Cancellation,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_daemon::{
    auth_startup::{AuthStartupFailure, StartupError},
    project_bootstrap::ProjectBootstrapInput,
};
use graphit_hub::{
    NativeBinding,
    policy::{HubResourcePolicy, ResourceSetting},
};
use serde_json::json;
use std::{
    fs, io,
    sync::Arc,
    time::{Duration, Instant},
};
type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Fixture {
    _directory: tempfile::TempDir,
    paths: InstallationPaths,
    projects: Vec<ProjectPaths>,
    config: ConfigSnapshot,
    policy: HubResourcePolicy,
}
impl Fixture {
    fn git_project(&self) -> TestResult {
        for arguments in [
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
            let output = std::process::Command::new("git")
                .args(arguments)
                .current_dir(self.projects[0].root())
                .output()?;
            if !output.status.success() {
                return Err(io::Error::other(String::from_utf8_lossy(&output.stderr)).into());
            }
        }
        Ok(())
    }

    async fn publish_versions(&self) -> TestResult {
        let identity = ProjectIdentityStore::at(&self.projects[0]).load()?.unwrap();
        let corpus = self.projects[0].project_runtime(identity.id(), &["ast", "source.lance"])?;
        fs::create_dir_all(corpus.parent().unwrap())?;
        let batch = |text: &str| {
            let columns: Vec<ArrayRef> = vec![
                Arc::new(StringArray::from(vec!["selected"])),
                Arc::new(StringArray::from(vec![text])),
            ];
            RecordBatch::try_new(source_schema(), columns)
        };
        let _main = graphit_data::lance_module_store::LanceModuleStore::create(
            corpus.to_str().unwrap(),
            source_schema(),
            vec![batch("main indexed")?],
        )
        .await?;
        let store = graphit_data::lance_branch_store::LanceBranchStore::open(
            corpus.to_str().unwrap(),
            source_schema(),
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
        Ok(())
    }

    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let brand = Brand::default();
        let paths = InstallationPaths::resolve(
            brand.clone(),
            directory.path(),
            None,
            Some(&directory.path().join("installation")),
        )
        .unwrap();
        let cancellation = Cancellation::default();
        let projects = ["first", "second"]
            .map(|name| {
                let root = directory.path().join(name);
                fs::create_dir(&root).unwrap();
                let project = ProjectPaths::at(&root, &paths).unwrap();
                ProjectIdentityStore::at(&project)
                    .writer(Duration::from_secs(5), &cancellation)
                    .unwrap()
                    .ensure(&ProjectMetadata::new(name).unwrap())
                    .unwrap();
                project
            })
            .to_vec();
        let config = ConfigSnapshot::new(&brand, []).unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        Self {
            _directory: directory,
            paths,
            projects,
            config,
            policy,
        }
    }
    fn inputs(&self) -> Vec<ProjectBootstrapInput<'_>> {
        self.projects
            .iter()
            .enumerate()
            .map(|(index, project)| ProjectBootstrapInput {
                project,
                explicit_version: if index == 0 { "" } else { "feature" },
                pin: None,
                detached_base: "",
            })
            .collect()
    }
    fn start(&self) -> Result<ForegroundSource, graphit_app::foreground_source::ForegroundFailure> {
        let cancellation = Cancellation::default();
        let original = self
            .policy
            .request_context(Instant::now(), &cancellation)
            .unwrap();
        ForegroundSource::start(
            &self.paths,
            &self.inputs(),
            0,
            &self.config,
            NativeBinding::Codex,
            &original,
        )
    }
    fn limits(&self) -> NativeDocumentFileLimits {
        NativeDocumentFileLimits::new(
            self.policy.limit(ResourceSetting::ContentInputBytes).value as usize,
            graphit_core::native_path::MAX_PATH_BYTES,
            self.policy.limit(ResourceSetting::SnapshotDepth).value as usize,
            self.policy.limit(ResourceSetting::ContentScalarBytes).value as usize,
        )
        .unwrap()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_checkout_retains_distinct_versions_and_explicit_current_index() -> TestResult {
    let fixture = Fixture::new();
    fixture.git_project()?;
    // A fallback file with different text would expose incorrect corpus selection.
    fs::write(
        fixture.projects[0].root().join("selected"),
        "physical fallback",
    )?;
    let inputs = ["", "feature"].map(|explicit_version| ProjectBootstrapInput {
        project: &fixture.projects[0],
        explicit_version,
        pin: None,
        detached_base: "",
    });
    let record_path = fixture.paths.root().join("daemon/mcp.json");
    let key_path = fixture.paths.root().join("daemon/mcp.key");
    let client = reqwest::Client::new();
    for current in [0, 1] {
        let startup_cancel = Cancellation::default();
        let startup = fixture
            .policy
            .request_context(Instant::now(), &startup_cancel)?;
        let owner = ForegroundSource::start(
            &fixture.paths,
            &inputs,
            current,
            &fixture.config,
            NativeBinding::Codex,
            &startup,
        )?;
        if current == 0 {
            // The SDK prepares the private installation before corpus publication.
            fixture.publish_versions().await?;
        }
        let key = fs::read(&key_path)?;
        let service = Cancellation::default();
        let stop = service.clone();
        let running = tokio::spawn(async move { owner.run(&service).await });
        for (value, expected) in [
            (
                json!({"path":"selected"}),
                if current == 0 {
                    "main indexed"
                } else {
                    "feature indexed"
                },
            ),
            (
                json!({"path":"selected","project_version":"main"}),
                "main indexed",
            ),
            (
                json!({"path":"selected","project_version":"feature"}),
                "feature indexed",
            ),
            (
                json!({"path":"selected","project_dir":fixture.projects[0].root(),"project_version":"feature"}),
                "feature indexed",
            ),
        ] {
            let cancel = Cancellation::default();
            let original = fixture.policy.request_context(Instant::now(), &cancel)?;
            let mut output = Vec::new();
            source_entry::call_source(
                &fixture.paths,
                &client,
                &serde_json::from_value(value)?,
                fixture.limits(),
                &original,
                &mut output,
            )
            .await?;
            assert_eq!(output, expected.as_bytes());
        }
        assert_eq!(fs::read(&key_path)?, key);
        stop.cancel();
        running.await??;
        assert!(!record_path.exists());
        assert!(!key_path.exists());
    }
    Ok(())
}

#[tokio::test]
async fn aliases_of_one_resolved_version_fail_before_publication_and_retain_cleanup_owner()
-> TestResult {
    let fixture = Fixture::new();
    fixture.git_project()?;
    for versions in [["", "main"], ["feature", "feature"]] {
        let inputs = versions.map(|explicit_version| ProjectBootstrapInput {
            project: &fixture.projects[0],
            explicit_version,
            pin: None,
            detached_base: "",
        });
        let cancel = Cancellation::default();
        let original = fixture.policy.request_context(Instant::now(), &cancel)?;
        let failure = ForegroundSource::start(
            &fixture.paths,
            &inputs,
            0,
            &fixture.config,
            NativeBinding::Codex,
            &original,
        )
        .err()
        .unwrap();
        assert_eq!(failure.error.kind(), io::ErrorKind::InvalidInput);
        assert!(!fixture.paths.root().join("daemon/mcp.json").exists());
        assert!(fixture.paths.root().join("daemon/mcp.key").exists());
        failure.owner.unwrap().shutdown().await?;
        assert!(!fixture.paths.root().join("daemon/mcp.key").exists());
    }
    fixture.start()?.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreground_serves_two_roots_and_releases_singleton_after_joined_stop() -> TestResult {
    let fixture = Fixture::new();
    fs::write(
        fixture.projects[0].root().join("raw"),
        "\u{feff}one\r\nβ\0\n",
    )?;
    fs::write(fixture.projects[0].root().join("empty"), "")?;
    fs::write(fixture.projects[1].root().join("raw"), "second")?;
    let owner = fixture.start()?;
    let address = owner.address().unwrap();
    let record_path = fixture.paths.root().join("daemon/mcp.json");
    let key_path = fixture.paths.root().join("daemon/mcp.key");
    let record = fs::read(&record_path)?;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&record)?["address"],
        address.to_string()
    );
    let key = fs::read(&key_path)?;
    let duplicate = fixture.start().err().unwrap();
    let auth = duplicate
        .error
        .get_ref()
        .unwrap()
        .downcast_ref::<AuthStartupFailure>()
        .unwrap();
    assert!(
        matches!(auth.cause(), StartupError::Io(error) if error.kind()==io::ErrorKind::WouldBlock)
    );
    assert_eq!(fs::read(&key_path)?, key);
    let cancellation = Cancellation::default();
    let signal = cancellation.clone();
    let running = tokio::spawn(async move { owner.run(&cancellation).await });
    let client = reqwest::Client::new();
    for (value, expected) in [
        (json!({"path":"raw"}), "\u{feff}one\r\nβ\0\n"),
        (json!({"path":"empty"}), ""),
        (
            json!({"path":"raw", "project_dir":fixture.projects[1].root(), "project_version":"feature"}),
            "second",
        ),
    ] {
        let original_cancel = Cancellation::default();
        let original = fixture
            .policy
            .request_context(Instant::now(), &original_cancel)?;
        let input: SourceArguments = serde_json::from_value(value)?;
        let mut output = Vec::new();
        source_entry::call_source(
            &fixture.paths,
            &client,
            &input,
            fixture.limits(),
            &original,
            &mut output,
        )
        .await?;
        assert_eq!(output, expected.as_bytes());
    }
    assert_eq!(fs::read(&key_path)?, key);
    signal.cancel();
    running.await??;
    assert!(!record_path.exists());
    assert!(!key_path.exists());
    fixture.start()?.shutdown().await?;
    assert!(!record_path.exists());
    assert!(!key_path.exists());
    Ok(())
}

#[tokio::test]
async fn changed_discovery_preserves_foreign_data_and_owner_for_explicit_cleanup() -> TestResult {
    let fixture = Fixture::new();
    let owner = fixture.start()?;
    let record_path = fixture.paths.root().join("daemon/mcp.json");
    let key_path = fixture.paths.root().join("daemon/mcp.key");
    let original = fs::read(&record_path)?;
    let mut foreign: serde_json::Value = serde_json::from_slice(&original)?;
    foreign["address"] = json!("127.0.0.1:1");
    let foreign = serde_json::to_vec(&foreign)?;
    fs::write(&record_path, &foreign)?;
    let failure = owner.shutdown().await.err().unwrap();
    assert_eq!(failure.error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&record_path)?, foreign);
    assert!(key_path.exists());
    assert!(failure.owner.is_some());
    // Explicit repair of ordinary fixture data, not key/authority restoration.
    fs::write(&record_path, original)?;
    failure.owner.unwrap().shutdown().await?;
    assert!(!record_path.exists());
    assert!(!key_path.exists());
    Ok(())
}

#[tokio::test]
async fn occupied_port_retains_started_owner_until_explicit_cleanup() -> TestResult {
    let mut fixture = Fixture::new();
    let occupied = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    fixture.config = ConfigSnapshot::new(
        fixture.paths.brand(),
        [ConfigDocument::new(
            fixture.paths.brand(),
            ConfigSource::Command,
            [(
                ConfigKey::new("mcp.port")?,
                json!(occupied.local_addr()?.port()),
            )],
        )?],
    )?;
    let failure = fixture.start().err().unwrap();
    assert!(
        failure
            .error
            .get_ref()
            .unwrap()
            .is::<graphit_core::tcp_listener::BindError>()
    );
    assert!(!fixture.paths.root().join("daemon/mcp.json").exists());
    assert!(fixture.paths.root().join("daemon/mcp.key").exists());
    failure.owner.unwrap().shutdown().await?;
    assert!(!fixture.paths.root().join("daemon/mcp.key").exists());
    drop(occupied);
    fixture.start()?.shutdown().await?;
    Ok(())
}

#[test]
fn invalid_configuration_missing_identity_and_cancelled_original_do_not_publish() -> TestResult {
    let mut fixture = Fixture::new();
    fixture.config = ConfigSnapshot::new(
        fixture.paths.brand(),
        [ConfigDocument::new(
            fixture.paths.brand(),
            ConfigSource::Command,
            [(ConfigKey::new("mcp.host")?, json!("192.0.2.1"))],
        )?],
    )?;
    let invalid = fixture
        .start()
        .err()
        .expect("non-loopback unicast must fail");
    assert_eq!(invalid.error.kind(), io::ErrorKind::InvalidInput);
    assert!(invalid.owner.is_none());
    assert!(!fixture.paths.root().join("daemon/mcp.json").exists());
    assert!(!fixture.paths.root().join("daemon/mcp.key").exists());
    fixture.config = ConfigSnapshot::new(fixture.paths.brand(), [])?;
    let root = fixture._directory.path().join("uninitialized");
    fs::create_dir(&root)?;
    fixture
        .projects
        .push(ProjectPaths::at(&root, &fixture.paths)?);
    let missing = fixture.start().err().unwrap();
    assert_eq!(missing.error.kind(), io::ErrorKind::NotFound);
    assert!(!fixture.paths.root().join("daemon/mcp.json").exists());
    fixture.projects.pop();
    let cancellation = Cancellation::default();
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)?;
    cancellation.cancel();
    let cancelled = ForegroundSource::start(
        &fixture.paths,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::Codex,
        &original,
    )
    .err()
    .unwrap();
    assert_eq!(cancelled.error.kind(), io::ErrorKind::Interrupted);
    assert!(!fixture.paths.root().join("daemon/mcp.json").exists());
    Ok(())
}
