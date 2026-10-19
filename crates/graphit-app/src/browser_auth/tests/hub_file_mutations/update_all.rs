//! The current Web mutation ingress executes collective updates with its own actor.
use super::*;
use object_store::ObjectStoreExt as _;

pub(super) async fn journey(
    fixture: &crate::source_server::test_fixture::Fixture,
    host: &WebHost,
    cookie: &str,
    actor: (&str, &str),
    publisher: &dyn object_store::ObjectStore,
    prefix: &str,
) {
    let (issuer, subject) = actor;
    let producer = fixture.projects[0].1;
    administrators(fixture, issuer, &[(subject, &["project", "global"])]);
    for id in ["fixture/update-a", "fixture/update-b"] {
        for (version, body) in [("release/one", ONE), ("release/two", TWO)] {
            hub_file_reads::seed_named_file_payload(
                fixture,
                publisher,
                prefix,
                producer,
                (id, version),
                body,
                crate::remote_file_install::client::Kind::Skill,
            )
            .await;
        }
    }
    let project = json!({"kind":"project","project_id":producer,"project_version":fixture.projects[0].2.version()});
    let (paths, id, _) = &fixture.projects[0];
    let limits = fixture.server.policy().native_install.hub.projection();
    let local = checked(
        graphit_hub::SourceRef::resources(
            paths,
            *id,
            ArtifactType::Skill,
            checked(
                graphit_hub::ArtifactId::new("fixture/update-local", limits),
                "local collective ID",
            ),
            checked(
                graphit_hub::ConcreteVersion::new("release/local", limits),
                "local collective version",
            ),
            limits,
        ),
        "local collective Resources",
    );
    checked(
        std::fs::create_dir_all(local.directory().as_path()),
        "local collective source directory",
    );
    checked(
        std::fs::write(
            local.directory().as_path().join("SKILL.md"),
            "---\nname: collective-local\ndescription: Unchanged local source\n---\nLocal remains unchanged\n",
        ),
        "local collective source bytes",
    );
    page(host, cookie, "/api/hub/authored/install", json!({
        "source":{"project_id":producer,"project_version":fixture.projects[0].2.version(),"artifact_type":"skill","id":"fixture/update-local","version":"release/local"},
        "scope":project,"binding":NativeBinding::KiroV3,"alias":"collective-local"
    })).await;
    let registry = project_registry(fixture);
    let before = checked(registry.load(), "collective local baseline");
    let local_before = before
        .get(ArtifactType::Skill, "fixture/update-local")
        .expect("actual local installation");
    for (id, alias) in [
        ("fixture/update-a", "collective-a"),
        ("fixture/update-b", "collective-b"),
    ] {
        page(host, cookie, INSTALL, json!({"selection":{"transport":"standalone","provider":"catalog-storage","profile":"catalog-keys", "producer":producer,"artifact_type":"skill","id":id,"version":"release/one","exact":true},
            "scope":project,"binding":NativeBinding::KiroV3,"alias":alias})).await;
    }
    let collective = json!({"scope":project,"binding":NativeBinding::KiroV3});
    let updated = page(host, cookie, UPDATE, collective.clone()).await;
    let items = updated["items"].as_array().expect("collective Web items");
    assert_eq!(items.len(), 2);
    for (index, id) in ["fixture/update-a", "fixture/update-b"].iter().enumerate() {
        assert_eq!(items[index]["id"], *id);
        assert_eq!(items[index]["status"], "ok");
        assert_eq!(items[index]["outcome"]["version"], "release/two");
    }
    let after = checked(registry.load(), "collective Web publication");
    assert_eq!(
        after.get(ArtifactType::Skill, "fixture/update-local"),
        Some(local_before)
    );
    for (id, alias) in [
        ("fixture/update-a", "collective-a"),
        ("fixture/update-b", "collective-b"),
    ] {
        let installed = &after
            .get(ArtifactType::Skill, id)
            .expect("updated remote")
            .installations[0];
        assert_eq!(installed.alias.as_deref(), Some(alias));
        assert_eq!(installed.original_source.version().as_str(), "release/two");
        assert_eq!(
            installed.original_source.remote_file_origin(),
            Some(&graphit_hub::RemoteFileOrigin::Standalone {
                provider: "catalog-storage".into(),
                profile: Some("catalog-keys".into())
            })
        );
    }
    for id in ["fixture/update-a", "fixture/update-b"] {
        page(host, cookie, INSTALL, json!({"selection":{"transport":"standalone","provider":"catalog-storage","profile":"catalog-keys","producer":producer,"artifact_type":"skill","id":id,"version":"release/one","exact":true},"scope":{"kind":"global_cache"}})).await;
    }
    let global = page(
        host,
        cookie,
        UPDATE,
        json!({"scope":{"kind":"global_cache"}}),
    )
    .await;
    assert_eq!(
        global["items"]
            .as_array()
            .expect("GlobalCache collective items")
            .len(),
        2
    );
    assert!(
        global["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["outcome"]["version"] == "release/two")
    );
    let mut invalid = collective.clone();
    invalid["project_dir"] = json!("/arbitrary-host");
    failure(host, cookie, UPDATE, &invalid, StatusCode::BAD_REQUEST).await;
    failure(
        host,
        cookie,
        UPDATE,
        &json!({"scope":{"kind":"global_native"}}),
        StatusCode::BAD_REQUEST,
    )
    .await;
    administrators(fixture, issuer, &[]);
    failure(host, cookie, UPDATE, &collective, StatusCode::FORBIDDEN).await;
    administrators(fixture, issuer, &[(subject, &["project", "global"])]);
    let id = checked(
        graphit_hub::ArtifactId::new("fixture/update-a", limits),
        "failed collective artifact",
    );
    let key = checked(
        graphit_hub::remote_storage::registry_key(producer, ArtifactType::Skill, &id),
        "failed collective key",
    );
    let object = object_store::path::Path::from(format!("{}/{key}", prefix.trim_end_matches('/')));
    // The real publication grant permits writes, but not registry deletion.
    // Malformed metadata causes a real read failure without expanding that grant.
    checked(
        publisher.put(&object, b"{}".to_vec().into()).await,
        "write invalid fixture catalog to cause real first-item failure",
    );
    let before = hub_file_reads::registry_bytes(&registry.path());
    let refused = failure(
        host,
        cookie,
        UPDATE,
        &collective,
        StatusCode::INTERNAL_SERVER_ERROR,
    )
    .await;
    assert_eq!(refused["error"]["code"], "file_installation_failed");
    assert_eq!(refused["outcome"]["items"][0]["code"], "update_failed");
    assert_eq!(refused["outcome"]["items"][0]["status"], "error");
    assert_eq!(
        refused["outcome"]["items"][0]["outcome"]["publication"],
        "not_attempted"
    );
    assert_eq!(refused["outcome"]["items"][1]["status"], "not_attempted");
    assert_eq!(hub_file_reads::registry_bytes(&registry.path()), before);
}
