use super::*;
use graphit_core::{brand::Brand, config::ConfigSnapshot, file_lock::Cancellation};

fn policy() -> HubResourcePolicy {
    let brand = Brand::new("graphit").unwrap();
    let config = ConfigSnapshot::new(
        &brand,
        [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
    )
    .unwrap();
    HubResourcePolicy::resolve(&config).unwrap()
}

fn manifest(
    versions: &[VersionInput<'_>],
    latest: Option<&str>,
    limits: MetadataLimits,
    original: &RequestContext<'_>,
) -> RemoteArtifactManifest {
    let producer = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
    let encoded = RemoteArtifactManifest::encode(
        &ManifestInput {
            producer,
            artifact_type: ArtifactType::Skill,
            id: "fixture/web-file-reader",
            name: "Retained catalogue",
            description: Some("Retained description"),
            tags: &["retained-tag"],
            author: Some(AuthorInput {
                username: "fixture-author",
            }),
            latest,
            versions,
            dependencies: &[DependencyInput {
                artifact_type: ArtifactType::Skill,
                id: "fixture/dependency",
                selector: "release/dependency",
            }],
        },
        limits,
        original,
    )
    .unwrap();
    RemoteArtifactManifest::decode(&encoded, limits, original).unwrap()
}

const FIRST_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const SECOND_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";

fn versions() -> [VersionInput<'static>; 3] {
    [
        VersionInput {
            version: "release/one",
            hash: FIRST_HASH,
        },
        VersionInput {
            version: "release/two",
            hash: SECOND_HASH,
        },
        VersionInput {
            version: "release/three",
            hash: FIRST_HASH,
        },
    ]
}

#[test]
fn removing_a_version_preserves_other_versions_metadata_and_dependencies() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let limits = native_publication::metadata_limits(policy.content(), policy.projection());
    let versions = versions();
    let current = manifest(&versions, Some("release/three"), limits, &original);
    let target = Target::Version {
        version: "release/two".into(),
        expected_sha256: Some(SECOND_HASH.into()),
    };
    let RemainingCatalog {
        targeted_versions: targeted,
        desired_latest: latest,
        encoded,
    } = remaining_catalog(&current, &target, limits, &original).unwrap();
    assert_eq!(targeted, ["release/two"]);
    assert_eq!(latest.as_deref(), Some("release/three"));
    let after = RemoteArtifactManifest::decode(&encoded.unwrap(), limits, &original).unwrap();
    assert_eq!(after.producer(), current.producer());
    assert_eq!(after.artifact_type(), current.artifact_type());
    assert_eq!(after.id().as_str(), current.id().as_str());
    assert_eq!(after.name(), current.name());
    assert_eq!(after.description(), current.description());
    assert_eq!(after.tags(), current.tags());
    assert_eq!(after.author(), current.author());
    assert_eq!(after.dependencies().len(), 1);
    assert_eq!(after.dependencies()[0].id().as_str(), "fixture/dependency");
    assert_eq!(after.dependencies()[0].selector(), "release/dependency");
    assert_eq!(
        after
            .versions()
            .iter()
            .map(|v| v.version().as_str())
            .collect::<Vec<_>>(),
        ["release/one", "release/three"],
    );
    assert_eq!(after.versions()[0].hash(), current.versions()[0].hash());
    assert_eq!(after.versions()[1].hash(), current.versions()[2].hash());
}

#[test]
fn removing_latest_clears_it_without_promoting_another_version() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let limits = native_publication::metadata_limits(policy.content(), policy.projection());
    let versions = versions();
    let current = manifest(&versions, Some("release/three"), limits, &original);
    let RemainingCatalog {
        targeted_versions: targeted,
        desired_latest: latest,
        encoded,
    } = remaining_catalog(
        &current,
        &Target::Version {
            version: "release/three".into(),
            expected_sha256: None,
        },
        limits,
        &original,
    )
    .unwrap();
    assert_eq!(targeted, ["release/three"]);
    assert!(latest.is_none());
    let after = RemoteArtifactManifest::decode(&encoded.unwrap(), limits, &original).unwrap();
    assert!(after.latest().is_none());
    assert_eq!(
        after
            .versions()
            .iter()
            .map(|v| v.version().as_str())
            .collect::<Vec<_>>(),
        ["release/one", "release/two"],
    );
}

#[test]
fn last_version_and_whole_artifact_require_removal_instead_of_an_empty_manifest() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let limits = native_publication::metadata_limits(policy.content(), policy.projection());
    let versions = versions();
    let only = manifest(&versions[..1], Some("release/one"), limits, &original);
    let RemainingCatalog {
        targeted_versions: targeted,
        desired_latest: latest,
        encoded,
    } = remaining_catalog(
        &only,
        &Target::Version {
            version: "release/one".into(),
            expected_sha256: None,
        },
        limits,
        &original,
    )
    .unwrap();
    assert_eq!(targeted, ["release/one"]);
    assert!(latest.is_none());
    assert!(encoded.is_none());
    let all = manifest(&versions, Some("release/three"), limits, &original);
    let RemainingCatalog {
        targeted_versions: targeted,
        desired_latest: latest,
        encoded,
    } = remaining_catalog(&all, &Target::Artifact {}, limits, &original).unwrap();
    assert_eq!(targeted, ["release/one", "release/two", "release/three"]);
    assert!(latest.is_none());
    assert!(encoded.is_none());
}

#[test]
fn missing_version_and_mismatched_expected_hash_fail_before_catalog_mutation() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let limits = native_publication::metadata_limits(policy.content(), policy.projection());
    let versions = versions();
    let current = manifest(&versions, Some("release/three"), limits, &original);
    for (version, expected, kind) in [
        ("release/missing", None, io::ErrorKind::NotFound),
        (
            "release/two",
            Some(FIRST_HASH.into()),
            io::ErrorKind::AlreadyExists,
        ),
    ] {
        let error = remaining_catalog(
            &current,
            &Target::Version {
                version: version.into(),
                expected_sha256: expected,
            },
            limits,
            &original,
        )
        .unwrap_err();
        assert_eq!(error.kind(), kind);
    }
    assert_eq!(current.versions().len(), 3);
    assert_eq!(current.latest().unwrap().as_str(), "release/three");
}

#[test]
fn shared_input_codec_distinguishes_omission_from_null_and_requires_literal_targets() {
    let policy = policy();
    let base = serde_json::json!({
        "provider": "fixture-provider",
        "producer": "01KSH1CRFFG8Z74B5ZS78WW808",
        "artifact_type": "skill",
        "id": "fixture/web-file-reader",
        "target": { "kind": "version", "version": "release/one" },
    });
    assert!(
        serde_json::from_value::<Arguments>(base.clone())
            .unwrap()
            .validate(&policy)
            .is_ok()
    );
    let mut null_hash = base.clone();
    null_hash["target"]["expected_sha256"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<Arguments>(null_hash).is_err());
    let mut alias = base.clone();
    alias["target"]["version"] = serde_json::json!("latest");
    assert_eq!(
        serde_json::from_value::<Arguments>(alias)
            .unwrap()
            .validate(&policy)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput,
    );
    let mut whole = base.clone();
    whole["target"] = serde_json::json!({ "kind": "artifact" });
    assert!(
        serde_json::from_value::<Arguments>(whole.clone())
            .unwrap()
            .validate(&policy)
            .is_ok()
    );
    whole["target"]["version"] = serde_json::json!("release/one");
    assert!(serde_json::from_value::<Arguments>(whole).is_err());
    let mut project = base;
    project["project_dir"] = serde_json::json!("/not-an-authority");
    assert!(serde_json::from_value::<Arguments>(project).is_err());
}

#[test]
fn conditional_delete_uses_header_sigv4_without_query_credentials() {
    let credentials = AwsCredential {
        key_id: "fixture-access".into(),
        secret_key: "fixture-secret".into(),
        token: Some("fixture-session".into()),
    };
    let key = ObjectPath::from("registry/skill/fixture/web-file-reader.json");
    let url = delete_url(
        Some("http://127.0.0.1:9000/"),
        "fixture-bucket",
        "local",
        &key,
    )
    .unwrap();
    assert_eq!(
        url.as_str(),
        "http://127.0.0.1:9000/fixture-bucket/registry/skill/fixture/web-file-reader.json"
    );
    let request = delete_request(url, "\"observed-etag\"", &credentials, "local").unwrap();
    assert_eq!(request.method(), reqwest::Method::DELETE);
    assert!(request.url().query().is_none());
    assert_eq!(
        request.headers()[reqwest::header::IF_MATCH],
        "\"observed-etag\""
    );
    assert_eq!(request.headers()["x-amz-security-token"], "fixture-session");
    assert!(request.headers().contains_key("x-amz-date"));
    assert_eq!(
        request.headers()["x-amz-content-sha256"],
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    let signature = request.headers()[reqwest::header::AUTHORIZATION]
        .to_str()
        .unwrap();
    assert!(signature.starts_with("AWS4-HMAC-SHA256 Credential=fixture-access/"));
    assert!(signature.contains("/local/s3/aws4_request"));
    assert!(signature.contains(
        "SignedHeaders=host;if-match;x-amz-content-sha256;x-amz-date;x-amz-security-token"
    ));
    assert!(request.body().is_none());
}

#[test]
fn conditional_delete_urls_match_the_sdk_path_style_and_escape_object_keys() {
    let key = ObjectPath::parse("registry/name +%/ação.json").unwrap();
    let path = "/bucket/registry/name%20%2B%25/a%C3%A7%C3%A3o.json";
    for (endpoint, region, base) in [
        (
            Some("https://gateway.example.invalid/"),
            "local",
            "https://gateway.example.invalid",
        ),
        (None, "us-east-1", "https://s3.us-east-1.amazonaws.com"),
        (None, "eu-west-1", "https://s3.eu-west-1.amazonaws.com"),
    ] {
        let url = delete_url(endpoint, "bucket", region, &key).unwrap();
        assert_eq!(url.as_str(), format!("{base}{path}"));
        assert!(url.query().is_none());
        assert!(url.fragment().is_none());
    }
}
