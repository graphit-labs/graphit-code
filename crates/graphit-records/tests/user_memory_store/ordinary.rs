use super::*;
use graphit_auth::{
    UserMemoryWriteOperation,
    broker::{
        BrokerRequestContext,
        oauth::{OAuthLimits, web::WebOAuth},
    },
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits, UserIdentity},
    file_lock::Cancellation,
    installation_identity::InstallationId,
    project_paths::InstallationPaths,
};
use std::{
    num::NonZeroUsize,
    time::{Duration, Instant},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_create_and_reopen_use_actual_user_schema_default_pin_and_original_attempt() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    assert_eq!(
        NativeUserMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap(),
        UserMemoryHeadRead::NoCorpus
    );
    assert_eq!(
        NativeUserMemoryStore::reobserve_create(
            ctx.snapshot(),
            &write,
            input("same '\nβ"),
            write_limits()
        )
        .await
        .unwrap(),
        MemoryCreateObservation::Unresolved
    );
    assert_eq!(
        NativeUserMemoryStore::maintain_lexical_index(
            ctx.snapshot(),
            &write,
            MemoryLexicalIndexOperation::Ensure,
            write_limits()
        )
        .await
        .unwrap(),
        UserMemoryLexicalIndexOutcome::NoCorpus
    );
    assert!(!f.user.runtime_root().exists());
    let (a, b) = tokio::join!(
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("same '\nβ"), write_limits()),
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("same '\nβ"), write_limits())
    );
    let a = a.unwrap();
    assert_eq!(a, b.unwrap());
    assert_eq!(a.revision, 1);
    assert_eq!(a.reference_generation, 1);
    assert_eq!(a.title, input("x").title);
    assert_eq!(a.tags, input("x").tags);
    let detail = NativeUserMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.creation.input, input("same '\nβ"));
    assert_eq!(detail.creation.memory, a);
    assert_eq!(detail.generation.owner.identity.subject(), SUBJECT);
    assert_eq!(detail.generation.owner.identity.issuer(), f.server.origin);
    assert_eq!(detail.generation.physical_branch, None);
    assert_eq!(detail.generation.inherited_target, None);
    let uri = url::Url::from_file_path(f.user.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    let reopened =
        graphit_data::lance_branch_store::LanceBranchStore::open(&uri, user_memory_schema(), None)
            .await
            .unwrap();
    let pin = reopened.capture_target(None).await.unwrap();
    assert_eq!(pin.version(), detail.generation.sdk_version);
    assert_eq!(user_memory_schema().fields().len(), 25);
    let changed = NativeUserMemoryStore::update(
        ctx.snapshot(),
        &write,
        a.id,
        update("later", 1),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(changed.revision, 2);
    assert_eq!(
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("same '\nβ"), write_limits())
            .await
            .unwrap(),
        a
    );
    assert_eq!(
        NativeUserMemoryStore::reobserve_create(
            ctx.snapshot(),
            &write,
            input("same '\nβ"),
            write_limits()
        )
        .await
        .unwrap(),
        MemoryCreateObservation::Found(a.clone())
    );
    let mut conflict = input("same '\nβ");
    conflict.references = Some(vec![]);
    assert!(matches!(
        NativeUserMemoryStore::create(ctx.snapshot(), &write, conflict, write_limits()).await,
        Err(MemoryStoreError::IdempotencyConflict)
    ));
    let after = NativeUserMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.memory, changed);
    assert_eq!(
        after.generation.sdk_version,
        detail.generation.sdk_version + 1
    );
    let fresh_ctx = request(&cancel);
    let raw = f.server.token(SUBJECT, RESOURCE, now_seconds() + 600);
    let fresh_proof = verifier
        .verify(&raw, BrokerRequestContext::new(fresh_ctx.snapshot()))
        .await
        .unwrap();
    let fresh_read = f
        .issuer
        .admit_incoming_user_memory_read(&fresh_proof, &verifier, &fresh_ctx, "service", f.scope())
        .unwrap();
    assert_eq!(
        NativeUserMemoryStore::get(fresh_ctx.snapshot(), &fresh_read, a.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        changed
    );
    assert_eq!(f.server.count("/userinfo"), 0);
    assert_eq!(f.server.count("/token"), 0);
}

#[tokio::test]
async fn missing_permissions_and_foreign_original_deny_without_birth() {
    for permissions in [
        vec![],
        vec![ALL[0]],
        vec![ALL[1]],
        vec![ALL[2]],
        vec![ALL[3]],
    ] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::with_permissions(&ctx, &permissions).await;
        let verifier = f.server.verifier(true, f.provider_revision(&ctx));
        let raw = f.server.token(SUBJECT, RESOURCE, now_seconds() + 600);
        let proof = verifier
            .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        assert!(
            f.issuer
                .admit_incoming_user_memory_read(&proof, &verifier, &ctx, "service", f.scope())
                .is_err()
        );
        assert!(
            f.issuer
                .admit_incoming_user_memory_write(
                    &proof,
                    &verifier,
                    &ctx,
                    "service",
                    f.scope(),
                    UserMemoryWriteOperation::Memory
                )
                .is_err()
        );
        assert!(!f.user.runtime_root().exists());
    }
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let other = request(&cancel);
    assert!(
        NativeUserMemoryStore::observe_heads(other.snapshot(), &read, head_limits())
            .await
            .is_err()
    );
    assert!(
        NativeUserMemoryStore::create(other.snapshot(), &write, input("foreign"), write_limits())
            .await
            .is_err()
    );
    assert!(!f.user.runtime_root().exists());
    let missing = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
    assert!(
        NativeUserMemoryStore::update(
            ctx.snapshot(),
            &write,
            missing,
            update("absent", 1),
            write_limits()
        )
        .await
        .is_err()
    );
    assert_eq!(
        NativeUserMemoryStore::history(ctx.snapshot(), &read, missing, read_limits())
            .await
            .unwrap(),
        None
    );
    assert!(!f.user.runtime_root().exists());
}

#[tokio::test]
async fn actual_incoming_config_cancel_deadline_expiry_and_physical_replacement_invalidate_reads() {
    for ending in 0..5 {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx).await;
        let verifier = f.server.verifier(true, f.provider_revision(&ctx));
        let expiry = now_seconds() + if ending == 3 { 2 } else { 600 };
        let raw = f.server.token(SUBJECT, RESOURCE, expiry);
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
        let memory =
            NativeUserMemoryStore::create(ctx.snapshot(), &write, input("valid"), write_limits())
                .await
                .unwrap();
        match ending {
            0 => {
                let result = f
                    .store
                    .mutate(f.store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                        let p = doc.provider("service").unwrap();
                        let revision = p.revision();
                        let at = p.updated_at() + 1;
                        let config = p.config().clone();
                        doc.update_provider("service", config, doc.revision(), revision, at, ctx)
                    })
                    .unwrap();
                assert!(result.failure().is_none());
            }
            1 => cancel.cancel(),
            2 => {
                let deadline = Instant::now() + Duration::from_millis(100);
                let timed = request_until(&cancel, deadline);
                let raw = f.server.token(SUBJECT, RESOURCE, now_seconds() + 600);
                let p = verifier
                    .verify(&raw, BrokerRequestContext::new(timed.snapshot()))
                    .await
                    .unwrap();
                let r = f
                    .issuer
                    .admit_incoming_user_memory_read(&p, &verifier, &timed, "service", f.scope())
                    .unwrap();
                tokio::time::sleep_until(deadline.into()).await;
                assert!(
                    NativeUserMemoryStore::get(timed.snapshot(), &r, memory.id, read_limits())
                        .await
                        .is_err()
                );
                continue;
            }
            3 => {
                let until = std::time::UNIX_EPOCH + Duration::from_secs(expiry);
                tokio::time::sleep(
                    until
                        .duration_since(std::time::SystemTime::now())
                        .unwrap_or_default()
                        + Duration::from_millis(20),
                )
                .await;
            }
            4 => {
                let root = f.user.user_root().unwrap();
                std::fs::rename(root, root.with_extension("old")).unwrap();
                std::fs::create_dir(root).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            NativeUserMemoryStore::get(ctx.snapshot(), &read, memory.id, read_limits())
                .await
                .is_err()
        );
        assert!(
            NativeUserMemoryStore::create(ctx.snapshot(), &write, input("late"), write_limits())
                .await
                .is_err()
        );
        assert_eq!(f.server.count("/userinfo"), 0);
        assert_eq!(f.server.count("/token"), 0);
    }
}

#[tokio::test]
async fn native_profile_admits_real_crud_and_logout_invalidates_both_grants() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::native(&ctx).await;
    f.install_native_profile(&ctx);
    let verifier = f.server.verifier(false, f.provider_revision(&ctx));
    let secret = f.secret();
    let caller = f
        .issuer
        .authenticate_runtime(&secret, &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .unwrap();
    let read = f
        .issuer
        .admit_authenticated_user_memory_read(&caller, &ctx, f.scope())
        .unwrap();
    let write = f
        .issuer
        .admit_authenticated_user_memory_write(
            &caller,
            &ctx,
            f.scope(),
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    let memory =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("native"), write_limits())
            .await
            .unwrap();
    assert_eq!(
        NativeUserMemoryStore::get(ctx.snapshot(), &read, memory.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        memory
    );
    let result = f
        .store
        .mutate(f.store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
            doc.logout(None, doc.revision(), ctx)
        })
        .unwrap();
    assert!(result.failure().is_none());
    assert!(
        NativeUserMemoryStore::get(ctx.snapshot(), &read, memory.id, read_limits())
            .await
            .is_err()
    );
    assert!(
        NativeUserMemoryStore::update(
            ctx.snapshot(),
            &write,
            memory.id,
            update("logout", 1),
            write_limits()
        )
        .await
        .is_err()
    );
    assert_eq!(f.server.count("/userinfo"), 0);
    assert_eq!(f.server.count("/token"), 0);
}

#[tokio::test]
async fn registered_web_reads_same_corpus_without_local_profile_or_userinfo() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let memory =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("web"), write_limits())
            .await
            .unwrap();
    let verifier_web = f.server.verifier(false, f.provider_revision(&ctx));
    let flow = WebOAuth::new(
        &verifier_web,
        OAuthLimits {
            authorization_url_bytes: 8192,
            callback_bytes: 8192,
            callback_query_entries: 8,
            form_body_bytes: 8192,
            token_bytes: 8192,
            response_nodes: 32,
            response_scalar_bytes: 8192,
        },
        "https://workspace.example/callback",
        false,
    )
    .unwrap();
    let registered = flow
        .register(BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let raw=f.server.sign(&serde_json::json!({"iss":f.server.origin,"sub":SUBJECT,"aud":["broker-api",registered.resource()],"client_id":"web-client","jti":"web-records","token_use":"access","iat":now_seconds().saturating_sub(1),"exp":now_seconds()+600}));
    let web = flow
        .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let web_read = f
        .issuer
        .admit_web_user_memory_read(&web, &verifier_web, &ctx, "service", f.scope())
        .unwrap();
    assert_eq!(
        NativeUserMemoryStore::get(ctx.snapshot(), &web_read, memory.id, read_limits())
            .await
            .unwrap(),
        NativeUserMemoryStore::get(ctx.snapshot(), &read, memory.id, read_limits())
            .await
            .unwrap()
    );
    assert_eq!(f.server.count("/register"), 1);
    assert_eq!(f.server.count("/userinfo"), 0);
    assert_eq!(f.server.count("/token"), 0);
}

#[tokio::test]
async fn literal_foreign_issuer_subject_and_installation_cannot_adopt_corpus() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let memory =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("isolation"), write_limits())
            .await
            .unwrap();
    let paths = InstallationPaths::resolve(
        f.user.brand().clone(),
        f.installation.owner_root(),
        None,
        Some(f.installation.owner_root()),
    )
    .unwrap();
    for (issuer, subject, installation) in [
        (
            f.server.origin.clone(),
            SUBJECT.to_uppercase(),
            "user-records-fixture",
        ),
        (
            format!("{}/", f.server.origin),
            SUBJECT.into(),
            "user-records-fixture",
        ),
        (
            f.server.origin.clone(),
            SUBJECT.into(),
            "other-installation",
        ),
    ] {
        let foreign = CapturedPublicationScope::user(
            &paths,
            InstallationId::new(installation).unwrap(),
            UserIdentity::new(issuer, subject).unwrap(),
            ScopeLimits::new(16, 4096, 128).unwrap(),
            ctx.snapshot(),
        )
        .unwrap();
        let binding = foreign.binding().unwrap();
        assert!(
            f.issuer
                .admit_incoming_user_memory_read(
                    &proof,
                    &verifier,
                    &ctx,
                    "service",
                    graphit_auth::ScopeCapture {
                        live: &foreign,
                        record: &binding
                    }
                )
                .is_err()
        );
    }
    assert_eq!(
        NativeUserMemoryStore::get(ctx.snapshot(), &read, memory.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        memory
    );
}

#[tokio::test]
async fn input_guard_and_complete_reads_preserve_no_birth_on_rejection() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let mut limits = write_limits();
    limits.max_input_bytes = NonZeroUsize::new(1).unwrap();
    assert!(
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("too-large"), limits)
            .await
            .is_err()
    );
    assert!(!f.user.runtime_root().exists());
    let memory =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("bounded"), write_limits())
            .await
            .unwrap();
    let mut query_only = read_limits();
    query_only.max_query_bytes = NonZeroUsize::MIN;
    assert!(
        NativeUserMemoryStore::get(ctx.snapshot(), &read, memory.id, query_only)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        NativeUserMemoryStore::history(ctx.snapshot(), &read, memory.id, query_only)
            .await
            .is_ok()
    );
}
