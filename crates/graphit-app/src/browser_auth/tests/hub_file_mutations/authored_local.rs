//! Authored HTTP selection installs a real LocalLink through the existing mutable Web journey.
use super::*;
use graphit_hub::{ArtifactId, ConcreteVersion, SourceRef, registry::Installation};
const LOCAL_INSTALL: &str = "/api/hub/authored/install";
const LOCAL_ID: &str = "fixture/authored-local";
const LOCAL_VERSION: &str = "release/local-one";
const FIRST: &str = "---\nname: authored-local\ndescription: Local authored installation\n---\nFirst authored body\n";
const SECOND: &str = "---\nname: authored-local\ndescription: Local authored installation\n---\nUpdated authored body\n";

fn resource(
    fixture: &crate::source_server::test_fixture::Fixture,
    index: usize,
    version: &str,
) -> std::path::PathBuf {
    let (paths, id, _) = &fixture.projects[index];
    let limits = fixture.server.policy().native_install.hub.projection();
    let source = checked(
        SourceRef::resources(
            paths,
            *id,
            ArtifactType::Skill,
            checked(ArtifactId::new(LOCAL_ID, limits), "authored LocalLink ID"),
            checked(
                ConcreteVersion::new(version, limits),
                "authored LocalLink version",
            ),
            limits,
        ),
        "actual Resources codec",
    );
    let root = source.directory().as_path().to_owned();
    checked(
        std::fs::create_dir_all(root.join("references")),
        "authored Resource directory",
    );
    checked(
        std::fs::write(root.join("SKILL.md"), FIRST),
        "authored instruction source",
    );
    checked(
        std::fs::write(root.join("references/guide.md"), "original companion"),
        "authored companion",
    );
    root
}
fn installed(fixture: &crate::source_server::test_fixture::Fixture) -> Installation {
    checked(
        project_registry(fixture).load(),
        "LocalLink registry observation",
    )
    .get(ArtifactType::Skill, LOCAL_ID)
    .expect("actual installed authored identity")
    .installations
    .iter()
    .find(|entry| {
        entry.target == InstallationTarget::for_artifact(NativeBinding::KiroV3, ArtifactType::Skill)
    })
    .expect("actual authored target")
    .clone()
}
fn destination(fixture: &crate::source_server::test_fixture::Fixture) -> std::path::PathBuf {
    let policy = fixture.server.policy();
    let cancel = Cancellation::default();
    let original = checked(
        policy
            .native_install
            .hub
            .request_context(Instant::now(), &cancel),
        "authored physical observation Original",
    );
    let (paths, id, _) = &fixture.projects[0];
    let scope = checked(
        CapturedPublicationScope::project(paths, *id, policy.scope, original.snapshot()),
        "authored consumer Core scope",
    );
    let store = ArtifactStore::new(scope, policy.native_install.hub.artifacts());
    let key = checked(
        InstallationTarget::for_artifact(NativeBinding::KiroV3, ArtifactType::Skill)
            .key(ArtifactType::Skill, LOCAL_ID),
        "actual authored physical key",
    );
    checked(store.load(), "authored Core catalog")
        .expect("authored physical receipt")
        .entries()
        .iter()
        .find(|entry| entry.key() == &key)
        .expect("owned authored physical entry")
        .destination()
        .to_owned()
}
fn assert_source(
    fixture: &crate::source_server::test_fixture::Fixture,
    root: &std::path::Path,
    mode: ArtifactMode,
) {
    let entry = installed(fixture);
    assert_eq!(entry.original_source.origin(), SourceOrigin::LocalLink);
    assert_eq!(entry.original_source.producer(), fixture.projects[0].1);
    assert_eq!(
        entry.original_source.directory().as_path(),
        checked(std::fs::canonicalize(root), "canonical authored source")
    );
    assert_eq!(entry.original_source.version().as_str(), LOCAL_VERSION);
    assert!(
        entry.original_source.remote_file_origin().is_none(),
        "authored source is not remote storage"
    );
    assert!(
        matches!(entry.backend,InstalledBackend::Physical {mode:actual,..} if actual==mode),
        "actual requested physical mode"
    );
    assert_eq!(entry.alias.as_deref(), Some("web-authored-local"));
    assert_eq!(entry.entry_name.as_deref(), Some("web-authored-entry"));
    assert_eq!(
        checked(
            std::fs::symlink_metadata(destination(fixture)),
            "actual authored destination"
        )
        .file_type()
        .is_symlink(),
        matches!(mode, ArtifactMode::Link)
    );
}

pub(super) async fn journey(
    fixture: &crate::source_server::test_fixture::Fixture,
    host: &WebHost,
    cookie: &str,
    issuer: &str,
    subject: &str,
) {
    let root = resource(fixture, 0, LOCAL_VERSION);
    let _other = resource(fixture, 1, LOCAL_VERSION);
    let project = fixture.projects[0].1;
    let data_version = fixture.projects[0].2.version();
    let selection = json!({"project_id":project,"project_version":data_version,
        "artifact_type":"skill","id":LOCAL_ID,"version":LOCAL_VERSION,"limit":1});
    administrators(fixture, issuer, &[]);
    let listed = page(host, cookie, "/api/hub/authored/list", selection).await;
    let tuple = listed["entries"]
        .as_array()
        .expect("actual authored list")
        .first()
        .expect("selected authored tuple");
    let mut input = json!({"source":{"project_id":listed["project_id"],"project_version":listed["project_version"],
        "artifact_type":tuple["artifact_type"],"id":tuple["id"],"version":tuple["version"]},
        "scope":{"kind":"project","project_id":project,"project_version":data_version},
        "binding":NativeBinding::KiroV3,"alias":"web-authored-local","entry_name":"web-authored-entry"});
    let before = hub_file_reads::registry_bytes(&project_registry(fixture).path());
    failure(host, cookie, LOCAL_INSTALL, &input, StatusCode::FORBIDDEN).await;
    assert_eq!(
        hub_file_reads::registry_bytes(&project_registry(fixture).path()),
        before,
        "source read alone grants no local mutation"
    );
    administrators(fixture, issuer, &[(subject, &["project"])]);
    let mut wrong_source = input.clone();
    wrong_source["source"]["project_id"] = json!(fixture.projects[1].1);
    wrong_source["source"]["project_version"] = json!(fixture.projects[1].2.version());
    failure(
        host,
        cookie,
        LOCAL_INSTALL,
        &wrong_source,
        StatusCode::FORBIDDEN,
    )
    .await;
    let mut wrong_consumer = input.clone();
    wrong_consumer["scope"]["project_id"] = json!(fixture.projects[1].1);
    wrong_consumer["scope"]["project_version"] = json!(fixture.projects[1].2.version());
    failure(
        host,
        cookie,
        LOCAL_INSTALL,
        &wrong_consumer,
        StatusCode::FORBIDDEN,
    )
    .await;
    let mut missing_source = input.clone();
    missing_source["source"]["project_version"] = json!("missing-version");
    failure(
        host,
        cookie,
        LOCAL_INSTALL,
        &missing_source,
        StatusCode::NOT_FOUND,
    )
    .await;
    for (field, value) in [
        ("source_dir", json!("/not-an-input")),
        ("provider", json!("not-an-input")),
        ("mode", Value::Null),
    ] {
        let mut invalid = input.clone();
        invalid[field] = value;
        failure(
            host,
            cookie,
            LOCAL_INSTALL,
            &invalid,
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
    let mut unsupported = input.clone();
    unsupported["source"]["artifact_type"] = json!("ast");
    failure(
        host,
        cookie,
        LOCAL_INSTALL,
        &unsupported,
        StatusCode::BAD_REQUEST,
    )
    .await;
    assert_eq!(
        hub_file_reads::registry_bytes(&project_registry(fixture).path()),
        before,
        "denied/invalid local selection has no registry effect"
    );
    assert_eq!(
        checked(
            std::fs::read_to_string(root.join("SKILL.md")),
            "original source after denials"
        ),
        FIRST
    );
    let created = page(host, cookie, LOCAL_INSTALL, input.clone()).await;
    assert_eq!(created["operation"], "install_local");
    assert_eq!(created["source"], "local");
    assert_eq!(created["cache"], "not_attempted");
    assert_ne!(created["publication"], "not_attempted");
    assert_source(
        fixture,
        &root,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
    );
    let projected = destination(fixture);
    assert!(
        checked(
            std::fs::read_to_string(projected.join("SKILL.md")),
            "real authored Copy"
        )
        .contains("First authored body")
    );
    assert_eq!(
        checked(
            std::fs::read_to_string(projected.join("references/guide.md")),
            "copied authored companion"
        ),
        "original companion"
    );
    let listed = page(
        host,
        cookie,
        "/api/hub/installed/files",
        json!({"scope":input["scope"],"artifact_type":"skill","id":LOCAL_ID,"limit":10}),
    )
    .await;
    assert!(
        listed["entries"]
            .as_array()
            .expect("actual installed inventory")
            .iter()
            .any(|entry| {
                entry["id"] == LOCAL_ID
                    && entry["source_origin"] == "LocalLink"
                    && entry["origin"].is_null()
            }),
        "HTTP inventory reports actual provenance"
    );
    input["mode"] = json!("link");
    page(host, cookie, LOCAL_INSTALL, input.clone()).await;
    assert_source(fixture, &root, ArtifactMode::Link);
    assert_ne!(
        checked(std::fs::canonicalize(&projected), "rendered physical Link"),
        checked(std::fs::canonicalize(&root), "authored original path"),
        "rendered instruction Link retains its original provenance"
    );
    checked(
        std::fs::write(root.join("SKILL.md"), SECOND),
        "authored source edit",
    );
    checked(
        std::fs::write(root.join("references/guide.md"), "updated companion"),
        "authored companion edit",
    );
    for field in ["alias", "entry_name", "mode"] {
        input
            .as_object_mut()
            .expect("strict reapply body")
            .remove(field);
    }
    page(host, cookie, LOCAL_INSTALL, input.clone()).await;
    assert_source(fixture, &root, ArtifactMode::Link);
    assert_eq!(destination(fixture), projected);
    assert!(
        checked(
            std::fs::read_to_string(projected.join("SKILL.md")),
            "updated rendered LocalLink"
        )
        .contains("Updated authored body")
    );
    assert_eq!(
        checked(
            std::fs::read_to_string(projected.join("references/guide.md")),
            "updated rendered companion"
        ),
        "updated companion"
    );
    let alternative = resource(fixture, 0, "release/other-source");
    let before_conflict = hub_file_reads::registry_bytes(&project_registry(fixture).path());
    let mut changed_root = input.clone();
    changed_root["source"]["version"] = json!("release/other-source");
    failure(
        host,
        cookie,
        LOCAL_INSTALL,
        &changed_root,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(
        hub_file_reads::registry_bytes(&project_registry(fixture).path()),
        before_conflict,
        "different LocalLink root cannot replace the current provenance"
    );
    assert!(alternative.join("SKILL.md").is_file());
    input["mode"] = json!("copy");
    page(host, cookie, LOCAL_INSTALL, input.clone()).await;
    assert_source(
        fixture,
        &root,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
    );
    let held = root.with_file_name("source-preserved-during-remove");
    checked(
        std::fs::rename(&root, &held),
        "temporarily unavailable authored source",
    );
    let removed = page(
        host,
        cookie,
        REMOVE,
        json!({"scope":input["scope"],"artifact_type":"skill",
        "id":LOCAL_ID,"version":LOCAL_VERSION,"binding":NativeBinding::KiroV3}),
    )
    .await;
    assert_eq!(removed["publication"]["change"], "Physical(Removed)");
    assert!(
        !projected.exists(),
        "removal retires only the owned projection"
    );
    assert_eq!(
        checked(
            std::fs::read_to_string(held.join("SKILL.md")),
            "preserved authored original"
        ),
        SECOND
    );
    checked(
        std::fs::rename(&held, &root),
        "restore preserved source location",
    );
    assert!(
        checked(
            project_registry(fixture).load(),
            "registry after authored remove"
        )
        .get(ArtifactType::Skill, LOCAL_ID)
        .is_none()
    );
}
