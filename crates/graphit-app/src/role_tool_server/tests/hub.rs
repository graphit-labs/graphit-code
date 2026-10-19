use super::*;

mod types;
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
    registry::{
        ArtifactDisplayMetadata, Installation, InstalledArtifact, InstalledBackend,
        InstalledPayload,
    },
};

fn project_registry(server: &RoleToolServer, index: usize) -> HubRegistry {
    let snapshot = project_snapshot(server);
    let paths = &snapshot.projects[index].0;
    let identity = ProjectIdentityStore::at(paths).load().unwrap().unwrap();
    HubRegistry::project(
        paths,
        &identity,
        server.0.policy.source.native_install.hub.hub(),
    )
    .unwrap()
}
fn put(
    registry: &HubRegistry,
    source: SourceRef,
    target: InstallationTarget,
    backend: InstalledBackend,
) {
    let cancellation = Cancellation::default();
    registry
        .writer(Duration::from_secs(5), &cancellation)
        .unwrap()
        .put_metadata(InstalledArtifact {
            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
            artifact_type: source.artifact_type(),
            id: source.id().clone(),
            installations: vec![Installation {
                target,
                original_source: source,
                alias: None,
                entry_name: None,
                display: Some(
                    ArtifactDisplayMetadata::new(
                        "Installed SDK artifact",
                        Some("**Author metadata**"),
                        &["role".into()],
                        registry.limits(),
                    )
                    .unwrap(),
                ),
                backend,
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        })
        .unwrap();
}
fn local(server: &RoleToolServer, id: &str, version: &str) -> SourceRef {
    let snapshot = project_snapshot(server);
    let policy = &server.0.policy.source.native_install.hub;
    SourceRef::local_link(
        graphit_core::native_path::NativePath::new(
            snapshot.projects[0].0.root().join("unopened-payload"),
        )
        .unwrap(),
        snapshot.projects[0].1,
        ArtifactType::Mcp,
        ArtifactId::new(id, policy.projection()).unwrap(),
        ConcreteVersion::new(version, policy.projection()).unwrap(),
        policy.projection(),
    )
    .unwrap()
}
fn global_native(server: &RoleToolServer, version: &str) {
    let policy = &server.0.policy.source.native_install.hub;
    let source = SourceRef::hub_cache(
        &server.0.policy.source.installation,
        project_snapshot(server).projects[0].1,
        ArtifactType::Ast,
        ArtifactId::new("vendor/graph", policy.projection()).unwrap(),
        None,
        ConcreteVersion::new(version, policy.projection()).unwrap(),
        policy.projection(),
    )
    .unwrap();
    let backend = InstalledBackend::RemoteNative(
        graphit_hub::remote_native::RemoteNativeRecord::new(
            "private-provider",
            "private-bucket",
            "us-east-1",
            Some("https://private-storage.invalid"),
            "private-prefix",
            [7; 32],
            policy.projection(),
        )
        .unwrap(),
    );
    put(
        &server.0.policy.source.registry,
        source,
        InstallationTarget::GlobalNative,
        backend,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_hub_installed_metadata_pages_scope_binding_and_safe_projection() {
    let (_temp, server, raw) = fixture();
    let registry = project_registry(&server, 0);
    put(
        &registry,
        local(&server, "alpha/mcp", "release-one"),
        InstallationTarget::for_artifact(NativeBinding::KiroV3, ArtifactType::Mcp),
        InstalledBackend::Native,
    );
    put(
        &registry,
        local(&server, "beta/mcp", "release-two"),
        InstallationTarget::for_artifact(NativeBinding::KiroV3, ArtifactType::Mcp),
        InstalledBackend::Native,
    );
    put(
        &registry,
        local(&server, "foreign/binding", "foreign-version"),
        InstallationTarget::for_artifact(NativeBinding::Codex, ArtifactType::Mcp),
        InstalledBackend::Native,
    );
    global_native(&server, "release-one");
    global_native(&server, "release-two");
    assert!(
        !project_snapshot(&server).projects[0]
            .0
            .root()
            .join("unopened-payload")
            .exists()
    );
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Hub)],
        Duration::from_secs(90),
    );
    let (_other_session, _other_run, denied_ref, denied_bearer) = credential(
        &server,
        &raw,
        Role::Tracker,
        &[Permission::Read(Module::Task)],
        Duration::from_secs(90),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    let denied = connect(
        &uri,
        role_credential(&denied_ref, &denied_bearer, server.0.task.content),
    )
    .await;
    let tools = client.list_all_tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "graphit_hub_list"));
    assert!(tools.iter().any(|t| t.name == "graphit_hub_show"));
    assert!(tools.iter().any(|t| t.name == "graphit_hub_content"));
    assert!(tools.iter().any(|t| t.name == "graphit_hub_projects"));
    assert!(tools.iter().any(|t| t.name == "graphit_hub_search"));
    assert!(!tools.iter().any(|t| t.name == "graphit_hub_install"));
    assert!(
        !denied
            .list_all_tools()
            .await
            .unwrap()
            .iter()
            .any(|t| t.name == "graphit_hub_list")
    );
    call(
        &denied,
        "graphit_hub_list",
        json!({"input":{"scope":"project","limit":1}}),
        true,
    )
    .await;
    let first = call(
        &client,
        "graphit_hub_list",
        json!({"input":{"scope":"project","limit":1}}),
        false,
    )
    .await;
    assert_eq!(first["entries"][0]["id"], "alpha/mcp");
    assert_eq!(first["next_offset"], 1);
    let second = call(
        &client,
        "graphit_hub_list",
        json!({"input":{"scope":"project","limit":1,"offset":1}}),
        false,
    )
    .await;
    assert_eq!(second["entries"][0]["id"], "beta/mcp");
    assert!(second["next_offset"].is_null());
    let all = call(
        &client,
        "graphit_hub_list",
        json!({"input":{"scope":"global_installation","artifact_type":"ast","limit":10}}),
        false,
    )
    .await;
    assert_eq!(all["entries"].as_array().unwrap().len(), 2);
    for entry in all["entries"].as_array().unwrap() {
        let keys = entry.as_object().unwrap();
        assert_eq!(keys.len(), 9);
        for private in [
            "root",
            "directory",
            "backend",
            "payload",
            "sha256",
            "endpoint",
            "bucket",
            "prefix",
            "credentials",
        ] {
            assert!(!keys.contains_key(private));
        }
        assert_eq!(entry["display"]["description"], "**Author metadata**");
    }
    let selected=call(&client,"graphit_hub_show",json!({"input":{"scope":"global_installation","artifact_type":"ast","id":"vendor/graph","version":"release-two"}}),false).await;
    assert_eq!(selected["entries"].as_array().unwrap().len(), 1);
    assert_eq!(selected["entries"][0]["version"], "release-two");
    let absent=call(&client,"graphit_hub_show",json!({"input":{"scope":"global_installation","artifact_type":"ast","id":"vendor/graph","version":"latest"}}),false).await;
    assert_eq!(absent["kind"], "artifact_not_installed");
    for input in [
        json!({"scope":"user","limit":1}),
        json!({"scope":"project","limit":0}),
        json!({"scope":"project","limit":1,"id":null}),
        json!({"scope":"project","limit":1,"root":"/forged"}),
    ] {
        call(&client, "graphit_hub_list", json!({"input":input}), true).await;
    }
    call(
        &client,
        "graphit_hub_show",
        json!({"input":{"scope":"project","artifact_type":"mcp","id":"alpha/mcp"}}),
        true,
    )
    .await;
    for input in [
        json!({"scope":"global_installation","artifact_type":"ast","id":"vendor/graph","version":"release-two","path":"main.rs"}),
        json!({"scope":"project","artifact_type":"mcp","id":"alpha/mcp","version":"release-one","path":"../outside"}),
        json!({"scope":"project","artifact_type":"mcp","id":"alpha/mcp","version":"release-one","path":"/outside"}),
        json!({"scope":"project","artifact_type":"mcp","id":"alpha/mcp","version":"release-one"}),
    ] {
        assert_eq!(
            call(&client, "graphit_hub_content", json!({"input":input}), true).await["code"],
            "invalid_arguments"
        );
    }
    let unavailable=call(&client,"graphit_hub_content",json!({"input":{"scope":"project","artifact_type":"mcp","id":"alpha/mcp","version":"release-one","path":"MCP.md"}}),true).await;
    assert_eq!(unavailable["code"], "hub_read_failed");
    assert_eq!(unavailable["io_kind"], "NotFound");
    call(&denied,"graphit_hub_content",json!({"input":{"scope":"project","artifact_type":"mcp","id":"alpha/mcp","version":"release-one","path":"MCP.md"}}),true).await;
    let peer = graphit_core::native_path::NativePath::new(
        project_snapshot(&server).projects[1].0.root().to_owned(),
    )
    .unwrap();
    call(
        &client,
        "graphit_hub_list",
        json!({"selection":{"project_dir":peer},"input":{"scope":"project","limit":1}}),
        true,
    )
    .await;
    for (reference, tools) in [
        (
            None,
            vec![
                "graphit_hub_list",
                "graphit_hub_list",
                "graphit_hub_show",
                "graphit_hub_content",
                "graphit_hub_content",
                "graphit_hub_projects",
                "graphit_module_skill",
            ],
        ),
        (
            Some("references/discovery-cases.md"),
            vec!["graphit_hub_list", "graphit_hub_show"],
        ),
    ] {
        let mut input = json!({"module":"hub"});
        if let Some(reference) = reference {
            input["reference"] = reference.into();
        }
        let rendered = call(
            &client,
            "graphit_module_skill",
            json!({"input":input}),
            false,
        )
        .await;
        let source = rendered["source"].as_str().unwrap();
        assert!(source.contains("graphit_hub_list"));
        assert!(source.contains("graphit_hub_show"));
        let examples: Vec<Value> = source
            .split("```json\n")
            .skip(1)
            .map(|body| serde_json::from_str(body.split_once("```").unwrap().0).unwrap())
            .collect();
        assert_eq!(examples.len(), tools.len());
        for (tool, payload) in tools.into_iter().zip(examples) {
            serde_json::from_value::<Arguments>(payload.clone()).unwrap();
            let result = call(
                &client,
                tool,
                payload,
                matches!(tool, "graphit_hub_content" | "graphit_hub_projects"),
            )
            .await;
            if tool == "graphit_hub_content" {
                assert_eq!(result["code"], "artifact_not_installed");
            }
        }
    }
    client.cancel().await.unwrap();
    denied.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_hub_local_content_reads_owned_resources_and_links_without_network() {
    let (_temp, server, raw) = fixture();
    assert!(server.0.hub_http.is_none());
    let snapshot = project_snapshot(&server);
    let (paths, producer, _) = &snapshot.projects[0];
    let policy = &server.0.policy.source.native_install.hub;
    let registry = project_registry(&server, 0);
    let resource = SourceRef::resources(
        paths,
        *producer,
        ArtifactType::Mcp,
        ArtifactId::new("local/resource", policy.projection()).unwrap(),
        ConcreteVersion::new("local-release", policy.projection()).unwrap(),
        policy.projection(),
    )
    .unwrap();
    std::fs::create_dir_all(resource.directory().as_path()).unwrap();
    std::fs::write(
        resource.directory().as_path().join("config.json"),
        "{\"name\":\"Owned local resource\"}\n",
    )
    .unwrap();
    let linked = paths.root().join("linked");
    std::fs::create_dir(&linked).unwrap();
    std::fs::write(linked.join("README.md"), "# Owned local link\n").unwrap();
    let link = SourceRef::local_link(
        graphit_core::native_path::NativePath::new(linked.clone()).unwrap(),
        *producer,
        ArtifactType::Mcp,
        ArtifactId::new("local/link", policy.projection()).unwrap(),
        ConcreteVersion::new("local-release", policy.projection()).unwrap(),
        policy.projection(),
    )
    .unwrap();
    let foreign = snapshot.projects[1].0.root().join("private");
    std::fs::create_dir(&foreign).unwrap();
    std::fs::write(foreign.join("SECRET.md"), [0xffu8, 0xfe]).unwrap();
    let escaped = SourceRef::local_link(
        graphit_core::native_path::NativePath::new(foreign.clone()).unwrap(),
        *producer,
        ArtifactType::Mcp,
        ArtifactId::new("outside/project", policy.projection()).unwrap(),
        ConcreteVersion::new("local-release", policy.projection()).unwrap(),
        policy.projection(),
    )
    .unwrap();
    for source in [resource, link, escaped] {
        put(
            &registry,
            source,
            InstallationTarget::for_artifact(NativeBinding::KiroV3, ArtifactType::Mcp),
            InstalledBackend::Native,
        );
    }
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Hub)],
        Duration::from_secs(90),
    );
    let (_denied_session, _denied_run, denied_reference, denied_bearer) = credential(
        &server,
        &raw,
        Role::Tracker,
        &[Permission::Read(Module::Task)],
        Duration::from_secs(90),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    let denied = connect(
        &uri,
        role_credential(&denied_reference, &denied_bearer, server.0.task.content),
    )
    .await;
    let input = |id, path| json!({"input":{"scope":"project","artifact_type":"mcp","id":id,"version":"local-release","path":path}});
    let rendered = call(
        &client,
        "graphit_module_skill",
        json!({"input":{"module":"hub"}}),
        false,
    )
    .await;
    let instruction = rendered["source"].as_str().unwrap();
    let example = instruction
        .split("```json\n")
        .skip(1)
        .map(|s| serde_json::from_str::<Value>(s.split_once("```").unwrap().0).unwrap())
        .find(|p| p["input"]["id"] == "local/resource")
        .unwrap();
    serde_json::from_value::<Arguments>(example.clone()).unwrap();
    let from_instruction = call(&client, "graphit_hub_content", example, false).await;
    assert_eq!(
        from_instruction["content"]["content"],
        "{\"name\":\"Owned local resource\"}\n"
    );
    let resource = call(
        &client,
        "graphit_hub_content",
        input("local/resource", "config.json"),
        false,
    )
    .await;
    assert_eq!(resource["content"]["path"], "config.json");
    assert_eq!(
        resource["content"]["content"],
        "{\"name\":\"Owned local resource\"}\n"
    );
    let linked_body = call(
        &client,
        "graphit_hub_content",
        input("local/link", "README.md"),
        false,
    )
    .await;
    assert_eq!(linked_body["content"]["content"], "# Owned local link\n");
    assert_eq!(
        call(
            &client,
            "graphit_hub_content",
            input("outside/project", "SECRET.md"),
            true
        )
        .await["code"],
        "authentication_or_permission_denied"
    );
    for path in [
        "",
        "../private/SECRET.md",
        "/outside",
        "nested/../../outside",
        "nested\\SECRET.md",
    ] {
        assert_eq!(
            call(
                &client,
                "graphit_hub_content",
                input("local/link", path),
                true
            )
            .await["code"],
            "invalid_arguments"
        );
    }
    assert_eq!(
        call(
            &client,
            "graphit_hub_content",
            input("local/link", "missing.md"),
            true
        )
        .await["code"],
        "artifact_file_absent"
    );
    call(
        &denied,
        "graphit_hub_content",
        input("local/resource", "config.json"),
        true,
    )
    .await;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&foreign, linked.join("alias")).unwrap();
        let rejected = call(
            &client,
            "graphit_hub_content",
            input("local/link", "alias/SECRET.md"),
            true,
        )
        .await;
        assert!(rejected.get("content").is_none());
    }
    client.cancel().await.unwrap();
    denied.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

fn install_broker_profile(server: &RoleToolServer, ready: &Value, disabled: bool) {
    use graphit_auth::{
        profile::{
            AiService, BrokerService, OidcTokens, ProfileCredentials, ProfileData,
            ProfileExpectation, ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
        },
        store::AuthStore,
    };
    use graphit_core::auth_document::AuthDocumentStore;
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let live = server.0.auth.installation().live;
    let StoreOwner::Installation { installation_id } = live.owner() else {
        panic!("actual installation owner")
    };
    let captured = CapturedPublicationScope::installation(
        &server.0.policy.source.installation,
        installation_id.clone(),
        live.allowed_roots().map(ToOwned::to_owned),
        server.0.task.scope,
        original.snapshot(),
    )
    .unwrap();
    let store = AuthStore::new(
        AuthDocumentStore::capture(
            captured,
            AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
            original.snapshot(),
        )
        .unwrap(),
        ProfileLimits {
            bytes: 65536,
            nodes: 4096,
            scalar_bytes: 4096,
            collection_entries: 64,
            providers: 16,
            profiles: 16,
        },
        &original,
    )
    .unwrap();
    let text = |field| ready[field].as_str().expect("private Broker field");
    let outcome = store
        .mutate(
            store.load(&original).unwrap(),
            &original,
            |doc, original| {
                if doc.provider("broker").is_none() {
                    doc.add_provider(
                        "broker".into(),
                        ProviderConfig {
                            kind: ProviderKind::Broker(BrokerService {
                                endpoint: text("endpoint").into(),
                                mcp_resource: Some(text("resource").into()),
                                allow_anonymous: false,
                            }),
                            embedding: AiService::Disabled,
                            rerank: AiService::Disabled,
                            s3: S3Location {
                                bucket: None,
                                region: None,
                                endpoint: None,
                                prefix: None,
                                source: S3Source::BrokerScope,
                            },
                        },
                        doc.revision(),
                        1,
                        original,
                    )?;
                }
                let expectation = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("broker").unwrap().revision(),
                    profile_revision: doc.profile("role-hub").map(|p| p.revision()),
                };
                let updated = doc.profile("role-hub").map_or(1, |p| p.updated_at() + 1);
                doc.replace_profile_data(
                    ProfileData {
                        name: "role-hub".into(),
                        provider: "broker".into(),
                        username: "display-only".into(),
                        organization: None,
                        teams: vec![],
                        credentials: ProfileCredentials::Broker {
                            issuer: text("issuer").into(),
                            subject: text("subject").into(),
                            s3_disabled: disabled,
                            tokens: OidcTokens {
                                access_token: Secret::new(text("access_bearer").to_owned().into()),
                                refresh_token: None,
                                id_token: Secret::new("historical-fixture-login-metadata".into()),
                                expires_at: ready["expires_at"].as_i64().unwrap(),
                            },
                        },
                    },
                    expectation,
                    updated,
                    original,
                )?;
                doc.activate("role-hub", doc.revision(), original)
            },
        )
        .unwrap();
    assert!(
        outcome.failure().is_none() && outcome.cleanup_failure().is_none(),
        "actual profile publication"
    );
}
fn cache_file(server: &RoleToolServer, producer: ProjectId, id: &str) {
    let policy = &server.0.policy.source.native_install.hub;
    let source = SourceRef::hub_cache(
        &server.0.policy.source.installation,
        producer,
        ArtifactType::Skill,
        ArtifactId::new(id, policy.projection()).unwrap(),
        None,
        ConcreteVersion::new("literal-release", policy.projection()).unwrap(),
        policy.projection(),
    )
    .unwrap()
    .with_remote_file_origin(
        graphit_hub::RemoteFileOrigin::Broker {
            provider: "broker".into(),
        },
        [6; 32],
        policy.projection(),
    )
    .unwrap();
    // Logical metadata is valid without retaining/downloading the FILE payload.
    assert!(!source.directory().as_path().exists());
    put(
        &server.0.policy.source.registry,
        source,
        InstallationTarget::GlobalCache,
        InstalledBackend::GlobalCache,
    );
}
// Exercise the actual FILE producer kernel with independent local-source and remote
// namespace admissions; the Root CLI publisher's own-project selector is reused evidence.
async fn publish_file_kernel(
    server: &RoleToolServer,
    raw: &str,
    producer_source: &crate::source_server::test_fixture::Fixture,
    kind: crate::remote_file_install::client::Kind,
    artifact_id: &str,
    entry: &str,
    body: &str,
) {
    use graphit_core::artifact_store::{CapturedSourceReader, CopyPolicy, SourceReadLimits};
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        server.0.task.content,
        &cancel,
        Instant::now() + Duration::from_secs(90),
    )
    .unwrap();
    let (paths, id, version) = &producer_source.projects[0];
    let producer = *id;
    let policy = &server.0.policy.source.native_install.hub;
    let source = SourceRef::resources(
        paths,
        *id,
        kind.artifact_type(),
        ArtifactId::new(artifact_id, policy.projection()).unwrap(),
        ConcreteVersion::new("literal-release", policy.projection()).unwrap(),
        policy.projection(),
    )
    .unwrap();
    std::fs::create_dir_all(source.directory().as_path()).unwrap();
    std::fs::write(source.directory().as_path().join(entry), body).unwrap();
    let live =
        CapturedPublicationScope::project(paths, *id, server.0.task.scope, original.snapshot())
            .unwrap();
    let binding = live.binding().unwrap();
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    let source_issuer = producer_source.auth.issuer();
    let source_caller = source_issuer
        .authenticate_runtime(&producer_source.raw, &original)
        .unwrap();
    let authority = ProjectRecordAuthority::new(source_issuer);
    let read = authority
        .admit_authenticated_read(
            &source_caller,
            &original,
            graphit_auth::ScopeInput {
                installation: producer_source.auth.installation(),
                project: Some(graphit_auth::ProjectCapture {
                    scope: graphit_auth::ScopeCapture {
                        live: &live,
                        record: &binding,
                    },
                    version,
                }),
                providers: &[],
                binding: producer_source.server.policy().binding,
                global_version: "",
            },
            graphit_auth::ProjectReadModule::Hub,
        )
        .unwrap();
    let mut service = issuer.capture_hub_write_service(&caller, "broker").unwrap();
    let verifier = server
        .0
        .hub_verifiers
        .as_ref()
        .unwrap()
        .get(&mut service)
        .unwrap();
    let lease = service.native_credential(&verifier).await.unwrap();
    let storage = crate::native_storage_credentials::broker_credentials(
        &lease,
        producer,
        server.0.hub_http.as_ref().unwrap(),
        true,
    )
    .await
    .unwrap();
    let before = || -> io::Result<()> {
        original.check()?;
        read.recheck().map_err(io::Error::other)?;
        lease.check().map_err(io::Error::other)?;
        storage.check().map_err(io::Error::from)
    };
    let relative = source
        .directory()
        .as_path()
        .strip_prefix(paths.root())
        .unwrap();
    let reader = CapturedSourceReader::capture_project(
        &live,
        relative,
        graphit_fs::NodeKind::Directory,
        &policy
            .snapshot_policy(CopyPolicy::RejectFileLinks, [])
            .unwrap(),
        SourceReadLimits::new(64 * 1024, 64 * 1024, policy.copy().total_file_bytes_limit())
            .unwrap(),
        original.snapshot(),
    )
    .unwrap();
    let metadata = crate::file_publication::Metadata {
        provider: "broker".into(),
        artifact_type: kind,
        id: artifact_id.into(),
        version: "literal-release".into(),
        name: "Published FILE".into(),
        description: Some("Actual protected FILE body".into()),
        tags: vec![],
        author: None,
        dependencies: None,
        expected_sha256: None,
        set_latest: false,
    };
    let prepared =
        crate::file_publication::prepare(&metadata, producer, &reader, policy, &original, &before)
            .unwrap();
    let metadata_effect = crate::hub_project_metadata::publish(
        storage.store.as_ref(),
        &storage.prefix,
        (paths, *id),
        policy.content().max_input_bytes,
        &original,
        &before,
    )
    .await
    .unwrap();
    assert_eq!(
        metadata_effect,
        if matches!(kind, crate::remote_file_install::client::Kind::Skill) {
            "published"
        } else {
            "unchanged"
        }
    );
    let receipt = crate::file_publication::publish(
        crate::file_publication::Input {
            store: storage.store.as_ref(),
            storage_prefix: &storage.prefix,
            prepared,
        },
        &original,
        before,
    )
    .await
    .unwrap();
    assert_eq!(receipt.producer, producer);
    assert_eq!(receipt.id, artifact_id);
}
async fn install_file_through_source(
    source: &crate::source_server::test_fixture::Fixture,
    producer: ProjectId,
) -> (tokio::task::JoinHandle<io::Result<()>>, SourceRef, Vec<u8>) {
    let cancel = Cancellation::default();
    let original = request(&cancel, source.server.policy().content);
    let listener = graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &cancel,
    )
    .unwrap();
    let address = listener.address();
    let owner = source.server.clone();
    let serving = tokio::spawn(async move { owner.serve(listener).await });
    let connection = crate::ast_source::connect_source(
        &source.paths,
        address,
        &reqwest::Client::new(),
        source.server.policy().index,
        &original,
        (),
    )
    .await
    .unwrap();
    let selection = json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":"published/file","version":"literal-release","exact":true});
    let installed = connection
        .peer()
        .call_tool(
            CallToolRequestParams::new("graphit_hub_install").with_arguments(
                json!({"ai_optimized":false,"selection":selection,"global":true})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    let code = installed
        .structured_content
        .as_ref()
        .and_then(|v| v.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("no_public_code");
    assert_ne!(
        installed.is_error,
        Some(true),
        "real FILE install failed: {code}"
    );
    let content=connection.peer().call_tool(CallToolRequestParams::new("graphit_hub_content").with_arguments(json!({"ai_optimized":false,"installed":{"artifact_type":"skill","id":"published/file","version":"literal-release","global":true,"path":"SKILL.md"}}).as_object().unwrap().clone())).await.unwrap();
    assert_ne!(
        content.is_error,
        Some(true),
        "Root installed FILE reader failed"
    );
    assert!(
        serde_json::to_string(&content.structured_content)
            .unwrap()
            .contains("Actual protected FILE body")
    );
    let remote = connection
        .peer()
        .call_tool(
            CallToolRequestParams::new("graphit_hub_content").with_arguments(
                json!({"ai_optimized":false,"remote":{"selection":selection,"path":"SKILL.md"}})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_ne!(
        remote.is_error,
        Some(true),
        "Root remote FILE reader failed"
    );
    assert!(
        serde_json::to_string(&remote.structured_content)
            .unwrap()
            .contains("Actual protected FILE body")
    );
    let agent_selection = json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"agent","id":"custom/hub-reader","version":"literal-release","exact":true});
    let installed = connection
        .peer()
        .call_tool(
            CallToolRequestParams::new("graphit_hub_install").with_arguments(
                json!({"ai_optimized":false,"selection":agent_selection,"global":true})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    let code = installed
        .structured_content
        .as_ref()
        .and_then(|v| v.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("no_public_code");
    assert_ne!(
        installed.is_error,
        Some(true),
        "real custom Agent install failed: {code}"
    );
    let agent=connection.peer().call_tool(CallToolRequestParams::new("graphit_hub_content").with_arguments(json!({"ai_optimized":false,"installed":{"artifact_type":"agent","id":"custom/hub-reader","version":"literal-release","global":true,"path":"AGENT.md"}}).as_object().unwrap().clone())).await.unwrap();
    assert_ne!(agent.is_error, Some(true), "real custom Agent read failed");
    let body = agent.structured_content.unwrap()["content"]["content"]
        .as_str()
        .unwrap()
        .as_bytes()
        .to_vec();
    let policy = &source.server.policy().native_install.hub;
    let registry = source.server.policy().registry.load().unwrap();
    let installed = registry
        .get_version(
            ArtifactType::Agent,
            "custom/hub-reader",
            &ConcreteVersion::new("literal-release", policy.projection()).unwrap(),
        )
        .unwrap()
        .installations
        .iter()
        .find(|i| i.target == InstallationTarget::GlobalCache)
        .unwrap()
        .original_source
        .clone();
    connection.cancel().await.unwrap();
    (serving, installed, body)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the owned Broker storage fixture binary"]
async fn role_hub_file_metadata_uses_current_role_service_and_real_broker_project_grant() {
    use crate::browser_auth::tests::remote_native_web::BrokerProcess;
    use futures_util::FutureExt;
    use graphit_auth::{
        CredentialSource, ExternalEffect, RemoteServiceBindingInput, ServiceOrigin, ServicePurpose,
        ServiceRuleInput,
    };
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("owned Broker fixture path");
    let producer_source = crate::source_server::test_fixture::fixture_with_projects_and_authority(
        &[Permission::Read(Module::Hub)],
        &[],
        Brand::default(),
        NativeBinding::KiroV3,
        &["producer"],
        &[],
    );
    let producer = producer_source.projects[0].1;
    let broker = BrokerProcess::start(std::path::Path::new(&binary), &producer.to_string()).await;
    let result=Box::pin(std::panic::AssertUnwindSafe(async{
        let ready=&broker.ready;
        let remote=Permission::External(ExternalEffect::RemoteRequest);
        let rules=[ServicePurpose::HubRead,ServicePurpose::HubWrite].map(|purpose|ServiceRuleInput::Remote{origin:ServiceOrigin::Runtime,binding:RemoteServiceBindingInput::Broker{provider:"broker",purpose,endpoint:ready["endpoint"].as_str().unwrap(),resource:Some(ready["resource"].as_str().unwrap())},credential:CredentialSource::NativeAccess,incoming:None});
        let mut source=crate::source_server::test_fixture::fixture_with_projects_and_authority(
            &[Permission::Delegate,Permission::Read(Module::Hub),remote],&[Permission::Delegate,Permission::Read(Module::Hub),remote,Permission::ManageAuthProfiles,Permission::ManageAccessGrants,Permission::Configure,Permission::ManageCache,Permission::External(ExternalEffect::PublishedFilesystem)],Brand::default(),NativeBinding::KiroV3,&["consumer"],&rules);
        let outcome=Box::pin(std::panic::AssertUnwindSafe(async{
            let mut policy=source.server.policy().clone();policy.content=policy.native_install.hub.content();
            let content=policy.content;
            // Configure the real Source owner before serving; Role transport configuration
            // alone does not authorize the Root FILE installer's loopback origin.
            source.server.shutdown().await.unwrap();
            source.server=crate::source_server::SourceServer::new(
                Arc::clone(&source.auth),Arc::clone(&source.projects),0,policy.clone(),CancellationToken::new()).unwrap();
            let brand=source.paths.brand();
            let loopback=graphit_core::config::ConfigKey::new("broker.allow_loopback_http").unwrap();
            let environment=ConfigSnapshot::capture_environment(brand,std::slice::from_ref(&loopback),[(loopback.environment_variable(brand).into(),"true".into())]).unwrap();
            let config=ConfigSnapshot::new(brand,[graphit_hub::policy::HubResourcePolicy::distribution_defaults(brand).unwrap(),environment]).unwrap();
            assert_eq!(config.get::<bool>(&loopback).unwrap(),Some(true));
            let cache=Arc::new(graphit_daemon::broker_verifiers::BrokerVerifierCache::new(graphit_http::client::Limits{
                request_body_bytes:content.max_input_bytes,response_body_bytes:content.max_input_bytes,url_bytes:content.max_scalar_bytes,header_bytes:content.max_input_bytes,header_count:content.max_collection_entries,max_in_flight:1,max_idle_per_host:1,idle_timeout:Duration::from_secs(30),max_certificates:1,max_certificate_bytes:content.max_input_bytes,
            },crate::admin_auth::verifier_limits(content),true,None,&[]));
            let http=reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(30)).build().unwrap();
            source.server.attach_native_verifiers(Arc::clone(&cache)).unwrap();
            source.server.attach_role_tools(config.clone()).unwrap();
            let server=RoleToolServer::new(Arc::clone(&source.auth),Arc::clone(&source.projects),0,RoleToolPolicy::capture(policy,config).unwrap(),CancellationToken::new()).unwrap().with_hub_transport(http,true,Some(cache)).unwrap();
            install_broker_profile(&server,ready,false);
            Box::pin(publish_file_kernel(&server,&source.raw,&producer_source,crate::remote_file_install::client::Kind::Skill,"published/file","SKILL.md","---\nname: published-file\ndescription: Actual FILE consumer fixture\n---\n# Published FILE\n\nActual protected FILE body.\n")).await;
            Box::pin(publish_file_kernel(&server,&source.raw,&producer_source,crate::remote_file_install::client::Kind::Agent,"custom/hub-reader","AGENT.md","---\nname: custom-hub-reader\ndescription: Explicit authenticated remote Hub reader\nread_only: true\nallows_shell: false\n---\nRead admitted Hub metadata and FILE content.\n")).await;
            let (source_serving,agent_source,agent_body)=install_file_through_source(&source,producer).await;
            cache_file(&server,producer,"allowed/file");
            cache_file(&server,source.projects[0].1,"denied/file");
            use sha2::{Digest as _,Sha256};
            let definition_cancel=Cancellation::default();
            let definition_original=request(&definition_cancel,content);
            let mut render=graphit_hub::native::RenderContext::new(content,definition_original.cancellation(),definition_original.snapshot().required_deadline().unwrap()).unwrap();
            graphit_hub::native::AgentDescriptor::parse(&agent_body,&mut render).unwrap();
            let permissions=[Permission::Read(Module::Hub),remote];
            let definition=RoleDefinition::new(Role::Coordinator,&agent_source,NativeBinding::KiroV3,&permissions,Sha256::digest(&agent_body).into(),IssuerLimits{max_permissions:64,max_scope_bytes:65536,max_parent_bearer_bytes:4096}).unwrap();
            let (_session,_run,reference,bearer)=credential_with_definition(&server,&source.raw,0,definition,&permissions,Duration::from_secs(90));
            assert!(matches!(reference.source(),graphit_hub::native::RoleSource::HubArtifact(_)));
            let (_restricted_session,_restricted_run,restricted_ref,restricted_bearer)=credential(&server,&source.raw,Role::Scout,&[Permission::Read(Module::Hub)],Duration::from_secs(90));
            let (uri,serving)=serve(&server).await;
            let client=connect(&uri,role_credential(&reference,&bearer,server.0.task.content)).await;
            let restricted=connect(&uri,role_credential(&restricted_ref,&restricted_bearer,server.0.task.content)).await;
            let query=|id|json!({"input":{"scope":"global_installation","artifact_type":"skill","id":id,"limit":1}});
            let allowed=call(&client,"graphit_hub_list",query("allowed/file"),false).await;
            assert_eq!(allowed["entries"][0]["producer"],producer.to_string());
            assert_eq!(allowed["entries"][0]["id"],"allowed/file");
            call(&restricted,"graphit_hub_list",query("allowed/file"),true).await;
            call(&client,"graphit_hub_list",query("denied/file"),true).await;
            let content=json!({"input":{"scope":"global_installation","artifact_type":"skill","id":"published/file","version":"literal-release","path":"SKILL.md"}});
            let rendered=call(&client,"graphit_module_skill",json!({"input":{"module":"hub"}}),false).await;
            let markdown=rendered["source"].as_str().unwrap();
            assert!(markdown.contains("graphit_hub_content"));
            let payload:Value=markdown.split("```json\n").skip(1).map(|s|serde_json::from_str::<Value>(s.split_once("```").unwrap().0).unwrap()).find(|p|p["input"]["id"]=="published/file"&&p["input"]["path"]=="SKILL.md").unwrap();
            serde_json::from_value::<Arguments>(payload.clone()).unwrap();
            assert_eq!(payload,content);
            let body=call(&client,"graphit_hub_content",payload,false).await;
            assert!(body["content"]["content"].as_str().unwrap().contains("Actual protected FILE body"));
            call(&restricted,"graphit_hub_content",content.clone(),true).await;
            let forged_content=json!({"input":{"scope":"global_installation","artifact_type":"skill","id":"allowed/file","version":"literal-release","path":"SKILL.md"}});
            assert_eq!(call(&client,"graphit_hub_content",forged_content,true).await["code"],"installed_file_cache_unavailable");
            let projects_payload=markdown.split("```json\n").skip(1).map(|body|serde_json::from_str::<Value>(body.split_once("```").unwrap().0).unwrap()).find(|p|p["input"]["name_prefix"]=="prod").unwrap();
            serde_json::from_value::<Arguments>(projects_payload.clone()).unwrap();
            let projects=call(&client,"graphit_hub_projects",projects_payload,false).await;
            assert_eq!(projects["projects"],json!([{"id":producer,"name":"producer"}]));
            assert!(projects["next_cursor"].is_null());
            let search=json!({"input":{"transport":"broker","provider":"broker","producer":projects["projects"][0]["id"],"artifact_type":"skill","id":"published/file","version":"literal-release","limit":1}});
            let available=call(&client,"graphit_hub_search",search.clone(),false).await;
            assert_eq!(available["producer"],json!(producer));
            assert_eq!(available["artifacts"][0]["id"],"published/file");
            assert_eq!(available["artifacts"][0]["versions"],json!(["literal-release"]));
            assert!(available["artifacts"][0]["latest"].is_null());
            assert!(available["next_cursor"].is_null());
            let metadata=available["artifacts"][0].as_object().unwrap();
            assert_eq!(metadata.len(),8);
            for private in ["root","backend","credentials","archive_sha256","bucket","prefix","endpoint"] {assert!(!metadata.contains_key(private));}
            assert_eq!(call(&restricted,"graphit_hub_search",search.clone(),true).await["code"],"authentication_or_permission_denied");
            assert_eq!(call(&restricted,"graphit_hub_projects",json!({"input":{"transport":"broker","provider":"broker","limit":1}}),true).await["code"],"authentication_or_permission_denied");
            call(&client,"graphit_hub_search",json!({"input":{"transport":"broker","provider":"broker","producer":source.projects[0].1,"limit":1}}),true).await;
            call(&client,"graphit_hub_search",json!({"input":{"transport":"broker","provider":"broker","producer":producer,"limit":1,"query":"unsupported free text"}}),true).await;
            call(&client,"graphit_hub_projects",json!({"input":{"transport":"broker","provider":"broker","id":producer,"name_prefix":"prod","limit":1}}),true).await;
            install_broker_profile(&server,ready,true);
            assert_eq!(call(&client,"graphit_hub_search",search,true).await["code"],"hub_service_disabled");
            assert_eq!(call(&client,"graphit_hub_projects",json!({"input":{"transport":"broker","provider":"broker","limit":1}}),true).await["code"],"hub_service_disabled");
            assert_eq!(call(&client,"graphit_hub_content",content,true).await["code"],"hub_service_disabled");
            assert_eq!(call(&client,"graphit_hub_list",query("allowed/file"),true).await["code"],"hub_service_disabled");
            client.cancel().await.unwrap();restricted.cancel().await.unwrap();server.shutdown().await.unwrap();serving.await.unwrap().unwrap();
            source.server.shutdown().await.unwrap();source_serving.await.unwrap().unwrap();
        }).catch_unwind()).await;
        source.server.shutdown().await.unwrap();
        if let Err(failure)=outcome{std::panic::resume_unwind(failure);}
    }).catch_unwind()).await;
    broker.close().await;
    producer_source.server.shutdown().await.unwrap();
    if let Err(failure) = result {
        std::panic::resume_unwind(failure);
    }
}
