#![cfg(any(target_os = "linux", target_os = "macos", windows))]
#[path = "memory_store/fixture.rs"]
mod fixture;
#[path = "memory_store/sdk.rs"]
mod sdk;
use fixture::*;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::{
    file_lock::Cancellation,
    version_context::{Checkout, VersionContext},
};
use graphit_records::memory_store as memory_api;
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::*,
    reference_projection::ReferenceTargetKind,
};
use std::{
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, Instant},
};

fn input(key: &str) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: " Literal title \n".into(),
        body: " Literal body \n".into(),
        attributes: MemoryAttributes {
            classification: MemoryClass::Decision,
            mandatory: true,
            important: false,
        },
        tags: vec![" tag ".into(), " tag ".into()],
        references: None,
    }
}
macro_rules! admissions {
    ($f:ident,$ctx:ident,$issuer:ident,$authority:ident,$grant:ident,$read:ident,$write:ident,$bearer:ident,$reference:ident,$admin:ident) => {
        let $issuer = $f.issuer(&$ctx);
        let ($bearer, $reference, _run, $admin) = $f.issue(&$issuer, &$ctx);
        let $grant = $issuer
            .verify(
                $bearer.expose_for_transport(),
                &$reference,
                $f.scope(),
                &$ctx,
            )
            .unwrap();
        let $authority = ProjectRecordAuthority::new(&$issuer);
        let $read = $authority
            .admit_read(&$grant, &$ctx, ProjectReadModule::Memory)
            .unwrap();
        let $write = $authority
            .admit_write(&$grant, &$ctx, ProjectWriteOperation::Memory)
            .unwrap();
    };
}
fn published(value: MemoryHeadRead) -> (MemoryGeneration, Vec<MemoryActivityHead>) {
    match value {
        MemoryHeadRead::Published { generation, heads } => (generation, heads),
        _ => panic!("published"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn current_reads_switch_projects_without_pinning_old_commits() {
    #[cfg(target_os = "linux")]
    fn footprint() -> (u64, u64) {
        let text = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap();
        let mut pss = 0;
        let mut private = 0;
        for line in text.lines() {
            let mut fields = line.split_whitespace();
            let name = fields.next().unwrap_or_default();
            if matches!(name, "Pss:" | "Private_Clean:" | "Private_Dirty:") {
                let bytes = fields.next().unwrap().parse::<u64>().unwrap() * 1024;
                if name == "Pss:" {
                    pss = bytes;
                } else {
                    private += bytes;
                }
            }
        }
        (pss, private)
    }
    #[cfg(target_os = "linux")]
    let before_owners = footprint();
    {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let a = Fixture::new(&ctx, "main");
        let b = Fixture::new_for(&ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
        admissions!(a, ctx, ia, aa, ga, ra, wa, ba, refa, _admina);
        admissions!(b, ctx, ib, ab, gb, rb, wb, bb, refb, _adminb);
        let first = NativeMemoryStore::create(ctx.snapshot(), &wa, input("first"), write_limits())
            .await
            .unwrap();
        let other = NativeMemoryStore::create(ctx.snapshot(), &wb, input("other"), write_limits())
            .await
            .unwrap();
        for read in [&ra, &rb] {
            assert_eq!(
                published(
                    NativeMemoryStore::observe_heads(ctx.snapshot(), read, head_limits())
                        .await
                        .unwrap()
                )
                .1
                .len(),
                1
            );
        }
        #[cfg(target_os = "linux")]
        let before = footprint();
        let started = Instant::now();
        for index in 0..32 {
            let (read, expected) = if index % 2 == 0 {
                (&ra, first.id)
            } else {
                (&rb, other.id)
            };
            let (_, heads) = published(
                NativeMemoryStore::observe_heads(ctx.snapshot(), read, head_limits())
                    .await
                    .unwrap(),
            );
            assert_eq!(heads.len(), 1);
            assert_eq!(heads[0].id, expected);
        }
        let elapsed = started.elapsed();
        #[cfg(target_os = "linux")]
        let after = footprint();
        let added = NativeMemoryStore::create(ctx.snapshot(), &wa, input("later"), write_limits())
            .await
            .unwrap();
        let (_, fresh) = published(
            NativeMemoryStore::observe_heads(ctx.snapshot(), &ra, head_limits())
                .await
                .unwrap(),
        );
        assert_eq!(fresh.len(), 2);
        assert!(fresh.iter().any(|head| head.id == added.id));
        assert_eq!(
            published(
                NativeMemoryStore::observe_heads(ctx.snapshot(), &rb, head_limits())
                    .await
                    .unwrap()
            )
            .1
            .len(),
            1
        );
        eprintln!(
            "memory_current_reads iterations=32 elapsed_us={}",
            elapsed.as_micros()
        );
        #[cfg(target_os = "linux")]
        eprintln!(
            "memory_current_reads pss_before={} pss_after={} private_before={} private_after={}",
            before.0, after.0, before.1, after.1
        );
    }
    #[cfg(target_os = "linux")]
    {
        let released = footprint();
        eprintln!(
            "memory_current_reads pss_before_owners={} pss_released={} private_before_owners={} private_released={}",
            before_owners.0, released.0, before_owners.1, released.1
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_same_attempt_one_pair_literal_input_and_fresh_reobservation() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    assert_eq!(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap(),
        MemoryHeadRead::NoCorpus
    );
    assert_eq!(
        NativeMemoryStore::reobserve_create(ctx.snapshot(), &write, input("a'\nβ"), write_limits())
            .await
            .unwrap(),
        MemoryCreateObservation::Unresolved
    );
    assert!(!f.project.runtime_root().join("memory").exists());
    let (a, b) = tokio::join!(
        NativeMemoryStore::create(ctx.snapshot(), &write, input("a'\nβ"), write_limits()),
        NativeMemoryStore::create(ctx.snapshot(), &write, input("a'\nβ"), write_limits())
    );
    let a = a.unwrap();
    assert_eq!(a, b.unwrap());
    assert_eq!(a.revision, 1);
    assert_eq!(a.reference_generation, 1);
    assert_eq!(a.created_by, a.updated_by);
    assert_eq!(a.created_at, a.updated_at);
    assert_eq!(a.title, input("x").title);
    assert_eq!(a.tags, input("x").tags);
    let detail = NativeMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.memory, a);
    assert_eq!(detail.creation.memory, a);
    assert_eq!(detail.creation.input, input("a'\nβ"));
    assert_eq!(detail.creation.operation_version, "main");
    assert_eq!(detail.creation.actor, a.created_by);
    let pin = sdk::pin(&f).await;
    assert_eq!(sdk::all(&pin).await.num_rows(), 3);
    assert_eq!(pin.version(), detail.generation.sdk_version);
    let mut changed = input("a'\nβ");
    changed.references = Some(vec![]);
    assert!(matches!(
        NativeMemoryStore::create(ctx.snapshot(), &write, changed.clone(), write_limits()).await,
        Err(MemoryStoreError::IdempotencyConflict)
    ));
    assert!(matches!(
        NativeMemoryStore::reobserve_create(ctx.snapshot(), &write, changed, write_limits()).await,
        Err(MemoryStoreError::IdempotencyConflict)
    ));
    let ctx2 = request(&cancel);
    let fresh = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx2)
        .unwrap();
    let fresh_write = authority
        .admit_write(&fresh, &ctx2, ProjectWriteOperation::Memory)
        .unwrap();
    assert_eq!(
        NativeMemoryStore::reobserve_create(
            ctx2.snapshot(),
            &fresh_write,
            input("a'\nβ"),
            write_limits()
        )
        .await
        .unwrap(),
        MemoryCreateObservation::Found(a.clone())
    );
    assert_eq!(sdk::pin(&f).await.version(), pin.version());
    let (_, heads) = published(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap(),
    );
    assert_eq!(
        heads,
        vec![MemoryActivityHead {
            id: a.id,
            title: a.title,
            updated_by: a.updated_by,
            updated_at: a.updated_at
        }]
    );
    let mut explicit_empty = input("explicit-empty");
    explicit_empty.references = Some(vec![]);
    let empty = NativeMemoryStore::create(
        ctx.snapshot(),
        &write,
        explicit_empty.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    let detail = NativeMemoryStore::get(ctx.snapshot(), &read, empty.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.creation.input, explicit_empty);
    assert!(detail.memory.references.is_empty());
    assert_eq!(detail.memory.reference_generation, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn distinct_actors_keys_and_cross_scope_references_preserve_history() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, reference, _, _admin) = f.issue(&issuer, &ctx);
    let (other, _) = f.issue_registered_for(&issuer, &ctx, f.scope(), &reference);
    let a = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let b = issuer
        .verify(other.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let wa = authority
        .admit_write(&a, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    let wb = authority
        .admit_write(&b, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    let read = authority
        .admit_read(&b, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let mut value = input("same");
    value.references = Some(vec![
        MemoryReference {
            target_kind: ReferenceTargetKind::Memory,
            scope: MemoryReferenceScope::User("logical-user".into()),
            id: "opaque-target".into(),
            context: None,
            relation: "supports".into(),
            field: Some("body".into()),
        },
        MemoryReference {
            target_kind: ReferenceTargetKind::Knowledge,
            scope: MemoryReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap()),
            id: "nested/page".into(),
            context: Some("artifact@1.0.0".into()),
            relation: "uses".into(),
            field: None,
        },
    ]);
    let duplicate = value.references.as_ref().unwrap()[0].clone();
    value.references.as_mut().unwrap().push(duplicate);
    let ma = NativeMemoryStore::create(ctx.snapshot(), &wa, value.clone(), write_limits())
        .await
        .unwrap();
    let mb = NativeMemoryStore::create(ctx.snapshot(), &wb, value.clone(), write_limits())
        .await
        .unwrap();
    assert_ne!(ma.id, mb.id);
    assert_ne!(ma.created_by, mb.created_by);
    assert_eq!(ma.references, value.references.clone().unwrap());
    let (_, heads) = published(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap(),
    );
    assert_eq!(heads.len(), 2);
    assert_eq!(
        heads.iter().find(|h| h.id == ma.id).unwrap().updated_by,
        ma.created_by
    );
    assert_eq!(sdk::all(&sdk::pin(&f).await).await.num_rows(), 6);
    // Reader B observes A's authorship; a fresh B attempt cannot recover an A-only key.
    let only_a = NativeMemoryStore::create(ctx.snapshot(), &wa, input("only-a"), write_limits())
        .await
        .unwrap();
    assert_eq!(
        NativeMemoryStore::reobserve_create(ctx.snapshot(), &wb, input("only-a"), write_limits())
            .await
            .unwrap(),
        MemoryCreateObservation::Unresolved
    );
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &read, only_a.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .creation
            .actor,
        ma.created_by
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inherited_default_named_attempt_is_distinct_and_reobserve_does_not_materialize() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let mut f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (a, ar, _, _admin) = f.issue(&issuer, &ctx);
    let authority = ProjectRecordAuthority::new(&issuer);
    let default = {
        let grant = issuer
            .verify(a.expose_for_transport(), &ar, f.scope(), &ctx)
            .unwrap();
        let write = authority
            .admit_write(&grant, &ctx, ProjectWriteOperation::Memory)
            .unwrap();
        NativeMemoryStore::create(ctx.snapshot(), &write, input("same"), write_limits())
            .await
            .unwrap()
    };
    let paths = graphit_core::project_paths::ProjectPaths::at(
        &f.root,
        &graphit_core::project_paths::InstallationPaths::resolve(
            graphit_core::brand::Brand::default(),
            &f.root,
            None,
            Some(f.installation.owner_root()),
        )
        .unwrap(),
    )
    .unwrap();
    f.version = VersionContext::select(&paths, Checkout::NonGit, "feature", None, "").unwrap();
    assert!(
        issuer
            .verify(a.expose_for_transport(), &ar, f.scope(), &ctx)
            .is_err()
    );
    let (b, br, _, _admin2) = f.issue_for(&issuer, &ctx, f.scope());
    let grant = issuer
        .verify(b.expose_for_transport(), &br, f.scope(), &ctx)
        .unwrap();
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    let (inherited, heads) = published(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap(),
    );
    assert!(inherited.inherited_target.is_some());
    assert_eq!(heads.len(), 1);
    assert_eq!(heads[0].id, default.id);
    assert_eq!(
        NativeMemoryStore::reobserve_create(ctx.snapshot(), &write, input("same"), write_limits())
            .await
            .unwrap(),
        MemoryCreateObservation::Unresolved
    );
    let (after, _) = published(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap(),
    );
    assert_eq!(after, inherited);
    let named = NativeMemoryStore::create(ctx.snapshot(), &write, input("same"), write_limits())
        .await
        .unwrap();
    assert_ne!(named.id, default.id);
    let detail = NativeMemoryStore::get(ctx.snapshot(), &read, named.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.creation.operation_version, "feature");
    assert_ne!(named.created_by, default.created_by);
    assert!(detail.generation.inherited_target.is_none());
    let old = NativeMemoryStore::get(ctx.snapshot(), &read, default.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(old.creation.operation_version, "main");
    let (_, heads) = published(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap(),
    );
    assert_eq!(heads.len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn input_and_output_budgets_absence_empty_and_incomplete_are_distinct() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let mut limits = write_limits();
    limits.max_input_bytes = NonZeroUsize::new(1).unwrap();
    assert!(matches!(
        NativeMemoryStore::create(ctx.snapshot(), &write, input("x"), limits).await,
        Err(MemoryStoreError::Limit(_))
    ));
    let mut blank = input("x");
    blank.body = " \t\n".into();
    assert!(matches!(
        NativeMemoryStore::create(ctx.snapshot(), &write, blank, write_limits()).await,
        Err(MemoryStoreError::Invalid(_))
    ));
    assert!(!f.project.runtime_root().join("memory").exists());
    limits = write_limits();
    limits.max_row_bytes = NonZeroUsize::new(1).unwrap();
    assert!(matches!(
        NativeMemoryStore::create(ctx.snapshot(), &write, input("x"), limits).await,
        Err(MemoryStoreError::Limit(_))
    ));
    let (empty, heads) = published(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap(),
    );
    assert!(heads.is_empty());
    assert_eq!(
        NativeMemoryStore::reobserve_create(ctx.snapshot(), &write, input("x"), write_limits())
            .await
            .unwrap(),
        MemoryCreateObservation::Unresolved
    );
    assert_eq!(sdk::pin(&f).await.version(), empty.sdk_version);
    let created = NativeMemoryStore::create(ctx.snapshot(), &write, input("x"), write_limits())
        .await
        .unwrap();
    let mut query_only = read_limits();
    query_only.max_query_bytes = NonZeroUsize::MIN;
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &read, created.id, query_only)
            .await
            .unwrap()
            .unwrap()
            .memory,
        created
    );
    let f2 = Fixture::new_for(&ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    admissions!(
        f2,
        ctx,
        issuer2,
        authority2,
        grant2,
        read2,
        _write2,
        _bearer2,
        _reference2,
        _admin2
    );
    drop(sdk::directory_only(&f2, &ctx).await);
    let error = NativeMemoryStore::create(ctx.snapshot(), &_write2, input("x"), write_limits())
        .await
        .unwrap_err();
    assert!(
        matches!(error, MemoryStoreError::IncompleteCorpus),
        "{error:?}"
    );
    assert!(matches!(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read2, head_limits()).await,
        Err(MemoryStoreError::IncompleteCorpus)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreign_request_wrong_module_fenced_revoke_cancel_deadline_and_project_isolation() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let other_cancel = Cancellation::default();
    let other = request(&other_cancel);
    assert!(matches!(
        NativeMemoryStore::observe_heads(other.snapshot(), &read, head_limits()).await,
        Err(MemoryStoreError::DifferentRequest)
    ));
    let task_read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let task_write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let fenced = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::FencedMemory)
        .unwrap();
    assert!(matches!(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &task_read, head_limits()).await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    for wrong in [&task_write, &fenced] {
        assert!(matches!(
            NativeMemoryStore::create(ctx.snapshot(), wrong, input("x"), write_limits()).await,
            Err(MemoryStoreError::WrongAdmission)
        ));
    }
    assert!(!f.project.runtime_root().join("memory").exists());
    let memory = NativeMemoryStore::create(ctx.snapshot(), &write, input("x"), write_limits())
        .await
        .unwrap();
    let f2 = Fixture::new_for(&other, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    admissions!(
        f2,
        other,
        issuer2,
        authority2,
        grant2,
        read2,
        _write2,
        _bearer2,
        _reference2,
        _admin2
    );
    assert!(
        NativeMemoryStore::get(other.snapshot(), &read2, memory.id, read_limits())
            .await
            .unwrap()
            .is_none()
    );
    assert!(!f2.project.runtime_root().join("memory").exists());
    let fresh = request(&other_cancel);
    let freshgrant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &fresh)
        .unwrap();
    let freshread = authority
        .admit_read(&freshgrant, &fresh, ProjectReadModule::Memory)
        .unwrap();
    other_cancel.cancel();
    assert!(
        NativeMemoryStore::observe_heads(fresh.snapshot(), &freshread, head_limits())
            .await
            .is_err()
    );
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &_admin).unwrap();
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .is_err()
    );
    assert!(
        NativeMemoryStore::create(ctx.snapshot(), &write, input("y"), write_limits())
            .await
            .is_err()
    );
    let deadline_cancel = Cancellation::default();
    let fd = Fixture::new(&ctx, "main");
    let id = fd.issuer(&ctx);
    let (bd, rd, _, _admin3) = fd.issue(&id, &ctx);
    let deadline = request_until(
        &deadline_cancel,
        Instant::now() + Duration::from_millis(200),
    );
    let gd = id
        .verify(bd.expose_for_transport(), &rd, fd.scope(), &deadline)
        .unwrap();
    let ad = ProjectRecordAuthority::new(&id);
    let read = ad
        .admit_read(&gd, &deadline, ProjectReadModule::Memory)
        .unwrap();
    tokio::time::sleep_until(deadline.snapshot().deadline().unwrap().into()).await;
    assert!(
        NativeMemoryStore::observe_heads(deadline.snapshot(), &read, head_limits())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn metadata_complete_projection_eof_flags_and_body_independence() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let mut values = vec![];
    for i in 0..6 {
        let mut value = input(&format!("{i}"));
        value.title = format!("β {i}");
        value.body = "large body".repeat(10000);
        values.push(
            NativeMemoryStore::create(ctx.snapshot(), &write, value, write_limits())
                .await
                .unwrap(),
        );
    }
    let old = sdk::pin(&f).await;
    let mut limits = head_limits();
    limits.batch_rows = std::num::NonZeroU32::new(1).unwrap();
    let (generation, heads) = published(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, limits)
            .await
            .unwrap(),
    );
    assert_eq!(heads.len(), 6);
    assert!(heads.windows(2).all(|h| h[0].id < h[1].id));
    assert_eq!(generation.sdk_version, old.version());
    let raw = sdk::all(&old).await;
    let poison = sdk::text_column(
        &raw,
        17,
        vec!["invalid JSON with a large body ".repeat(5000); raw.num_rows()],
    );
    let superseded = (0..raw.num_rows())
        .map(|i| {
            sdk::string(&raw, 1, i) == "creation"
                || sdk::string(&raw, 2, i) == values[0].id.to_string()
        })
        .collect();
    let tombstone = (0..raw.num_rows())
        .map(|i| {
            sdk::string(&raw, 1, i) == "head" && sdk::string(&raw, 2, i) == values[1].id.to_string()
        })
        .collect();
    let poison = sdk::flags(&sdk::flags(&poison, 13, superseded), 14, tombstone);
    let dates = (0..raw.num_rows())
        .map(|i| {
            if sdk::string(&raw, 2, i) == values[2].id.to_string() {
                u64::MAX
            } else {
                0
            }
        })
        .collect();
    sdk::replace(&old, sdk::numbers(&poison, 10, dates)).await;
    let (fresh, heads) = published(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap(),
    );
    assert!(fresh.sdk_version > old.version());
    assert_eq!(heads.len(), 4);
    assert!(
        !heads
            .iter()
            .any(|h| h.id == values[0].id || h.id == values[1].id)
    );
    assert_eq!(
        heads
            .iter()
            .find(|h| h.id == values[2].id)
            .unwrap()
            .updated_at,
        u64::MAX
    );
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &read, values[2].id, read_limits())
            .await
            .is_err()
    );
    assert_eq!(sdk::all(&old).await.num_rows(), 18);
    assert!(
        sdk::all(&old)
            .await
            .column(17)
            .as_any()
            .downcast_ref::<arrow_array::StringArray>()
            .unwrap()
            .iter()
            .all(|v| v.unwrap().starts_with('{'))
    );
    assert_eq!(sdk::pin(&f).await.version(), fresh.sdk_version);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn duplicate_after_first_batch_fails_complete_metadata_and_full_pair() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let value = NativeMemoryStore::create(ctx.snapshot(), &write, input("x"), write_limits())
        .await
        .unwrap();
    let pin = sdk::pin(&f).await;
    let raw = sdk::all(&pin).await;
    let duplicate = sdk::select(&raw, "head");
    let duplicate = sdk::text_column(&duplicate, 0, vec![format!("duplicate/{}", value.id)]);
    sdk::replace(&pin, duplicate).await;
    let mut limits = head_limits();
    limits.batch_rows = std::num::NonZeroU32::new(1).unwrap();
    assert!(matches!(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, limits).await,
        Err(MemoryStoreError::Invalid("duplicate Memory head ID"))
    ));
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &read, value.id, read_limits())
            .await
            .is_err()
    );
    assert!(
        NativeMemoryStore::reobserve_create(ctx.snapshot(), &write, input("x"), write_limits())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_read_rejects_missing_audit_header_and_reference_disagreement() {
    for column in [1, 7, 16] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        admissions!(
            f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
        );
        let value = NativeMemoryStore::create(ctx.snapshot(), &write, input("x"), write_limits())
            .await
            .unwrap();
        let pin = sdk::pin(&f).await;
        let raw = sdk::all(&pin).await;
        let row = sdk::select(&raw, "creation");
        let text = if column == 16 {
            "not references"
        } else {
            "disagrees"
        };
        sdk::replace(&pin, sdk::text_column(&row, column, vec![text.into()])).await;
        assert!(
            NativeMemoryStore::get(ctx.snapshot(), &read, value.id, read_limits())
                .await
                .is_err()
        );
        assert!(
            NativeMemoryStore::reobserve_create(ctx.snapshot(), &write, input("x"), write_limits())
                .await
                .is_err()
        );
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_schema_symlink_and_captured_project_replacement_are_errors() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    sdk::empty_with_schema(
        &f,
        &ctx,
        Arc::new(arrow_schema::Schema::new(vec![arrow_schema::Field::new(
            "payload",
            arrow_schema::DataType::Utf8,
            false,
        )])),
    )
    .await;
    assert!(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .is_err()
    );
    assert!(
        NativeMemoryStore::list(
            ctx.snapshot(),
            &read,
            MemoryListFilter::default(),
            read_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeMemoryStore::create(ctx.snapshot(), &write, input("x"), write_limits())
            .await
            .is_err()
    );
    let f2 = Fixture::new_for(&ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    admissions!(
        f2,
        ctx,
        issuer2,
        authority2,
        grant2,
        read2,
        write2,
        _bearer2,
        _reference2,
        _admin2
    );
    let memorydir = f2.project.runtime_root().join("memory");
    drop(sdk::directory_only(&f2, &ctx).await);
    std::fs::remove_dir(&memorydir).unwrap();
    std::os::unix::fs::symlink(f2.root.join("missing"), &memorydir).unwrap();
    assert!(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read2, head_limits())
            .await
            .is_err()
    );
    assert!(
        NativeMemoryStore::list(
            ctx.snapshot(),
            &read2,
            MemoryListFilter::default(),
            read_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeMemoryStore::create(ctx.snapshot(), &write2, input("x"), write_limits())
            .await
            .is_err()
    );
    let moved = f2.root.with_file_name("moved");
    std::fs::rename(&f2.root, moved).unwrap();
    std::fs::create_dir(&f2.root).unwrap();
    assert!(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read2, head_limits())
            .await
            .is_err()
    );
    assert!(
        NativeMemoryStore::list(
            ctx.snapshot(),
            &read2,
            MemoryListFilter::default(),
            read_limits()
        )
        .await
        .is_err()
    );
}

#[test]
fn canonical_identifiers_reject_zero_lowercase_overflow_and_extra_bytes() {
    let id: MemoryId = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
    assert_eq!(id.to_string(), "01KSH1CRFFG8Z74B5ZS78WW808");
    assert_eq!(
        serde_json::from_str::<MemoryId>(&serde_json::to_string(&id).unwrap()).unwrap(),
        id
    );
    for bad in [
        "00000000000000000000000000",
        "01ksh1crffg8z74b5zs78ww808",
        "81KSH1CRFFG8Z74B5ZS78WW808",
        " 01KSH1CRFFG8Z74B5ZS78WW808",
        "01KSH1CRFFG8Z74B5ZS78WW808x",
    ] {
        assert!(bad.parse::<MemoryId>().is_err());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_capture_rejects_wrong_kind_and_native_aliases() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, _write, _bearer, _reference, _admin
    );
    assert!(matches!(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap(),
        MemoryHeadRead::NoCorpus
    ));
    drop(sdk::directory_only(&f, &ctx).await);
    let memory = f.project.runtime_root().join("memory");
    let index = memory.join("index.lance");
    std::fs::write(&index, b"not a directory").unwrap();
    assert!(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .is_err()
    );
    read.recheck().unwrap();
    std::fs::remove_file(&index).unwrap();

    // Following either alias would expose a valid, published empty dataset.
    let foreign = f.root.join("foreign");
    std::fs::create_dir(&foreign).unwrap();
    let foreign_index = foreign.join("index.lance");
    let schema = sdk::schema();
    let published = graphit_data::lance_module_store::LanceModuleStore::create(
        foreign_index.to_str().unwrap(),
        Arc::clone(&schema),
        vec![arrow_array::RecordBatch::new_empty(schema)],
    )
    .await
    .unwrap();
    assert_eq!(
        published
            .snapshot()
            .scan()
            .try_into_batch()
            .await
            .unwrap()
            .num_rows(),
        0
    );
    for at_index in [true, false] {
        let (target, alias) = if at_index {
            (&foreign_index, &index)
        } else {
            std::fs::remove_dir(&memory).unwrap();
            (&foreign, &memory)
        };
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, alias).unwrap();
        #[cfg(windows)]
        graphit_fs::LinkPlan::capture(target, alias, graphit_fs::NodeKind::Directory)
            .unwrap()
            .create()
            .unwrap();
        assert!(
            NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
                .await
                .is_err(),
            "alias at index: {at_index}"
        );
        ctx.snapshot().check().unwrap();
        read.recheck().unwrap();
        if at_index {
            #[cfg(unix)]
            std::fs::remove_file(&index).unwrap();
            #[cfg(windows)]
            std::fs::remove_dir(&index).unwrap();
        }
    }
}

#[path = "memory_store/catalogue.rs"]
mod catalogue;
#[path = "memory_store/revision.rs"]
mod revision;

#[path = "memory_store/lexical_index.rs"]
mod lexical_index;
#[path = "memory_store/lexical_search.rs"]
mod lexical_search;

#[path = "memory_store/embeddings.rs"]
mod embeddings;

#[path = "memory_store/automatic.rs"]
mod automatic;
#[path = "memory_store/fenced.rs"]
mod fenced;
#[path = "memory_store/references.rs"]
mod references;

#[path = "memory_store/hybrid.rs"]
mod hybrid;
