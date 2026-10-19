use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_core::brand::Brand;
use graphit_core::config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource, ConfigStore};
use graphit_core::file_lock::Cancellation;
use graphit_core::installation_identity::{
    InstallationId, InstallationIdentity, InstallationIdentityStore, InstallationSettings,
};
use graphit_core::project_paths::InstallationPaths;
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::process::Command;
use std::time::Duration;

mod support;
use support::Fixture;

fn key(value: &str) -> ConfigKey {
    ConfigKey::new(value).unwrap()
}
fn stores(fixture: &Fixture) -> (InstallationIdentityStore, ConfigStore) {
    let paths = fixture.installation();
    (
        InstallationIdentityStore::new(&paths).unwrap(),
        ConfigStore::global(&paths).unwrap(),
    )
}
fn set(store: &ConfigStore, name: &str, value: Value) {
    store
        .set(
            key(name),
            value,
            Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap();
}
fn fingerprint(identity: &InstallationIdentity) -> String {
    format!(
        "{} {}",
        identity.unit_id().as_str(),
        URL_SAFE_NO_PAD.encode(Sha256::digest(
            identity.client_secret().expose_secret().as_bytes()
        ))
    )
}

#[test]
fn absent_and_partial_reads_have_no_provisioning_side_effects() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<InstallationId>();
    send_sync::<InstallationSettings>();
    send_sync::<InstallationIdentity>();
    let fixture = Fixture::new();
    let (store, config) = stores(&fixture);
    assert!(!store.load().unwrap().is_provisioned());
    assert!(!config.path().parent().unwrap().exists());
    let empty = ConfigSnapshot::new(&Brand::default(), []).unwrap();
    assert!(
        !InstallationSettings::resolve(&empty)
            .unwrap()
            .is_provisioned()
    );
    assert!(!config.path().parent().unwrap().exists());
    set(&config, "unit.id", json!(" Custom-Owner "));
    let before = fs::read(config.path()).unwrap();
    let state = store.load().unwrap();
    assert_eq!(state.unit_id().unwrap().as_str(), "Custom-Owner");
    assert!(state.client_secret().is_none());
    assert_eq!(fs::read(config.path()).unwrap(), before);
}

#[test]
fn generated_pair_is_stable_and_partial_fields_and_other_settings_are_preserved() {
    for preexisting in [None, Some("unit.id"), Some("client.secret")] {
        let fixture = Fixture::new();
        let (store, config) = stores(&fixture);
        set(&config, "custom.enabled", json!(false));
        if let Some(name) = preexisting {
            set(
                &config,
                name,
                json!(if name == "unit.id" {
                    "custom-Unit".into()
                } else {
                    "s".repeat(32)
                }),
            );
        }
        let identity = store
            .ensure(Duration::from_secs(5), &Cancellation::default())
            .unwrap();
        if preexisting == Some("unit.id") {
            assert_eq!(identity.unit_id().as_str(), "custom-Unit");
        } else {
            let id = ulid::Ulid::from_string(identity.unit_id().as_str()).unwrap();
            assert_eq!(id.to_string(), identity.unit_id().as_str());
            assert!(!id.is_nil());
        }
        if preexisting == Some("client.secret") {
            assert_eq!(identity.client_secret().expose_secret(), "s".repeat(32));
        } else {
            assert_eq!(identity.client_secret().expose_secret().len(), 43);
            assert_eq!(
                URL_SAFE_NO_PAD
                    .decode(identity.client_secret().expose_secret())
                    .unwrap()
                    .len(),
                32
            );
        }
        assert_eq!(
            config
                .load()
                .unwrap()
                .get::<bool>(&key("custom.enabled"))
                .unwrap(),
            Some(false)
        );
        let before = fs::read(config.path()).unwrap();
        let modified = fs::metadata(config.path()).unwrap().modified().unwrap();
        let reopened = InstallationIdentityStore::new(&fixture.installation()).unwrap();
        let again = reopened
            .ensure(Duration::ZERO, &Cancellation::default())
            .unwrap();
        assert_eq!(fingerprint(&identity), fingerprint(&again));
        assert_eq!(fs::read(config.path()).unwrap(), before);
        assert_eq!(
            fs::metadata(config.path()).unwrap().modified().unwrap(),
            modified
        );
    }
}

#[test]
fn captured_overrides_are_pure_and_old_views_remain_independent() {
    let fixture = Fixture::new();
    let (store, config) = stores(&fixture);
    let persisted = store
        .ensure(Duration::ZERO, &Cancellation::default())
        .unwrap();
    let before = fs::read(config.path()).unwrap();
    let brand = Brand::default();
    let environment = ConfigSnapshot::capture_environment(
        &brand,
        &[key("unit.id"), key("client.secret")],
        [
            ("GRAPHIT_UNIT__ID".into(), "environment-owner".into()),
            ("GRAPHIT_CLIENT__SECRET".into(), "e".repeat(32).into()),
        ],
    )
    .unwrap();
    let command = ConfigDocument::new(
        &brand,
        ConfigSource::Command,
        [(key("unit.id"), json!("command-owner"))],
    )
    .unwrap();
    let snapshot =
        ConfigSnapshot::new(&brand, [command, environment, config.load().unwrap()]).unwrap();
    let state = InstallationSettings::resolve(&snapshot).unwrap();
    assert_eq!(state.unit_id().unwrap().as_str(), "command-owner");
    assert_eq!(
        state.client_secret().unwrap().expose_secret(),
        "e".repeat(32)
    );
    assert_eq!(fs::read(config.path()).unwrap(), before);
    assert_eq!(
        fingerprint(
            &store
                .ensure(Duration::ZERO, &Cancellation::default())
                .unwrap()
        ),
        fingerprint(&persisted)
    );
    set(&config, "unit.id", json!("later-owner"));
    assert_eq!(state.unit_id().unwrap().as_str(), "command-owner");
    assert_eq!(
        InstallationSettings::resolve(&snapshot)
            .unwrap()
            .unit_id()
            .unwrap()
            .as_str(),
        "command-owner"
    );
}

#[test]
fn invalid_present_fields_fail_without_repair_and_errors_never_echo_values() {
    let fixture = Fixture::new();
    let (store, config) = stores(&fixture);
    fs::create_dir_all(config.path().parent().unwrap()).unwrap();
    for (name, value) in [
        ("unit.id", json!("")),
        ("unit.id", json!("bad/fixture-canary")),
        ("unit.id", json!(7)),
        ("client.secret", json!("fixture-sensitive-canary")),
        ("client.secret", json!(" ".repeat(32))),
        ("client.secret", json!("x".repeat(4097))),
    ] {
        fs::write(
            config.path(),
            serde_json::to_vec(&json!({"settings":{(name): value}})).unwrap(),
        )
        .unwrap();
        let before = fs::read(config.path()).unwrap();
        let error = store
            .ensure(Duration::ZERO, &Cancellation::default())
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(!error.to_string().contains("canary"));
        assert_eq!(fs::read(config.path()).unwrap(), before);
        let document = ConfigDocument::new(
            &Brand::default(),
            ConfigSource::Command,
            [(key(name), value)],
        )
        .unwrap();
        let snapshot = ConfigSnapshot::new(&Brand::default(), [document]).unwrap();
        assert_eq!(
            InstallationSettings::resolve(&snapshot).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    for name in ["_bad", "bad_", "bad/id", "á", "", ".bad", "bad."] {
        assert!(InstallationId::new(name).is_err());
    }
    assert!(InstallationId::new(&"x".repeat(129)).is_err());
    assert!(InstallationId::new(&"x".repeat(128)).is_ok());
    let valid = ConfigDocument::new(
        &Brand::default(),
        ConfigSource::Command,
        [(key("client.secret"), json!("s".repeat(4096)))],
    )
    .unwrap();
    assert!(
        InstallationSettings::resolve(&ConfigSnapshot::new(&Brand::default(), [valid]).unwrap())
            .is_ok()
    );
}

#[test]
fn secret_views_are_redacted_and_installations_remain_independent() {
    const CANARY: &str = "artificial-sensitive-salt-canary-123456";
    let fixture = Fixture::new();
    let (store, config) = stores(&fixture);
    set(&config, "client.secret", json!(CANARY));
    let identity = store
        .ensure(Duration::ZERO, &Cancellation::default())
        .unwrap();
    let state = store.load().unwrap();
    for debug in [
        format!("{identity:?}"),
        format!("{state:?}"),
        format!("{store:?}"),
    ] {
        assert!(!debug.contains(CANARY));
    }
    assert!(
        !serde_json::to_string(&config.load().unwrap().inspect())
            .unwrap()
            .contains(CANARY)
    );
    assert_eq!(identity.client_secret().expose_secret(), CANARY);
    let paths = InstallationPaths::resolve(
        Brand::new("acme-code").unwrap(),
        fixture.root(),
        None,
        Some(&fixture.path("private-global")),
    )
    .unwrap();
    let other = InstallationIdentityStore::new(&paths)
        .unwrap()
        .ensure(Duration::ZERO, &Cancellation::default())
        .unwrap();
    assert_ne!(identity.unit_id(), other.unit_id());
    assert_ne!(
        identity.client_secret().expose_secret(),
        other.client_secret().expose_secret()
    );
}

#[test]
fn the_same_config_target_controls_lock_timeout_aliases_and_cancellation() {
    let fixture = Fixture::new();
    let (store, config) = stores(&fixture);
    let writer = config
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap();
    assert_eq!(
        store
            .ensure(Duration::ZERO, &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(config.path().parent().unwrap(), fixture.path("alias")).unwrap();
        let paths = InstallationPaths::resolve(
            Brand::default(),
            fixture.root(),
            None,
            Some(&fixture.path("alias")),
        )
        .unwrap();
        let alias = InstallationIdentityStore::new(&paths).unwrap();
        assert_eq!(
            alias
                .ensure(Duration::ZERO, &Cancellation::default())
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
    }
    drop(writer);
    assert!(config.lock_path().exists());
    let cancellation = Cancellation::default();
    cancellation.cancel();
    assert_eq!(
        store
            .ensure(Duration::ZERO, &cancellation)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert!(!config.path().exists());
    assert!(
        store
            .ensure(Duration::ZERO, &Cancellation::default())
            .is_ok()
    );
}

#[test]
fn readonly_partial_failure_preserves_state_and_complete_noop_is_allowed() {
    let fixture = Fixture::new();
    let (store, config) = stores(&fixture);
    set(&config, "unit.id", json!("local-owner"));
    let before = fs::read(config.path()).unwrap();
    let original = fs::metadata(config.path()).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(config.path(), readonly.clone()).unwrap();
    assert_eq!(
        store
            .ensure(Duration::ZERO, &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(fs::read(config.path()).unwrap(), before);
    fs::set_permissions(config.path(), original.clone()).unwrap();
    store
        .ensure(Duration::ZERO, &Cancellation::default())
        .unwrap();
    let complete = fs::read(config.path()).unwrap();
    fs::set_permissions(config.path(), readonly).unwrap();
    assert!(
        store
            .ensure(Duration::ZERO, &Cancellation::default())
            .is_ok()
    );
    assert_eq!(fs::read(config.path()).unwrap(), complete);
    fs::set_permissions(config.path(), original).unwrap();
}

#[test]
fn concurrent_processes_observe_one_pair_and_preserve_ordinary_config_edits() {
    let fixture = Fixture::new();
    let mut children: Vec<_> = (0..8)
        .map(|index| {
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "installation_child", "--nocapture"])
                .env("GRAPHIT_INSTALLATION_TEST_ROOT", fixture.root())
                .env("GRAPHIT_INSTALLATION_TEST_INDEX", index.to_string())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in &mut children {
        assert!(child.wait().unwrap().success());
    }
    let first = fs::read_to_string(fixture.path("result-0")).unwrap();
    for index in 1..8 {
        assert_eq!(
            fs::read_to_string(fixture.path(&format!("result-{index}"))).unwrap(),
            first
        );
    }
    let (store, config) = stores(&fixture);
    assert_eq!(
        fingerprint(
            &store
                .ensure(Duration::ZERO, &Cancellation::default())
                .unwrap()
        ),
        first
    );
    let document = config.load().unwrap();
    for index in 0..8 {
        assert_eq!(
            document
                .get::<u64>(&key(&format!("custom.child{index}")))
                .unwrap(),
            Some(index)
        );
    }
}

#[test]
fn installation_child() {
    let Some(root) = std::env::var_os("GRAPHIT_INSTALLATION_TEST_ROOT") else {
        return;
    };
    let index: u64 = std::env::var("GRAPHIT_INSTALLATION_TEST_INDEX")
        .unwrap()
        .parse()
        .unwrap();
    let root = std::path::PathBuf::from(root);
    let paths = InstallationPaths::resolve(
        Brand::default(),
        &root,
        None,
        Some(&root.join("installation")),
    )
    .unwrap();
    let config = ConfigStore::global(&paths).unwrap();
    set(&config, &format!("custom.child{index}"), json!(index));
    let identity = InstallationIdentityStore::new(&paths)
        .unwrap()
        .ensure(Duration::from_secs(5), &Cancellation::default())
        .unwrap();
    fs::write(root.join(format!("result-{index}")), fingerprint(&identity)).unwrap();
}
