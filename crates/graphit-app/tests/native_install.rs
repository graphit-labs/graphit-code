//! Actual installation SDK integration. Empty canonical Icebug is not Parquet row proof.
use crate::native_install_auth as auth;
use arrow_array::{ArrayRef, RecordBatch, StringArray};
use graphit_app::native_install::{
    self, FailureCategory, InstallExecution, InstallRequest, InstallResources, PayloadEffect,
    RegistryEffect,
};
use graphit_auth::profile::{
    LocalCredentials, OidcTokens, ProfileCredentials, ProfileData, Secret,
};
use graphit_core::{
    artifact_store::{CapturedSourceReader, CopyPolicy, SourceReadLimits},
    file_lock::Cancellation,
    native_document_file::NativeDocumentFileLimits,
    project_identity::ProjectId,
    text_slice::Request,
};
use graphit_data::{LocalGraphPolicy, lance_module_store::LanceModuleStore};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, NativeBinding, SourceRef,
    native::ContentLimits,
    native_package::{self, PackageError, PackageKind},
    policy::RequestContext,
};
use serde_json::json;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
fn original(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        cancel,
        Instant::now() + Duration::from_secs(120),
    )
    .unwrap()
}
fn native_test(f: impl Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(f)
        })
        .unwrap()
        .join()
        .unwrap();
}
fn read_limits() -> SourceReadLimits {
    SourceReadLimits::new(65536, 4096, 536870912).unwrap()
}
fn index_limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 65536, 256, 65536).unwrap()
}
fn resources<'a>(f: &'a auth::Fixture, ctx: &RequestContext<'_>) -> InstallResources<'a> {
    InstallResources {
        auth: &f.auth,
        installation: &f.paths,
        registry: &f.registry,
        policy: &f.policy,
        graph_policy: LocalGraphPolicy::resolve(&f.config, ctx.snapshot()).unwrap(),
    }
}
async fn install(
    f: &auth::Fixture,
    request: &InstallRequest,
    ctx: &RequestContext<'_>,
) -> InstallExecution {
    native_install::execute_install(resources(f, ctx), &f.key(ctx), request, ctx).await
}
fn request(input: PathBuf, id: &str, version: &str) -> InstallRequest {
    InstallRequest {
        input,
        producer: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse::<ProjectId>().unwrap(),
        id: id.into(),
        version: version.into(),
        kind: None,
    }
}
async fn payload(path: &Path, kind: PackageKind, source: bool, wrong_schema: bool) {
    fs::create_dir(path).unwrap();
    if kind == PackageKind::Ast {
        fs::create_dir(path.join("graph.icebug")).unwrap();
        let manifest = json!({"manifest_version":3,"format":"icebug-canonical","storage":"direct-parquet","finished":true,
            "add_reverse_edges":false,"schema_file":"schema.cypher","node_tables":[],"relationships":[],"relation_uids":true,
            "generation":"native-install-fixture","n_edges":0,"repaired_strings":0,"invariants":{"indptr_row_groups":1,"self_loops":"forward-once"}});
        fs::write(
            path.join("graph.icebug/icebug.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(
            path.join("graph.icebug/schema.cypher"),
            "opaque native schema metadata",
        )
        .unwrap();
        if source {
            let schema = if wrong_schema {
                graphit_data::knowledge_index::physical_schema()
            } else {
                graphit_ast::indexed_source::source_schema()
            };
            let batch = if wrong_schema {
                RecordBatch::new_empty(Arc::clone(&schema))
            } else {
                let columns: Vec<ArrayRef> = vec![
                    Arc::new(StringArray::from(vec!["file.any"])),
                    Arc::new(StringArray::from(vec![Some(
                        "native stored source\nsecond line",
                    )])),
                ];
                RecordBatch::try_new(Arc::clone(&schema), columns).unwrap()
            };
            let store = LanceModuleStore::create(
                path.join("graph.icebug/source.lance").to_str().unwrap(),
                schema,
                vec![batch],
            )
            .await
            .unwrap();
            drop(store);
        }
    } else {
        let schema = if wrong_schema {
            graphit_ast::indexed_source::source_schema()
        } else {
            graphit_data::knowledge_index::physical_schema()
        };
        let batch = RecordBatch::new_empty(Arc::clone(&schema));
        let store = LanceModuleStore::create(
            path.join("index.lance").to_str().unwrap(),
            schema,
            vec![batch],
        )
        .await
        .unwrap();
        drop(store);
    }
}
fn export(
    f: &auth::Fixture,
    path: &Path,
    name: &str,
    kind: PackageKind,
    ctx: &RequestContext<'_>,
) -> PathBuf {
    let policy = f
        .policy
        .snapshot_policy(CopyPolicy::RejectFileLinks, [])
        .unwrap();
    let source = CapturedSourceReader::capture(
        path,
        graphit_fs::NodeKind::Directory,
        &policy,
        read_limits(),
        Arc::clone(ctx.snapshot()),
    )
    .unwrap();
    let zip = f.temp.path().join(format!("{name}.zip"));
    native_package::export(&source, kind, &zip, ctx.snapshot()).unwrap();
    zip
}
fn not_published(execution: &InstallExecution, category: FailureCategory) {
    assert_eq!(
        execution.outcome.payload,
        PayloadEffect::NotPublished,
        "{execution:?}"
    );
    assert_eq!(
        execution.outcome.registry,
        RegistryEffect::NotAttempted,
        "{execution:?}"
    );
    assert_eq!(
        execution.outcome.failure.as_ref().unwrap().category,
        category,
        "{execution:?}"
    );
    assert!(execution.cause.is_some());
    if let Some(path) = &execution.outcome.destination {
        assert!(!path.exists());
    }
}
#[test]
fn actual_signed_native_ast_knowledge_and_imported_source() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        let f = auth::Fixture::new(&ctx, true);
        let broker = auth::Broker::start().await;
        let ast = f.temp.path().join("ast");
        payload(&ast, PackageKind::Ast, true, false).await;
        let schema = graphit_ast::corpus::schema();
        LanceModuleStore::create(
            ast.join("graph.icebug")
                .join(graphit_ast::corpus::DIRECTORY)
                .to_str()
                .unwrap(),
            Arc::clone(&schema),
            vec![RecordBatch::new_empty(schema)],
        )
        .await
        .unwrap();
        let zip = export(&f, &ast, "ast", PackageKind::Ast, &ctx);
        let mut req = request(zip, "namespace/ast", "v1");
        // Runtime proof alone is not Native installation admission.
        not_published(
            &install(&f, &req, &ctx).await,
            FailureCategory::Authentication,
        );
        assert_eq!(broker.requests(), [0, 0, 0]);
        f.profile(&broker, auth::now() + 600, "broker-api", false, &ctx);
        f.registry
            .writer(ctx.remaining().unwrap().unwrap(), ctx.cancellation())
            .unwrap()
            .set_agents(&[NativeBinding::KiroV3])
            .unwrap();
        let first = install(&f, &req, &ctx).await;
        assert!(first.outcome.succeeded(), "{first:?}");
        assert_eq!(broker.requests(), [1, 1, 1]);
        assert_eq!(first.outcome.kind, Some(PackageKind::Ast));
        let corpus = first
            .outcome
            .destination
            .as_ref()
            .unwrap()
            .join("graph.icebug")
            .join(graphit_ast::corpus::DIRECTORY);
        LanceModuleStore::open(corpus.to_str().unwrap(), graphit_ast::corpus::schema())
            .await
            .unwrap();
        let id = ArtifactId::new(&req.id, f.policy.projection()).unwrap();
        let version = ConcreteVersion::new(&req.version, f.policy.projection()).unwrap();
        let document = f.registry.load().unwrap();
        let entry = document
            .get_version(ArtifactType::Ast, &req.id, &version)
            .unwrap();
        assert_eq!(entry.installations.len(), 1);
        assert_eq!(
            entry.installations[0].target,
            graphit_hub::InstallationTarget::GlobalNative
        );
        assert_eq!(
            entry.installations[0].original_source.producer(),
            req.producer
        );
        assert_eq!(entry.installations[0].original_source.physical_id(), &id);
        assert_eq!(document.agents(), [NativeBinding::KiroV3]);
        let selection = Request::default();
        let file_policy = f
            .policy
            .snapshot_policy(CopyPolicy::RejectFileLinks, [])
            .unwrap();
        let found = graphit_ast::imported_source::inspect_imported_source_runtime(
            f.auth.issuer(),
            &f.key(&ctx),
            graphit_ast::imported_source::ImportedSourceInput {
                registry: &f.registry,
                id: &id,
                version: &version,
                installation: f.auth.installation(),
                fallback: None,
                requested_relative: Path::new("file.any"),
                selection: &selection,
                file_policy: &file_policy,
                read_limits: read_limits(),
                path_limits: f.policy.retained(),
                index_limits: index_limits(),
                request: &ctx,
            },
        )
        .await
        .unwrap();
        assert_eq!(found.selection.source, "native stored source\nsecond line");
        let existing = install(&f, &req, &ctx).await;
        assert_eq!(existing.outcome.payload, PayloadEffect::NotPublished);
        assert_eq!(
            existing.outcome.failure.unwrap().category,
            FailureCategory::AlreadyExists
        );
        assert_eq!(f.registry.load().unwrap(), document);
        req.version = "v2".into();
        let second = install(&f, &req, &ctx).await;
        assert!(second.outcome.succeeded(), "{second:?}");
        assert_eq!(f.registry.load().unwrap().entries().count(), 2);
        let no_source = f.temp.path().join("ast-no-source");
        payload(&no_source, PackageKind::Ast, false, false).await;
        let req = request(
            export(&f, &no_source, "optional", PackageKind::Ast, &ctx),
            "namespace/optional",
            "v1",
        );
        let optional = install(&f, &req, &ctx).await;
        assert!(optional.outcome.succeeded(), "{optional:?}");
        assert!(
            !optional
                .outcome
                .destination
                .unwrap()
                .join("graph.icebug/source.lance")
                .exists()
        );
        let wiki = f.temp.path().join("wiki");
        payload(&wiki, PackageKind::Knowledge, false, false).await;
        let req = request(
            export(&f, &wiki, "wiki", PackageKind::Knowledge, &ctx),
            "namespace/wiki",
            "exact-version",
        );
        let knowledge = install(&f, &req, &ctx).await;
        assert!(knowledge.outcome.succeeded(), "{knowledge:?}");
        let store = LanceModuleStore::open(
            knowledge
                .outcome
                .destination
                .unwrap()
                .join("index.lance")
                .to_str()
                .unwrap(),
            graphit_data::knowledge_index::physical_schema(),
        )
        .await
        .unwrap();
        assert!(store.snapshot().version() > 0);
        drop(store);
        f.remove(&ctx);
        broker.shutdown().await;
    });
}
#[test]
fn actual_auth_schema_crc_and_original_failures() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        let broker = auth::Broker::start().await;
        let f = auth::Fixture::new(&ctx, true);
        let ast = f.temp.path().join("ast");
        payload(&ast, PackageKind::Ast, true, false).await;
        let zip = export(&f, &ast, "actual", PackageKind::Ast, &ctx);
        let req = request(zip.clone(), "negative/test", "v1");
        let mut wire = serde_json::to_value(&req).unwrap();
        wire.as_object_mut().unwrap().remove("kind");
        assert!(serde_json::from_value::<InstallRequest>(wire.clone()).is_ok());
        wire["kind"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<InstallRequest>(wire.clone()).is_err());
        wire["kind"] = json!("ast");
        wire["host"] = json!("kiro-v3");
        assert!(serde_json::from_value::<InstallRequest>(wire).is_err());
        f.profile(&broker, auth::now() + 600, "wrong-audience", false, &ctx);
        not_published(
            &install(&f, &req, &ctx).await,
            FailureCategory::Authentication,
        );
        f.profile(&broker, auth::now() + 600, "broker-api", false, &ctx);
        not_published(
            &native_install::execute_install(resources(&f, &ctx), "wrong-runtime-key", &req, &ctx)
                .await,
            FailureCategory::Authentication,
        );
        let mut mismatch = req.clone();
        mismatch.kind = Some(PackageKind::Knowledge);
        not_published(
            &install(&f, &mismatch, &ctx).await,
            FailureCategory::Package,
        );
        let mut bad_id = req.clone();
        bad_id.id = "../escape".into();
        not_published(
            &install(&f, &bad_id, &ctx).await,
            FailureCategory::InvalidInput,
        );
        let bad_source = f.temp.path().join("bad-source");
        payload(&bad_source, PackageKind::Ast, true, true).await;
        let req_bad = request(
            export(
                &f,
                &bad_source,
                "wrong-source-schema",
                PackageKind::Ast,
                &ctx,
            ),
            "negative/source-schema",
            "v1",
        );
        not_published(&install(&f, &req_bad, &ctx).await, FailureCategory::Lance);
        let bad_wiki = f.temp.path().join("bad-wiki");
        payload(&bad_wiki, PackageKind::Knowledge, false, true).await;
        let req_bad = request(
            export(
                &f,
                &bad_wiki,
                "wrong-wiki-schema",
                PackageKind::Knowledge,
                &ctx,
            ),
            "negative/wiki-schema",
            "v1",
        );
        not_published(&install(&f, &req_bad, &ctx).await, FailureCategory::Lance);
        fs::write(ast.join("graph.icebug/icebug.json"), b"{}").unwrap();
        let bad_graph = request(
            export(&f, &ast, "bad-graph", PackageKind::Ast, &ctx),
            "negative/graph",
            "v1",
        );
        not_published(&install(&f, &bad_graph, &ctx).await, FailureCategory::Graph);
        let mut bytes = fs::read(&zip).unwrap();
        let offset = bytes
            .windows(b"opaque native schema metadata".len())
            .position(|b| b == b"opaque native schema metadata")
            .unwrap();
        bytes[offset] ^= 1;
        let corrupt = f.temp.path().join("corrupt.zip");
        fs::write(&corrupt, bytes).unwrap();
        let corrupt_req = request(corrupt, "negative/crc", "v1");
        not_published(
            &install(&f, &corrupt_req, &ctx).await,
            FailureCategory::Package,
        );
        let stopped = Cancellation::default();
        let stopped_ctx = original(&stopped);
        stopped.cancel();
        not_published(
            &native_install::execute_install(resources(&f, &ctx), &f.key(&ctx), &req, &stopped_ctx)
                .await,
            FailureCategory::Cancelled,
        );
        let expiry = Cancellation::default();
        let expired = RequestContext::new(
            f.policy.content(),
            &expiry,
            Instant::now() + Duration::from_millis(1),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(3));
        not_published(
            &native_install::execute_install(resources(&f, &ctx), &f.key(&ctx), &req, &expired)
                .await,
            FailureCategory::Deadline,
        );
        assert_eq!(f.registry.load().unwrap().entries().count(), 0);
        f.remove(&ctx);
        let denied = auth::Fixture::new(&ctx, false);
        denied.profile(&broker, auth::now() + 600, "broker-api", false, &ctx);
        let execution = native_install::execute_install(
            resources(&denied, &ctx),
            &denied.key(&ctx),
            &req,
            &ctx,
        )
        .await;
        not_published(&execution, FailureCategory::Authentication);
        denied.remove(&ctx);
        let local = auth::Fixture::new(&ctx, true);
        local.profile(&broker, auth::now() + 600, "broker-api", true, &ctx);
        let before = broker.requests();
        not_published(
            &install(&local, &req, &ctx).await,
            FailureCategory::Authentication,
        );
        assert_eq!(broker.requests(), before);
        let captured = local.store.load(&ctx).unwrap();
        let publication = local
            .store
            .mutate(captured, &ctx, |doc, ctx| {
                let profile = doc.profile("native").unwrap();
                let old = profile.data().unwrap();
                let data = ProfileData {
                    name: old.name.clone(),
                    provider: old.provider.clone(),
                    username: "anonymous".into(),
                    organization: old.organization.clone(),
                    teams: old.teams.clone(),
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: true,
                        mcp_key: None,
                        broker_key: None,
                        embedding_api_key: None,
                        rerank_api_key: None,
                        s3: None,
                    }),
                };
                let expected = graphit_auth::profile::ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("service").unwrap().revision(),
                    profile_revision: Some(profile.revision()),
                };
                let updated = profile.updated_at() + 1;
                doc.replace_profile_data(data, expected, updated, ctx)?;
                Ok(())
            })
            .unwrap();
        assert!(publication.failure().is_none());
        not_published(
            &install(&local, &req, &ctx).await,
            FailureCategory::Authentication,
        );
        local.remove(&ctx);
        let expired_native = auth::Fixture::new(&ctx, true);
        expired_native.profile(
            &broker,
            auth::now().saturating_sub(1),
            "broker-api",
            false,
            &ctx,
        );
        not_published(
            &install(&expired_native, &req, &ctx).await,
            FailureCategory::Authentication,
        );
        expired_native.remove(&ctx);
        broker.shutdown().await;
    });
}
#[test]
fn actual_registry_failure_and_cancel_after_publication_keep_payload() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        let broker = auth::Broker::start().await;
        let f = auth::Fixture::new(&ctx, true);
        f.profile(&broker, auth::now() + 600, "broker-api", false, &ctx);
        let ast = f.temp.path().join("ast");
        payload(&ast, PackageKind::Ast, true, false).await;
        let zip = export(&f, &ast, "ast", PackageKind::Ast, &ctx);
        let first = request(zip.clone(), "partial/first", "v1");
        assert!(install(&f, &first, &ctx).await.outcome.succeeded());
        let before = f.registry.load().unwrap();
        let mut permission = fs::metadata(f.registry.path()).unwrap().permissions();
        let original_permission = permission.clone();
        permission.set_readonly(true);
        fs::set_permissions(f.registry.path(), permission).unwrap();
        let req = request(zip.clone(), "partial/registry", "v1");
        let failure = install(&f, &req, &ctx).await;
        fs::set_permissions(f.registry.path(), original_permission).unwrap();
        assert_eq!(
            failure.outcome.payload,
            PayloadEffect::Published,
            "{failure:?}"
        );
        assert_eq!(
            failure.outcome.registry,
            RegistryEffect::NotRegistered,
            "{failure:?}"
        );
        assert_eq!(
            failure.outcome.failure.unwrap().category,
            FailureCategory::Registry
        );
        assert!(
            failure
                .outcome
                .destination
                .unwrap()
                .join("graph.icebug")
                .is_dir()
        );
        assert_eq!(f.registry.load().unwrap(), before);
        let stop = Cancellation::default();
        let stopped = original(&stop);
        let req = request(zip, "partial/cancelled", "v1");
        let destination = SourceRef::hub_cache(
            &f.paths,
            req.producer,
            ArtifactType::Ast,
            ArtifactId::new(&req.id, f.policy.projection()).unwrap(),
            None,
            ConcreteVersion::new(&req.version, f.policy.projection()).unwrap(),
            f.policy.projection(),
        )
        .unwrap()
        .directory()
        .as_path()
        .to_owned();
        let writer = f
            .registry
            .writer(ctx.remaining().unwrap().unwrap(), ctx.cancellation())
            .unwrap();
        let c = stop.clone();
        let published = destination.clone();
        let monitor = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            while !published.is_dir() {
                assert!(Instant::now() < until, "publication never arrived");
                std::thread::sleep(Duration::from_millis(1));
            }
            c.cancel();
            drop(writer);
        });
        let failure = install(&f, &req, &stopped).await;
        monitor.join().unwrap();
        assert_eq!(
            failure.outcome.payload,
            PayloadEffect::Published,
            "{failure:?}"
        );
        assert_eq!(
            failure.outcome.registry,
            RegistryEffect::NotAttempted,
            "{failure:?}"
        );
        assert_eq!(
            failure.outcome.failure.as_ref().unwrap().category,
            FailureCategory::Cancelled,
            "cause: {:?}",
            failure.cause
        );
        if let Some(native_install::InstallCause::AuthenticationRequest { cause, observation }) =
            &failure.cause
        {
            assert!(matches!(
                cause,
                graphit_auth::ServiceUseError::Auth(graphit_auth::AuthError::Request)
            ));
            assert_eq!(observation.kind(), std::io::ErrorKind::Interrupted);
        }
        assert!(destination.is_dir());
        assert_eq!(f.registry.load().unwrap(), before);
        f.remove(&ctx);
        broker.shutdown().await;
    });
}

#[test]
fn actual_profile_replacement_after_publication_refuses_registration() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        let broker = auth::Broker::start().await;
        let f = auth::Fixture::new(&ctx, true);
        f.profile(&broker, auth::now() + 600, "broker-api", false, &ctx);
        let ast = f.temp.path().join("ast");
        payload(&ast, PackageKind::Ast, true, false).await;
        let req = request(
            export(&f, &ast, "ast", PackageKind::Ast, &ctx),
            "partial/revoked",
            "v1",
        );
        let destination = SourceRef::hub_cache(
            &f.paths,
            req.producer,
            ArtifactType::Ast,
            ArtifactId::new(&req.id, f.policy.projection()).unwrap(),
            None,
            ConcreteVersion::new(&req.version, f.policy.projection()).unwrap(),
            f.policy.projection(),
        )
        .unwrap()
        .directory()
        .as_path()
        .to_owned();
        let writer = f
            .registry
            .writer(ctx.remaining().unwrap().unwrap(), ctx.cancellation())
            .unwrap();
        let store = Arc::clone(&f.store);
        let published = destination.clone();
        let monitor = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            while !published.is_dir() {
                assert!(Instant::now() < until, "publication never arrived");
                std::thread::sleep(Duration::from_millis(1));
            }
            let cancel = Cancellation::default();
            let ctx = original(&cancel);
            let before = store.load(&ctx).unwrap();
            let publication = store
                .mutate(before, &ctx, |doc, ctx| {
                    let profile = doc.profile("native").unwrap();
                    let old = profile.data().unwrap();
                    let ProfileCredentials::Broker {
                        issuer,
                        subject,
                        tokens,
                        s3_disabled,
                    } = &old.credentials
                    else {
                        panic!("native Broker fixture");
                    };
                    let data = ProfileData {
                        name: old.name.clone(),
                        provider: old.provider.clone(),
                        username: "new profile observation".into(),
                        organization: old.organization.clone(),
                        teams: old.teams.clone(),
                        credentials: ProfileCredentials::Broker {
                            issuer: issuer.clone(),
                            subject: subject.clone(),
                            s3_disabled: *s3_disabled,
                            tokens: OidcTokens {
                                access_token: Secret::new(tokens.access_token.expose().into()),
                                refresh_token: tokens
                                    .refresh_token
                                    .as_ref()
                                    .map(|s| Secret::new(s.expose().into())),
                                id_token: Secret::new(tokens.id_token.expose().into()),
                                expires_at: tokens.expires_at,
                            },
                        },
                    };
                    let expected = graphit_auth::profile::ProfileExpectation {
                        document_revision: doc.revision(),
                        provider_revision: doc.provider("service").unwrap().revision(),
                        profile_revision: Some(profile.revision()),
                    };
                    let updated = profile.updated_at() + 1;
                    doc.replace_profile_data(data, expected, updated, ctx)?;
                    Ok(())
                })
                .unwrap();
            assert_eq!(
                publication.effect(),
                graphit_core::auth_document::publication::AuthPublicationEffect::Applied
            );
            assert!(publication.failure().is_none());
            drop(writer);
        });
        let execution = install(&f, &req, &ctx).await;
        monitor.join().unwrap();
        assert_eq!(
            execution.outcome.payload,
            PayloadEffect::Published,
            "{execution:?}"
        );
        assert_eq!(
            execution.outcome.registry,
            RegistryEffect::NotAttempted,
            "{execution:?}"
        );
        assert_eq!(
            execution.outcome.failure.unwrap().category,
            FailureCategory::Authentication
        );
        assert!(destination.join("graph.icebug").is_dir());
        assert_eq!(f.registry.load().unwrap().entries().count(), 0);
        f.remove(&ctx);
        broker.shutdown().await;
    });
}

#[test]
fn staged_install_refuses_changed_input_and_competing_destination() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        let f = auth::Fixture::new(&ctx, true);
        let ast = f.temp.path().join("ast");
        payload(&ast, PackageKind::Ast, true, false).await;
        let policy = f
            .policy
            .snapshot_policy(CopyPolicy::RejectFileLinks, [])
            .unwrap();
        for case in ["source", "destination"] {
            let zip = export(&f, &ast, case, PackageKind::Ast, &ctx);
            let captured = CapturedSourceReader::capture(
                &zip,
                graphit_fs::NodeKind::File,
                &policy,
                read_limits(),
                Arc::clone(ctx.snapshot()),
            )
            .unwrap();
            let destination = f.temp.path().join(format!("installed-{case}"));
            let stage = native_package::stage_import(
                &captured,
                Some(PackageKind::Ast),
                &destination,
                f.policy.copy(),
                ctx.snapshot(),
            )
            .unwrap();
            let staged_path = stage.path().to_owned();
            assert!(staged_path.join("graph.icebug/source.lance").is_dir());
            assert!(!destination.exists());
            if case == "source" {
                fs::write(&zip, b"changed captured ZIP").unwrap();
                assert!(matches!(
                    stage.publish(),
                    Err(PackageError::Io(error)) if error.kind() == io::ErrorKind::InvalidData
                ));
                assert!(!destination.exists());
            } else {
                fs::create_dir(&destination).unwrap();
                fs::write(destination.join("foreign"), b"preserved").unwrap();
                assert!(matches!(
                    stage.publish(),
                    Err(PackageError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists
                ));
                assert_eq!(fs::read(destination.join("foreign")).unwrap(), b"preserved");
                assert!(!destination.join("graph.icebug").exists());
            }
            assert!(!staged_path.exists());
        }
        assert_eq!(f.registry.load().unwrap().entries().count(), 0);
        f.remove(&ctx);
    });
}
