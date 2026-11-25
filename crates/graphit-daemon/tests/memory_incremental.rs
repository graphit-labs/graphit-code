#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../../graphit-records/tests/memory_store/fixture.rs"]
mod project;
#[allow(dead_code)]
#[path = "../../graphit-records/tests/user_memory_store/fixture.rs"]
mod user;

use graphit_auth::{
    ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation, UserMemoryWriteOperation,
    broker::BrokerRequestContext,
};
use graphit_core::file_lock::Cancellation;
use graphit_daemon::model_service::{EmbedInput, MemoryEmbeddingModel, RpcLimits};
use graphit_records::{memory_store as memory_api, memory_store::*, user_memory_store::*};
use std::{
    io,
    num::{NonZeroU32, NonZeroUsize},
    time::Duration,
};

fn identity() -> MemoryEmbeddingIdentity {
    MemoryEmbeddingIdentity::new(
        [17; 32],
        NonZeroU32::new(2).unwrap(),
        MEMORY_DOCUMENT_RECIPE.into(),
    )
    .unwrap()
}
fn options() -> MemorySemanticSearchOptions {
    MemorySemanticSearchOptions {
        filter: MemoryListFilter::default(),
        include_deleted_history: false,
        top_k: NonZeroUsize::new(8).unwrap(),
    }
}
fn hybrid_options() -> MemoryHybridSearchOptions {
    MemoryHybridSearchOptions {
        filter: MemoryListFilter::default(),
        include_deleted_history: false,
        top_k: NonZeroUsize::new(2).unwrap(),
    }
}

#[tokio::test]
async fn project_batches_are_searchable_before_completion_and_resume_without_reinference() {
    let cancel = Cancellation::default();
    let ctx = project::request(&cancel);
    let f = project::Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, reference, _run, _admin) = f.issue(&issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    let model =
        MemoryEmbeddingModel::new(ctx.snapshot(), identity(), Duration::from_secs(30)).unwrap();
    assert_eq!(
        model
            .query_hybrid(
                &read,
                "incremental".into(),
                hybrid_options(),
                project::read_limits(),
                |_| async {
                    Err::<Vec<Vec<f32>>, _>(io::Error::other("absent corpus must not infer"))
                },
            )
            .await
            .unwrap(),
        MemoryHybridSearchRead::NoCorpus
    );
    let input = |key: &str| CreateMemory {
        idempotency_key: key.into(),
        title: key.into(),
        body: "incremental document".into(),
        attributes: graphit_records::memory_attributes::MemoryAttributes {
            classification: graphit_records::memory_attributes::MemoryClass::Fact,
            mandatory: false,
            important: false,
        },
        tags: vec![],
        references: None,
    };
    let a = NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), project::write_limits())
        .await
        .unwrap();
    NativeMemoryStore::create(ctx.snapshot(), &write, input("b"), project::write_limits())
        .await
        .unwrap();
    let batch = RpcLimits::new(131072, 1, 65536, 2).unwrap();
    let mut calls = 0;
    let failed = model
        .index(
            &read,
            &write,
            project::read_limits(),
            project::write_limits(),
            batch,
            |operation| {
                calls += 1;
                let n = calls;
                let read = &read;
                let ctx = &ctx;
                let model = &model;
                async move {
                    assert_eq!(operation.texts.len(), 1);
                    if n == 2 {
                        let result = NativeMemoryStore::search_semantic(
                            ctx.snapshot(),
                            read,
                            identity(),
                            &[1., 0.],
                            options(),
                            project::read_limits(),
                        )
                        .await
                        .unwrap();
                        assert!(matches!(
                            result,
                            MemorySemanticSearchRead::Published { total_hits: 1, .. }
                        ));
                        let MemoryHybridSearchRead::Published {
                            total_hits, hits, ..
                        } = model
                            .query_hybrid(
                                read,
                                "incremental".into(),
                                hybrid_options(),
                                project::read_limits(),
                                |query| async move {
                                    assert!(matches!(query.input, EmbedInput::Query));
                                    assert_eq!(query.texts, ["incremental"]);
                                    Ok::<_, io::Error>(vec![vec![1., 0.]])
                                },
                            )
                            .await
                            .unwrap()
                        else {
                            panic!("published hybrid results during partial indexing")
                        };
                        assert_eq!(total_hits, 2);
                        assert_eq!(hits.len(), 2);
                        assert!(hits.iter().all(|hit| hit.lexical.is_some()));
                        assert_eq!(hits.iter().filter(|hit| hit.semantic.is_some()).count(), 1);
                        Err(io::Error::other("second batch interrupted"))
                    } else {
                        Ok(vec![vec![1., 0.]])
                    }
                }
            },
        )
        .await
        .unwrap_err();
    assert!(failed.last_attachment.is_some());
    assert_eq!(calls, 2);
    let mut resumed = 0;
    model
        .index(
            &read,
            &write,
            project::read_limits(),
            project::write_limits(),
            batch,
            |op| {
                resumed += op.texts.len();
                async { Ok::<_, io::Error>(vec![vec![1., 0.]]) }
            },
        )
        .await
        .unwrap();
    assert_eq!(resumed, 1);
    let mut flags = UpdateMemory {
        expected_revision: a.revision,
        idempotency_key: "metadata".into(),
        reason: "metadata only".into(),
        title: None,
        body: None,
        classification: None,
        mandatory: Some(true),
        important: None,
        tags: None,
        references: None,
    };
    let a2 = NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        a.id,
        flags.clone(),
        project::write_limits(),
    )
    .await
    .unwrap();
    model
        .index(
            &read,
            &write,
            project::read_limits(),
            project::write_limits(),
            batch,
            |_| async {
                panic!("unchanged projection must reuse its vector");
                #[allow(unreachable_code)]
                Ok::<Vec<Vec<f32>>, io::Error>(vec![])
            },
        )
        .await
        .unwrap();
    let result = NativeMemoryStore::search_semantic(
        ctx.snapshot(),
        &read,
        identity(),
        &[1., 0.],
        options(),
        project::read_limits(),
    )
    .await
    .unwrap();
    let MemorySemanticSearchRead::Published {
        hits, total_hits, ..
    } = result
    else {
        panic!("published")
    };
    assert_eq!(total_hits, 2);
    assert_eq!(
        hits.iter()
            .find(|hit| hit.current.id == a.id)
            .unwrap()
            .matched_revision
            .get(),
        a2.revision
    );
    flags.expected_revision = a2.revision;
    flags.idempotency_key = "text-change".into();
    flags.body = Some("actually changed input".into());
    NativeMemoryStore::update(ctx.snapshot(), &write, a.id, flags, project::write_limits())
        .await
        .unwrap();
    let mut changed = 0;
    model
        .index(
            &read,
            &write,
            project::read_limits(),
            project::write_limits(),
            batch,
            |op| {
                changed += op.texts.len();
                async { Ok::<_, io::Error>(vec![vec![1., 0.]]) }
            },
        )
        .await
        .unwrap();
    assert_eq!(changed, 1);
}

#[tokio::test]
async fn user_incremental_skips_covered_revisions_and_reuses_metadata_only_updates() {
    let cancel = Cancellation::default();
    let ctx = user::request(&cancel);
    let f = user::Fixture::new(&ctx).await;
    let verifier = f.server.verifier(true, f.provider_revision(&ctx));
    let raw = f
        .server
        .token(user::SUBJECT, user::RESOURCE, user::now_seconds() + 600);
    let proof = verifier
        .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let read = f
        .issuer
        .admit_incoming_user_memory_read(&proof, &verifier, &ctx, "service", f.scope())
        .unwrap();
    let write = f
        .issuer
        .admit_incoming_user_memory_write(
            &proof,
            &verifier,
            &ctx,
            "service",
            f.scope(),
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    let model =
        MemoryEmbeddingModel::new(ctx.snapshot(), identity(), Duration::from_secs(30)).unwrap();
    assert_eq!(
        model
            .query_user_hybrid(
                &read,
                "quartz".into(),
                hybrid_options(),
                user::read_limits(),
                |_| async {
                    Err::<Vec<Vec<f32>>, _>(io::Error::other("absent corpus must not infer"))
                },
            )
            .await
            .unwrap(),
        UserMemoryHybridSearchRead::NoCorpus
    );
    let a = NativeUserMemoryStore::create(
        ctx.snapshot(),
        &write,
        user::input("a"),
        user::write_limits(),
    )
    .await
    .unwrap();
    let batch = RpcLimits::new(131072, 1, 65536, 2).unwrap();
    let mut inferred = 0;
    model
        .index_user(
            &read,
            &write,
            user::read_limits(),
            user::write_limits(),
            batch,
            |op| {
                inferred += op.texts.len();
                async { Ok::<_, io::Error>(vec![vec![1., 0.]]) }
            },
        )
        .await
        .unwrap();
    assert_eq!(inferred, 1);
    let mut edit = user::update("flags", a.revision);
    edit.body = None;
    edit.important = Some(true);
    let updated =
        NativeUserMemoryStore::update(ctx.snapshot(), &write, a.id, edit, user::write_limits())
            .await
            .unwrap();
    let reuse_only = RpcLimits::new(131072, 1, 1, 1).unwrap();
    for _ in 0..2 {
        model
            .index_user(
                &read,
                &write,
                user::read_limits(),
                user::write_limits(),
                reuse_only,
                |_| async {
                    panic!("covered revision or identical input must not infer");
                    #[allow(unreachable_code)]
                    Ok::<Vec<Vec<f32>>, io::Error>(vec![])
                },
            )
            .await
            .unwrap();
    }
    let result = NativeUserMemoryStore::search_semantic(
        ctx.snapshot(),
        &read,
        identity(),
        &[1., 0.],
        options(),
        user::read_limits(),
    )
    .await
    .unwrap();
    let UserMemorySemanticSearchRead::Published {
        hits, total_hits, ..
    } = result
    else {
        panic!("published")
    };
    assert_eq!(total_hits, 1);
    assert_eq!(hits[0].matched_revision.get(), updated.revision);
    let UserMemoryHybridSearchRead::Published {
        total_hits, hits, ..
    } = model
        .query_user_hybrid(
            &read,
            "quartz".into(),
            hybrid_options(),
            user::read_limits(),
            |query| async move {
                assert!(matches!(query.input, EmbedInput::Query));
                assert_eq!(query.texts, ["quartz"]);
                Ok::<_, io::Error>(vec![vec![1., 0.]])
            },
        )
        .await
        .unwrap()
    else {
        panic!("published USER hybrid result")
    };
    assert_eq!(total_hits, 1);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].current, updated);
    assert!(hits[0].lexical.is_some() && hits[0].semantic.is_some());
}

#[tokio::test]
async fn project_query_keeps_its_snapshot_when_source_changes_during_inference() {
    let cancel = Cancellation::default();
    let ctx = project::request(&cancel);
    let f = project::Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, reference, _run, _admin) = f.issue(&issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    let memory = NativeMemoryStore::create(
        ctx.snapshot(),
        &write,
        CreateMemory {
            idempotency_key: "captured".into(),
            title: "captured".into(),
            body: "original".into(),
            attributes: graphit_records::memory_attributes::MemoryAttributes {
                classification: graphit_records::memory_attributes::MemoryClass::Fact,
                mandatory: false,
                important: false,
            },
            tags: vec![],
            references: None,
        },
        project::write_limits(),
    )
    .await
    .unwrap();
    let model =
        MemoryEmbeddingModel::new(ctx.snapshot(), identity(), Duration::from_secs(30)).unwrap();
    model
        .index(
            &read,
            &write,
            project::read_limits(),
            project::write_limits(),
            RpcLimits::new(131072, 1, 65536, 2).unwrap(),
            |_| async { Ok::<_, io::Error>(vec![vec![1., 0.]]) },
        )
        .await
        .unwrap();
    let result = model
        .query(
            &read,
            "query".into(),
            options(),
            project::read_limits(),
            |_| {
                let write = &write;
                let ctx = &ctx;
                async move {
                    NativeMemoryStore::update(
                        ctx.snapshot(),
                        write,
                        memory.id,
                        UpdateMemory {
                            expected_revision: 1,
                            idempotency_key: "during-inference".into(),
                            reason: "concurrent edit".into(),
                            title: None,
                            body: Some("changed".into()),
                            classification: None,
                            mandatory: None,
                            important: None,
                            tags: None,
                            references: None,
                        },
                        project::write_limits(),
                    )
                    .await
                    .unwrap();
                    Ok::<_, io::Error>(vec![vec![1., 0.]])
                }
            },
        )
        .await
        .unwrap();
    let MemorySemanticSearchRead::Published {
        hits, total_hits, ..
    } = result
    else {
        panic!("published")
    };
    assert_eq!(total_hits, 1);
    assert_eq!(hits[0].current.revision, 1);
    assert_eq!(hits[0].current.body, "original");
    assert_eq!(hits[0].location, MemoryMatchLocation::Current);
    let MemorySemanticSearchRead::Published { hits, .. } = NativeMemoryStore::search_semantic(
        ctx.snapshot(),
        &read,
        identity(),
        &[1., 0.],
        options(),
        project::read_limits(),
    )
    .await
    .unwrap() else {
        panic!("published")
    };
    assert_eq!(hits[0].current.revision, 2);
    assert!(matches!(
        hits[0].location,
        MemoryMatchLocation::Archived { .. }
    ));
}

#[tokio::test]
async fn user_query_keeps_its_snapshot_when_source_changes_during_inference() {
    let cancel = Cancellation::default();
    let ctx = user::request(&cancel);
    let f = user::Fixture::new(&ctx).await;
    let verifier = f.server.verifier(true, f.provider_revision(&ctx));
    let raw = f
        .server
        .token(user::SUBJECT, user::RESOURCE, user::now_seconds() + 600);
    let proof = verifier
        .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let read = f
        .issuer
        .admit_incoming_user_memory_read(&proof, &verifier, &ctx, "service", f.scope())
        .unwrap();
    let write = f
        .issuer
        .admit_incoming_user_memory_write(
            &proof,
            &verifier,
            &ctx,
            "service",
            f.scope(),
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    let memory = NativeUserMemoryStore::create(
        ctx.snapshot(),
        &write,
        user::input("captured"),
        user::write_limits(),
    )
    .await
    .unwrap();
    let original_body = memory.body.clone();
    let model =
        MemoryEmbeddingModel::new(ctx.snapshot(), identity(), Duration::from_secs(30)).unwrap();
    model
        .index_user(
            &read,
            &write,
            user::read_limits(),
            user::write_limits(),
            RpcLimits::new(131072, 1, 65536, 2).unwrap(),
            |_| async { Ok::<_, io::Error>(vec![vec![1., 0.]]) },
        )
        .await
        .unwrap();
    let result = model
        .query_user(
            &read,
            "query".into(),
            options(),
            user::read_limits(),
            |_| {
                let write = &write;
                let ctx = &ctx;
                async move {
                    NativeUserMemoryStore::update(
                        ctx.snapshot(),
                        write,
                        memory.id,
                        UpdateMemory {
                            expected_revision: 1,
                            idempotency_key: "during-inference".into(),
                            reason: "concurrent edit".into(),
                            title: None,
                            body: Some("changed".into()),
                            classification: None,
                            mandatory: None,
                            important: None,
                            tags: None,
                            references: None,
                        },
                        user::write_limits(),
                    )
                    .await
                    .unwrap();
                    Ok::<_, io::Error>(vec![vec![1., 0.]])
                }
            },
        )
        .await
        .unwrap();
    let UserMemorySemanticSearchRead::Published {
        hits, total_hits, ..
    } = result
    else {
        panic!("published")
    };
    assert_eq!(total_hits, 1);
    assert_eq!(hits[0].current.revision, 1);
    assert_eq!(hits[0].current.body, original_body);
    assert_eq!(hits[0].location, MemoryMatchLocation::Current);
    let UserMemorySemanticSearchRead::Published { hits, .. } =
        NativeUserMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(),
            &[1., 0.],
            options(),
            user::read_limits(),
        )
        .await
        .unwrap()
    else {
        panic!("published")
    };
    assert_eq!(hits[0].current.revision, 2);
}
