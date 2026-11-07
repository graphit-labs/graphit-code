use graphit_core::brand::Brand;
use graphit_core::config::{
    ConfigDocument, ConfigKey, ConfigMutation, ConfigSnapshot, ConfigSource, ConfigStore,
};
use graphit_core::file_lock::Cancellation;
use graphit_core::project_paths::{InstallationPaths, ProjectPaths};
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use std::fs;
use std::io;
use std::process::Command;
use std::time::{Duration, Instant};

mod support;
use support::Fixture;

fn key(text: &str) -> ConfigKey {
    ConfigKey::new(text).unwrap()
}
fn document(source: ConfigSource, values: &[(&str, Value)]) -> ConfigDocument {
    ConfigDocument::new(
        &Brand::default(),
        source,
        values
            .iter()
            .map(|(name, value)| (key(name), value.clone())),
    )
    .unwrap()
}
fn ensure_project_identity(project: &ProjectPaths) {
    graphit_core::project_identity::ProjectIdentityStore::at(project)
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .ensure(&graphit_core::project_identity::ProjectMetadata::new("config-fixture").unwrap())
        .unwrap();
}
fn stores(fixture: &Fixture) -> (ConfigStore, ConfigStore) {
    let installation = fixture.installation();
    (
        ConfigStore::global(&installation).unwrap(),
        ConfigStore::project(&fixture.project()).unwrap(),
    )
}
fn set(store: &ConfigStore, name: &str, value: Value) -> io::Result<bool> {
    store.set(
        key(name),
        value,
        Duration::from_secs(5),
        &Cancellation::default(),
    )
}

#[test]
fn command_values_keep_csv_items_and_secrets_literal_without_coercing_other_settings() {
    for name in [
        "ast.grammar",
        "ast.grammars_whitelist",
        "ast.grammars_blacklist",
        "ast.scip.parsers",
        "knowledge.extensions",
    ] {
        for text in ["", "false", "123", "md, txt", "língua, 日本語"] {
            assert_eq!(key(name).parse_value(text).unwrap(), json!(text));
        }
        assert_eq!(
            key(name).parse_value(r#""md,txt""#).unwrap(),
            json!("md,txt")
        );
        // Structured input retains its type; it is never reinterpreted as CSV.
        assert_eq!(
            key(name).parse_value("[1,false]").unwrap(),
            json!([1, false])
        );
        assert!(key(name).parse_value("null").is_err());
    }
    for name in ["client.secret", "ui.auth.cookie_encryption_key"] {
        for text in ["", "null", "true", "123", "a,b", r#""quoted""#] {
            assert_eq!(key(name).parse_value(text).unwrap(), json!(text));
        }
    }
    for (name, text, expected) in [
        ("modules.ast", "false", json!(false)),
        ("custom.count", "123", json!(123)),
        ("custom.value", "a,b", json!("a,b")),
        (
            "custom.value",
            "  literal text  ",
            json!("  literal text  "),
        ),
        ("daemon.models", r#"{"slots":[]}"#, json!({"slots":[]})),
        (
            "task.browser",
            r#"{"providers":["one","two"]}"#,
            json!({"providers":["one","two"]}),
        ),
    ] {
        assert_eq!(key(name).parse_value(text).unwrap(), expected);
    }
    assert!(key("custom.value").parse_value("null").is_err());
}

#[test]
fn five_layers_resolve_presence_types_and_independent_snapshots() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ConfigDocument>();
    assert_send_sync::<ConfigSnapshot>();
    let sources = [
        ConfigSource::Command,
        ConfigSource::Environment,
        ConfigSource::Project,
        ConfigSource::Global,
        ConfigSource::Distribution,
    ];
    for winner in 0..sources.len() {
        let layers = sources[winner..]
            .iter()
            .map(|source| document(*source, &[("custom.value", json!(format!("{source:?}")))]));
        let snapshot = ConfigSnapshot::new(&Brand::default(), layers).unwrap();
        assert_eq!(snapshot.source(&key("custom.value")), Some(sources[winner]));
        assert_eq!(
            snapshot
                .get::<String>(&key("custom.value"))
                .unwrap()
                .unwrap(),
            format!("{:?}", sources[winner])
        );
    }
    let global = document(
        ConfigSource::Global,
        &[
            ("custom.value", json!("global")),
            ("modules.ast", json!(true)),
        ],
    );
    let command = document(
        ConfigSource::Command,
        &[
            ("custom.value", json!("")),
            ("modules.ast", json!(false)),
            ("custom.count", json!(0)),
        ],
    );
    let snapshot = ConfigSnapshot::new(&Brand::default(), [global.clone(), command]).unwrap();
    assert_eq!(
        snapshot.get::<&str>(&key("custom.value")).unwrap(),
        Some("")
    );
    assert_eq!(
        snapshot.get::<bool>(&key("modules.ast")).unwrap(),
        Some(false)
    );
    assert_eq!(snapshot.get::<i64>(&key("custom.count")).unwrap(), Some(0));
    assert!(snapshot.get::<bool>(&key("custom.count")).is_err());
    assert_eq!(snapshot.get::<String>(&key("custom.absent")).unwrap(), None);
    assert_eq!(
        snapshot
            .layer(ConfigSource::Global)
            .unwrap()
            .get::<&str>(&key("custom.value"))
            .unwrap(),
        Some("global")
    );
    let other = ConfigSnapshot::new(&Brand::default(), [global]).unwrap();
    assert_eq!(
        other.get::<&str>(&key("custom.value")).unwrap(),
        Some("global")
    );
    assert!(
        ConfigSnapshot::new(
            &Brand::new("acme-code").unwrap(),
            [document(ConfigSource::Global, &[])]
        )
        .is_err()
    );
    assert!(
        ConfigSnapshot::new(
            &Brand::default(),
            [
                document(ConfigSource::Global, &[]),
                document(ConfigSource::Global, &[])
            ]
        )
        .is_err()
    );
}

#[test]
fn canonical_keys_environment_names_and_scope_policy_have_no_aliases() {
    let brand = Brand::new("acme-code").unwrap();
    assert_eq!(key(" Knowledge.Docs_Dir ").as_str(), "knowledge.docs_dir");
    assert_eq!(
        key("knowledge.docs_dir").environment_variable(&brand),
        "ACME_CODE_KNOWLEDGE__DOCS_DIR"
    );
    assert_ne!(
        key("a.b").environment_variable(&brand),
        key("a_b").environment_variable(&brand)
    );
    for name in [
        "",
        ".x",
        "x.",
        "x..y",
        "x__y",
        "_x",
        "x_",
        "á",
        "x-y",
        "x/y",
        "global_dir",
        "model_cache",
        "project_version",
        "ai.agent_args.claude",
        "ast.cluster",
        "ast.allow_native_grammars",
        "hub.bucket",
        "hub.subject.user",
        "mcp.api_key",
        "ai.embedding.api_key",
        "auth.key",
        "providers.local",
        "accounts.name",
    ] {
        assert!(ConfigKey::new(name).is_err(), "{name}");
    }
    assert!(ConfigKey::new(&"a".repeat(129)).is_err());
    assert!(ConfigKey::new(&["a"; 17].join(".")).is_err());
    for name in [
        "client.secret",
        "ui.auth.cookie_encryption_key",
        "unit.id",
        "ai.cli",
        "models.embedding.id",
        "models.rerank.id",
    ] {
        assert!(
            ConfigDocument::new(
                &Brand::default(),
                ConfigSource::Project,
                [(key(name), json!("fixture"))]
            )
            .is_err()
        );
    }
    let selected_cli = br#"{"settings":{"ai.cli":"bin/copilot"}}"#;
    let selected = ConfigDocument::decode(&brand, ConfigSource::Global, selected_cli).unwrap();
    assert_eq!(
        selected.get::<String>(&key("ai.cli")).unwrap().as_deref(),
        Some("bin/copilot")
    );
    assert_eq!(
        ConfigDocument::decode(&brand, ConfigSource::Project, selected_cli)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData,
    );
    let native_grammars = br#"{"settings":{"ast.allow_native_grammars":true}}"#;
    for source in [
        ConfigSource::Command,
        ConfigSource::Environment,
        ConfigSource::Project,
        ConfigSource::Global,
        ConfigSource::Distribution,
    ] {
        assert_eq!(
            ConfigDocument::decode(&brand, source, native_grammars)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData,
        );
    }
    assert!(
        ConfigKey::new("ast.allow_native_grammars")
            .unwrap_err()
            .to_string()
            .contains("was removed")
    );
    let compiler_images = br#"{"settings":{"ast.scip.parsers":".zig=registry.example/parser:1"}}"#;
    for source in [ConfigSource::Project, ConfigSource::Global] {
        let bytes = if source == ConfigSource::Project {
            let mut value: Value = serde_json::from_slice(compiler_images).unwrap();
            value["id"] = json!("01ARZ3NDEKTSV4RRFFQ69G5FAV");
            value["name"] = json!("config-fixture");
            serde_json::to_vec(&value).unwrap()
        } else {
            compiler_images.to_vec()
        };
        let document = ConfigDocument::decode(&brand, source, &bytes).unwrap();
        let parsers = document
            .get::<String>(&key("ast.scip.parsers"))
            .unwrap()
            .unwrap();
        assert_eq!(parsers, ".zig=registry.example/parser:1");
    }
    assert!(
        ConfigDocument::new(
            &brand,
            ConfigSource::Global,
            [(key("modules.daemon_ui"), json!(true))]
        )
        .is_err()
    );
    assert!(
        ConfigDocument::new(
            &brand,
            ConfigSource::Environment,
            [(key("modules.daemon_ui"), json!("true"))]
        )
        .is_ok()
    );
}

#[test]
fn environment_capture_is_explicit_bounded_and_preserves_empty_overrides() {
    let brand = Brand::new("acme-code").unwrap();
    let keys = [key("custom.value"), key("modules.ast"), key("custom.count")];
    let mut variables: Vec<_> = [
        ("ACME_CODE_CUSTOM__VALUE", ""),
        ("ACME_CODE_MODULES__AST", "false"),
        ("ACME_CODE_CUSTOM__COUNT", "12"),
        ("GRAPHIT_CUSTOM__VALUE", "foreign"),
        ("ACME_CODE_GLOBAL_DIR", "bootstrap"),
    ]
    .into_iter()
    .map(|(name, value)| (name.into(), value.into()))
    .collect();
    let captured = ConfigSnapshot::capture_environment(&brand, &keys, variables.clone()).unwrap();
    variables.clear();
    assert_eq!(captured.get::<&str>(&keys[0]).unwrap(), Some(""));
    assert_eq!(captured.get::<bool>(&keys[1]).unwrap(), Some(false));
    assert_eq!(captured.get::<i64>(&keys[2]).unwrap(), Some(12));
    assert_eq!(captured.inspect().len(), 3);
    assert!(
        ConfigSnapshot::capture_environment(
            &brand,
            &keys,
            [
                ("ACME_CODE_CUSTOM__VALUE".into(), "one".into()),
                ("ACME_CODE_CUSTOM__VALUE".into(), "two".into())
            ]
        )
        .is_err()
    );
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        assert!(
            ConfigSnapshot::capture_environment(
                &brand,
                &keys,
                [(
                    "ACME_CODE_CUSTOM__VALUE".into(),
                    std::ffi::OsString::from_vec(vec![0xff])
                )]
            )
            .is_err()
        );
    }
}

#[test]
fn environment_budget_stops_consuming_before_accumulating_remaining_values() {
    let brand = Brand::default();
    let keys: Vec<_> = (0..1024)
        .map(|index| key(&format!("custom.k{index}")))
        .collect();
    let consumed = std::cell::Cell::new(0);
    let variables = keys.iter().map(|key| {
        consumed.set(consumed.get() + 1);
        (
            key.environment_variable(&brand).into(),
            "x".repeat(150 * 1024).into(),
        )
    });
    assert_eq!(
        ConfigSnapshot::capture_environment(&brand, &keys, variables)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(consumed.get(), 2);
}

#[test]
fn textual_conversion_is_environment_only_and_namespace_uniqueness_is_per_brand() {
    for source in [
        ConfigSource::Command,
        ConfigSource::Project,
        ConfigSource::Global,
        ConfigSource::Distribution,
    ] {
        let document = document(
            source,
            &[
                ("custom.bool", json!("true")),
                ("custom.list", json!("[1]")),
            ],
        );
        assert_eq!(
            document
                .get::<bool>(&key("custom.bool"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            document
                .get::<Vec<u64>>(&key("custom.list"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            document.get::<&str>(&key("custom.bool")).unwrap(),
            Some("true")
        );
    }
    let environment = document(
        ConfigSource::Environment,
        &[
            ("custom.bool", json!("true")),
            ("custom.list", json!("[1]")),
        ],
    );
    assert_eq!(
        environment.get::<bool>(&key("custom.bool")).unwrap(),
        Some(true)
    );
    assert_eq!(
        environment.get::<Vec<u64>>(&key("custom.list")).unwrap(),
        Some(vec![1])
    );
    assert_eq!(
        key("modules.ast").environment_variable(&Brand::new("acme-code").unwrap()),
        key("code_modules.ast").environment_variable(&Brand::new("acme").unwrap())
    );
}

#[test]
fn logical_input_errors_and_persisted_data_errors_are_distinct() {
    let fixture = Fixture::new();
    let (global, _) = stores(&fixture);
    for (name, value) in [
        ("custom.null", Value::Null),
        ("client.secret", json!(1)),
        ("custom.large", json!("x".repeat(256 * 1024))),
    ] {
        assert_eq!(
            set(&global, name, value).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(!global.path().parent().unwrap().exists());
    }
    fs::create_dir_all(global.path().parent().unwrap()).unwrap();
    for bytes in [
        br#"{"settings":{"custom.null":null}}"#.as_slice(),
        br#"{"settings":{"client.secret":1}}"#.as_slice(),
    ] {
        fs::write(global.path(), bytes).unwrap();
        assert_eq!(
            global.load().unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(fs::read(global.path()).unwrap(), bytes);
    }
}

#[test]
fn inspection_debug_and_errors_never_echo_secret_payloads() {
    const CANARY: &str = "fixture-sensitive-salt-canary";
    let secret = key("client.secret");
    let document = document(
        ConfigSource::Global,
        &[
            (secret.as_str(), json!(CANARY)),
            ("custom.public", json!({"flag": true})),
        ],
    );
    let snapshot = ConfigSnapshot::new(&Brand::default(), [document.clone()]).unwrap();
    assert_eq!(
        snapshot
            .expose_secret(&secret)
            .unwrap()
            .unwrap()
            .expose_secret(),
        CANARY
    );
    assert!(document.get::<String>(&secret).is_err());
    for text in [
        format!("{document:?}"),
        format!("{snapshot:?}"),
        serde_json::to_string(&document.inspect()).unwrap(),
        serde_json::to_string(&snapshot.inspect()).unwrap(),
        format!(
            "{:?}",
            ConfigMutation::Set {
                key: secret.clone(),
                value: json!(CANARY)
            }
        ),
    ] {
        assert!(!text.contains(CANARY));
    }
    let error = ConfigDocument::new(
        &Brand::default(),
        ConfigSource::Project,
        [(secret, json!(CANARY))],
    )
    .unwrap_err();
    assert!(!error.to_string().contains(CANARY));
}

#[test]
fn scoped_crud_preserves_identity_snapshots_and_zero_effect_reads() {
    let fixture = Fixture::new();
    let (global, project) = stores(&fixture);
    assert!(global.load().unwrap().inspect().is_empty());
    assert!(project.load().unwrap().inspect().is_empty());
    assert_eq!(fs::read_dir(fixture.root()).unwrap().count(), 0);
    assert!(set(&project, "client.secret", json!("fixture-rejected-secret")).is_err());
    assert_eq!(fs::read_dir(fixture.root()).unwrap().count(), 0);
    assert!(set(&global, "custom.value", json!("global")).unwrap());
    ensure_project_identity(&fixture.project());
    assert!(set(&project, "custom.value", json!([false, 0, "project"])).unwrap());
    let prior = project.load().unwrap();
    let before = fs::read(project.path()).unwrap();
    assert!(!set(&project, "custom.value", json!([false, 0, "project"])).unwrap());
    assert_eq!(fs::read(project.path()).unwrap(), before);
    assert!(set(&project, "custom.value", json!("new")).unwrap());
    assert_eq!(
        prior.get::<Value>(&key("custom.value")).unwrap(),
        Some(json!([false, 0, "project"]))
    );
    assert_eq!(
        global
            .load()
            .unwrap()
            .get::<&str>(&key("custom.value"))
            .unwrap(),
        Some("global")
    );
    assert!(
        project
            .unset(
                key("custom.value"),
                Duration::ZERO,
                &Cancellation::default()
            )
            .unwrap()
    );
    assert!(
        !project
            .unset(
                key("custom.value"),
                Duration::ZERO,
                &Cancellation::default()
            )
            .unwrap()
    );
    assert!(project.path().is_file());
    assert!(project.lock_path().is_file());
    assert!(fixture.path(".graphit/project.json").exists());
    assert!(!fixture.path(".graphit/config.json").exists());
    assert!(!fixture.path("graphit.lock.json").exists());
}

#[test]
fn malformed_duplicate_and_noncanonical_documents_fail_without_repair() {
    let fixture = Fixture::new();
    let (global, project) = stores(&fixture);
    fs::create_dir_all(global.path().parent().unwrap()).unwrap();
    for text in [
        "null",
        "{",
        "{\"settings\":null}",
        "{\"settings\":{},\"other\":1}",
        "{\"settings\":{},\"settings\":{}}",
        "{\"settings\":{\"custom.flag\":true,\"custom.flag\":false}}",
        "{\"settings\":{\"Custom.flag\":true}}",
        "{\"settings\":{\"custom.flag\":null}}",
        "{\"settings\":{\"hub.bucket\":\"fixture\"}}",
        "{\"settings\":{\"client.secret\":false}}",
    ] {
        fs::write(global.path(), text).unwrap();
        assert_eq!(
            global.load().unwrap_err().kind(),
            io::ErrorKind::InvalidData,
            "{text}"
        );
        assert!(set(&global, "custom.flag", json!(true)).is_err());
        assert_eq!(fs::read_to_string(global.path()).unwrap(), text);
    }
    fs::create_dir_all(project.path().parent().unwrap()).unwrap();
    fs::write(
        project.path(),
        "{\"settings\":{\"client.secret\":\"fixture\"}}",
    )
    .unwrap();
    assert_eq!(
        project.load().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    fs::write(global.path(), vec![b'x'; 256 * 1024 + 1]).unwrap();
    assert_eq!(
        global.load().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn budgets_cancellation_and_readonly_noops_preserve_published_data() {
    let fixture = Fixture::new();
    let (global, _) = stores(&fixture);
    assert!(
        ConfigDocument::new(
            &Brand::default(),
            ConfigSource::Global,
            [(key("custom.value"), Value::Null)]
        )
        .is_err()
    );
    assert!(
        ConfigDocument::new(
            &Brand::default(),
            ConfigSource::Global,
            (0..1025).map(|i| (key(&format!("custom.key{i}")), json!(i)))
        )
        .is_err()
    );
    assert!(set(&global, "custom.value", json!("a".repeat(200_000))).unwrap());
    let before = fs::read(global.path()).unwrap();
    assert!(set(&global, "custom.other", json!("b".repeat(100_000))).is_err());
    assert_eq!(fs::read(global.path()).unwrap(), before);
    let cancellation = Cancellation::default();
    cancellation.cancel();
    assert_eq!(
        global
            .writer(Duration::ZERO, &cancellation)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    let original_permissions = fs::metadata(global.path()).unwrap().permissions();
    let mut permissions = original_permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(global.path(), permissions.clone()).unwrap();
    assert!(!set(&global, "custom.value", json!("a".repeat(200_000))).unwrap());
    assert_eq!(
        set(&global, "custom.value", json!("changed"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(fs::read(global.path()).unwrap(), before);
    fs::set_permissions(global.path(), original_permissions).unwrap();
}

#[cfg(unix)]
#[test]
fn global_files_are_private_and_observed_links_and_special_sources_are_rejected() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let fixture = Fixture::new();
    let (global, project) = stores(&fixture);
    set(&global, "client.secret", json!("fixture-private-salt")).unwrap();
    assert_eq!(
        fs::metadata(global.path()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(global.path().parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    fs::set_permissions(global.path(), fs::Permissions::from_mode(0o644)).unwrap();
    set(&global, "custom.value", json!("changed")).unwrap();
    assert_eq!(
        fs::metadata(global.path()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::create_dir_all(project.path().parent().unwrap()).unwrap();
    fs::set_permissions(global.path(), fs::Permissions::from_mode(0o460)).unwrap();
    set(&global, "custom.value", json!("restricted")).unwrap();
    assert_eq!(
        fs::metadata(global.path()).unwrap().permissions().mode() & 0o777,
        0o400
    );
    assert_eq!(
        set(&global, "custom.value", json!("denied"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    symlink(global.path(), project.path()).unwrap();
    assert_eq!(
        project.load().unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    fs::remove_file(project.path()).unwrap();
    let socket = std::os::unix::net::UnixListener::bind(project.path()).unwrap();
    assert_eq!(
        project.load().unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    drop(socket);
    fs::remove_file(project.path()).unwrap();
    symlink(fixture.path("absent"), project.lock_path()).unwrap();
    assert_eq!(
        project
            .writer(Duration::ZERO, &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn concurrent_processes_edit_independent_keys_without_lost_updates() {
    let fixture = Fixture::new();
    let mut children = Vec::new();
    for i in 0..6 {
        children.push(
            child(&fixture, "write")
                .env("GRAPHIT_CONFIG_TEST_NUMBER", i.to_string())
                .spawn()
                .unwrap(),
        );
    }
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }
    let global = child_store(fixture.root());
    assert_eq!(global.load().unwrap().inspect().len(), 48);
    for i in 0..6 {
        for n in 0..8 {
            assert_eq!(
                global
                    .load()
                    .unwrap()
                    .get::<i64>(&key(&format!("process.p{i}_k{n}")))
                    .unwrap(),
                Some(n)
            );
        }
    }
}

#[test]
fn killed_writer_releases_the_stable_kernel_lock() {
    let fixture = Fixture::new();
    let mut process = child(&fixture, "hold").spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fixture.path("ready").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    let global = child_store(fixture.root());
    assert_eq!(
        global
            .writer(Duration::from_millis(5), &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert!(global.lock_path().is_file());
    process.kill().unwrap();
    process.wait().unwrap();
    assert!(set(&global, "custom.after_kill", json!(true)).unwrap());
    assert!(global.lock_path().is_file());
}

#[test]
fn atomic_batches_keep_readers_and_old_snapshots_consistent() {
    let fixture = Fixture::new();
    let (global, _) = stores(&fixture);
    let mut writer = global
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap();
    let batch = |value| {
        [
            ConfigMutation::Set {
                key: key("custom.left"),
                value: json!(value),
            },
            ConfigMutation::Set {
                key: key("custom.right"),
                value: json!(value),
            },
        ]
    };
    writer.apply(batch(0)).unwrap();
    let prior = global.load().unwrap();
    std::thread::scope(|scope| {
        let reader = scope.spawn(|| {
            for _ in 0..100 {
                let document = global.load().unwrap();
                assert_eq!(
                    document.get::<i64>(&key("custom.left")).unwrap(),
                    document.get::<i64>(&key("custom.right")).unwrap()
                );
            }
        });
        for i in 1..32 {
            writer.apply(batch(i)).unwrap();
        }
        reader.join().unwrap();
    });
    assert_eq!(prior.get::<i64>(&key("custom.left")).unwrap(), Some(0));
    assert_eq!(
        global
            .load()
            .unwrap()
            .get::<i64>(&key("custom.left"))
            .unwrap(),
        Some(31)
    );
    assert!(
        writer
            .apply([
                ConfigMutation::Unset {
                    key: key("custom.left")
                },
                ConfigMutation::Unset {
                    key: key("custom.left")
                }
            ])
            .is_err()
    );
    assert_eq!(
        global
            .load()
            .unwrap()
            .get::<i64>(&key("custom.left"))
            .unwrap(),
        Some(31)
    );
}

#[test]
fn physical_aliases_share_global_lock_while_project_document_remains_independent() {
    let fixture = Fixture::new();
    let installation = InstallationPaths::resolve(
        Brand::default(),
        fixture.root(),
        None,
        Some(&fixture.path(".graphit")),
    )
    .unwrap();
    let project = ProjectPaths::at(fixture.root(), &installation).unwrap();
    let global = ConfigStore::global(&installation).unwrap();
    let project_store = ConfigStore::project(&project).unwrap();
    assert_ne!(global.path(), project_store.path());
    assert_ne!(global.lock_path(), project_store.lock_path());
    let first = global
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap();
    let global_lock = fs::File::open(global.lock_path()).unwrap();
    let global_lock_id = graphit_core::native_file_identity::handle_id(&global_lock).unwrap();
    drop(
        project_store
            .writer(Duration::ZERO, &Cancellation::default())
            .unwrap(),
    );
    let lock_file = fs::File::open(project_store.lock_path()).unwrap();
    graphit_fs::verify_private_file(&lock_file).unwrap();
    assert_eq!(lock_file.metadata().unwrap().len(), 0);
    let lock_id = graphit_core::native_file_identity::handle_id(&lock_file).unwrap();
    let private_installation = InstallationPaths::resolve(
        Brand::new("acme-code").unwrap(),
        fixture.root(),
        None,
        Some(&fixture.path(".graphit")),
    )
    .unwrap();
    let private_global = ConfigStore::global(&private_installation).unwrap();
    assert_eq!(global.lock_path(), private_global.lock_path());
    assert_eq!(
        private_global
            .writer(Duration::from_millis(5), &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    #[cfg(unix)]
    {
        let alias = fixture.path("alias");
        std::os::unix::fs::symlink(fixture.path(".graphit"), &alias).unwrap();
        let aliased =
            InstallationPaths::resolve(Brand::default(), fixture.root(), None, Some(&alias))
                .unwrap();
        let store = ConfigStore::global(&aliased).unwrap();
        assert_eq!(store.lock_path(), global.lock_path());
        assert_eq!(
            store
                .writer(Duration::from_millis(5), &Cancellation::default())
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
    }
    drop(first);
    ensure_project_identity(&project);
    let identity_store = graphit_core::project_identity::ProjectIdentityStore::at(&project);
    let identity_writer = identity_store
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap();
    assert_eq!(
        project_store
            .writer(Duration::from_millis(5), &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    drop(identity_writer);
    drop(
        project_store
            .writer(Duration::ZERO, &Cancellation::default())
            .unwrap(),
    );
    assert!(set(&project_store, "custom.shared", json!(true)).unwrap());
    let reopened_lock = fs::File::open(project_store.lock_path()).unwrap();
    graphit_fs::verify_private_file(&reopened_lock).unwrap();
    assert_eq!(reopened_lock.metadata().unwrap().len(), 0);
    assert!(graphit_core::native_file_identity::handle_id(&reopened_lock).unwrap() == lock_id);
    let reopened_global_lock = fs::File::open(global.lock_path()).unwrap();
    assert!(
        graphit_core::native_file_identity::handle_id(&reopened_global_lock).unwrap()
            == global_lock_id
    );
    assert_eq!(
        global
            .load()
            .unwrap()
            .get::<bool>(&key("custom.shared"))
            .unwrap(),
        None
    );
    assert_eq!(
        project_store
            .load()
            .unwrap()
            .get::<bool>(&key("custom.shared"))
            .unwrap(),
        Some(true)
    );
}

#[test]
fn private_project_documents_remain_distinct_and_snapshots_ignore_later_writes() {
    let fixture = Fixture::new();
    let (_, graphit) = stores(&fixture);
    let installation = InstallationPaths::resolve(
        Brand::new("acme-code").unwrap(),
        fixture.root(),
        None,
        Some(&fixture.path("private_global")),
    )
    .unwrap();
    let acme =
        ConfigStore::project(&ProjectPaths::at(fixture.root(), &installation).unwrap()).unwrap();
    ensure_project_identity(&fixture.project());
    ensure_project_identity(&ProjectPaths::at(fixture.root(), &installation).unwrap());
    set(&graphit, "custom.value", json!("graphit")).unwrap();
    set(&acme, "custom.value", json!("acme")).unwrap();
    assert_ne!(graphit.path(), acme.path());
    let old = ConfigSnapshot::new(&Brand::default(), [graphit.load().unwrap()]).unwrap();
    set(&graphit, "custom.value", json!("later")).unwrap();
    assert_eq!(
        old.get::<&str>(&key("custom.value")).unwrap(),
        Some("graphit")
    );
    assert_eq!(
        acme.load()
            .unwrap()
            .get::<&str>(&key("custom.value"))
            .unwrap(),
        Some("acme")
    );
    assert!(ConfigSnapshot::new(&Brand::default(), [acme.load().unwrap()]).is_err());
}

#[test]
fn structural_and_aggregate_mutation_budgets_fail_before_publication() {
    let fixture = Fixture::new();
    let (global, _) = stores(&fixture);
    let mut deep = json!(true);
    for _ in 0..65 {
        deep = json!([deep]);
    }
    assert!(set(&global, "custom.deep", deep).is_err());
    assert!(set(&global, "custom.wide", json!(vec![true; 8192])).is_err());
    assert!(!global.path().parent().unwrap().exists());
    assert!(
        ConfigDocument::new(
            &Brand::default(),
            ConfigSource::Global,
            [
                (key("custom.one"), json!("x".repeat(150_000))),
                (key("custom.two"), json!("y".repeat(150_000)))
            ]
        )
        .is_err()
    );
    set(&global, "custom.original", json!(true)).unwrap();
    let before = fs::read(global.path()).unwrap();
    let mut writer = global
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap();
    assert!(
        writer
            .apply([
                ConfigMutation::Set {
                    key: key("custom.one"),
                    value: json!("x".repeat(150_000))
                },
                ConfigMutation::Set {
                    key: key("custom.two"),
                    value: json!("y".repeat(150_000))
                }
            ])
            .is_err()
    );
    assert_eq!(fs::read(global.path()).unwrap(), before);
}

fn child(fixture: &Fixture, mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "config_child", "--nocapture"])
        .env("GRAPHIT_CONFIG_TEST_ROOT", fixture.root())
        .env("GRAPHIT_CONFIG_TEST_MODE", mode);
    command
}
fn child_store(root: &std::path::Path) -> ConfigStore {
    let installation =
        InstallationPaths::resolve(Brand::default(), root, None, Some(&root.join("global")))
            .unwrap();
    ConfigStore::global(&installation).unwrap()
}
#[test]
fn config_child() {
    let Some(root) = std::env::var_os("GRAPHIT_CONFIG_TEST_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let global = child_store(&root);
    match std::env::var("GRAPHIT_CONFIG_TEST_MODE").unwrap().as_str() {
        "write" => {
            let i = std::env::var("GRAPHIT_CONFIG_TEST_NUMBER").unwrap();
            for n in 0..8 {
                set(&global, &format!("process.p{i}_k{n}"), json!(n)).unwrap();
            }
        }
        "hold" => {
            let _writer = global
                .writer(Duration::ZERO, &Cancellation::default())
                .unwrap();
            fs::write(root.join("ready"), "ready").unwrap();
            loop {
                std::thread::park();
            }
        }
        _ => panic!("invalid fixture mode"),
    }
}

#[test]
fn public_decoder_preserves_current_format_scope_and_secret_redaction_without_io() {
    const CANARY: &str = "explicit-config-sensitive-canary";
    let fixture = Fixture::new();
    let brand = Brand::default();
    let bytes =
        serde_json::to_vec(&json!({"settings": {"custom.enabled": true, "client.secret": CANARY}}))
            .unwrap();
    let document = ConfigDocument::decode(&brand, ConfigSource::Global, &bytes).unwrap();
    assert_eq!(document.source(), ConfigSource::Global);
    assert_eq!(
        document.get::<bool>(&key("custom.enabled")).unwrap(),
        Some(true)
    );
    let snapshot = ConfigSnapshot::new(&brand, [document.clone()]).unwrap();
    assert_eq!(
        snapshot
            .expose_secret(&key("client.secret"))
            .unwrap()
            .unwrap()
            .expose_secret(),
        CANARY
    );
    for visible in [
        format!("{document:?}"),
        format!("{snapshot:?}"),
        serde_json::to_string(&document.inspect()).unwrap(),
    ] {
        assert!(!visible.contains(CANARY));
    }
    assert_eq!(
        ConfigDocument::decode(&brand, ConfigSource::Project, &bytes)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    for invalid in [
        b"{".as_slice(),
        br#"{"settings":{},"settings":{}}"#,
        br#"{"settings":{"custom.enabled":true,"custom.enabled":false}}"#,
        br#"{"settings":{"Custom.enabled":true}}"#,
        br#"{"settings":{"custom.enabled":null}}"#,
        br#"{"settings":{"auth.profile":"private"}}"#,
        br#"{"settings":{},"unknown":true}"#,
    ] {
        assert_eq!(
            ConfigDocument::decode(&brand, ConfigSource::Global, invalid)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
    assert_eq!(
        ConfigDocument::decode(&brand, ConfigSource::Global, &vec![b' '; 256 * 1024 + 1])
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read_dir(fixture.root()).unwrap().count(), 0);
}

#[test]
fn unified_project_metadata_and_config_writers_share_lock_and_preserve_all_sections() {
    use graphit_core::project_identity::{ProjectId, ProjectIdentityStore, ProjectMetadata};
    let fixture = Fixture::new();
    let paths = fixture.project();
    let identity = ProjectIdentityStore::at(&paths);
    let id: ProjectId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
    let metadata = ProjectMetadata::new("shared-document").unwrap();
    identity
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap()
        .ensure_with_id(&metadata, id)
        .unwrap();
    let config = ConfigStore::project(&paths).unwrap();
    let hooks = json!({"pre-commit":["printf 'ação, 日本語 \\\"texto\\\"\\n'", "git diff --cached --check"]});
    set(&config, "githooks", hooks.clone()).unwrap();
    let held = config
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap();
    assert_eq!(
        identity
            .writer(Duration::from_millis(10), &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    drop(held);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    std::thread::scope(|scope| {
        let gate = barrier.clone();
        let config = &config;
        scope.spawn(move || {
            gate.wait();
            for i in 0..12 {
                set(config, &format!("custom.item_{i}"), json!(i)).unwrap();
            }
        });
        scope.spawn(|| {
            barrier.wait();
            for i in 0..12 {
                let mut metadata = metadata.clone();
                metadata.description = format!("ação {i}");
                metadata
                    .cluster
                    .insert("team".into(), vec!["compiler".into()]);
                identity
                    .writer(Duration::from_secs(5), &Cancellation::default())
                    .unwrap()
                    .update(id, &metadata)
                    .unwrap();
            }
        });
    });
    let current = identity.load().unwrap().unwrap();
    assert_eq!(current.id(), id);
    assert_eq!(current.metadata().description, "ação 11");
    assert_eq!(current.metadata().cluster["team"], ["compiler"]);
    let document = config.load().unwrap();
    assert_eq!(
        document.get::<Value>(&key("githooks")).unwrap(),
        Some(hooks.clone())
    );
    for i in 0..12 {
        assert_eq!(
            document
                .get::<i64>(&key(&format!("custom.item_{i}")))
                .unwrap(),
            Some(i)
        );
    }
    let wire: Value = serde_json::from_slice(&fs::read(config.path()).unwrap()).unwrap();
    assert_eq!(wire["id"], id.to_string());
    assert_eq!(wire["name"], "shared-document");
    assert_eq!(wire["githooks"], hooks);
    assert!(wire.get("metadata").is_none());
    assert!(wire["settings"].get("githooks").is_none());
    assert!(!paths.brand_directory().join("config.json").exists());
    assert!(!paths.brand_directory().join(".config.json.lock").exists());
}

#[test]
fn unified_document_keeps_large_valid_settings_readable_by_identity_and_ignores_old_file() {
    use graphit_core::project_identity::ProjectIdentityStore;
    let fixture = Fixture::new();
    let paths = fixture.project();
    ensure_project_identity(&paths);
    let identity = ProjectIdentityStore::at(&paths);
    let original = identity.load().unwrap().unwrap();
    let config = ConfigStore::project(&paths).unwrap();
    let text = "λ".repeat(40_000);
    set(&config, "custom.text", json!(text)).unwrap();
    // An abandoned file is not a second configuration source or a recovery path.
    fs::write(
        paths.brand_directory().join("config.json"),
        b"invalid obsolete document",
    )
    .unwrap();
    assert_eq!(identity.load().unwrap().unwrap(), original);
    let mut metadata = original.metadata().clone();
    metadata.description = "updated without rekey".into();
    identity
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap()
        .update(original.id(), &metadata)
        .unwrap();
    assert_eq!(
        config
            .load()
            .unwrap()
            .get::<String>(&key("custom.text"))
            .unwrap(),
        Some(text)
    );
    assert_eq!(identity.load().unwrap().unwrap().id(), original.id());
}

#[test]
fn root_githooks_is_canonical_and_replaces_whole_maps_across_layers() {
    let fixture = Fixture::new();
    let (global, project) = stores(&fixture);
    ensure_project_identity(&fixture.project());
    let inherited = json!({"pre-push":["printf 'inherited, value'"]});
    set(&global, "githooks", inherited.clone()).unwrap();
    let raw: Value = serde_json::from_slice(&fs::read(global.path()).unwrap()).unwrap();
    assert_eq!(raw["githooks"], inherited);
    assert!(raw["settings"].get("githooks").is_none());
    let read = || {
        ConfigSnapshot::new(
            &Brand::default(),
            [project.load().unwrap(), global.load().unwrap()],
        )
        .unwrap()
    };
    assert_eq!(
        read().get::<Value>(&key("githooks")).unwrap(),
        Some(inherited.clone())
    );
    set(&project, "githooks", json!({})).unwrap();
    assert_eq!(
        read().get::<Value>(&key("githooks")).unwrap(),
        Some(json!({}))
    );
    project
        .unset(key("githooks"), Duration::ZERO, &Cancellation::default())
        .unwrap();
    assert_eq!(
        read().get::<Value>(&key("githooks")).unwrap(),
        Some(inherited)
    );
    let sources = [
        ConfigSource::Command,
        ConfigSource::Environment,
        ConfigSource::Project,
        ConfigSource::Global,
        ConfigSource::Distribution,
    ];
    for winner in 0..sources.len() {
        let layers = sources[winner..].iter().map(|source| {
            let hooks = json!({"pre-commit":[format!("printf 'Unicode λ, {source:?}'")]});
            document(
                *source,
                &[(
                    "githooks",
                    if *source == ConfigSource::Environment {
                        json!(hooks.to_string())
                    } else {
                        hooks
                    },
                )],
            )
        });
        let snapshot = ConfigSnapshot::new(&Brand::default(), layers).unwrap();
        let hooks: std::collections::BTreeMap<String, Vec<String>> =
            snapshot.get(&key("githooks")).unwrap().unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(
            hooks["pre-commit"],
            [format!("printf 'Unicode λ, {:?}'", sources[winner])]
        );
        assert_eq!(snapshot.source(&key("githooks")), Some(sources[winner]));
    }
    assert_eq!(
        key("githooks").environment_variable(&Brand::default()),
        "GRAPHIT_GITHOOKS"
    );
    for removed in [
        "git.hooks",
        "settings.githooks",
        "id",
        "name",
        "cluster",
        "description",
        "ephemeral",
        "metadata",
    ] {
        assert!(ConfigKey::new(removed).is_err(), "{removed}");
    }
}

#[test]
fn invalid_hook_shapes_and_duplicate_representations_fail_without_repair() {
    let brand = Brand::default();
    for value in [
        json!(null),
        json!(false),
        json!([]),
        json!("pre-commit=a,b"),
        json!({"invalid-event":["ok"]}),
        json!({"pre-commit":[""]}),
        json!({"pre-commit":["a\0b"]}),
        json!({"pre-commit":[false]}),
        json!({"proc-receive":["one", "two"]}),
    ] {
        assert!(
            ConfigDocument::new(&brand, ConfigSource::Global, [(key("githooks"), value)]).is_err()
        );
    }
    for bytes in [
        br#"{"settings":{"githooks":{}}}"#.as_slice(),
        br#"{"githooks":{},"settings":{"githooks":{}}}"#,
        br#"{"settings":{"git.hooks":{}}}"#,
        br#"{"githooks":null,"settings":{}}"#,
        br#"{"githooks":{},"githooks":{},"settings":{}}"#,
        br#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","settings":{}}"#,
    ] {
        assert!(ConfigDocument::decode(&brand, ConfigSource::Global, bytes).is_err());
    }
    let fixture = Fixture::new();
    let project = ConfigStore::project(&fixture.project()).unwrap();
    assert_eq!(
        set(&project, "custom.value", json!(true))
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
    assert!(!project.path().exists());
}
