use super::*;
use crate::native::{NativeBinding, ToolGrants};
use graphit_core::artifact_store::SourceSnapshot;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path};

fn config(
    entries: impl IntoIterator<Item = (ResourceSetting, Value)>,
    source: ConfigSource,
) -> ConfigSnapshot {
    let brand = Brand::default();
    ConfigSnapshot::new(
        &brand,
        [ConfigDocument::new(
            &brand,
            source,
            entries
                .into_iter()
                .map(|(setting, value)| (ConfigKey::new(setting.spec().key).unwrap(), value)),
        )
        .unwrap()],
    )
    .unwrap()
}
fn default_policy() -> HubResourcePolicy {
    HubResourcePolicy::resolve(&ConfigSnapshot::new(&Brand::default(), []).unwrap()).unwrap()
}

#[test]
fn catalog_maps_every_documented_key_and_default_to_a_resolved_policy() {
    let policy = default_policy();
    let mut documented = BTreeSet::new();
    let contract = include_str!("../../../../docs/specs/hub_consumer_contract.md");
    for row in contract
        .lines()
        .filter(|line| line.starts_with("| `") && line.contains(" | "))
    {
        let columns: Vec<_> = row.split('|').collect();
        let group = columns[1].trim().trim_matches('`');
        let Some((prefix, suffixes)) = group.split_once('.') else {
            continue;
        };
        if !RESOURCE_LIMITS
            .iter()
            .any(|spec| spec.key.starts_with(&format!("hub.limits.{prefix}.")))
        {
            continue;
        }
        let numbers: Vec<u64> = columns[2]
            .split(',')
            .map(|n| n.trim().parse().unwrap())
            .collect();
        let suffixes: Vec<_> = suffixes.split(',').map(str::trim).collect();
        assert_eq!(suffixes.len(), numbers.len());
        for (suffix, number) in suffixes.into_iter().zip(numbers) {
            let key = format!("hub.limits.{prefix}.{suffix}");
            let spec = RESOURCE_LIMITS.iter().find(|spec| spec.key == key).unwrap();
            assert_eq!(spec.default_value, number, "{key}");
            assert!(documented.insert(key));
            assert_eq!(
                policy.limit(spec.setting),
                ResolvedLimit {
                    setting: spec.setting,
                    value: number,
                    origin: LimitOrigin::DistributionDefault
                }
            );
        }
    }
    assert_eq!(documented.len(), RESOURCE_LIMITS.len());
    let distribution = HubResourcePolicy::distribution_defaults(&Brand::default()).unwrap();
    let explicit = HubResourcePolicy::resolve(
        &ConfigSnapshot::new(&Brand::default(), [distribution]).unwrap(),
    )
    .unwrap();
    for spec in RESOURCE_LIMITS {
        assert_eq!(
            explicit.limit(spec.setting).origin,
            LimitOrigin::Configuration(ConfigSource::Distribution)
        );
    }
    // Exercise the actual validated consumers, not only the catalog array.
    assert_eq!(policy.copy().node_limit(), 100000);
    assert_eq!(policy.document().json_nodes, 1000000);
    assert_eq!(policy.content().max_depth, 32);
    assert_eq!(policy.shared().journal_bytes, 4194304);
    assert_eq!(policy.transport().inflight, 32);
}

#[test]
fn every_key_rejects_invalid_winners_and_preserves_origin_without_fallback() {
    for spec in RESOURCE_LIMITS {
        for source in [
            ConfigSource::Command,
            ConfigSource::Environment,
            ConfigSource::Project,
            ConfigSource::Global,
            ConfigSource::Distribution,
        ] {
            let policy = HubResourcePolicy::resolve(&config(
                [(spec.setting, json!(spec.default_value))],
                source,
            ))
            .unwrap();
            assert_eq!(
                policy.limit(spec.setting).origin,
                LimitOrigin::Configuration(source)
            );
            for invalid in [
                json!(0),
                json!(-1),
                json!(1.5),
                json!(true),
                json!([]),
                json!({}),
                json!("private-invalid-input-marker"),
            ] {
                let brand = Brand::default();
                let key = ConfigKey::new(spec.key).unwrap();
                let winner = ConfigDocument::new(&brand, source, [(key, invalid)]).unwrap();
                let mut layers = vec![winner];
                if source != ConfigSource::Distribution {
                    layers.push(HubResourcePolicy::distribution_defaults(&brand).unwrap());
                }
                let error =
                    HubResourcePolicy::resolve(&ConfigSnapshot::new(&brand, layers).unwrap())
                        .unwrap_err();
                assert_eq!(error.setting, spec.setting);
                assert_eq!(error.origin, LimitOrigin::Configuration(source));
                assert!(!format!("{error:?} {error}").contains("private-invalid-input-marker"));
            }
            assert!(
                ConfigDocument::new(
                    &Brand::default(),
                    source,
                    [(ConfigKey::new(spec.key).unwrap(), Value::Null)]
                )
                .is_err()
            );
        }
        if let Some(max) = spec.hard_max {
            let error = HubResourcePolicy::resolve(&config(
                [(spec.setting, json!(max + 1))],
                ConfigSource::Command,
            ))
            .unwrap_err();
            assert_eq!(error.setting, spec.setting);
            assert_eq!(error.kind, PolicyErrorKind::HardBound);
        }
    }
}

#[test]
fn captured_precedence_environment_json_and_generic_resource_keys_are_exact() {
    let brand = Brand::default();
    let key = ConfigKey::new(ResourceSetting::ContentDepth.spec().key).unwrap();
    let snapshot = |sources: &[ConfigSource]| {
        ConfigSnapshot::new(
            &brand,
            sources.iter().map(|source| {
                ConfigDocument::new(&brand, *source, [(key.clone(), json!(10 + *source as u64))])
                    .unwrap()
            }),
        )
        .unwrap()
    };
    let all = [
        ConfigSource::Command,
        ConfigSource::Environment,
        ConfigSource::Project,
        ConfigSource::Global,
        ConfigSource::Distribution,
    ];
    for index in 0..all.len() {
        let selected = HubResourcePolicy::resolve(&snapshot(&all[index..]))
            .unwrap()
            .limit(ResourceSetting::ContentDepth);
        assert_eq!(selected.value, 10 + all[index] as u64);
        assert_eq!(selected.origin, LimitOrigin::Configuration(all[index]));
    }
    let env = HubResourcePolicy::resolve(&config(
        [(ResourceSetting::ContentDepth, json!("42"))],
        ConfigSource::Environment,
    ))
    .unwrap();
    assert_eq!(env.limit(ResourceSetting::ContentDepth).value, 42);
    assert!(
        HubResourcePolicy::resolve(&config(
            [(ResourceSetting::ContentDepth, json!("42"))],
            ConfigSource::Command
        ))
        .is_err()
    );
    // Native binding settings cannot silently specialize the generic resource packet.
    let unrelated = ConfigKey::new("hub.agents.kiro_v3.limits.content.depth").unwrap();
    let snapshot = ConfigSnapshot::new(
        &brand,
        [ConfigDocument::new(&brand, ConfigSource::Command, [(unrelated, json!(1))]).unwrap()],
    )
    .unwrap();
    assert_eq!(
        HubResourcePolicy::resolve(&snapshot)
            .unwrap()
            .limit(ResourceSetting::ContentDepth)
            .value,
        32
    );
}

#[test]
fn coherent_tightening_and_checked_boundaries_precede_any_io() {
    let tightened = config(
        RESOURCE_LIMITS
            .iter()
            .map(|spec| (spec.setting, json!(spec.default_value / 2))),
        ConfigSource::Project,
    );
    let policy = HubResourcePolicy::resolve(&tightened).unwrap();
    assert_eq!(policy.content().max_input_bytes, 4194304);
    assert_eq!(policy.transport().inflight, 16);
    for (setting, value) in [
        (ResourceSetting::LineageTotalBytes, 128),
        (ResourceSetting::RetainedFiles, 10001),
        (ResourceSetting::ArtifactKeyBytes, 32768),
        (ResourceSetting::ContentScalarBytes, 16777216),
        (ResourceSetting::TransportCollectionEntries, 100001),
    ] {
        assert_eq!(
            HubResourcePolicy::resolve(&config([(setting, json!(value))], ConfigSource::Project))
                .unwrap_err()
                .kind,
            PolicyErrorKind::Incoherent
        );
    }
    let error = HubResourcePolicy::resolve(&config(
        [(ResourceSetting::ContentNodes, json!(u64::MAX))],
        ConfigSource::Command,
    ))
    .unwrap_err();
    assert_eq!(error.kind, PolicyErrorKind::Overflow);
    assert_eq!(mib_to_bytes(8), Some(8388608));
    assert_eq!(mib_to_bytes(0), None);
    assert_eq!(mib_to_bytes(u64::MAX), None);
    // A streamed u64 capacity does not allocate its ceiling or convert it to usize.
    assert!(
        HubResourcePolicy::resolve(&config(
            [(ResourceSetting::CopyBytes, json!(u64::MAX))],
            ConfigSource::Command
        ))
        .is_ok()
    );
    #[cfg(target_pointer_width = "32")]
    assert_eq!(
        HubResourcePolicy::resolve(&config(
            [(ResourceSetting::CopyNodes, json!(u64::from(u32::MAX) + 1))],
            ConfigSource::Command
        ))
        .unwrap_err()
        .kind,
        PolicyErrorKind::Overflow
    );
}

#[test]
fn frozen_maintained_corpus_fits_snapshot_and_actual_descriptor_rendering() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/policy-corpus");
    let manifest: Value =
        serde_json::from_slice(&fs::read(corpus.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["source_commit"],
        "e12fe4bc77ffb1fadb914139358c855a77875667"
    );
    let files = manifest["files"].as_array().unwrap();
    assert_eq!(files.len(), 22);
    assert_eq!(
        files
            .iter()
            .map(|f| f["bytes"].as_u64().unwrap())
            .sum::<u64>(),
        298946
    );
    assert_eq!(
        files.iter().map(|f| f["bytes"].as_u64().unwrap()).max(),
        Some(31623)
    );
    let directory = tempfile::tempdir().unwrap();
    let mut retained = Vec::new();
    for file in files {
        let path = file["path"].as_str().unwrap();
        let bytes = fs::read(corpus.join(path)).unwrap();
        assert_eq!(bytes.len() as u64, file["bytes"]);
        assert_eq!(
            Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            file["sha256"]
        );
        let destination = directory.path().join(path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, &bytes).unwrap();
        if path.ends_with("SKILL.md") || path.starts_with(".agents/agents/") {
            retained.push(path.into());
        }
    }
    let policy = default_policy();
    let cancellation = Cancellation::default();
    let mut request = policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let snapshot = SourceSnapshot::capture(
        directory.path(),
        graphit_fs::NodeKind::Directory,
        &policy
            .snapshot_policy(CopyPolicy::RejectFileLinks, retained)
            .unwrap(),
        request.snapshot(),
    )
    .unwrap();

    assert!(snapshot.retained_bytes() <= 8388608);
    let mut agents = 0;
    let mut skills = 0;
    for file in files {
        let path = file["path"].as_str().unwrap();
        let Some(bytes) = snapshot.retained(Path::new(path)) else {
            continue;
        };
        if path.ends_with("SKILL.md") {
            let descriptor = request.parse_skill(bytes).unwrap();
            let output = request
                .render_skill(NativeBinding::KiroV3, &descriptor, None)
                .unwrap();
            assert!(
                std::str::from_utf8(&output.bytes)
                    .unwrap()
                    .contains(descriptor.body())
            );
            skills += 1;
        } else {
            // Managed roles are trusted producer fields, not imported native tool grants.
            let text = std::str::from_utf8(bytes).unwrap();
            let (header, body) = text
                .strip_prefix("---\n")
                .unwrap()
                .split_once("---\n")
                .unwrap();
            let fields: Value = serde_saphyr::from_str(header).unwrap();
            let descriptor = request
                .managed_agent(
                    fields["name"].as_str().unwrap(),
                    fields["description"].as_str().unwrap(),
                    body,
                    true,
                    false,
                )
                .unwrap();
            let output = request
                .render_agent(
                    NativeBinding::KiroV3,
                    &descriptor,
                    None,
                    &ToolGrants {
                        binding: NativeBinding::KiroV3,
                        read_only: true,
                        allows_shell: false,
                        tools: Vec::new(),
                    },
                )
                .unwrap();
            assert!(!output.bytes.is_empty());
            agents += 1;
        }
    }
    assert_eq!((agents, skills), (3, 5));
    snapshot.revalidate_content(request.snapshot()).unwrap();
}

#[test]
fn mp_asset_workload_streams_eight_mib_and_retains_only_its_descriptor() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("assets")).unwrap();
    for index in 0..16 {
        fs::write(
            directory.path().join(format!("assets/{index}.bin")),
            vec![index as u8; 512 * 1024],
        )
        .unwrap();
    }
    let descriptor =
        b"---\nname: resource-workload\ndescription: MP asset admission\n---\nAsset workload\n";
    fs::write(directory.path().join("SKILL.md"), descriptor).unwrap();
    let policy = default_policy();
    let cancel = Cancellation::default();
    let mut request = policy.request_context(Instant::now(), &cancel).unwrap();
    let snapshot = SourceSnapshot::capture(
        directory.path(),
        graphit_fs::NodeKind::Directory,
        &policy
            .snapshot_policy(CopyPolicy::RejectFileLinks, ["SKILL.md".into()])
            .unwrap(),
        request.snapshot(),
    )
    .unwrap();
    assert_eq!(snapshot.retained_bytes(), descriptor.len());

    let parsed = request
        .parse_skill(snapshot.retained(Path::new("SKILL.md")).unwrap())
        .unwrap();
    assert!(
        request
            .render_skill(NativeBinding::KiroV3, &parsed, None)
            .is_ok()
    );
    snapshot.revalidate_content(request.snapshot()).unwrap();
}

#[test]
fn request_context_preserves_failed_parse_limits_cancel_and_deadline() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.md");
    fs::write(&source, b"abcde").unwrap();
    let policy = default_policy();
    let cancel = Cancellation::default();
    let mut request = policy.request_context(Instant::now(), &cancel).unwrap();
    let snapshot_policy = policy
        .snapshot_policy(CopyPolicy::RejectFileLinks, [])
        .unwrap();
    for _ in 0..3 {
        SourceSnapshot::capture(
            &source,
            graphit_fs::NodeKind::File,
            &snapshot_policy,
            request.snapshot(),
        )
        .unwrap();
    }
    for _ in 0..2 {
        assert_eq!(
            request.parse_skill(b"invalid").unwrap_err(),
            ContentError::InvalidContent
        );
    }
    assert_eq!(request.input_bytes(), 14); // Failed parses remain charged.
    cancel.cancel();
    assert_eq!(
        request.remaining().unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(
        request.parse_skill(b"more").unwrap_err(),
        ContentError::Cancelled
    );
    assert_eq!(
        SourceSnapshot::capture(
            &source,
            graphit_fs::NodeKind::File,
            &snapshot_policy,
            request.snapshot()
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(fs::read(source).unwrap(), b"abcde");
    let fresh = Cancellation::default();
    assert!(matches!(
        RequestContext::new(policy.content(), &fresh, Instant::now()),
        Err(RequestError::Content(ContentError::TimedOut))
    ));
    assert!(
        RequestContext::new(
            policy.content(),
            &fresh,
            Instant::now() + Duration::from_secs(1)
        )
        .is_ok()
    );
}

#[test]
fn aggregate_content_admits_multiple_captures_under_a_smaller_per_capture_retention() {
    let policy = HubResourcePolicy::resolve(&config(
        [(ResourceSetting::RetainedBytes, json!(1048576))],
        ConfigSource::Command,
    ))
    .unwrap();
    assert_eq!(policy.content().max_input_bytes, 8388608);
    let directory = tempfile::tempdir().unwrap();
    let cancel = Cancellation::default();
    let mut request = policy.request_context(Instant::now(), &cancel).unwrap();
    for index in 0..2 {
        let path = directory.path().join(format!("skill-{index}.md"));
        let input = format!(
            "---\nname: skill-{index}\ndescription: Aggregate quota\n---\n{}\n",
            "a".repeat(600 * 1024)
        );
        fs::write(&path, input).unwrap();
        let snapshot = SourceSnapshot::capture(
            &path,
            graphit_fs::NodeKind::File,
            &policy
                .snapshot_policy(CopyPolicy::RejectFileLinks, ["".into()])
                .unwrap(),
            request.snapshot(),
        )
        .unwrap();
        assert!(snapshot.retained_bytes() < 1048576);
        let descriptor = request
            .parse_skill(snapshot.retained(Path::new("")).unwrap())
            .unwrap();
        request
            .render_skill(NativeBinding::KiroV3, &descriptor, None)
            .unwrap();
    }
    assert!(request.input_bytes() > 1048576);
    assert!(request.output_bytes() > 1048576);
}

#[test]
fn service_request_context_preserves_original_cancellation_without_deadline() {
    let policy = default_policy();
    let cancel = Cancellation::default();
    let request = RequestContext::service(policy.content(), &cancel).unwrap();
    assert_eq!(request.deadline(), None);
    assert_eq!(request.remaining().unwrap(), None);
    assert!(request.snapshot().same_request(request.snapshot()));
    cancel.cancel();
    assert_eq!(
        request.check().unwrap_err().kind(),
        std::io::ErrorKind::Interrupted
    );
    assert_eq!(
        request.remaining().unwrap_err().kind(),
        std::io::ErrorKind::Interrupted
    );
}
