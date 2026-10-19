use super::*;
use graphit_core::{brand::Brand, config::ConfigDocument, file_lock::Cancellation};
use serde_json::{Value, json};

fn capture(layers: Vec<(ConfigSource, Value)>) -> io::Result<Option<ForegroundTaskConfig>> {
    let brand = Brand::default();
    let documents = layers
        .into_iter()
        .map(|(source, value)| {
            ConfigDocument::new(
                &brand,
                source,
                [(ConfigKey::new("task.browser").unwrap(), value)],
            )
        })
        .collect::<io::Result<Vec<_>>>()?;
    let snapshot = ConfigSnapshot::new(&brand, documents)?;
    let hub = HubResourcePolicy::resolve(&snapshot).map_err(io::Error::other)?;
    let cancel = Cancellation::default();
    let original = hub
        .request_context(Instant::now(), &cancel)
        .map_err(io::Error::other)?;
    ForegroundTaskConfig::capture(
        &snapshot,
        &hub,
        ScopeLimits::new(2, 4096, 64)?,
        NativeDocumentFileLimits::new(65536, 4096, 32, 4096)?,
        NativeBinding::Codex,
        original.snapshot(),
    )
}
fn valid() -> Value {
    json!({"providers":["broker"],"project_endpoint":"https://broker.example/v1/project/access/authorize"})
}

#[test]
fn browser_csv_providers_use_the_same_capture_validation_and_precedence() {
    for source in [
        ConfigSource::Global,
        ConfigSource::Environment,
        ConfigSource::Command,
    ] {
        let mut value = valid();
        value["providers"] = json!("first, second");
        let value = if source == ConfigSource::Environment {
            json!(value.to_string())
        } else {
            value
        };
        let captured = capture(vec![(source, value)]).unwrap().unwrap();
        assert_eq!(captured.browser_providers(), ["first", "second"]);
    }
    let mut command = valid();
    command["providers"] = json!("selected");
    let captured = capture(vec![
        (ConfigSource::Global, valid()),
        (ConfigSource::Command, command),
    ])
    .unwrap()
    .unwrap();
    assert_eq!(captured.browser_providers(), ["selected"]);
    for providers in [
        json!(""),
        json!("  "),
        json!("first,,second"),
        json!("first,"),
        json!("first, first"),
        json!(null),
        json!(false),
        json!(123),
        json!(["first", 123]),
    ] {
        let mut config = valid();
        config["providers"] = providers;
        assert!(capture(vec![(ConfigSource::Global, config)]).is_err());
    }
}

#[test]
fn global_opt_in_ignores_project_and_preserves_sdk_bounds() {
    assert!(capture(vec![]).unwrap().is_none());
    assert!(
        capture(vec![(ConfigSource::Project, json!({}))])
            .unwrap()
            .is_none()
    );
    let mut global = valid();
    global["public_origin"] = json!("https://graphit.example");
    let mut project = valid();
    project["public_origin"] = json!("https://untrusted-project.example");
    let captured = capture(vec![
        (ConfigSource::Global, global),
        (ConfigSource::Project, project),
    ])
    .unwrap()
    .unwrap();
    assert_eq!(
        captured.config.public_origin.as_deref(),
        Some("https://graphit.example")
    );
    assert_eq!(captured.browser.login_timeout, Duration::from_secs(300));
    assert_eq!(captured.browser.session_lifetime, Duration::from_secs(3600));
    assert_eq!(captured.browser.refresh_margin, Duration::from_secs(60));
    assert!(!captured.browser.allow_loopback_http);
    assert_eq!(captured.task.read.batch_rows.get(), 8192);
    let mut zero = valid();
    zero["refresh_margin_ms"] = json!(0);
    assert!(
        capture(vec![(ConfigSource::Command, zero)])
            .unwrap()
            .unwrap()
            .browser
            .refresh_margin
            .is_zero()
    );
}
#[test]
fn invalid_objects_are_causal_and_never_become_disabled() {
    for value in [
        json!(null),
        json!("false"),
        json!({}),
        json!({"providers":[],"project_endpoint":"x"}),
        json!({"providers":[" "],"project_endpoint":"x"}),
        json!({"providers":["x","x"],"project_endpoint":"x"}),
        json!({"providers":["x"],"project_endpoint":" "}),
    ] {
        let error = match capture(vec![(ConfigSource::Global, value)]) {
            Err(e) => e,
            Ok(_) => panic!("invalid browser configuration accepted"),
        };
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
    for (key, value) in [
        ("login_timeout_ms", json!(0)),
        ("session_lifetime_ms", json!(0)),
        ("refresh_margin_ms", json!(-1)),
        ("allow_loopback_http", json!("true")),
        ("public_origin", json!(false)),
        ("unknown", json!(true)),
    ] {
        let mut config = valid();
        config[key] = value;
        assert!(capture(vec![(ConfigSource::Global, config)]).is_err());
    }
}
#[test]
fn environment_json_and_command_precedence_use_core_capture() {
    let brand = Brand::default();
    let key = ConfigKey::new("task.browser").unwrap();
    let env = ConfigSnapshot::capture_environment(
        &brand,
        std::slice::from_ref(&key),
        [(
            key.environment_variable(&brand).into(),
            valid().to_string().into(),
        )],
    )
    .unwrap();
    let snapshot = ConfigSnapshot::new(&brand, [env]).unwrap();
    assert_eq!(
        snapshot
            .get::<BrowserConfig>(&key)
            .unwrap()
            .unwrap()
            .providers,
        ["broker"]
    );
    let mut command = valid();
    command["refresh_margin_ms"] = json!(0);
    let captured = capture(vec![
        (ConfigSource::Global, json!({"providers":[]})),
        (ConfigSource::Command, command),
    ])
    .unwrap()
    .unwrap();
    assert_eq!(captured.browser.refresh_margin, Duration::ZERO);
}
#[test]
fn cancelled_original_does_not_capture_policy() {
    let brand = Brand::default();
    let config = ConfigSnapshot::new(&brand, []).unwrap();
    let hub = HubResourcePolicy::resolve(&config).unwrap();
    let cancellation = Cancellation::default();
    let original = hub.request_context(Instant::now(), &cancellation).unwrap();
    cancellation.cancel();
    let result = ForegroundTaskConfig::capture(
        &config,
        &hub,
        ScopeLimits::new(1, 4096, 64).unwrap(),
        NativeDocumentFileLimits::new(65536, 4096, 32, 4096).unwrap(),
        NativeBinding::Codex,
        original.snapshot(),
    );
    assert!(result.is_err());
}
