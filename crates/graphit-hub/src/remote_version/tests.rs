use super::*;
use crate::{
    ArtifactType, ProjectionLimits, native::ContentLimits, remote_metadata::MetadataLimits,
};
use graphit_core::file_lock::Cancellation;
use serde_json::json;
use std::{
    thread,
    time::{Duration, Instant},
};

const PRODUCER: &str = "01KSH1CRFFG8Z74B5ZS78WW808";
const CONSUMER: &str = "01M1YEAE1WYMWG9KJKV1XQC4T4";
const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn projection() -> ProjectionLimits {
    ProjectionLimits::new(512, 4096, 128, 255, 128).unwrap()
}
fn context(cancellation: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 64 * 1024,
            max_output_bytes: 64 * 1024,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 1024,
            max_collection_entries: 128,
        },
        cancellation,
        deadline,
    )
    .unwrap()
}
fn limits() -> VersionSelectionLimits {
    VersionSelectionLimits {
        max_versions: 128,
        max_version_bytes: 512,
    }
}
fn manifest(
    kind: ArtifactType,
    versions: &[&str],
    latest: Option<&str>,
    original: &RequestContext<'_>,
) -> RemoteArtifactManifest {
    let mut wire = json!({
        "v": 1, "producer": PRODUCER, "artifact_type": kind,
        "id": "org/tool", "name": "tool",
        "versions": versions.iter().enumerate().map(|(index, version)| {
            json!({ "version": version, "hash": if index % 2 == 0 { HASH } else {
                "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
            } })
        }).collect::<Vec<_>>(), "dependencies": []
    });
    if let Some(latest) = latest {
        wire["latest"] = json!(latest);
    }
    RemoteArtifactManifest::decode(
        &serde_json::to_vec(&wire).unwrap(),
        MetadataLimits {
            bytes: 64 * 1024,
            nodes: 4096,
            scalar_bytes: 1024,
            collection_entries: 128,
            versions: 128,
            dependencies: 32,
            projection: projection(),
        },
        original,
    )
    .unwrap()
}
fn snapshot(manifest: &RemoteArtifactManifest) -> Vec<(String, [u8; 32])> {
    manifest
        .versions()
        .iter()
        .map(|member| (member.version().as_str().into(), *member.hash()))
        .collect()
}
fn selected<'m>(
    manifest: &'m RemoteArtifactManifest,
    selector: VersionSelector<'_>,
    original: &RequestContext<'_>,
) -> &'m RemoteVersion {
    match select_version(manifest, selector, limits(), original) {
        Ok(member) => member,
        Err(error) => panic!("unexpected selection error: {error}"),
    }
}
fn denied(
    manifest: &RemoteArtifactManifest,
    selector: VersionSelector<'_>,
    policy: VersionSelectionLimits,
    original: &RequestContext<'_>,
) -> VersionSelectionError {
    match select_version(manifest, selector, policy, original) {
        Ok(_) => panic!("unexpected selected reference"),
        Err(error) => error,
    }
}

#[test]
fn all_ten_types_select_borrowed_members_without_changing_identity_or_order() {
    let cancellation = Cancellation::default();
    let original = context(&cancellation, Instant::now() + Duration::from_secs(10));
    assert_ne!(PRODUCER, CONSUMER);
    for kind in ArtifactType::ALL {
        let manifest = manifest(
            kind,
            &["1.2.0", "1.2.9", "1.3.0", "main", "feature/hub-✓", "v2.0.0"],
            Some("1.2.0"),
            &original,
        );
        let before = snapshot(&manifest);
        let choices = [
            (VersionSelector::Latest, 0),
            (
                VersionSelector::ReleasePrefix {
                    major: 1,
                    minor: Some(2),
                },
                1,
            ),
            (
                VersionSelector::ReleasePrefix {
                    major: 1,
                    minor: None,
                },
                2,
            ),
            (
                VersionSelector::ReleasePrefix {
                    major: 2,
                    minor: None,
                },
                5,
            ),
        ];
        for (selector, index) in choices {
            assert!(std::ptr::eq(
                selected(&manifest, selector, &original),
                &manifest.versions()[index]
            ));
        }
        for index in [3, 4, 5] {
            let exact =
                ConcreteVersion::new(manifest.versions()[index].version().as_str(), projection())
                    .unwrap();
            assert!(std::ptr::eq(
                selected(&manifest, VersionSelector::Exact(&exact), &original),
                &manifest.versions()[index]
            ));
        }
        assert_eq!(manifest.producer().to_string(), PRODUCER);
        assert_eq!(manifest.artifact_type(), kind);
        assert_eq!(manifest.id().as_str(), "org/tool");
        assert_eq!(snapshot(&manifest), before);
    }
}

#[test]
fn missing_latest_and_missing_versions_have_no_numeric_or_named_fallback() {
    let cancellation = Cancellation::default();
    let original = context(&cancellation, Instant::now() + Duration::from_secs(10));
    let catalog = manifest(
        ArtifactType::Skill,
        &[
            "main",
            "123-fix",
            "1",
            "1.2",
            "1.2.3-alpha",
            "vv1.2.3",
            "V1.2.3",
        ],
        None,
        &original,
    );
    assert_eq!(
        denied(&catalog, VersionSelector::Latest, limits(), &original),
        VersionSelectionError::LatestUnavailable
    );
    for major in [1, 2] {
        assert_eq!(
            denied(
                &catalog,
                VersionSelector::ReleasePrefix { major, minor: None },
                limits(),
                &original
            ),
            VersionSelectionError::VersionNotFound
        );
    }
    for (index, name) in [
        "main",
        "123-fix",
        "1",
        "1.2",
        "1.2.3-alpha",
        "vv1.2.3",
        "V1.2.3",
    ]
    .into_iter()
    .enumerate()
    {
        let exact = ConcreteVersion::new(name, projection()).unwrap();
        assert!(std::ptr::eq(
            selected(&catalog, VersionSelector::Exact(&exact), &original),
            &catalog.versions()[index]
        ));
    }
    let absent = ConcreteVersion::new("feature/absent-✓", projection()).unwrap();
    assert_eq!(
        denied(
            &catalog,
            VersionSelector::Exact(&absent),
            limits(),
            &original
        ),
        VersionSelectionError::VersionNotFound
    );
}

#[test]
fn precedence_is_numeric_stable_and_only_winning_ties_are_ambiguous() {
    let cancellation = Cancellation::default();
    let original = context(&cancellation, Instant::now() + Duration::from_secs(10));
    let prefix = VersionSelector::ReleasePrefix {
        major: 1,
        minor: None,
    };
    for versions in [
        vec!["1.2.3", "v1.2.3"],
        vec!["v1.2.3", "1.2.3"],
        vec!["1.2.3+one", "1.2.3+two"],
        vec!["1.2.3+two", "1.2.3+one"],
    ] {
        let catalog = manifest(ArtifactType::Command, &versions, None, &original);
        assert_eq!(
            denied(&catalog, prefix, limits(), &original),
            VersionSelectionError::AmbiguousVersion
        );
        for (index, name) in versions.iter().enumerate() {
            let exact = ConcreteVersion::new(name, projection()).unwrap();
            assert!(std::ptr::eq(
                selected(&catalog, VersionSelector::Exact(&exact), &original),
                &catalog.versions()[index]
            ));
        }
    }
    for versions in [
        vec!["1.2.3+one", "1.2.3+two", "1.19.0", "1.20.0-alpha"],
        vec!["1.19.0", "1.2.3+two", "1.20.0-alpha", "1.2.3+one"],
    ] {
        let catalog = manifest(ArtifactType::Command, &versions, None, &original);
        assert_eq!(
            selected(&catalog, prefix, &original).version().as_str(),
            "1.19.0"
        );
    }
}

#[test]
fn current_limits_cover_entire_manifest_and_exact_selector_before_lookup() {
    let cancellation = Cancellation::default();
    let original = context(&cancellation, Instant::now() + Duration::from_secs(10));
    let catalog = manifest(
        ArtifactType::Agent,
        &["1.2.3", "feature/current-policy-oversized"],
        Some("1.2.3"),
        &original,
    );
    let exact = ConcreteVersion::new("1.2.3", projection()).unwrap();
    let boundary = VersionSelectionLimits {
        max_versions: 2,
        max_version_bytes: "feature/current-policy-oversized".len(),
    };
    assert!(
        select_version(
            &catalog,
            VersionSelector::Exact(&exact),
            boundary,
            &original
        )
        .is_ok()
    );
    for policy in [
        VersionSelectionLimits {
            max_versions: 1,
            ..boundary
        },
        VersionSelectionLimits {
            max_version_bytes: boundary.max_version_bytes - 1,
            ..boundary
        },
    ] {
        for selector in [VersionSelector::Exact(&exact), VersionSelector::Latest] {
            assert_eq!(
                denied(&catalog, selector, policy, &original),
                VersionSelectionError::Limit
            );
        }
    }
    for policy in [
        VersionSelectionLimits {
            max_versions: 0,
            ..boundary
        },
        VersionSelectionLimits {
            max_version_bytes: 0,
            ..boundary
        },
    ] {
        assert_eq!(
            denied(&catalog, VersionSelector::Latest, policy, &original),
            VersionSelectionError::InvalidPolicy
        );
    }
    let short = manifest(ArtifactType::Agent, &["1.2.3"], Some("1.2.3"), &original);
    let oversized = ConcreteVersion::new("feature/current-policy-oversized", projection()).unwrap();
    let policy = VersionSelectionLimits {
        max_versions: 1,
        max_version_bytes: 8,
    };
    assert_eq!(
        denied(
            &short,
            VersionSelector::Exact(&oversized),
            policy,
            &original
        ),
        VersionSelectionError::Limit
    );
    assert!(
        select_version(
            &short,
            VersionSelector::Latest,
            VersionSelectionLimits {
                max_versions: 1,
                max_version_bytes: 5
            },
            &original
        )
        .is_ok()
    );
    assert_eq!(
        denied(
            &short,
            VersionSelector::Latest,
            VersionSelectionLimits {
                max_versions: 1,
                max_version_bytes: 4
            },
            &original
        ),
        VersionSelectionError::Limit
    );
}

#[test]
fn original_pre_cancelled_and_pre_expired_requests_remain_terminal() {
    let cancellation = Cancellation::default();
    let original = context(&cancellation, Instant::now() + Duration::from_secs(10));
    let catalog = manifest(ArtifactType::Rule, &["1.2.3"], Some("1.2.3"), &original);
    cancellation.cancel();
    for _ in 0..2 {
        assert_eq!(
            denied(&catalog, VersionSelector::Latest, limits(), &original),
            VersionSelectionError::Cancelled
        );
    }
    let fresh = Cancellation::default();
    let expired = context(&fresh, Instant::now() + Duration::from_millis(20));
    while Instant::now() < expired.deadline().unwrap() {
        thread::sleep(
            expired
                .deadline()
                .unwrap()
                .saturating_duration_since(Instant::now()),
        );
    }
    assert_eq!(
        denied(&catalog, VersionSelector::Latest, limits(), &expired),
        VersionSelectionError::Deadline
    );
}

#[test]
fn original_gate_observes_cancellation_and_expiration_after_admission() {
    let cancellation = Cancellation::default();
    let original = context(&cancellation, Instant::now() + Duration::from_secs(10));
    check(&original).unwrap();
    cancellation.cancel();
    assert_eq!(check(&original), Err(VersionSelectionError::Cancelled));
    assert_eq!(check(&original), Err(VersionSelectionError::Cancelled));

    let fresh = Cancellation::default();
    let original = context(&fresh, Instant::now() + Duration::from_millis(20));
    check(&original).unwrap();
    thread::sleep(
        original
            .deadline()
            .unwrap()
            .saturating_duration_since(Instant::now()),
    );
    assert_eq!(check(&original), Err(VersionSelectionError::Deadline));
}
