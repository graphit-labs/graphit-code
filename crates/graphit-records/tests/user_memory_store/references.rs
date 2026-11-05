use super::*;
use graphit_core::{artifact_store::StoreOwner, file_lock::Cancellation};
use graphit_records::reference_projection::ReferenceTargetKind;
use std::num::{NonZeroU32, NonZeroUsize};
fn limits() -> MemoryStoreReadLimits {
    read_limits()
}
fn owner(f: &Fixture) -> UserMemoryOwner {
    match f.user.owner() {
        StoreOwner::User {
            installation_id,
            identity,
        } => UserMemoryOwner {
            installation_id: installation_id.clone(),
            identity: identity.clone(),
        },
        _ => panic!("USER owner"),
    }
}
fn scope(f: &Fixture) -> MemoryReferenceScope {
    MemoryReferenceScope::User(
        UserMemoryScopeId::from_owner(&owner(f))
            .unwrap()
            .as_str()
            .into(),
    )
}
fn reference(f: &Fixture, kind: ReferenceTargetKind, id: &str) -> MemoryReference {
    MemoryReference {
        target_kind: kind,
        scope: scope(f),
        id: id.into(),
        context: None,
        relation: " supports β \n".into(),
        field: Some(" body ".into()),
    }
}
fn target(r: &MemoryReference) -> MemoryReferenceTarget {
    MemoryReferenceTarget {
        target_kind: r.target_kind,
        scope: r.scope.clone(),
        id: r.id.clone(),
        context: r.context.clone(),
    }
}
fn query(
    outgoing: Option<MemoryId>,
    incoming: Option<MemoryReferenceTarget>,
) -> UserMemoryReferencesQuery {
    UserMemoryReferencesQuery {
        outgoing,
        incoming,
        page_size: NonZeroUsize::new(100).unwrap(),
        outgoing_cursor: None,
        incoming_cursor: None,
    }
}
type Pages = (
    UserMemoryGeneration,
    Option<Box<UserMemoryReferencePage>>,
    Option<Box<UserMemoryReferencePage>>,
    UserMemoryReferencesCoverage,
    Vec<MemoryReferencesWarning>,
);
fn pages(result: UserMemoryReferencesRead) -> Pages {
    match result {
        UserMemoryReferencesRead::Published {
            generation,
            outgoing,
            incoming,
            coverage,
            warnings,
        } => (generation, outgoing, incoming, *coverage, warnings),
        _ => panic!("published USER references"),
    }
}
fn cursor(value: serde_json::Value) -> UserMemoryReferencesCursor {
    serde_json::from_value(value).unwrap()
}

#[tokio::test]
async fn user_reference_literals_duplicates_ordinals_and_same_pin_rich_targets() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let anchor =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("anchor"), write_limits())
            .await
            .unwrap();
    let rich = reference(&f, ReferenceTargetKind::Memory, &anchor.id.to_string());
    let task = reference(&f, ReferenceTargetKind::Task, " tsk-literal β ");
    let session = reference(&f, ReferenceTargetKind::Session, "ses-literal");
    let mut imported = reference(&f, ReferenceTargetKind::Knowledge, "doc#literal");
    imported.context = Some("catalog@v1".into());
    let mut foreign = rich.clone();
    foreign.scope = MemoryReferenceScope::User(
        serde_json::json!([
            owner(&f).installation_id.as_str(),
            format!("{}/", f.server.origin),
            SUBJECT
        ])
        .to_string(),
    );
    let mut project = rich.clone();
    project.scope = MemoryReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap());
    let mut opaque = rich.clone();
    opaque.scope = MemoryReferenceScope::User("literal opaque issuer/sub".into());
    let mut noncanonical = rich.clone();
    noncanonical.id = format!(" {} ", anchor.id);
    let authored = vec![
        rich.clone(),
        rich.clone(),
        task.clone(),
        session,
        imported.clone(),
        foreign,
        project,
        opaque,
        noncanonical,
    ];
    let mut create = input("source");
    create.references = Some(authored.clone());
    let source = NativeUserMemoryStore::create(ctx.snapshot(), &write, create, write_limits())
        .await
        .unwrap();
    let (generation, outgoing, incoming, coverage, warnings) = pages(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(Some(source.id), Some(target(&rich))),
            limits(),
        )
        .await
        .unwrap(),
    );
    let out = outgoing.unwrap();
    let inc = incoming.unwrap();
    assert_eq!(out.total, authored.len());
    assert_eq!(inc.total, 2);
    assert!(out.complete && inc.complete);
    assert!(warnings.is_empty());
    assert_eq!(coverage.owner, owner(&f));
    assert!(coverage.memory_user_source_complete && coverage.inbound_other_sources_not_qualified);
    assert_eq!(generation.physical_branch, None);
    assert_eq!(generation.inherited_target, None);
    for (ordinal, edge) in out.results.iter().enumerate() {
        assert_eq!(edge.ordinal, ordinal);
        assert_eq!(edge.reference, authored[ordinal]);
        assert_eq!(edge.source, source.id);
        assert_eq!(
            edge.source_scope,
            UserMemoryScopeId::from_owner(&owner(&f)).unwrap()
        );
        assert_eq!(edge.source_revision, 1);
        assert_eq!(edge.reference_generation, 1);
        assert_eq!(edge.source_snapshot, generation);
        assert_eq!(edge.provenance, MemoryReferenceProvenance::Authored);
        assert_eq!(
            edge.source_observation,
            MemoryReferenceResolution::Memory {
                id: source.id,
                title: source.title.clone()
            }
        );
        assert_eq!(
            edge.target_resolution,
            if ordinal < 2 {
                MemoryReferenceResolution::Memory {
                    id: anchor.id,
                    title: anchor.title.clone(),
                }
            } else {
                MemoryReferenceResolution::Unresolved
            }
        );
    }
    assert_eq!(inc.results[0], out.results[0]);
    assert_eq!(inc.results[1], out.results[1]);
    for (r, total) in [(task, 1), (imported, 1)] {
        let (_, out, inc, _, _) = pages(
            NativeUserMemoryStore::references(
                ctx.snapshot(),
                &read,
                query(None, Some(target(&r))),
                limits(),
            )
            .await
            .unwrap(),
        );
        assert!(out.is_none());
        assert_eq!(inc.unwrap().total, total);
    }
    let encoded = serde_json::to_value(&out).unwrap();
    assert!(!encoded.to_string().contains("quartz initial body"));
}

#[tokio::test]
async fn current_markers_track_domain_revision_separately_clear_delete_and_archive() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let anchor = NativeUserMemoryStore::create(
        ctx.snapshot(),
        &write,
        input("lifecycle-anchor"),
        write_limits(),
    )
    .await
    .unwrap();
    let r = reference(&f, ReferenceTargetKind::Memory, &anchor.id.to_string());
    let mut create = input("lifecycle-source");
    create.references = Some(vec![r.clone(), r.clone()]);
    let source = NativeUserMemoryStore::create(ctx.snapshot(), &write, create, write_limits())
        .await
        .unwrap();
    let second = NativeUserMemoryStore::update(
        ctx.snapshot(),
        &write,
        source.id,
        update("body-only", 1),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(second.reference_generation, 1);
    let (_, out, inc, _, _) = pages(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(Some(source.id), Some(target(&r))),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(inc.unwrap().total, 2);
    assert!(
        out.unwrap()
            .results
            .iter()
            .all(|edge| edge.source_revision == 2 && edge.reference_generation == 1)
    );
    let mut clear = update("clear-refs", 2);
    clear.body = None;
    clear.references = Some(vec![]);
    let cleared =
        NativeUserMemoryStore::update(ctx.snapshot(), &write, source.id, clear, write_limits())
            .await
            .unwrap();
    assert_eq!(cleared.reference_generation, 2);
    let (_, out, inc, coverage, warnings) = pages(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(Some(source.id), Some(target(&r))),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(out.unwrap().total, 0);
    assert_eq!(inc.unwrap().total, 0);
    assert!(coverage.memory_user_source_complete && warnings.is_empty());
    let mut restore = update("restore-refs", 3);
    restore.body = None;
    restore.references = Some(vec![r.clone()]);
    NativeUserMemoryStore::update(ctx.snapshot(), &write, source.id, restore, write_limits())
        .await
        .unwrap();
    NativeUserMemoryStore::delete(
        ctx.snapshot(),
        &write,
        anchor.id,
        DeleteMemory {
            expected_revision: 1,
            idempotency_key: "delete-anchor".into(),
            reason: "archive target".into(),
        },
        write_limits(),
    )
    .await
    .unwrap();
    let (_, out, _, _, _) = pages(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(Some(source.id), None),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        out.unwrap().results[0].target_resolution,
        MemoryReferenceResolution::Unresolved
    );
    NativeUserMemoryStore::delete(
        ctx.snapshot(),
        &write,
        source.id,
        DeleteMemory {
            expected_revision: 4,
            idempotency_key: "delete-source".into(),
            reason: "end authored edges".into(),
        },
        write_limits(),
    )
    .await
    .unwrap();
    let (_, _, inc, coverage, warnings) = pages(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(None, Some(target(&r))),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(inc.unwrap().total, 0);
    assert!(coverage.memory_user_source_complete && warnings.is_empty());
    assert!(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(Some(source.id), None),
            limits()
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn closed_cursors_bind_both_foci_owner_root_pin_direction_and_all_public_caps() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let r = reference(&f, ReferenceTargetKind::Task, "tsk-cursor");
    let mut create = input("cursor-source");
    create.references = Some(vec![r.clone(); 4]);
    let source = NativeUserMemoryStore::create(ctx.snapshot(), &write, create, write_limits())
        .await
        .unwrap();
    let mut first = query(Some(source.id), Some(target(&r)));
    first.page_size = NonZeroUsize::new(1).unwrap();
    let (_, out, inc, _, _) = pages(
        NativeUserMemoryStore::references(ctx.snapshot(), &read, first.clone(), limits())
            .await
            .unwrap(),
    );
    let out_cursor = out.unwrap().next_cursor.unwrap();
    let inc_cursor = inc.unwrap().next_cursor.unwrap();
    let mut continuation = first.clone();
    continuation.page_size = NonZeroUsize::new(3).unwrap();
    continuation.outgoing_cursor = Some(out_cursor.clone());
    continuation.incoming_cursor = Some(inc_cursor.clone());
    let (_, out, inc, _, _) = pages(
        NativeUserMemoryStore::references(ctx.snapshot(), &read, continuation.clone(), limits())
            .await
            .unwrap(),
    );
    assert_eq!(
        out.unwrap()
            .results
            .iter()
            .map(|e| e.ordinal)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(inc.unwrap().results.len(), 3);
    let mut json = serde_json::to_value(&out_cursor).unwrap();
    json["offset"] = serde_json::json!(4);
    let mut end = first.clone();
    end.outgoing_cursor = Some(cursor(json.clone()));
    let (_, out, _, _, _) = pages(
        NativeUserMemoryStore::references(ctx.snapshot(), &read, end.clone(), limits())
            .await
            .unwrap(),
    );
    assert!(out.unwrap().results.is_empty());
    json["offset"] = serde_json::json!(5);
    end.outgoing_cursor = Some(cursor(json));
    assert!(
        NativeUserMemoryStore::references(ctx.snapshot(), &read, end, limits())
            .await
            .is_err()
    );
    let mut swapped = first.clone();
    swapped.outgoing_cursor = Some(inc_cursor);
    assert!(
        NativeUserMemoryStore::references(ctx.snapshot(), &read, swapped, limits())
            .await
            .is_err()
    );
    let mut changed = continuation.clone();
    changed.incoming.as_mut().unwrap().id.push('x');
    assert!(
        NativeUserMemoryStore::references(ctx.snapshot(), &read, changed, limits())
            .await
            .is_err()
    );
    let mut changed = continuation.clone();
    changed.outgoing = None;
    assert!(
        NativeUserMemoryStore::references(ctx.snapshot(), &read, changed, limits())
            .await
            .is_err()
    );
    for member in [
        "root",
        "owner",
        "sdk_version",
        "physical_branch",
        "inherited_target",
    ] {
        let mut json = serde_json::to_value(&out_cursor).unwrap();
        match member {
            "root" => {
                json["binding"]["root"] = serde_json::to_value(
                    graphit_core::native_path::NativePath::new(std::path::PathBuf::from(
                        "/foreign/root/index.lance",
                    ))
                    .unwrap(),
                )
                .unwrap()
            }
            "owner" => {
                json["binding"]["generation"]["owner"]["identity"]["subject"] =
                    serde_json::json!("foreign")
            }
            "sdk_version" => json["binding"]["generation"][member] = serde_json::json!(999999),
            _ => json["binding"]["generation"][member] = serde_json::json!("foreign"),
        };
        let c = cursor(json);
        let mut changed = first.clone();
        changed.outgoing_cursor = Some(c);
        assert!(
            NativeUserMemoryStore::references(ctx.snapshot(), &read, changed, limits())
                .await
                .is_err(),
            "binding {member}"
        );
    }
    let mut changed_policy = limits();
    changed_policy.batch_rows = NonZeroU32::MIN;
    changed_policy.max_query_bytes = NonZeroUsize::MIN;
    assert!(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &read,
            continuation.clone(),
            changed_policy
        )
        .await
        .is_ok()
    );
    let mut json = serde_json::to_value(&out_cursor).unwrap();
    json["unknown"] = serde_json::json!(true);
    assert!(serde_json::from_value::<UserMemoryReferencesCursor>(json).is_err());
    let mut json = serde_json::to_value(&out_cursor).unwrap();
    json["binding"]["unknown"] = serde_json::json!(true);
    assert!(serde_json::from_value::<UserMemoryReferencesCursor>(json).is_err());
    let mut json = serde_json::to_value(&first).unwrap();
    json["unknown"] = serde_json::json!(true);
    assert!(serde_json::from_value::<UserMemoryReferencesQuery>(json).is_err());
    let absent = Fixture::new(&ctx).await;
    let absent_verifier = absent.server.verifier(true, absent.provider_revision(&ctx));
    let raw = absent.server.token(SUBJECT, RESOURCE, now_seconds() + 600);
    let absent_proof = absent_verifier
        .verify(
            &raw,
            graphit_auth::broker::BrokerRequestContext::new(ctx.snapshot()),
        )
        .await
        .unwrap();
    let absent_read = absent
        .issuer
        .admit_incoming_user_memory_read(
            &absent_proof,
            &absent_verifier,
            &ctx,
            "service",
            absent.scope(),
        )
        .unwrap();
    assert!(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &absent_read,
            continuation.clone(),
            limits()
        )
        .await
        .is_err()
    );
    assert!(!absent.user.runtime_root().exists());
    NativeUserMemoryStore::update(
        ctx.snapshot(),
        &write,
        source.id,
        update("advance-pin", 1),
        write_limits(),
    )
    .await
    .unwrap();
    assert!(
        NativeUserMemoryStore::references(ctx.snapshot(), &read, continuation, limits())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn missing_current_marker_is_incomplete_without_old_edge_resurrection_or_repair() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let r = reference(&f, ReferenceTargetKind::Task, "missing-current");
    let mut create = input("missing-marker");
    create.references = Some(vec![r.clone()]);
    let source = NativeUserMemoryStore::create(ctx.snapshot(), &write, create, write_limits())
        .await
        .unwrap();
    NativeUserMemoryStore::update(
        ctx.snapshot(),
        &write,
        source.id,
        update("current-body", 1),
        write_limits(),
    )
    .await
    .unwrap();
    let good = integrity::physical_rows(&f).await;
    let rows = good
        .iter()
        .filter(|b| {
            !(integrity::text(b, "kind") == "references"
                && integrity::text(b, "row_key") == format!("references/{}/2", source.id))
        })
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(rows.len() + 1, good.len());
    integrity::replace(&f, rows).await;
    let (generation, out, inc, coverage, warnings) = pages(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(Some(source.id), Some(target(&r))),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert!(!coverage.memory_user_source_complete);
    assert_eq!(
        warnings,
        vec![MemoryReferencesWarning::MissingCurrentProjection {
            source: source.id,
            revision: 2
        }]
    );
    assert_eq!(out.unwrap().total, 0);
    assert_eq!(inc.unwrap().total, 0);
    let (again, _, _, _, _) = pages(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(None, Some(target(&r))),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(again, generation);
    assert_eq!(integrity::physical_rows(&f).await.len(), good.len() - 1);
    integrity::replace(&f, good).await;
    let (_, out, _, coverage, warnings) = pages(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(Some(source.id), None),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(out.unwrap().total, 1);
    assert!(coverage.memory_user_source_complete && warnings.is_empty());
}

#[tokio::test]
async fn eof_rejects_old_orphan_duplicate_tampered_owner_header_payload_and_embedding_rows() {
    use arrow_array::UInt64Array;
    use std::sync::Arc;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let r = reference(&f, ReferenceTargetKind::Task, "integrity-reference");
    let mut create = input("refs-integrity");
    create.references = Some(vec![r.clone()]);
    let source = NativeUserMemoryStore::create(ctx.snapshot(), &write, create, write_limits())
        .await
        .unwrap();
    NativeUserMemoryStore::update(
        ctx.snapshot(),
        &write,
        source.id,
        update("body-integrity", 1),
        write_limits(),
    )
    .await
    .unwrap();
    let good = integrity::physical_rows(&f).await;
    let current = good
        .iter()
        .find(|b| integrity::text(b, "row_key") == format!("references/{}/2", source.id))
        .unwrap();
    let old = good
        .iter()
        .find(|b| integrity::text(b, "row_key") == format!("references/{}/1", source.id))
        .unwrap();
    let malformed = integrity::replace_text(
        &integrity::replace_text(old, "row_key", "references/01KSH1CRFFG8Z74B5ZS78WW809/999"),
        "payload",
        "{broken",
    );
    let foreign = integrity::replace_text(current, "user_issuer", "foreign literal issuer");
    let mismatched = integrity::replace_text(current, "references", "[]");
    let mut payload: serde_json::Value =
        serde_json::from_str(&integrity::text(current, "payload")).unwrap();
    payload["references"][0]["relation"] = serde_json::json!("tampered");
    let payload_tamper = integrity::replace_text(
        current,
        "payload",
        &serde_json::to_string(&payload).unwrap(),
    );
    let head_mismatch = integrity::replace_text(
        &payload_tamper,
        "references",
        &serde_json::to_string(&payload["references"]).unwrap(),
    );
    let mut columns = current.columns().to_vec();
    columns[current.schema().index_of("revision").unwrap()] = Arc::new(UInt64Array::from(vec![99]));
    let header_tamper = arrow_array::RecordBatch::try_new(current.schema(), columns).unwrap();
    let unknown = integrity::replace_text(current, "kind", "unknown-marker-family");
    let mut embedding_columns = current.columns().to_vec();
    embedding_columns[current.schema().index_of("embedding_recipe").unwrap()] =
        Arc::new(arrow_array::StringArray::from(vec!["foreign embedding"]));
    let embedding = arrow_array::RecordBatch::try_new(current.schema(), embedding_columns).unwrap();
    assert!(
        !embedding
            .column_by_name("embedding_recipe")
            .unwrap()
            .is_null(0)
    );
    let mut duplicate = good.clone();
    duplicate.push(old.clone());
    let mut orphan = good.clone();
    orphan.push(malformed);
    let mut fixtures = vec![duplicate, orphan];
    for bad in [
        foreign,
        mismatched,
        payload_tamper,
        head_mismatch,
        header_tamper,
        unknown,
        embedding,
    ] {
        fixtures.push(
            good.iter()
                .map(|b| {
                    if integrity::text(b, "row_key") == integrity::text(current, "row_key") {
                        bad.clone()
                    } else {
                        b.clone()
                    }
                })
                .collect(),
        );
    }
    let mut bounds = limits();
    bounds.batch_rows = NonZeroU32::new(1).unwrap();
    for (index, rows) in fixtures.into_iter().enumerate() {
        integrity::replace(&f, rows).await;
        assert!(
            NativeUserMemoryStore::references(
                ctx.snapshot(),
                &read,
                query(Some(source.id), Some(target(&r))),
                bounds
            )
            .await
            .is_err(),
            "corrupt fixture {index}"
        );
    }
    integrity::replace(&f, good).await;
    assert!(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(Some(source.id), None),
            limits()
        )
        .await
        .is_ok()
    );
}

#[tokio::test]
async fn complete_reference_eof_and_both_json_copies_validate() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let r = reference(&f, ReferenceTargetKind::Task, "bound-reference");
    let mut create = input("refs-bounds");
    create.references = Some(vec![r.clone(); 3]);
    let source = NativeUserMemoryStore::create(ctx.snapshot(), &write, create, write_limits())
        .await
        .unwrap();
    let q = query(Some(source.id), Some(target(&r)));
    let mut query_only = limits();
    query_only.max_query_bytes = NonZeroUsize::MIN;
    assert!(
        NativeUserMemoryStore::references(ctx.snapshot(), &read, q.clone(), query_only)
            .await
            .is_ok()
    );
    let good = integrity::physical_rows(&f).await;
    let marker = good
        .iter()
        .find(|b| integrity::text(b, "kind") == "references")
        .unwrap();
    let oversized = serde_json::to_string(&vec![r.clone(); 20]).unwrap();
    let mut raw_payload: serde_json::Value =
        serde_json::from_str(&integrity::text(marker, "payload")).unwrap();
    raw_payload["references"] = serde_json::to_value(vec![r.clone(); 20]).unwrap();
    // Each JSON copy must be valid even when the other copy is unchanged.
    for column in ["references", "payload"] {
        let malformed = if column == "references" {
            format!(
                "[{{\"invalid\":true}},{}]",
                &oversized[1..oversized.len() - 1]
            )
        } else {
            let mut p = raw_payload.clone();
            p["references"][0] = serde_json::json!({"invalid":true});
            p.to_string()
        };
        let bad = integrity::replace_text(marker, column, &malformed);
        let rows = good
            .iter()
            .map(|b| {
                if integrity::text(b, "kind") == "references" {
                    bad.clone()
                } else {
                    b.clone()
                }
            })
            .collect();
        integrity::replace(&f, rows).await;
        let bounded = limits();
        assert!(
            matches!(
                NativeUserMemoryStore::references(ctx.snapshot(), &read, q.clone(), bounded).await,
                Err(MemoryStoreError::Json(_)) | Err(MemoryStoreError::Invalid(_))
            ),
            "raw {column} must reject malformed owned element"
        );
    }
    integrity::replace(&f, good).await;
}

#[tokio::test]
async fn absent_corpus_query_preflight_and_real_authority_failures_do_not_birth_or_fake_empty() {
    use graphit_auth::broker::BrokerRequestContext;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, _write);
    let r = reference(&f, ReferenceTargetKind::Task, "absent-inbound");
    assert_eq!(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(None, Some(target(&r))),
            limits()
        )
        .await
        .unwrap(),
        UserMemoryReferencesRead::NoCorpus
    );
    assert!(!f.user.runtime_root().exists());
    for bad in [
        query(None, None),
        {
            let mut q = query(None, Some(target(&r)));
            q.page_size = NonZeroUsize::new(101).unwrap();
            q
        },
        {
            let mut t = target(&r);
            t.context = Some("non-Knowledge-context".into());
            query(None, Some(t))
        },
    ] {
        assert!(
            NativeUserMemoryStore::references(ctx.snapshot(), &read, bad, limits())
                .await
                .is_err()
        );
        assert!(!f.user.runtime_root().exists());
    }
    let foreign = request(&cancel);
    assert!(
        NativeUserMemoryStore::references(
            foreign.snapshot(),
            &read,
            query(None, Some(target(&r))),
            limits()
        )
        .await
        .is_err()
    );
    for permissions in [vec![], vec![ALL[0]], vec![ALL[1]]] {
        let denied = Fixture::with_permissions(&ctx, &permissions).await;
        let verifier = denied.server.verifier(true, denied.provider_revision(&ctx));
        let raw = denied.server.token(SUBJECT, RESOURCE, now_seconds() + 600);
        let proof = verifier
            .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        assert!(
            denied
                .issuer
                .admit_incoming_user_memory_read(&proof, &verifier, &ctx, "service", denied.scope())
                .is_err()
        );
        assert!(!denied.user.runtime_root().exists());
    }
    cancel.cancel();
    assert!(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &read,
            query(None, Some(target(&r))),
            limits()
        )
        .await
        .is_err()
    );
    assert!(!f.user.runtime_root().exists());
}

#[tokio::test]
async fn live_query_config_signed_expiry_deadline_and_physical_swap_recheck_before_output() {
    use graphit_auth::broker::BrokerRequestContext;
    use std::time::{Duration, Instant};
    for ending in 0..4 {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx).await;
        let verifier = f.server.verifier(true, f.provider_revision(&ctx));
        let seed_expiry = now_seconds() + 600;
        let raw = f.server.token(SUBJECT, RESOURCE, seed_expiry);
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
                graphit_auth::UserMemoryWriteOperation::Memory,
            )
            .unwrap();
        let r = reference(&f, ReferenceTargetKind::Task, "live-reference");
        let mut create = input("live-source");
        create.references = Some(vec![r.clone()]);
        let source = NativeUserMemoryStore::create(ctx.snapshot(), &write, create, write_limits())
            .await
            .unwrap();
        assert!(
            NativeUserMemoryStore::references(
                ctx.snapshot(),
                &read,
                query(Some(source.id), None),
                limits()
            )
            .await
            .is_ok()
        );
        // Seed/SDK setup uses a durable proof; expiry is measured only after it completes.
        let expiry = now_seconds() + if ending == 1 { 5 } else { 600 };
        let raw = f.server.token(SUBJECT, RESOURCE, expiry);
        let proof = verifier
            .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let read = f
            .issuer
            .admit_incoming_user_memory_read(&proof, &verifier, &ctx, "service", f.scope())
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
            1 => {
                let end = std::time::UNIX_EPOCH + Duration::from_secs(expiry);
                tokio::time::sleep(
                    end.duration_since(std::time::SystemTime::now())
                        .unwrap_or_default()
                        + Duration::from_millis(20),
                )
                .await;
            }
            2 => {
                let deadline = Instant::now() + Duration::from_millis(150);
                let timed = request_until(&cancel, deadline);
                let raw = f.server.token(SUBJECT, RESOURCE, now_seconds() + 600);
                let p = verifier
                    .verify(&raw, BrokerRequestContext::new(timed.snapshot()))
                    .await
                    .unwrap();
                let admission = f
                    .issuer
                    .admit_incoming_user_memory_read(&p, &verifier, &timed, "service", f.scope())
                    .unwrap();
                tokio::time::sleep_until(deadline.into()).await;
                assert!(
                    NativeUserMemoryStore::references(
                        timed.snapshot(),
                        &admission,
                        query(Some(source.id), None),
                        limits()
                    )
                    .await
                    .is_err()
                );
                continue;
            }
            3 => {
                let root = f.user.user_root().unwrap();
                std::fs::rename(root, root.with_extension("replaced")).unwrap();
                std::fs::create_dir(root).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            NativeUserMemoryStore::references(
                ctx.snapshot(),
                &read,
                query(Some(source.id), Some(target(&r))),
                limits()
            )
            .await
            .is_err()
        );
        assert_eq!(f.server.count("/userinfo"), 0);
        assert_eq!(f.server.count("/token"), 0);
    }
}

#[tokio::test]
async fn real_native_and_registered_web_read_reference_pin_and_markers_never_become_fts_hits() {
    use graphit_auth::broker::{
        BrokerRequestContext,
        oauth::{OAuthLimits, web::WebOAuth},
    };
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::native(&ctx).await;
    f.install_native_profile(&ctx);
    let native_verifier = f.server.verifier(false, f.provider_revision(&ctx));
    let secret = f.secret();
    let caller = f
        .issuer
        .authenticate_runtime(&secret, &ctx)
        .unwrap()
        .authenticate_native(&native_verifier)
        .await
        .unwrap();
    let native_read = f
        .issuer
        .admit_authenticated_user_memory_read(&caller, &ctx, f.scope())
        .unwrap();
    let write = f
        .issuer
        .admit_authenticated_user_memory_write(
            &caller,
            &ctx,
            f.scope(),
            graphit_auth::UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    let r = reference(&f, ReferenceTargetKind::Task, "zirkoniumreferencesonly");
    let mut create = input("factory-source");
    create.references = Some(vec![r.clone()]);
    let source = NativeUserMemoryStore::create(ctx.snapshot(), &write, create, write_limits())
        .await
        .unwrap();
    let expected = NativeUserMemoryStore::references(
        ctx.snapshot(),
        &native_read,
        query(Some(source.id), Some(target(&r))),
        limits(),
    )
    .await
    .unwrap();
    let flow = WebOAuth::new(
        &native_verifier,
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
    let raw=f.server.sign(&serde_json::json!({"iss":f.server.origin,"sub":SUBJECT,"aud":["broker-api",registered.resource()],"client_id":"web-client","jti":"web-reference-query","token_use":"access","iat":now_seconds().saturating_sub(1),"exp":now_seconds()+600}));
    let web = flow
        .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let web_read = f
        .issuer
        .admit_web_user_memory_read(&web, &native_verifier, &ctx, "service", f.scope())
        .unwrap();
    assert_eq!(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &web_read,
            query(Some(source.id), Some(target(&r))),
            limits()
        )
        .await
        .unwrap(),
        expected
    );
    let lexical = MemoryLexicalQuery {
        text: r.id.clone(),
        filter: MemoryListFilter::default(),
        include_deleted_history: true,
        top_k: NonZeroUsize::new(10).unwrap(),
    };
    assert!(matches!(
        NativeUserMemoryStore::search_lexical(ctx.snapshot(), &web_read, lexical, read_limits())
            .await
            .unwrap(),
        UserMemoryLexicalSearchRead::Published { total_hits: 0, hits, .. } if hits.is_empty()
    ));
    let (_, out, _, coverage, warnings) = pages(
        NativeUserMemoryStore::references(
            ctx.snapshot(),
            &web_read,
            query(Some(source.id), None),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(out.unwrap().total, 1);
    assert!(coverage.memory_user_source_complete && warnings.is_empty());
    assert_eq!(f.server.count("/userinfo"), 0);
    assert_eq!(f.server.count("/token"), 0);
}
