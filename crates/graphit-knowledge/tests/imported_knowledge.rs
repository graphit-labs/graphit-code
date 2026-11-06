//! Public imported reads with real signed Web authority, Hub registry and Lance datasets.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]
#[path = "support/imported_knowledge_fixture.rs"]
mod fixture;
use fixture::*;
use graphit_auth::{AuthError, broker::BrokerRequestContext};
use graphit_core::file_lock::Cancellation;
use graphit_data::{
    knowledge_index::*, knowledge_references::*, knowledge_snapshot::KnowledgeSnapshotPage,
    lance_module_store::LanceModuleStore,
};
use graphit_hub::{ArtifactId, ConcreteVersion};
use graphit_knowledge::imported_knowledge::*;
use std::{
    error::Error,
    time::{Duration, Instant},
};
fn limits() -> ImportedKnowledgeReadLimits {
    ImportedKnowledgeReadLimits {
        directory: native(),
        index: page_limits(),
    }
}
fn input<'a>(f: &'a Fixture, alias: Option<&'a str>) -> ImportedKnowledgeInput<'a> {
    ImportedKnowledgeInput {
        registry: &f.registry,
        id: &f.id,
        version: &f.artifact_version,
        context_alias: alias,
    }
}

#[test]
fn registry_spelling_preserves_cache_metadata_and_rejects_foreign_brand_and_root() {
    native_test(async {
        use graphit_core::{
            artifact_store::{PublicationSlot, StoreOwner},
            brand::Brand,
            project_paths::InstallationPaths,
        };
        use graphit_hub::{
            ArtifactType, InstallationTarget, RemoteFileOrigin, SourceRef,
            registry::{
                HubRegistry, Installation, InstalledArtifact, InstalledBackend, InstalledPayload,
            },
        };
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let mut f = Fixture::new(&ctx);
        let StoreOwner::Installation { installation_id } = f.installation.owner() else {
            panic!()
        };
        let installation_id = installation_id.clone();
        // Keep an explicit spelling component in the registry's parent without
        // relying on TMP or introducing parent traversal, which NativePath rejects.
        let root = f.paths.root();
        let spelling = root
            .parent()
            .unwrap()
            .join(".")
            .join(root.file_name().unwrap());
        f.paths = InstallationPaths::resolve(
            f.paths.brand().clone(),
            f.installation.owner_root(),
            None,
            Some(&spelling),
        )
        .unwrap();
        f.registry = HubRegistry::global(&f.paths, &installation_id, f.registry.limits()).unwrap();
        f.source = SourceRef::hub_cache(
            &f.paths,
            f.source.producer(),
            ArtifactType::Knowledge,
            f.id.clone(),
            None,
            f.artifact_version.clone(),
            f.registry.limits().projection(),
        )
        .unwrap();
        let issuer = f.issuer(&ctx);
        f.seed(&issuer, &ctx, &[page("captured", &[])], None).await;
        let secret =
            std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
        let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
        let read = issuer
            .admit_authenticated_imported_knowledge_read(&caller, f.realm())
            .unwrap();
        let projection = f.registry.limits().projection();
        let cache_id = ArtifactId::new("vendor/cached-skill", projection).unwrap();
        let cache_source = SourceRef::hub_cache(
            &f.paths,
            f.source.producer(),
            ArtifactType::Skill,
            cache_id.clone(),
            None,
            ConcreteVersion::new("cache-v1", projection).unwrap(),
            projection,
        )
        .unwrap()
        .with_remote_file_origin(
            RemoteFileOrigin::Broker {
                provider: "service".into(),
            },
            [7; 32],
            projection,
        )
        .unwrap();
        let cache = InstalledArtifact {
            consumer: PublicationSlot::Scope,
            artifact_type: ArtifactType::Skill,
            id: cache_id,
            installations: vec![Installation {
                target: InstallationTarget::GlobalCache,
                original_source: cache_source,
                display: None,
                alias: None,
                entry_name: None,
                backend: InstalledBackend::GlobalCache,
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        };
        f.registry
            .writer(ctx.remaining().unwrap().unwrap(), ctx.cancellation())
            .unwrap()
            .put_metadata(cache.clone())
            .unwrap();
        let before = std::fs::read(f.registry.path()).unwrap();
        let configured = f.registry.path();
        let source_record = f.source.clone();
        assert_ne!(
            configured.parent().unwrap().as_os_str(),
            f.installation.owner_root().as_os_str()
        );
        assert_eq!(
            graphit_core::local_files::canonical_directory(configured.parent().unwrap()).unwrap(),
            f.installation.owner_root()
        );
        let ImportedKnowledgeOpen::Published(reader) =
            open_imported_knowledge(&read, input(&f, None), limits())
                .await
                .unwrap()
        else {
            panic!("configured spelling must bind to the admitted owner")
        };
        assert!(matches!(reader.heads(heads_limits()).await.unwrap(),
            KnowledgeHeadRead::Published { heads, .. } if heads.len() == 1 && heads[0].slug == "captured"));
        assert_eq!(f.registry.path(), configured);
        assert_eq!(f.source, source_record);
        assert_eq!(std::fs::read(f.registry.path()).unwrap(), before);
        assert_eq!(
            f.registry.load().unwrap().get_version(
                ArtifactType::Skill,
                cache.id.as_str(),
                cache.installations[0].original_source.version()
            ),
            Some(&cache)
        );

        let wrong_brand = InstallationPaths::resolve(
            Brand::new("another-brand").unwrap(),
            f.installation.owner_root(),
            None,
            Some(f.installation.owner_root()),
        )
        .unwrap();
        let wrong_brand =
            HubRegistry::global(&wrong_brand, &installation_id, f.registry.limits()).unwrap();
        assert!(matches!(
            open_imported_knowledge(
                &read,
                ImportedKnowledgeInput {
                    registry: &wrong_brand,
                    ..input(&f, None)
                },
                limits()
            )
            .await,
            Err(ImportedKnowledgeReadError::InvalidRegistryScope)
        ));
        let foreign = f._dir.path().join("foreign-installation");
        std::fs::create_dir(&foreign).unwrap();
        let foreign =
            InstallationPaths::resolve(f.paths.brand().clone(), &foreign, None, Some(&foreign))
                .unwrap();
        let foreign = HubRegistry::global(&foreign, &installation_id, f.registry.limits()).unwrap();
        assert!(matches!(
            open_imported_knowledge(
                &read,
                ImportedKnowledgeInput {
                    registry: &foreign,
                    ..input(&f, None)
                },
                limits()
            )
            .await,
            Err(ImportedKnowledgeReadError::InvalidRegistryScope)
        ));
        read.recheck().unwrap();
        assert_eq!(std::fs::read(f.registry.path()).unwrap(), before);
    });
}
#[test]
fn signed_web_native_source_preserves_observation_state_and_same_pin() {
    native_test(async {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let revision = f.provider(&server, &ctx);
        let issuer = f.issuer(&ctx);
        let pages = [
            page("active", &["ghost", "active", "ghost"]),
            page("removed", &[]),
        ];
        let refs = [KnowledgeAuthoredReference {
            target_kind: KnowledgeReferenceTargetKind::Memory,
            scope: KnowledgeReferenceScope::User(" foreign user "),
            id: " é ",
            context: None,
            relation: " supports ",
            field: Some(""),
        }; 2];
        let fields = [
            fields("active", KnowledgeDocumentState::Active, &refs),
            fields("removed", KnowledgeDocumentState::Removed, &[]),
        ];
        f.seed(&issuer, &ctx, &pages, Some(&fields)).await;
        let verifier = server.verifier(revision);
        let flow = web(&verifier);
        let registered = flow
            .register(BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let raw = server.access(registered.resource());
        let proof = flow
            .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let read = issuer
            .admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm())
            .unwrap();
        let keys = server.count("/keys");
        let before = image(f.installation.owner_root());
        let ImportedKnowledgeOpen::Published(reader) =
            open_imported_knowledge(&read, input(&f, Some(" alias literal ")), limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        assert_eq!(reader.observation().version.as_str(), "exact-version");
        assert_eq!(reader.observation().context_alias, Some(" alias literal "));
        assert!(reader.observation().generation.version > 0);
        assert!(reader.observation().generation.physical_branch.is_none());
        let KnowledgeHeadRead::Published { generation, heads } =
            reader.heads(heads_limits()).await.unwrap()
        else {
            panic!()
        };
        assert_eq!(&generation, &reader.observation().generation);
        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].slug, "active");
        let KnowledgeSnapshotPage::Found { document, .. } =
            reader.page("active", page_limits()).await.unwrap()
        else {
            panic!()
        };
        assert_eq!(document.body, "# corpo integral é\n");
        assert_eq!(document.title, " título ");
        assert_eq!(document.resolved_wiki_targets, ["ghost", "active", "ghost"]);
        assert_eq!(document.description.as_deref(), Some("descrição completa"));
        assert_eq!(
            reader.page("removed", page_limits()).await.unwrap(),
            KnowledgeSnapshotPage::PageAbsent
        );
        assert_eq!(
            image(f.installation.owner_root()),
            before,
            "reads never birth/rewrite/reconcile"
        );
        assert_eq!(server.count("/keys"), keys);
        assert_eq!(server.count("/userinfo"), 0);
        assert_eq!(server.count("/token"), 0);
        let uri = reader.observation().generation.corpus_uri.clone();
        let store = LanceModuleStore::open(&uri, physical_schema())
            .await
            .unwrap();
        let new = store
            .snapshot()
            .prepare_replace(vec![])
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();
        assert!(new.snapshot().version() > generation.version);
        let KnowledgeSnapshotPage::Found { document, .. } =
            reader.page("active", page_limits()).await.unwrap()
        else {
            panic!()
        };
        assert_eq!(document.body, "# corpo integral é\n");
        assert_eq!(reader.observation().generation, generation);
    });
}
#[test]
fn registry_absence_is_distinct_from_required_child_errors_and_empty_publication() {
    native_test(async {
        for state in [
            "unregistered",
            "missing-child",
            "empty",
            "removed",
            "ordinary",
        ] {
            let server = Server::new().await;
            let cancel = Cancellation::default();
            let ctx = context(&cancel);
            let f = Fixture::new(&ctx);
            let revision = f.provider(&server, &ctx);
            let issuer = f.issuer(&ctx);
            match state {
                "missing-child" => {
                    std::fs::create_dir_all(f.source.directory().as_path()).unwrap();
                    f.register(&ctx)
                }
                "empty" => f.seed(&issuer, &ctx, &[], None).await,
                "removed" => {
                    let pages = [page("removed", &[])];
                    let entries = [fields("removed", KnowledgeDocumentState::Removed, &[])];
                    f.seed(&issuer, &ctx, &pages, Some(&entries)).await
                }
                "ordinary" => f.seed(&issuer, &ctx, &[page("active", &[])], None).await,
                _ => (),
            }
            let verifier = server.verifier(revision);
            let flow = web(&verifier);
            let registered = flow
                .register(BrokerRequestContext::new(ctx.snapshot()))
                .await
                .unwrap();
            let raw = server.access(registered.resource());
            let proof = flow
                .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
                .await
                .unwrap();
            let read = issuer
                .admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm())
                .unwrap();
            let before = image(f.installation.owner_root());
            let opened = open_imported_knowledge(&read, input(&f, None), limits()).await;
            match state {
                "unregistered" => {
                    assert!(matches!(opened, Ok(ImportedKnowledgeOpen::RegistryAbsent)))
                }
                "missing-child" => {
                    let error = opened.err().unwrap();
                    assert!(
                        matches!(&error,ImportedKnowledgeReadError::Capture(e) if e.kind()==std::io::ErrorKind::NotFound)
                    );
                    assert!(error.source().is_some())
                }
                _ => {
                    let ImportedKnowledgeOpen::Published(reader) = opened.unwrap() else {
                        panic!()
                    };
                    let KnowledgeHeadRead::Published { heads, .. } =
                        reader.heads(heads_limits()).await.unwrap()
                    else {
                        panic!()
                    };
                    let page = reader.page("active", page_limits()).await.unwrap();
                    match state {
                        "empty" => {
                            assert!(heads.is_empty());
                            assert_eq!(page, KnowledgeSnapshotPage::PublishedEmpty)
                        }
                        "removed" => {
                            assert!(heads.is_empty());
                            assert_eq!(page, KnowledgeSnapshotPage::PageAbsent)
                        }
                        _ => {
                            assert_eq!(heads.len(), 1);
                            assert!(matches!(page, KnowledgeSnapshotPage::Found { .. }))
                        }
                    }
                }
            }
            assert_eq!(image(f.installation.owner_root()), before);
        }
    });
}
#[test]
fn registry_change_after_open_and_during_scan_never_returns_a_healthy_prefix() {
    native_test(async {
        for in_flight in [false, true] {
            let server = Server::new().await;
            let cancel = Cancellation::default();
            let ctx = context(&cancel);
            let f = Fixture::new(&ctx);
            let revision = f.provider(&server, &ctx);
            let issuer = f.issuer(&ctx);
            f.seed(
                &issuer,
                &ctx,
                &[page("picked", &[]), page("other", &[])],
                None,
            )
            .await;
            let verifier = server.verifier(revision);
            let flow = web(&verifier);
            let registered = flow
                .register(BrokerRequestContext::new(ctx.snapshot()))
                .await
                .unwrap();
            let raw = server.access(registered.resource());
            let proof = flow
                .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
                .await
                .unwrap();
            let read = issuer
                .admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm())
                .unwrap();
            let ImportedKnowledgeOpen::Published(reader) =
                open_imported_knowledge(&read, input(&f, None), limits())
                    .await
                    .unwrap()
            else {
                panic!()
            };
            let result = if in_flight {
                let (result, ()) = tokio::join!(reader.page("picked", page_limits()), async {
                    f.remove_registration(&ctx)
                });
                result
            } else {
                f.remove_registration(&ctx);
                reader.page("picked", page_limits()).await
            };
            assert!(matches!(
                result,
                Err(ImportedKnowledgeReadError::SourceSelectionChanged)
            ));
            assert!(matches!(
                reader.heads(heads_limits()).await,
                Err(ImportedKnowledgeReadError::SourceSelectionChanged)
            ));
        }
    });
}
#[test]
fn current_auth_provider_replacement_denies_held_reader_without_network() {
    native_test(async {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let revision = f.provider(&server, &ctx);
        let issuer = f.issuer(&ctx);
        f.seed(&issuer, &ctx, &[page("picked", &[])], None).await;
        let verifier = server.verifier(revision);
        let flow = web(&verifier);
        let registered = flow
            .register(BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let raw = server.access(registered.resource());
        let proof = flow
            .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let read = issuer
            .admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm())
            .unwrap();
        let ImportedKnowledgeOpen::Published(reader) =
            open_imported_knowledge(&read, input(&f, None), limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        let keys = server.count("/keys");
        let result = f
            .store
            .mutate(f.store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                doc.update_provider(
                    "service",
                    broker_config(&server.origin),
                    doc.revision(),
                    revision,
                    2,
                    ctx,
                )
            })
            .unwrap();
        assert!(result.failure().is_none());
        assert!(matches!(
            reader.page("picked", page_limits()).await,
            Err(ImportedKnowledgeReadError::Auth(AuthError::Denied))
        ));
        assert!(matches!(
            reader.heads(heads_limits()).await,
            Err(ImportedKnowledgeReadError::Auth(AuthError::Denied))
        ));
        assert_eq!(reader.observation().id, &f.id);
        assert_eq!(server.count("/keys"), keys);
        assert_eq!(server.count("/userinfo"), 0);
        assert_eq!(server.count("/token"), 0);
    });
}
#[test]
fn exact_selection_alias_and_content_limits_reject_before_partial_delivery() {
    native_test(async {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let revision = f.provider(&server, &ctx);
        let issuer = f.issuer(&ctx);
        f.seed(
            &issuer,
            &ctx,
            &[page("picked", &["one", "two"]), page("late", &[])],
            None,
        )
        .await;
        let verifier = server.verifier(revision);
        let flow = web(&verifier);
        let registered = flow
            .register(BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let raw = server.access(registered.resource());
        let proof = flow
            .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let read = issuer
            .admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm())
            .unwrap();
        let absent = ArtifactId::new("vendor/other", f.registry_limits_projection()).unwrap();
        assert!(matches!(
            open_imported_knowledge(
                &read,
                ImportedKnowledgeInput {
                    id: &absent,
                    ..input(&f, None)
                },
                limits()
            )
            .await,
            Ok(ImportedKnowledgeOpen::RegistryAbsent)
        ));
        let other_version = ConcreteVersion::new("1", f.registry_limits_projection()).unwrap();
        assert!(matches!(
            open_imported_knowledge(
                &read,
                ImportedKnowledgeInput {
                    version: &other_version,
                    ..input(&f, None)
                },
                limits()
            )
            .await,
            Ok(ImportedKnowledgeOpen::RegistryAbsent)
        ));
        assert!(matches!(
            open_imported_knowledge(&read, input(&f, Some("")), limits()).await,
            Err(ImportedKnowledgeReadError::InvalidLocator)
        ));
        let alias = "é".repeat(129);
        assert!(matches!(
            open_imported_knowledge(&read, input(&f, Some(&alias)), limits()).await,
            Err(ImportedKnowledgeReadError::InvalidLocator)
        ));
        let ImportedKnowledgeOpen::Published(reader) =
            open_imported_knowledge(&read, input(&f, None), limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        let graphit_data::knowledge_index::KnowledgeHeadRead::Published { heads, .. } =
            reader.heads(heads_limits()).await.unwrap()
        else {
            panic!("published corpus heads");
        };
        assert!(!heads.is_empty());
        let mut page_limit = page_limits();
        page_limit.content.max_identifier_bytes = n(2);
        assert!(matches!(
            reader.page("picked", page_limit).await,
            Err(ImportedKnowledgeReadError::Data(
                KnowledgeIndexError::LimitExceeded("identifier_bytes")
            ))
        ));
        assert!(matches!(
            reader.page("../picked", page_limits()).await,
            Err(ImportedKnowledgeReadError::Data(
                KnowledgeIndexError::InvalidField("slug")
            ))
        ));
    });
}
#[test]
fn cancelled_original_and_expired_original_remain_auth_causes() {
    native_test(async {
        for (expire, in_flight) in [(false, false), (true, false), (false, true)] {
            let server = Server::new().await;
            let cancel = Cancellation::default();
            let base = context(&cancel);
            let f = Fixture::new(&base);
            let revision = f.provider(&server, &base);
            let issuer = f.issuer(&base);
            f.seed(&issuer, &base, &[page("picked", &[])], None).await;
            let deadline = if expire {
                Instant::now() + Duration::from_secs(2)
            } else {
                base.deadline().expect("finite fixture deadline")
            };
            let ctx = context_deadline(&cancel, deadline);
            let verifier = server.verifier(revision);
            let flow = web(&verifier);
            let registered = flow
                .register(BrokerRequestContext::new(ctx.snapshot()))
                .await
                .unwrap();
            let raw = server.access(registered.resource());
            let proof = flow
                .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
                .await
                .unwrap();
            let read = issuer
                .admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm())
                .unwrap();
            let ImportedKnowledgeOpen::Published(reader) =
                open_imported_knowledge(&read, input(&f, None), limits())
                    .await
                    .unwrap()
            else {
                panic!()
            };
            let result = if in_flight {
                let (result, ()) = tokio::join!(reader.page("picked", page_limits()), async {
                    cancel.cancel()
                });
                result
            } else {
                if expire {
                    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await
                } else {
                    cancel.cancel()
                }
                reader.page("picked", page_limits()).await
            };
            let error = result.expect_err("cancelled read must fail as a whole");
            if in_flight {
                // Data checks the same Original before invoking the external Auth
                // guard; whichever check observes cancellation first retains its cause.
                assert!(
                    matches!(
                        &error,
                        ImportedKnowledgeReadError::Auth(AuthError::Request)
                            | ImportedKnowledgeReadError::Data(KnowledgeIndexError::Request(_))
                    ),
                    "{error:?}"
                );
                if let ImportedKnowledgeReadError::Data(KnowledgeIndexError::Request(cause)) =
                    &error
                {
                    assert_eq!(cause.kind(), std::io::ErrorKind::Interrupted);
                }
                eprintln!("in-flight cancellation retained cause: {error:?}");
            } else {
                assert!(matches!(
                    error,
                    ImportedKnowledgeReadError::Auth(AuthError::Request)
                ));
            }
            assert!(matches!(
                reader.heads(heads_limits()).await,
                Err(ImportedKnowledgeReadError::Auth(AuthError::Request))
            ));
        }
    });
}

#[test]
fn foreign_registry_symlink_and_wrong_schema_are_causal_errors() {
    native_test(async {
        for fault in [
            "registry",
            #[cfg(unix)]
            "symlink",
            "schema",
            "capture",
        ] {
            let server = Server::new().await;
            let cancel = Cancellation::default();
            let ctx = context(&cancel);
            let f = Fixture::new(&ctx);
            let revision = f.provider(&server, &ctx);
            let issuer = f.issuer(&ctx);
            f.seed(&issuer, &ctx, &[page("picked", &[])], None).await;
            let verifier = server.verifier(revision);
            let flow = web(&verifier);
            let registered = flow
                .register(BrokerRequestContext::new(ctx.snapshot()))
                .await
                .unwrap();
            let raw = server.access(registered.resource());
            let proof = flow
                .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
                .await
                .unwrap();
            let read = issuer
                .admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm())
                .unwrap();
            if fault == "registry" {
                let foreign = f.foreign_registry();
                assert!(matches!(
                    open_imported_knowledge(
                        &read,
                        ImportedKnowledgeInput {
                            registry: &foreign,
                            ..input(&f, None)
                        },
                        limits()
                    )
                    .await,
                    Err(ImportedKnowledgeReadError::InvalidRegistryScope)
                ));
                continue;
            }
            let path = f.source.directory().as_path().join("index.lance");
            #[cfg(unix)]
            if fault == "symlink" {
                let moved = path.with_file_name("held-index.lance");
                std::fs::rename(&path, &moved).unwrap();
                std::os::unix::fs::symlink(&moved, &path).unwrap();
                assert!(matches!(
                    open_imported_knowledge(&read, input(&f, None), limits()).await,
                    Err(ImportedKnowledgeReadError::InvalidLocator)
                ));
                continue;
            }
            if fault == "capture" {
                let ImportedKnowledgeOpen::Published(reader) =
                    open_imported_knowledge(&read, input(&f, None), limits())
                        .await
                        .unwrap()
                else {
                    panic!()
                };
                let moved = path.with_file_name("replaced-index.lance");
                std::fs::rename(&path, &moved).unwrap();
                std::fs::create_dir(&path).unwrap();
                let error = reader.page("picked", page_limits()).await.err().unwrap();
                assert!(matches!(error, ImportedKnowledgeReadError::Capture(_)));
                assert!(error.source().is_some());
                assert!(matches!(
                    reader.heads(heads_limits()).await,
                    Err(ImportedKnowledgeReadError::Capture(_))
                ));
            } else {
                std::fs::remove_dir_all(&path).unwrap();
                let uri = path.to_str().unwrap().to_owned();
                let wrong = std::sync::Arc::new(physical_schema().as_ref().clone().with_metadata(
                    std::collections::HashMap::from([("foreign".into(), "schema".into())]),
                ));
                LanceModuleStore::create(&uri, wrong, vec![]).await.unwrap();
                let error = open_imported_knowledge(&read, input(&f, None), limits())
                    .await
                    .err()
                    .unwrap();
                assert!(
                    matches!(
                        &error,
                        ImportedKnowledgeReadError::Data(KnowledgeIndexError::Request(cause))
                            if matches!(
                                cause.get_ref().and_then(|source| source.downcast_ref::<
                                    graphit_data::knowledge_embeddings::KnowledgeEmbeddingError
                                >()),
                                Some(graphit_data::knowledge_embeddings::KnowledgeEmbeddingError::Index(
                                    KnowledgeIndexError::InvalidField("schema")
                                ))
                            )
                    ),
                    "{error:?}"
                );
                assert!(error.source().is_some());
            }
        }
    });
}

#[path = "imported_reference_query/sdk.rs"]
mod search_sdk;
#[test]
fn lexical_search_keeps_payload_namespace_and_imported_selection_bound_to_its_pin() {
    native_test(async {
        use graphit_data::knowledge_reference_query::KnowledgeReferenceOwnedScope;
        use graphit_data::knowledge_search::*;
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let revision = f.provider(&server, &ctx);
        let issuer = f.issuer(&ctx);
        let pages = [page("a", &[]), page("b", &[]), page("removed", &[])];
        let entries = [
            fields("a", KnowledgeDocumentState::Active, &[]),
            fields("b", KnowledgeDocumentState::Active, &[]),
            fields("removed", KnowledgeDocumentState::Removed, &[]),
        ];
        f.seed(&issuer, &ctx, &pages, Some(&entries)).await;
        let other = "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap();
        let rows = search_sdk::rows(&f).await;
        assert_eq!(search_sdk::row(&rows, "a").num_rows(), 1);
        let patched = rows
            .iter()
            .map(|row| {
                let slug = row
                    .column(0)
                    .as_any()
                    .downcast_ref::<arrow_array::StringArray>()
                    .unwrap()
                    .value(0);
                if slug == "removed" {
                    row.clone()
                } else {
                    search_sdk::namespace(
                        row,
                        KnowledgeReferenceScope::Project(other),
                        Some("payload source β"),
                        &[],
                        &[],
                    )
                }
            })
            .collect();
        search_sdk::replace(&f, patched).await;
        let verifier = server.verifier(revision);
        let flow = web(&verifier);
        let registered = flow
            .register(BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let raw = server.access(registered.resource());
        let proof = flow
            .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let read = issuer
            .admit_web_imported_knowledge_read(&proof, &verifier, &ctx, "service", f.realm())
            .unwrap();
        let ImportedKnowledgeOpen::Published(reader) =
            open_imported_knowledge(&read, input(&f, Some("mount α")), limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        let before = image(f.source.directory().as_path());
        let query = KnowledgeLexicalQuery {
            text: "corpo",
            top_k: 0,
            page_size: n(1),
            cursor: None,
            caller_binding: "actual HTTP source selection",
            preview: true,
        };
        let search_limits = KnowledgeSearchLimits {
            read: page_limits(),
            max_query_bytes: n(4096),
            max_output_bytes: n(65536),
        };
        let preview_limits = graphit_knowledge::preview::MarkdownPreviewLimits {
            max_input_bytes: n(65536),
            max_output_bytes: n(4096),
            max_body_scalars: n(8),
        };
        let first = reader
            .search(query, search_limits, preview_limits)
            .await
            .unwrap();
        assert_eq!(first.total, 2);
        assert_eq!(first.hits.len(), 1);
        assert_eq!(first.generation, reader.observation().generation);
        assert_eq!(
            first.hits[0].source.namespace.as_ref().unwrap().scope,
            KnowledgeReferenceOwnedScope::Project(other)
        );
        assert_eq!(
            first.hits[0]
                .source
                .namespace
                .as_ref()
                .unwrap()
                .context
                .as_deref(),
            Some("payload source β")
        );
        assert!(first.hits[0].source.witness.is_some());
        let preview = first.hits[0].preview.as_ref().unwrap();
        let graphit_data::knowledge_snapshot::KnowledgeSnapshotPage::Found { document, .. } =
            reader
                .page(&first.hits[0].source.slug, page_limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        let selected = match preview.source {
            graphit_data::knowledge_search::KnowledgeSearchPreviewSource::Body => {
                document.body.as_str()
            }
            graphit_data::knowledge_search::KnowledgeSearchPreviewSource::Summary => {
                document.summary.as_str()
            }
        };
        let literal = if preview.truncated {
            preview.text.strip_suffix('…').unwrap()
        } else {
            preview.text.as_str()
        };
        assert_eq!(literal, &selected[preview.range.clone()]);
        if matches!(
            preview.source,
            graphit_data::knowledge_search::KnowledgeSearchPreviewSource::Body
        ) {
            assert!(preview.truncated);
        }

        assert_ne!(reader.observation().producer, other);
        assert_eq!(reader.observation().context_alias, Some("mount α"));
        let cursor = first.next_cursor.as_ref().unwrap();
        let ImportedKnowledgeOpen::Published(other_mount) =
            open_imported_knowledge(&read, input(&f, Some("mount changed")), limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        assert!(matches!(
            other_mount
                .search(
                    KnowledgeLexicalQuery {
                        cursor: Some(cursor),
                        ..query
                    },
                    search_limits,
                    preview_limits
                )
                .await,
            Err(ImportedKnowledgeSearchError::Query(
                KnowledgeSearchError::InvalidCursor
            ))
        ));
        let second = reader
            .search(
                KnowledgeLexicalQuery {
                    cursor: Some(cursor),
                    ..query
                },
                search_limits,
                preview_limits,
            )
            .await
            .unwrap();
        assert_eq!(second.hits.len(), 1);
        assert!(second.next_cursor.is_none());
        assert_ne!(first.hits[0].source.slug, second.hits[0].source.slug);
        assert_eq!(image(f.source.directory().as_path()), before);
        assert_eq!(server.count("/userinfo"), 0);
        assert_eq!(server.count("/token"), 0);
        cancel.cancel();
        assert!(matches!(
            reader.search(query, search_limits, preview_limits).await,
            Err(ImportedKnowledgeSearchError::Reader(_))
        ));
    });
}
