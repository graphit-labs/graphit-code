use graphit_core::{
    artifact_store::UserIdentity, installation_identity::InstallationId,
    project_identity::ProjectId,
};
use graphit_data::native_storage_uri::{
    CorpusModule, GlobalHubCorpus, NativeStorageScope, ProjectHubCorpus, object_location,
};

fn project() -> ProjectId {
    "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap()
}

#[test]
fn native_scopes_preserve_project_installation_and_literal_user_identity() {
    let project = project();
    for (module, name) in [
        (CorpusModule::Task, "task"),
        (CorpusModule::Memory, "memory"),
        (CorpusModule::Knowledge, "knowledge"),
        (CorpusModule::Ast, "ast"),
    ] {
        let value = object_location(
            "graphit-tests",
            "provider/",
            NativeStorageScope::ProjectCorpus { project, module },
            "index.lance",
        )
        .unwrap();
        assert_eq!(
            value.key(),
            format!("provider/v1/projects/{project}/corpora/{name}/index.lance")
        );
    }
    let other = "01ARZ3NDEKTSV4RRFFQ69G5FAW".parse().unwrap();
    assert_ne!(
        object_location(
            "graphit-tests",
            "",
            NativeStorageScope::ProjectCorpus {
                project,
                module: CorpusModule::Memory,
            },
            "index.lance",
        )
        .unwrap()
        .key(),
        object_location(
            "graphit-tests",
            "",
            NativeStorageScope::ProjectCorpus {
                project: other,
                module: CorpusModule::Memory,
            },
            "index.lance",
        )
        .unwrap()
        .key()
    );
    let installation = InstallationId::new("Install-A").unwrap();
    let identity =
        UserIdentity::new("https://issuer.example/Realm".into(), "User/A".into()).unwrap();
    let user = object_location(
        "graphit-tests",
        "",
        NativeStorageScope::UserMemory {
            installation: &installation,
            identity: &identity,
        },
        "index.lance",
    )
    .unwrap();
    assert_eq!(
        user.key(),
        format!(
            "v1/installations/Install-A/users/{}/corpora/memory/index.lance",
            identity.namespace_key()
        )
    );
    assert!(!user.uri().as_str().contains("issuer.example"));
    assert!(!user.uri().as_str().contains("User/A"));
    for (issuer, subject, installation) in [
        ("https://issuer.example/realm", "User/A", "Install-A"),
        ("https://issuer.example/Realm", "user/A", "Install-A"),
        ("https://issuer.example/Realm", "User/A", "Install-B"),
    ] {
        let identity = UserIdentity::new(issuer.into(), subject.into()).unwrap();
        let installation = InstallationId::new(installation).unwrap();
        assert_ne!(
            user.key(),
            object_location(
                "graphit-tests",
                "",
                NativeStorageScope::UserMemory {
                    installation: &installation,
                    identity: &identity,
                },
                "index.lance",
            )
            .unwrap()
            .key()
        );
    }
    for (scope, root) in [
        (
            NativeStorageScope::GlobalHub(GlobalHubCorpus::Registry),
            "v1/hub/global/registry".to_owned(),
        ),
        (
            NativeStorageScope::GlobalHub(GlobalHubCorpus::Rules),
            "v1/hub/global/rules".to_owned(),
        ),
        (
            NativeStorageScope::ProjectHub {
                project,
                corpus: ProjectHubCorpus::Metadata,
            },
            format!("v1/projects/{project}/hub/metadata"),
        ),
        (
            NativeStorageScope::ProjectHub {
                project,
                corpus: ProjectHubCorpus::Events,
            },
            format!("v1/projects/{project}/hub/events"),
        ),
    ] {
        assert_eq!(
            object_location("graphit-tests", "", scope, "document.json")
                .unwrap()
                .key(),
            format!("{root}/document.json")
        );
    }
}

#[test]
fn mature_sdk_uri_parser_roundtrips_literal_unicode_percent_and_delimiters() {
    let value = object_location(
        "graphit-tests",
        "prefix/ação",
        NativeStorageScope::GlobalHub(GlobalHubCorpus::Registry),
        "literal%2F?#/文.json",
    )
    .unwrap();
    assert!(value.uri().query().is_none());
    assert!(value.uri().fragment().is_none());
    assert!(value.uri().username().is_empty());
    assert!(value.uri().password().is_none());
    let key = object_store::path::Path::from_url_path(value.uri().path()).unwrap();
    assert_eq!(key.as_ref(), value.key());
    assert!(value.uri().as_str().contains("literal%252F%3F%23"));
    let (key, uri) = value.into_parts();
    assert_eq!(
        object_store::path::Path::from_url_path(uri.path())
            .unwrap()
            .as_ref(),
        key
    );
}

#[test]
fn full_utf8_s3_key_limit_and_namespace_escapes_are_enforced() {
    let scope = NativeStorageScope::GlobalHub(GlobalHubCorpus::Rules);
    let base = object_location("graphit-tests", "", scope, "a").unwrap();
    let overhead = base.key().len() - 1;
    let object = "a".repeat(1024 - overhead);
    assert_eq!(
        object_location("graphit-tests", "", scope, &object)
            .unwrap()
            .key()
            .len(),
        1024
    );
    assert!(object_location("graphit-tests", "", scope, &(object + "a")).is_err());
    let prefix = "é".repeat(100);
    let object = "a".repeat(1024 - overhead - prefix.len() - 1);
    assert_eq!(
        object_location("graphit-tests", &prefix, scope, &object)
            .unwrap()
            .key()
            .len(),
        1024
    );
    assert!(object_location("graphit-tests", &prefix, scope, &(object + "a")).is_err());
    for object in [
        "",
        "/index.lance",
        "../index.lance",
        "x/./y",
        "x//y",
        "x/",
        "x\\y",
        "x\0y",
    ] {
        assert_eq!(
            object_location("graphit-tests", "", scope, object)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidInput
        );
    }
    for prefix in [
        "/",
        "/root",
        "root//",
        "../root",
        "root/../other",
        "root\\other",
    ] {
        assert!(object_location("graphit-tests", prefix, scope, "index.lance").is_err());
    }
    for bucket in [
        "",
        "Bucket",
        "bucket/path",
        "user@bucket",
        "bucket:123",
        "bucket?query",
        "bucket#fragment",
    ] {
        assert!(object_location(bucket, "", scope, "index.lance").is_err());
    }
}
