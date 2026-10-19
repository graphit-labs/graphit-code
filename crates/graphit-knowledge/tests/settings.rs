use graphit_core::{
    brand::Brand,
    config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource},
    file_io::SnapshotContext,
    file_lock::Cancellation,
};
use graphit_knowledge::settings::{
    KnowledgeSetting, KnowledgeSettings, KnowledgeSettingsError, KnowledgeSettingsLimits,
    SettingOrigin, SettingsProblem, SettingsStats,
};
use std::{
    error::Error,
    io,
    num::NonZeroUsize,
    time::{Duration, Instant},
};

fn cap(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

fn limits() -> KnowledgeSettingsLimits {
    KnowledgeSettingsLimits {
        max_input_bytes: cap(1_000),
        max_docs_dir_bytes: cap(1_000),
        max_path_components: cap(100),
        max_extension_tokens: cap(100),
        max_extensions: cap(100),
        max_extension_bytes: cap(100),
        max_projection_bytes: cap(1_000),
    }
}

#[test]
fn captured_settings_outlive_config_producers_under_original_request() {
    let original = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    );
    let view = {
        let brand = Brand::new("acme-code").unwrap();
        let keys = [
            ConfigKey::new("knowledge.docs_dir").unwrap(),
            ConfigKey::new("knowledge.include_readme").unwrap(),
            ConfigKey::new("knowledge.extensions").unwrap(),
        ];
        let environment = ConfigSnapshot::capture_environment(
            &brand,
            &keys,
            [
                (
                    keys[0].environment_variable(&brand).into(),
                    "invalid/../discarded".into(),
                ),
                (
                    keys[1].environment_variable(&brand).into(),
                    " false ".into(),
                ),
                (
                    keys[2].environment_variable(&brand).into(),
                    "JSON,YAML".into(),
                ),
            ],
        )
        .unwrap();
        let snapshot = ConfigSnapshot::new(&brand, [environment]).unwrap();
        KnowledgeSettings::resolve(
            &original,
            &snapshot,
            Some("."),
            KnowledgeSettingsLimits {
                max_input_bytes: cap(17),
                max_docs_dir_bytes: cap(1),
                max_path_components: cap(1),
                max_extension_tokens: cap(2),
                max_extensions: cap(2),
                max_extension_bytes: cap(5),
                max_projection_bytes: cap(11),
            },
        )
        .unwrap()
    };

    assert_eq!(view.docs_dir(), ".");
    assert!(!view.include_readme());
    assert_eq!(view.extensions(), [".json", ".yaml"]);
    assert_eq!(
        view.origin(KnowledgeSetting::DocsDir),
        SettingOrigin::IndexPath
    );
    for field in [
        KnowledgeSetting::IncludeReadme,
        KnowledgeSetting::Extensions,
    ] {
        assert_eq!(
            view.origin(field),
            SettingOrigin::Config(ConfigSource::Environment)
        );
    }
    assert_eq!(
        view.stats(),
        SettingsStats {
            input_bytes: 17,
            path_components: 1,
            extension_tokens: 2,
            extensions: 2,
            projection_bytes: 11,
        }
    );
}

#[test]
fn public_errors_preserve_winner_origin_and_original_request_liveness() {
    let brand = Brand::default();
    let key = ConfigKey::new("knowledge.include_readme").unwrap();
    let lower =
        ConfigDocument::new(&brand, ConfigSource::Global, [(key.clone(), true.into())]).unwrap();
    let winner = ConfigDocument::new(
        &brand,
        ConfigSource::Project,
        [(key, "invalid-boolean-marker".into())],
    )
    .unwrap();
    let snapshot = ConfigSnapshot::new(&brand, [lower, winner]).unwrap();
    let cancellation = Cancellation::default();
    let context = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancellation.clone(),
    );

    let error = KnowledgeSettings::resolve(&context, &snapshot, None, limits()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(matches!(
        &error,
        KnowledgeSettingsError::InvalidValue {
            field: KnowledgeSetting::IncludeReadme,
            origin: SettingOrigin::Config(ConfigSource::Project),
            reason: SettingsProblem::ExpectedBoolean,
            source: Some(_),
        }
    ));
    assert!(!error.to_string().contains("invalid-boolean-marker"));
    assert!(
        !error
            .source()
            .unwrap()
            .to_string()
            .contains("invalid-boolean-marker")
    );

    cancellation.cancel();
    let cancelled = KnowledgeSettings::resolve(&context, &snapshot, None, limits()).unwrap_err();
    assert!(matches!(cancelled, KnowledgeSettingsError::Context(_)));
    assert_eq!(cancelled.kind(), io::ErrorKind::Interrupted);
}
