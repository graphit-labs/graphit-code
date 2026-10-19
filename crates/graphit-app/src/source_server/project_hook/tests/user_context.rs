#![cfg(target_os = "linux")]
use super::*;
use crate::native_install_http::auth_fixture as native_support;
use graphit_auth::{
    Permission, ProjectWriteOperation, UserMemoryWriteOperation,
    profile::*,
    store::{AuthStore, AuthStoreError},
};
use graphit_core::{
    artifact_store::{StoreOwner, UserIdentity},
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
};
use graphit_daemon::broker_verifiers::BrokerVerifierCache;
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::{
        CreateMemory, DeleteMemory, Memory, MemoryListFilter, MemoryStoreWriteLimits,
        NativeMemoryStore, UpdateMemory,
    },
    user_memory_store::{NativeUserMemoryStore, UserMemoryListRead},
};
use std::num::NonZeroUsize;

type SourceFixture = super::super::super::test_fixture::Fixture;
fn request_context<'a>(source: &SourceFixture, cancel: &'a Cancellation) -> RequestContext<'a> {
    RequestContext::new(
        source.server.0.policy.content,
        cancel,
        Instant::now() + Duration::from_secs(180),
    )
    .unwrap()
}
fn write_limits(source: &SourceFixture) -> MemoryStoreWriteLimits {
    let policy = &source.server.0.policy;
    let content = policy.native_install.hub.content();
    MemoryStoreWriteLimits {
        read: source.server.0.hook.as_ref().unwrap().memory,
        artifacts: policy.native_install.hub.artifacts(),
        max_input_bytes: NonZeroUsize::new(content.max_input_bytes).unwrap(),
        max_row_bytes: NonZeroUsize::new(content.max_input_bytes).unwrap(),
    }
}
fn memory(key: &str, body: &str, mandatory: bool) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: format!("Native USER {key} ação"),
        body: body.into(),
        attributes: MemoryAttributes {
            classification: MemoryClass::Fact,
            mandatory,
            important: false,
        },
        tags: vec![],
        references: None,
    }
}
fn capture_user(
    source: &SourceFixture,
    caller: &graphit_auth::AuthenticatedCaller<'_>,
    request: &RequestContext<'_>,
) -> CapturedPublicationScope {
    let subject = source
        .auth
        .issuer()
        .user_from_authenticated(caller)
        .unwrap();
    let StoreOwner::Installation { installation_id } = source.auth.installation().live.owner()
    else {
        unreachable!()
    };
    CapturedPublicationScope::user(
        &source.paths,
        installation_id.clone(),
        UserIdentity::new(
            subject.issuer().unwrap().to_owned(),
            subject.subject().unwrap().to_owned(),
        )
        .unwrap(),
        source.server.0.policy.scope,
        request.snapshot(),
    )
    .unwrap()
}
async fn seed_user(
    source: &SourceFixture,
    cache: &BrokerVerifierCache,
    request: &RequestContext<'_>,
    input: CreateMemory,
) -> Memory {
    let caller = source
        .auth
        .issuer()
        .authenticate_runtime(&source.raw, request)
        .unwrap();
    let (binding, revision) = caller.native_provider().unwrap();
    let verifier = cache.get_native(&binding, revision).unwrap();
    let caller = caller.authenticate_native(&verifier).await.unwrap();
    let live = capture_user(source, &caller, request);
    let record = live.binding().unwrap();
    let write = source
        .auth
        .issuer()
        .admit_authenticated_user_memory_write(
            &caller,
            request,
            ScopeCapture {
                live: &live,
                record: &record,
            },
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    NativeUserMemoryStore::create(request.snapshot(), &write, input, write_limits(source))
        .await
        .unwrap()
}
async fn seed_project(source: &SourceFixture, request: &RequestContext<'_>) {
    let policy = &source.server.0.policy;
    let (paths, id, version) = &source.projects[0];
    let live =
        CapturedPublicationScope::project(paths, *id, policy.scope, request.snapshot()).unwrap();
    let record = live.binding().unwrap();
    let caller = source
        .auth
        .issuer()
        .authenticate_runtime(&source.raw, request)
        .unwrap();
    let authority = ProjectRecordAuthority::new(source.auth.issuer());
    let admission = authority
        .admit_authenticated_write(
            &caller,
            request,
            ScopeInput {
                installation: source.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &live,
                        record: &record,
                    },
                    version,
                }),
                providers: &[],
                binding: policy.binding,
                global_version: "",
            },
            ProjectWriteOperation::Memory,
        )
        .unwrap();
    NativeMemoryStore::create(
        request.snapshot(),
        &admission,
        memory("project", "PROJECT separate private-body", true),
        write_limits(source),
    )
    .await
    .unwrap();
}
async fn delete_user(
    source: &SourceFixture,
    cache: &BrokerVerifierCache,
    request: &RequestContext<'_>,
    current: &Memory,
) {
    let caller = source
        .auth
        .issuer()
        .authenticate_runtime(&source.raw, request)
        .unwrap();
    let (binding, revision) = caller.native_provider().unwrap();
    let verifier = cache.get_native(&binding, revision).unwrap();
    let caller = caller.authenticate_native(&verifier).await.unwrap();
    let live = capture_user(source, &caller, request);
    let record = live.binding().unwrap();
    let write = source
        .auth
        .issuer()
        .admit_authenticated_user_memory_write(
            &caller,
            request,
            ScopeCapture {
                live: &live,
                record: &record,
            },
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    NativeUserMemoryStore::delete(
        request.snapshot(),
        &write,
        current.id,
        DeleteMemory {
            expected_revision: current.revision,
            idempotency_key: "remove-transient".into(),
            reason: "Prepare actual published empty corpus.".into(),
        },
        write_limits(source),
    )
    .await
    .unwrap();
}
async fn user_list(
    source: &SourceFixture,
    cache: &BrokerVerifierCache,
    request: &RequestContext<'_>,
) -> UserMemoryListRead {
    let caller = source
        .auth
        .issuer()
        .authenticate_runtime(&source.raw, request)
        .unwrap();
    let (binding, revision) = caller.native_provider().unwrap();
    let verifier = cache.get_native(&binding, revision).unwrap();
    let caller = caller.authenticate_native(&verifier).await.unwrap();
    let live = capture_user(source, &caller, request);
    let record = live.binding().unwrap();
    let read = source
        .auth
        .issuer()
        .admit_authenticated_user_memory_read(
            &caller,
            request,
            ScopeCapture {
                live: &live,
                record: &record,
            },
        )
        .unwrap();
    NativeUserMemoryStore::list(
        request.snapshot(),
        &read,
        MemoryListFilter {
            mandatory: Some(true),
            ..Default::default()
        },
        write_limits(source).read,
    )
    .await
    .unwrap()
}
async fn update_body(
    source: &SourceFixture,
    cache: &BrokerVerifierCache,
    request: &RequestContext<'_>,
    current: &Memory,
    body: &str,
) -> Memory {
    let caller = source
        .auth
        .issuer()
        .authenticate_runtime(&source.raw, request)
        .unwrap();
    let (binding, revision) = caller.native_provider().unwrap();
    let verifier = cache.get_native(&binding, revision).unwrap();
    let caller = caller.authenticate_native(&verifier).await.unwrap();
    let live = capture_user(source, &caller, request);
    let record = live.binding().unwrap();
    let write = source
        .auth
        .issuer()
        .admit_authenticated_user_memory_write(
            &caller,
            request,
            ScopeCapture {
                live: &live,
                record: &record,
            },
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    NativeUserMemoryStore::update(
        request.snapshot(),
        &write,
        current.id,
        UpdateMemory {
            expected_revision: current.revision,
            idempotency_key: "current-user-body".into(),
            reason: "Refresh current USER context.".into(),
            title: None,
            body: Some(body.into()),
            classification: None,
            mandatory: None,
            important: None,
            tags: None,
            references: None,
        },
        write_limits(source),
    )
    .await
    .unwrap()
}
fn text(result: CallToolResult) -> String {
    assert_eq!(result.is_error, Some(false));
    assert_eq!(result.content.len(), 1);
    result.content[0].as_text().unwrap().text.clone()
}
fn projection(body: &str) -> Value {
    serde_json::from_str(
        body.split_once("Mandatory USER Memory JSON (data):\n")
            .unwrap()
            .1
            .trim(),
    )
    .unwrap()
}
fn assert_no_context(result: &CallToolResult) {
    assert_eq!(result.is_error, Some(true));
    for content in &result.content {
        if let Some(text) = content.as_text() {
            assert!(!text.text.contains("PROJECT Memory state:"));
            assert!(!text.text.contains("Mandatory USER Memory JSON"));
            assert!(!text.text.contains("private-user-body"));
        }
    }
}
fn native_store(source: &SourceFixture, request: &RequestContext<'_>) -> AuthStore {
    let StoreOwner::Installation { installation_id } = source.auth.installation().live.owner()
    else {
        unreachable!()
    };
    let live = CapturedPublicationScope::installation(
        &source.paths,
        installation_id.clone(),
        source
            .auth
            .installation()
            .live
            .allowed_roots()
            .map(ToOwned::to_owned),
        source.server.0.policy.scope,
        request.snapshot(),
    )
    .unwrap();
    AuthStore::new(
        AuthDocumentStore::capture(
            live,
            AuthDocumentLimits::new(1048576, 65536, 256, 65536).unwrap(),
            request.snapshot(),
        )
        .unwrap(),
        ProfileLimits {
            bytes: 1048576,
            nodes: 65536,
            scalar_bytes: 65536,
            collection_entries: 1024,
            providers: 16,
            profiles: 16,
        },
        request,
    )
    .unwrap()
}
fn native_profile(
    store: &AuthStore,
    broker: &native_support::Broker,
    key: &str,
    audience: &str,
    expiry: u64,
    request: &RequestContext<'_>,
) {
    let binding = BrokerService {
        endpoint: broker.origin.clone(),
        mcp_resource: None,
        allow_anonymous: false,
    };
    let data = ProfileData {
        name: key.into(),
        provider: key.into(),
        username: "display-not-principal".into(),
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Broker {
            issuer: broker.origin.clone(),
            subject: "native-fixture-sub".into(),
            s3_disabled: true,
            tokens: OidcTokens {
                access_token: Secret::new(broker.token(expiry, audience).into()),
                refresh_token: None,
                id_token: Secret::new("historical-fixture-id-token".into()),
                expires_at: expiry.try_into().unwrap(),
            },
        },
    };
    let result = store
        .mutate(store.load(request).unwrap(), request, |doc, request| {
            if doc.provider(key).is_none() {
                doc.add_provider(
                    key.into(),
                    ProviderConfig {
                        kind: ProviderKind::Broker(binding),
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
                    request,
                )?;
            }
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider(key).unwrap().revision(),
                profile_revision: doc.profile(key).map(|profile| profile.revision()),
            };
            let updated = doc
                .profile(key)
                .map_or(1, |profile| profile.updated_at() + 1);
            doc.replace_profile_data(data, expected, updated, request)?;
            doc.activate(key, doc.revision(), request)
        })
        .unwrap();
    assert!(result.failure().is_none() && result.cleanup_failure().is_none());
}
fn cache(source: &SourceFixture) -> Arc<BrokerVerifierCache> {
    Arc::new(BrokerVerifierCache::new(
        graphit_http::client::Limits {
            request_body_bytes: 8192,
            response_body_bytes: 8192,
            url_bytes: 2048,
            header_bytes: 16384,
            header_count: 32,
            max_in_flight: 8,
            max_idle_per_host: 2,
            idle_timeout: Duration::from_secs(30),
            max_certificates: 2,
            max_certificate_bytes: 8192,
        },
        crate::admin_auth::verifier_limits(source.server.0.policy.content),
        true,
        None,
        &[],
    ))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_user_mandatory_context_is_current_complete_and_readonly_over_mcp_cli() {
    let broker = native_support::Broker::start().await;
    let mut source = configured(
        &[
            Permission::Read(Module::Memory),
            Permission::WriteMemory,
            Permission::ReadUser,
            Permission::MutateUser,
        ],
        json!({"memory":true,"task":false}),
    );
    let cancel = Cancellation::default();
    let original = request_context(&source, &cancel);
    let store = native_store(&source, &original);
    native_profile(
        &store,
        &broker,
        "native",
        "broker-api",
        native_support::now() + 600,
        &original,
    );
    let cache = cache(&source);
    source
        .server
        .attach_native_verifiers(Arc::clone(&cache))
        .unwrap();
    let (connection, serving) = client(&source).await;
    let hook = json!({"engine":"kiro","event":"SessionStart"});
    let absent = text(connection.call_tool(request(hook.clone())).await.unwrap());
    assert!(absent.contains("USER Memory state: no_corpus."));
    assert!(!absent.contains("Mandatory USER Memory JSON"));
    let transient = seed_user(
        &source,
        &cache,
        &original,
        memory("transient", "transient-empty-fixture", true),
    )
    .await;
    delete_user(&source, &cache, &original, &transient).await;
    let published_empty = text(connection.call_tool(request(hook.clone())).await.unwrap());
    assert!(published_empty.contains("USER Memory state: empty."));
    assert!(
        matches!(user_list(&source,&cache,&original).await,UserMemoryListRead::Published{memories,..} if memories.is_empty())
    );
    seed_project(&source, &original).await;
    seed_user(
        &source,
        &cache,
        &original,
        memory("hidden", "nonmandatory-user-not-forwarded", false),
    )
    .await;
    let empty = text(connection.call_tool(request(hook.clone())).await.unwrap());
    assert!(empty.contains("USER Memory state: empty."));
    assert!(!empty.contains("nonmandatory-user-not-forwarded"));
    let body = "private-user-body α {{ literal }}\nLinha com \"aspas\" e \\ literal";
    let first = seed_user(&source, &cache, &original, memory("mandatory", body, true)).await;
    seed_user(
        &source,
        &cache,
        &original,
        memory("second", "second-mandatory-complete", true),
    )
    .await;
    let before = user_list(&source, &cache, &original).await;
    let loaded = text(connection.call_tool(request(hook)).await.unwrap());
    assert!(loaded.contains("USER Memory state: loaded."));
    assert!(!loaded.contains("USER Memory was not loaded by this hook."));
    assert!(!loaded.contains("nonmandatory-user-not-forwarded"));
    assert!(loaded.contains("PROJECT separate private-body"));
    let rows = projection(&loaded);
    assert!(!rows.to_string().contains("PROJECT separate private-body"));
    let UserMemoryListRead::Published { memories, .. } = &before else {
        panic!("published native USER")
    };
    assert_eq!(rows.as_array().unwrap().len(), memories.len());
    for (projected, actual) in rows.as_array().unwrap().iter().zip(memories) {
        assert_eq!(projected["id"], serde_json::to_value(actual.id).unwrap());
        assert_eq!(projected["revision"], json!(actual.revision));
        assert_eq!(projected["title"], json!(actual.title));
        assert_eq!(projected["body"], json!(actual.body));
        assert_eq!(projected["mandatory"], json!(true));
        assert_eq!(projected["classification"], json!("fact"));
    }
    assert_eq!(user_list(&source, &cache, &original).await, before);
    let changed = update_body(
        &source,
        &cache,
        &original,
        &first,
        "current USER checkpoint α after repeated prompt",
    )
    .await;
    let (paths, _, version) = &source.projects[0];
    let arguments = crate::session_hook_entry::Arguments {
        recipient: crate::session_hook_entry::Recipient::Coordinator,
        engine: Engine::Kiro,
        event: Event::UserPromptSubmit,
        project_dir: Some(paths.root().to_owned()),
        task_session: None,
        task_ids: Vec::new(),
    };
    let policy = graphit_hub::policy::HubResourcePolicy::resolve(
        &source.server.0.hook.as_ref().unwrap().base,
    )
    .unwrap();
    let prepared = crate::session_hook_entry::prepare(
        &source.paths,
        &arguments,
        Some(version.version()),
        &original,
    )
    .unwrap();
    let mut bytes = Vec::new();
    crate::session_hook_entry::call(
        &source.paths,
        arguments.recipient,
        prepared,
        source.server.0.policy.index,
        &policy,
        &original,
        &mut bytes,
    )
    .await
    .unwrap();
    let repeated = String::from_utf8(bytes).unwrap();
    let current = projection(&repeated);
    let row = current
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == serde_json::to_value(changed.id).unwrap())
        .unwrap();
    assert_eq!(row["body"], json!(changed.body));
    assert_eq!(row["revision"], json!(changed.revision));
    assert!(!loaded.contains("current USER checkpoint"));
    let after = user_list(&source, &cache, &original).await;
    let again = text(
        connection
            .call_tool(request(json!({"engine":"kiro","event":"UserPromptSubmit"})))
            .await
            .unwrap(),
    );
    assert_eq!(projection(&again), current);
    assert_eq!(user_list(&source, &cache, &original).await, after);
    connection.cancel().await.unwrap();
    source.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    broker.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_user_permissions_and_signed_tokens_fail_whole_context() {
    let broker = native_support::Broker::start().await;
    for mode in [
        "no_read_user",
        "no_read_memory",
        "wrong_audience",
        "expired",
        "no_verifier",
    ] {
        let mut permissions = vec![Permission::WriteMemory, Permission::MutateUser];
        if mode != "no_read_user" {
            permissions.push(Permission::ReadUser);
        }
        if mode != "no_read_memory" {
            permissions.push(Permission::Read(Module::Memory));
        }
        let mut source = configured(&permissions, json!({"memory":true,"task":false}));
        let cancel = Cancellation::default();
        let original = request_context(&source, &cancel);
        let store = native_store(&source, &original);
        native_profile(
            &store,
            &broker,
            "native",
            "broker-api",
            native_support::now() + 600,
            &original,
        );
        let cache = cache(&source);
        seed_user(
            &source,
            &cache,
            &original,
            memory("private", "private-user-body", true),
        )
        .await;
        if mode == "wrong_audience" || mode == "expired" {
            native_profile(
                &store,
                &broker,
                "native",
                if mode == "wrong_audience" {
                    "another-resource"
                } else {
                    "broker-api"
                },
                if mode == "expired" {
                    native_support::now() - 120
                } else {
                    native_support::now() + 600
                },
                &original,
            );
        }
        if mode != "no_verifier" {
            source.server.attach_native_verifiers(cache).unwrap();
        }
        let (connection, serving) = client(&source).await;
        let result = connection
            .call_tool(request(json!({"engine":"kiro","event":"SessionStart"})))
            .await
            .unwrap();
        assert_no_context(&result);
        let expected = match mode {
            "no_read_user" | "no_read_memory" => "Session hook USER Memory permission denied",
            "wrong_audience" | "expired" => "Session hook USER Native authentication failed",
            "no_verifier" => "Session hook USER Native verifier unavailable",
            _ => unreachable!(),
        };
        assert_eq!(result.content.len(), 1);
        assert!(
            result.content[0].as_text().unwrap().text.contains(expected),
            "{mode}: expected {expected}"
        );
        connection.cancel().await.unwrap();
        source.server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
    broker.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_user_gates_skip_invalid_native_and_absence_is_explicit() {
    let broker = native_support::Broker::start().await;
    for mode in ["no_active", "memory_off", "hooks_off"] {
        let modules = match mode {
            "memory_off" => json!({"memory":false,"task":false}),
            "hooks_off" => json!({"memory":true,"hooks":false,"task":false}),
            _ => json!({"memory":true,"task":false}),
        };
        let source = configured(&[Permission::Read(Module::Memory)], modules);
        let cancel = Cancellation::default();
        let original = request_context(&source, &cancel);
        if mode != "no_active" {
            let store = native_store(&source, &original);
            native_profile(
                &store,
                &broker,
                "native",
                "another-resource",
                native_support::now() - 120,
                &original,
            );
        }
        // No verifier is attached. Disabled gates must never reach Native authentication.
        let (connection, serving) = client(&source).await;
        let result = text(
            connection
                .call_tool(request(json!({"engine":"kiro","event":"SessionStart"})))
                .await
                .unwrap(),
        );
        match mode {
            "no_active" => assert!(result.contains("USER Memory state: unavailable.")),
            "memory_off" => assert!(result.contains("USER Memory state: disabled.")),
            "hooks_off" => assert!(result.is_empty()),
            _ => unreachable!(),
        }
        assert!(!result.contains("Mandatory USER Memory JSON"));
        let StoreOwner::Installation { installation_id } = source.auth.installation().live.owner()
        else {
            unreachable!()
        };
        let scope = CapturedPublicationScope::user(
            &source.paths,
            installation_id.clone(),
            UserIdentity::new(broker.origin.clone(), "native-fixture-sub".into()).unwrap(),
            source.server.0.policy.scope,
            original.snapshot(),
        )
        .unwrap();
        assert!(!scope.runtime_root().exists());
        assert_eq!(broker.requests(), [0; 3]);
        connection.cancel().await.unwrap();
        source.server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
    broker.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_user_identity_and_installation_changes_never_forward_previous_context() {
    let first = native_support::Broker::start().await;
    let second = native_support::Broker::start().await;
    let permissions = [
        Permission::Read(Module::Memory),
        Permission::WriteMemory,
        Permission::ReadUser,
        Permission::MutateUser,
    ];
    let mut source = configured(&permissions, json!({"memory":true,"task":false}));
    let cancel = Cancellation::default();
    let original = request_context(&source, &cancel);
    let store = native_store(&source, &original);
    native_profile(
        &store,
        &first,
        "identity-a",
        "broker-api",
        native_support::now() + 600,
        &original,
    );
    let cache = cache(&source);
    source
        .server
        .attach_native_verifiers(Arc::clone(&cache))
        .unwrap();
    seed_user(
        &source,
        &cache,
        &original,
        memory("a", "private-user-body identity A", true),
    )
    .await;
    let (connection, serving) = client(&source).await;
    let a = text(
        connection
            .call_tool(request(json!({"engine":"kiro","event":"SessionStart"})))
            .await
            .unwrap(),
    );
    assert!(projection(&a).to_string().contains("identity A"));
    native_profile(
        &store,
        &second,
        "identity-b",
        "broker-api",
        native_support::now() + 600,
        &original,
    );
    let b = text(
        connection
            .call_tool(request(json!({"engine":"kiro","event":"SessionStart"})))
            .await
            .unwrap(),
    );
    assert!(b.contains("USER Memory state: no_corpus."));
    assert!(!b.contains("identity A"));
    seed_user(
        &source,
        &cache,
        &original,
        memory("b", "private-user-body identity B", true),
    )
    .await;
    let b = text(
        connection
            .call_tool(request(json!({"engine":"kiro","event":"SessionStart"})))
            .await
            .unwrap(),
    );
    assert!(projection(&b).to_string().contains("identity B"));
    assert!(!b.contains("identity A"));
    connection.cancel().await.unwrap();
    source.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    let mut other = configured(&permissions, json!({"memory":true,"task":false}));
    let other_original = request_context(&other, &cancel);
    let other_store = native_store(&other, &other_original);
    native_profile(
        &other_store,
        &first,
        "identity-a",
        "broker-api",
        native_support::now() + 600,
        &other_original,
    );
    let other_cache = self::cache(&other);
    other.server.attach_native_verifiers(other_cache).unwrap();
    let (connection, serving) = client(&other).await;
    let isolated = text(
        connection
            .call_tool(request(json!({"engine":"kiro","event":"SessionStart"})))
            .await
            .unwrap(),
    );
    assert!(isolated.contains("USER Memory state: no_corpus."));
    assert!(!isolated.contains("identity A"));
    assert!(!isolated.contains("identity B"));
    connection.cancel().await.unwrap();
    other.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    first.shutdown().await;
    second.shutdown().await;
}

fn local_profile(store: &AuthStore, request: &RequestContext<'_>) {
    let result = store
        .mutate(store.load(request).unwrap(), request, |doc, request| {
            doc.add_provider(
                "local-service".into(),
                ProviderConfig {
                    kind: ProviderKind::Local {
                        allow_aws_credential_chain: false,
                        broker: None,
                    },
                    embedding: AiService::Disabled,
                    rerank: AiService::Disabled,
                    s3: S3Location {
                        bucket: None,
                        region: None,
                        endpoint: None,
                        prefix: None,
                        source: S3Source::LocalCredentials,
                    },
                },
                doc.revision(),
                1,
                request,
            )?;
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("local-service").unwrap().revision(),
                profile_revision: None,
            };
            doc.replace_profile_data(
                ProfileData {
                    name: "own-local".into(),
                    provider: "local-service".into(),
                    username: "synthetic-local".into(),
                    organization: None,
                    teams: vec![],
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: Some(Secret::new("local-fixture-key".into())),
                        broker_key: None,
                        embedding_api_key: None,
                        rerank_api_key: None,
                        s3: None,
                    }),
                },
                expected,
                1,
                request,
            )?;
            doc.activate("own-local", doc.revision(), request)
        })
        .unwrap();
    assert!(result.failure().is_none() && result.cleanup_failure().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_user_local_origin_and_local_mode_never_load_previous_native_user() {
    let broker = native_support::Broker::start().await;
    for local_origin in [false, true] {
        let mut source = configured(
            &[
                Permission::Read(Module::Memory),
                Permission::WriteMemory,
                Permission::ReadUser,
                Permission::MutateUser,
            ],
            json!({"memory":true,"task":false}),
        );
        let cancel = Cancellation::default();
        let original = request_context(&source, &cancel);
        let store = native_store(&source, &original);
        native_profile(
            &store,
            &broker,
            "native",
            "broker-api",
            native_support::now() + 600,
            &original,
        );
        let cache = cache(&source);
        seed_user(
            &source,
            &cache,
            &original,
            memory("native-private", "private-user-body", true),
        )
        .await;
        let network_before = broker.requests();
        local_profile(&store, &original);
        let origin = if local_origin {
            source.raw = "local-fixture-key".into();
            "local"
        } else {
            "runtime"
        };
        // The real local key proves Local admission, never a forwarded Runtime secret.
        if local_origin {
            source
                .auth
                .issuer()
                .authenticate_local(&source.raw, &original)
                .unwrap();
        }
        let (connection, serving) = client_origin(&source, origin).await;
        let result = text(
            connection
                .call_tool(request(json!({"engine":"kiro","event":"SessionStart"})))
                .await
                .unwrap(),
        );
        assert!(result.contains("USER Memory state: unavailable."));
        assert!(!result.contains("private-user-body"));
        assert_eq!(broker.requests(), network_before);
        assert!(!result.contains("Mandatory USER Memory JSON"));
        connection.cancel().await.unwrap();
        source.server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
    broker.shutdown().await;
}

async fn execute_start(source: &SourceFixture, cancel: &Cancellation) -> Result<String, String> {
    execute(
        &source.server.0,
        &source.projects[0],
        Arguments {
            engine: Engine::Kiro,
            event: Event::SessionStart,
            project_dir: None,
            project_version: None,
            task_context: None,
        },
        &source.raw,
        false,
        cancel,
        Instant::now() + Duration::from_secs(60),
    )
    .await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_user_escaped_cap_and_incomplete_corpus_fail_without_partial_context() {
    let broker = native_support::Broker::start().await;
    let mut source = configured(
        &[
            Permission::Read(Module::Memory),
            Permission::WriteMemory,
            Permission::ReadUser,
            Permission::MutateUser,
        ],
        json!({"memory":true,"task":false}),
    );
    let cancel = Cancellation::default();
    let original = request_context(&source, &cancel);
    let store = native_store(&source, &original);
    native_profile(
        &store,
        &broker,
        "native",
        "broker-api",
        native_support::now() + 600,
        &original,
    );
    let cache = cache(&source);
    source
        .server
        .attach_native_verifiers(Arc::clone(&cache))
        .unwrap();
    let first = seed_user(
        &source,
        &cache,
        &original,
        memory("escaped", "private-user-body", true),
    )
    .await;
    let baseline = execute_start(&source, &cancel).await.unwrap();
    let body = format!("private-user-body{}", "\"\\α".repeat(1536));
    let changed = update_body(&source, &cache, &original, &first, &body).await;
    let complete = execute_start(&source, &cancel).await.unwrap();
    let cap = baseline.len() + body.len();
    assert!(
        complete.len() > cap,
        "JSON escaped bytes must cause the boundary"
    );
    let before = user_list(&source, &cache, &original).await;
    let interrupted = Cancellation::default();
    interrupted.cancel();
    assert!(execute_start(&source, &interrupted).await.is_err());
    assert!(
        execute(
            &source.server.0,
            &source.projects[0],
            Arguments {
                engine: Engine::Kiro,
                event: Event::SessionStart,
                project_dir: None,
                project_version: None,
                task_context: None
            },
            &source.raw,
            false,
            &cancel,
            Instant::now()
        )
        .await
        .is_err()
    );
    {
        let hook = Arc::get_mut(
            Arc::get_mut(&mut source.server.0)
                .unwrap()
                .hook
                .as_mut()
                .unwrap(),
        )
        .unwrap();
        hook.output.max_context_bytes = cap;
        hook.output.max_output_bytes = cap;
    }
    let error = execute_start(&source, &cancel).await.unwrap_err();
    assert_eq!(
        error,
        "PROJECT hook context failed: HookContextError::Output"
    );
    assert!(!error.contains("private-user-body"));
    assert!(!error.contains("PROJECT Memory state:"));
    assert_eq!(user_list(&source, &cache, &original).await, before);
    let caller = source
        .auth
        .issuer()
        .authenticate_runtime(&source.raw, &original)
        .unwrap();
    let (binding, revision) = caller.native_provider().unwrap();
    let verifier = cache.get_native(&binding, revision).unwrap();
    let caller = caller.authenticate_native(&verifier).await.unwrap();
    let live = capture_user(&source, &caller, &original);
    let corpus = live.runtime_root().join("memory/index.lance");
    assert!(corpus.is_dir());
    std::fs::remove_dir_all(&corpus).unwrap();
    assert!(live.runtime_root().join("memory").is_dir());
    let error = execute_start(&source, &cancel).await.unwrap_err();
    assert!(error.contains("USER Memory read failed"));
    assert!(!error.contains("private-user-body"));
    assert!(!corpus.exists());
    assert_eq!(changed.body, body);
    broker.shutdown().await;
}

#[tokio::test]
async fn private_user_projection_is_denied_to_delegate_before_project_reads() {
    let source = configured(&[], json!({"memory":true,"task":false}));
    let cancel = Cancellation::default();
    let original = request_context(&source, &cancel);
    let hook = source.server.0.hook.as_ref().unwrap();
    let modules = hook
        .selected_modules(&source.paths, &source.projects[0].0, original.snapshot())
        .unwrap();
    let result = render_captured_hook(
        &original,
        ProjectHookInput {
            task_context: None,
            binding: NativeBinding::KiroV3,
            event: graphit_runtime::HookEvent::SessionStart,
            modules: &modules,
            audience: MandateAudience::Delegate,
            variables: &[],
            instruction_limits: hook.instructions,
            output_limits: hook.output,
        },
        None,
        None,
        Some(UserContext::Unavailable),
    )
    .await;
    assert!(matches!(
        result,
        Err(crate::project_hook_context::HookContextError::UserRecipientDenied)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selected_native_profile_missing_stale_and_invalid_are_not_absence() {
    let broker = native_support::Broker::start().await;
    for mode in ["missing", "stale", "invalid"] {
        let source = configured(
            &[Permission::Read(Module::Memory), Permission::ReadUser],
            json!({"memory":true,"task":false}),
        );
        let cancel = Cancellation::default();
        let original = request_context(&source, &cancel);
        let store = native_store(&source, &original);
        native_profile(
            &store,
            &broker,
            "native",
            "broker-api",
            native_support::now() + 600,
            &original,
        );
        let caller = source
            .auth
            .issuer()
            .authenticate_runtime(&source.raw, &original)
            .unwrap();
        let auth_file = source
            .auth
            .installation()
            .live
            .owner_root()
            .join("auth.json");
        assert!(auth_file.starts_with(source.temp.path()));
        let mut document: Value =
            serde_json::from_slice(&std::fs::read(&auth_file).unwrap()).unwrap();
        match mode {
            "missing" => document["active_profile"] = json!("missing-native-profile"),
            "stale" => {
                document["document_revision"] =
                    json!(document["document_revision"].as_u64().unwrap() + 1);
                document["providers"][0]["revision"] =
                    json!(document["providers"][0]["revision"].as_u64().unwrap() + 1);
            }
            "invalid" => {}
            _ => unreachable!(),
        }
        // Corrupt only this synthetic installation's real document, preserving private file mode.
        let bytes = if mode == "invalid" {
            b"{".to_vec()
        } else {
            serde_json::to_vec(&document).unwrap()
        };
        std::fs::write(&auth_file, bytes).unwrap();
        let error = caller.native_provider().unwrap_err();
        // Decode failures originate in AuthStore::load; active selection errors originate in Profile.
        assert!(
            matches!(
                (&error, mode),
                (ServiceUseError::Profile(ProfileError::Missing), "missing")
                    | (ServiceUseError::Profile(ProfileError::Stale), "stale")
                    | (
                        ServiceUseError::Store(AuthStoreError::Profile(
                            ProfileError::InvalidDocument
                        )),
                        "invalid"
                    )
            ),
            "mode {mode}, actual native provider error: {error:?}"
        );
        let (connection, serving) = client(&source).await;
        let result = connection
            .call_tool(request(json!({"engine":"kiro","event":"SessionStart"})))
            .await
            .unwrap();
        assert_no_context(&result);
        assert!(
            result.content[0]
                .as_text()
                .unwrap()
                .text
                .contains("Session hook USER Native profile unavailable")
        );
        connection.cancel().await.unwrap();
        source.server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
    assert_eq!(broker.requests(), [0; 3]);
    broker.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_inflight_profile_change_and_recipient_eof_preserve_failure_pulse_and_join() {
    use rmcp::model::{CallToolRequest, ClientRequest, ServerResult};
    for mode in ["profile_change", "recipient_eof"] {
        let (started, entered) = tokio::sync::oneshot::channel();
        let (release, waiting) = tokio::sync::oneshot::channel();
        let broker =
            native_support::Broker::start_with_discovery_gate(Some((started, waiting))).await;
        let mut source = configured(
            &[Permission::Read(Module::Memory), Permission::ReadUser],
            json!({"memory":true,"task":false}),
        );
        let cancel = Cancellation::default();
        let original = request_context(&source, &cancel);
        let store = native_store(&source, &original);
        native_profile(
            &store,
            &broker,
            "native",
            "broker-api",
            native_support::now() + 600,
            &original,
        );
        let native_cache = cache(&source);
        source.server.attach_native_verifiers(native_cache).unwrap();
        let (_activity, receiver) = attach_activity(&mut source);
        let before = *receiver.borrow();
        let (connection, serving) = client(&source).await;
        let pending = connection
            .send_cancellable_request(
                ClientRequest::CallToolRequest(CallToolRequest::new(request(
                    json!({"engine":"kiro","event":"UserPromptSubmit"}),
                ))),
                Default::default(),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), entered)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(broker.requests(), [1, 0, 0]);
        let accepted = *receiver.borrow();
        assert!(accepted > before);
        if mode == "profile_change" {
            local_profile(&store, &original);
            let caller = source
                .auth
                .issuer()
                .authenticate_runtime(&source.raw, &original)
                .unwrap();
            assert!(matches!(
                caller.native_provider(),
                Err(ServiceUseError::WrongMode)
            ));
            release.send(()).unwrap();
            let ServerResult::CallToolResult(result) = pending.await_response().await.unwrap()
            else {
                panic!("native hook response")
            };
            assert_no_context(&result);
            assert!(
                result.content[0]
                    .as_text()
                    .unwrap()
                    .text
                    .contains("Session hook USER Native authentication failed")
            );
            assert_eq!(*receiver.borrow(), accepted);
            connection.cancel().await.unwrap();
        } else {
            // The recipient closes while the real verifier awaits discovery.
            connection.cancel().await.unwrap();
            drop(pending);
            let _ = release.send(());
        }
        source.server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
        assert!(source.server.0.jobs.lock().await.tasks.is_empty());
        assert_eq!(*receiver.borrow(), accepted);
        broker.shutdown().await;
    }
}
