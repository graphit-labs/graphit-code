use graphit_core::brand::Brand;
use graphit_core::config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource, ConfigStore};
use graphit_core::file_lock::Cancellation;
use graphit_core::module_settings::{Module, ModuleSettings};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::time::Duration;

mod support;
use support::Fixture;

fn document(brand: &Brand, source: ConfigSource, values: &[(&str, Value)]) -> ConfigDocument {
    ConfigDocument::new(
        brand,
        source,
        values
            .iter()
            .map(|(key, value)| (ConfigKey::new(key).unwrap(), value.clone())),
    )
    .unwrap()
}

fn snapshot(layers: impl IntoIterator<Item = ConfigDocument>) -> ConfigSnapshot {
    ConfigSnapshot::new(&Brand::default(), layers).unwrap()
}

#[test]
fn registry_defaults_and_explicit_flags_cover_every_builtin() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ModuleSettings>();
    let defaults = ModuleSettings::resolve(&snapshot([])).unwrap();
    assert_eq!(Module::ALL.len(), 12);
    let names: BTreeSet<_> = Module::ALL.map(Module::name).into_iter().collect();
    assert_eq!(names.len(), 12);
    for (module, name, enabled) in [
        (Module::Task, "task", true),
        (Module::Memory, "memory", true),
        (Module::Ast, "ast", true),
        (Module::Knowledge, "knowledge", true),
        (Module::Hub, "hub", true),
        (Module::Daemon, "daemon", true),
        (Module::Sync, "sync", true),
        (Module::Embedding, "embedding", true),
        (Module::Hooks, "hooks", true),
        (Module::Agent, "agent", true),
        (Module::Dream, "dream", false),
        (Module::DaemonUi, "daemon_ui", false),
    ] {
        assert_eq!(module.name(), name);
        assert_eq!(defaults.enabled(module), enabled);
    }
    for (index, module) in Module::ALL.into_iter().enumerate() {
        assert_eq!(module as usize, index);
        assert_eq!(module.key().as_str(), format!("modules.{}", module.name()));
        assert_eq!(defaults.enabled(module), module.default_enabled());
        for value in [false, true] {
            let source = if module == Module::DaemonUi {
                ConfigSource::Environment
            } else {
                ConfigSource::Command
            };
            let captured = snapshot([document(
                &Brand::default(),
                source,
                &[(module.key().as_str(), json!(value))],
            )]);
            let resolved = ModuleSettings::resolve(&captured).unwrap();
            assert_eq!(resolved.enabled(module), value);
            for other in Module::ALL.into_iter().filter(|other| *other != module) {
                assert_eq!(resolved.enabled(other), defaults.enabled(other));
            }
        }
    }
    let headless = ModuleSettings::resolve(&snapshot([
        document(
            &Brand::default(),
            ConfigSource::Project,
            &[("modules.agent", json!(false))],
        ),
        document(
            &Brand::default(),
            ConfigSource::Environment,
            &[("modules.daemon_ui", json!("true"))],
        ),
    ]))
    .unwrap();
    assert!(!headless.enabled(Module::Agent));
    assert!(headless.enabled(Module::DaemonUi));
    assert!(!headless.enabled(Module::Dream));
    for module in [
        Module::Ast,
        Module::Knowledge,
        Module::Embedding,
        Module::Hub,
        Module::Task,
        Module::Memory,
    ] {
        assert!(headless.enabled(module));
    }
}

#[test]
fn precedence_uses_presence_and_ignores_shadowed_invalid_values() {
    let sources = [
        ConfigSource::Command,
        ConfigSource::Environment,
        ConfigSource::Project,
        ConfigSource::Global,
        ConfigSource::Distribution,
    ];
    for module in Module::ALL
        .into_iter()
        .filter(|module| *module != Module::DaemonUi)
    {
        for winner in 0..sources.len() {
            for value in [false, true] {
                let layers = sources[winner..].iter().enumerate().map(|(index, source)| {
                    document(
                        &Brand::default(),
                        *source,
                        &[(
                            module.key().as_str(),
                            if index == 0 {
                                json!(value)
                            } else {
                                json!("invalid-lower-layer")
                            },
                        )],
                    )
                });
                assert_eq!(
                    ModuleSettings::resolve(&snapshot(layers))
                        .unwrap()
                        .enabled(module),
                    value
                );
            }
        }
    }
}

#[test]
fn invalid_winners_fail_without_fallback_or_payload_in_errors() {
    const CANARY: &str = "artificial-module-canary";
    for source in [
        ConfigSource::Command,
        ConfigSource::Environment,
        ConfigSource::Project,
        ConfigSource::Global,
        ConfigSource::Distribution,
    ] {
        for value in [
            json!(CANARY),
            json!(""),
            json!("TRUE"),
            json!("1"),
            json!("yes"),
            json!("\"false\""),
            json!(0),
            json!([true]),
            json!({"canary": CANARY}),
        ] {
            let mut layers = vec![document(
                &Brand::default(),
                source,
                &[("modules.agent", value)],
            )];
            if source != ConfigSource::Distribution {
                layers.push(document(
                    &Brand::default(),
                    ConfigSource::Distribution,
                    &[("modules.agent", json!(true))],
                ));
            }
            let error = ModuleSettings::resolve(&snapshot(layers)).unwrap_err();
            let kind = if matches!(source, ConfigSource::Global | ConfigSource::Project) {
                io::ErrorKind::InvalidData
            } else {
                io::ErrorKind::InvalidInput
            };
            assert_eq!(error.kind(), kind);
            assert!(!error.to_string().contains(CANARY));
            assert!(!format!("{error:?}").contains(CANARY));
        }
    }
    for value in [" true ", "\nfalse\t"] {
        let resolved = ModuleSettings::resolve(&snapshot([document(
            &Brand::default(),
            ConfigSource::Environment,
            &[("modules.agent", json!(value))],
        )]))
        .unwrap();
        assert_eq!(resolved.enabled(Module::Agent), value.trim() == "true");
    }
}

#[test]
fn daemon_ui_is_environment_only_with_explicit_brand_capture() {
    for source in [
        ConfigSource::Command,
        ConfigSource::Project,
        ConfigSource::Global,
        ConfigSource::Distribution,
    ] {
        assert!(
            ConfigDocument::new(
                &Brand::default(),
                source,
                [(Module::DaemonUi.key(), json!(true))]
            )
            .is_err()
        );
    }
    let brand = Brand::new("acme-code").unwrap();
    let keys = Module::ALL.map(Module::key);
    let variables = [
        (
            OsString::from("ACME_CODE_MODULES__DAEMON_UI"),
            OsString::from("true"),
        ),
        (
            OsString::from("ACME_CODE_MODULES__AGENT"),
            OsString::from("false"),
        ),
        (
            OsString::from("GRAPHIT_MODULES__AGENT"),
            OsString::from("true"),
        ),
    ];
    let environment =
        ConfigSnapshot::capture_environment(&brand, &keys, variables.clone()).unwrap();
    let private = ConfigSnapshot::new(&brand, [environment]).unwrap();
    let modules = ModuleSettings::resolve(&private).unwrap();
    assert!(modules.enabled(Module::DaemonUi));
    assert!(!modules.enabled(Module::Agent));
    let environment =
        ConfigSnapshot::capture_environment(&Brand::default(), &keys, variables).unwrap();
    let other = ModuleSettings::resolve(&snapshot([environment])).unwrap();
    assert!(!other.enabled(Module::DaemonUi));
    assert!(other.enabled(Module::Agent));
}

#[test]
fn independent_views_can_be_shared_without_changing_other_settings() {
    let global = document(
        &Brand::default(),
        ConfigSource::Global,
        &[
            ("modules.agent", json!(false)),
            ("custom.unrelated", json!({"any": [1, 2, 3]})),
        ],
    );
    let first = ModuleSettings::resolve(&snapshot([global.clone()])).unwrap();
    let second = ModuleSettings::resolve(&snapshot([
        global,
        document(
            &Brand::default(),
            ConfigSource::Project,
            &[("modules.agent", json!(true))],
        ),
    ]))
    .unwrap();
    std::thread::scope(|scope| {
        for view in [first, second] {
            scope.spawn(move || {
                for _ in 0..1000 {
                    assert_eq!(view.enabled(Module::Agent), view == second);
                }
            });
        }
    });
    assert!(!first.enabled(Module::Agent));
    assert!(second.enabled(Module::Agent));
    assert!(first.enabled(Module::Task));
    assert!(second.enabled(Module::Task));
}

#[test]
fn persisted_resolution_reads_without_effects_and_recovers_only_after_explicit_edit() {
    let fixture = Fixture::new();
    let store = ConfigStore::global(&fixture.installation()).unwrap();
    let absent = ModuleSettings::resolve(&snapshot([store.load().unwrap()])).unwrap();
    assert!(absent.enabled(Module::Agent));
    assert!(!store.path().exists());
    let cancellation = Cancellation::default();
    let key = Module::Agent.key();
    store
        .set(key.clone(), json!(false), Duration::ZERO, &cancellation)
        .unwrap();
    let old = ModuleSettings::resolve(&snapshot([store.load().unwrap()])).unwrap();
    let bytes = fs::read(store.path()).unwrap();
    let modified = fs::metadata(store.path()).unwrap().modified().unwrap();
    assert!(
        !ModuleSettings::resolve(&snapshot([store.load().unwrap()]))
            .unwrap()
            .enabled(Module::Agent)
    );
    assert_eq!(fs::read(store.path()).unwrap(), bytes);
    assert_eq!(
        fs::metadata(store.path()).unwrap().modified().unwrap(),
        modified
    );
    store
        .set(key.clone(), json!("invalid"), Duration::ZERO, &cancellation)
        .unwrap();
    let invalid = fs::read(store.path()).unwrap();
    assert_eq!(
        ModuleSettings::resolve(&snapshot([store.load().unwrap()]))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read(store.path()).unwrap(), invalid);
    store
        .set(key, json!(true), Duration::ZERO, &cancellation)
        .unwrap();
    let repaired = ModuleSettings::resolve(&snapshot([store.load().unwrap()])).unwrap();
    assert!(repaired.enabled(Module::Agent));
    assert!(!old.enabled(Module::Agent));
}
