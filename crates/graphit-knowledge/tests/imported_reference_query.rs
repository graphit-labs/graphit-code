//! Actual signed Web authority, imported captures and SDK reference continuations.
#![cfg(target_os = "linux")]
#[path = "support/imported_knowledge_fixture.rs"]
mod fixture;
#[path = "imported_reference_query/sdk.rs"]
mod sdk;
use arrow_array::{RecordBatch, StringArray, UInt8Array};
use fixture::*;
use graphit_auth::{AuthError, broker::BrokerRequestContext};
use graphit_core::{file_lock::Cancellation, project_identity::ProjectId};
use graphit_data::{knowledge_index::*, knowledge_reference_query::*, knowledge_references::*};
use graphit_hub::{ArtifactId, ArtifactType, ConcreteVersion, SourceRef};
use graphit_knowledge::{
    imported_knowledge::*, imported_reference_query::*,
    native_reference_query::KnowledgeLogicalTarget,
};
use graphit_records::reference_projection::{LogicalScope, ReferenceTargetKind};
use std::{
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};

macro_rules! authorize {
    ($server:ident,$f:ident,$ctx:ident,$revision:ident,$issuer:ident,$read:ident) => {
        let verifier = $server.verifier($revision);
        let flow = web(&verifier);
        let registered = flow
            .register(BrokerRequestContext::new($ctx.snapshot()))
            .await
            .unwrap();
        let raw = $server.access(registered.resource());
        let proof = flow
            .verify_access(
                &registered,
                &raw,
                BrokerRequestContext::new($ctx.snapshot()),
            )
            .await
            .unwrap();
        let $read = $issuer
            .admit_web_imported_knowledge_read(&proof, &verifier, &$ctx, "service", $f.realm())
            .unwrap();
    };
}
fn open_limits() -> ImportedKnowledgeReadLimits {
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
fn limits() -> graphit_data::knowledge_index::KnowledgeIndexReadLimits {
    page_limits()
}
fn query<'a>(
    outgoing: Option<&'a str>,
    incoming: Option<KnowledgeLogicalTarget<'a>>,
) -> ImportedKnowledgeReferencesQuery<'a> {
    ImportedKnowledgeReferencesQuery {
        outgoing,
        incoming,
        page_size: n(100),
        outgoing_cursor: None,
        incoming_cursor: None,
    }
}
fn target<'a>(
    kind: ReferenceTargetKind,
    scope: LogicalScope<'a>,
    id: &'a str,
    context: Option<&'a str>,
) -> KnowledgeLogicalTarget<'a> {
    KnowledgeLogicalTarget {
        target_kind: kind,
        scope,
        id,
        context,
    }
}
fn memory() -> KnowledgeLogicalTarget<'static> {
    target(
        ReferenceTargetKind::Memory,
        LogicalScope::User("owner β"),
        "same-id",
        None,
    )
}
fn authored() -> KnowledgeAuthoredReference<'static> {
    KnowledgeAuthoredReference {
        target_kind: KnowledgeReferenceTargetKind::Memory,
        scope: KnowledgeReferenceScope::User("owner β"),
        id: "same-id",
        context: None,
        relation: " supports ",
        field: Some(""),
    }
}
async fn open<'a>(
    read: &'a graphit_auth::ImportedKnowledgeReadAdmission<'a>,
    f: &'a Fixture,
    alias: Option<&'a str>,
) -> Box<CapturedImportedKnowledge<'a>> {
    let ImportedKnowledgeOpen::Published(reader) =
        open_imported_knowledge(read, input(f, alias), open_limits())
            .await
            .unwrap()
    else {
        panic!("actual published SDK pin required")
    };
    reader
}
fn invalid_query(error: ImportedKnowledgeReferenceError, field: &str) {
    assert!(
        matches!(error,ImportedKnowledgeReferenceError::Query(KnowledgeReferenceQueryError::InvalidQuery(f)) if f==field)
    );
}
fn invalid_cursor(error: ImportedKnowledgeReferenceError) {
    assert!(matches!(
        error,
        ImportedKnowledgeReferenceError::Query(KnowledgeReferenceQueryError::InvalidCursor)
    ));
}
#[test]
fn literal_namespaces_targets_and_raw_provenance_use_the_imported_pin() {
    native_test(async {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let revision = f.provider(&server, &ctx);
        let issuer = f.issuer(&ctx);
        let other: ProjectId = "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap();
        let mut authored = vec![authored(), authored()];
        for kind in [
            KnowledgeReferenceTargetKind::Task,
            KnowledgeReferenceTargetKind::Session,
            KnowledgeReferenceTargetKind::Knowledge,
        ] {
            authored.push(KnowledgeAuthoredReference {
                target_kind: kind,
                context: if kind == KnowledgeReferenceTargetKind::Knowledge {
                    Some("target ctx")
                } else {
                    None
                },
                ..authored[0]
            });
        }
        let wiki = ["b", "", "a", "b", "ghost"];
        let pages = [
            page("b", &[]),
            page("a", &wiki),
            page("user-source", &["a"]),
            page("removed", &[]),
        ];
        let fields = [
            fields("b", KnowledgeDocumentState::Active, &[]),
            fields("a", KnowledgeDocumentState::Active, &authored),
            fields("user-source", KnowledgeDocumentState::Active, &[]),
            fields("removed", KnowledgeDocumentState::Removed, &[]),
        ];
        f.seed(&issuer, &ctx, &pages, Some(&fields)).await;
        let rows = sdk::rows(&f).await;
        let patched = rows
            .iter()
            .map(|r| {
                let slug = r
                    .column(0)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap()
                    .value(0);
                match slug {
                    "a" => sdk::namespace(
                        r,
                        KnowledgeReferenceScope::Project(other),
                        Some("source ctx"),
                        &authored,
                        &wiki,
                    ),
                    "b" => sdk::namespace(
                        r,
                        KnowledgeReferenceScope::Project(other),
                        Some("source ctx"),
                        &[],
                        &[],
                    ),
                    "user-source" => sdk::namespace(
                        r,
                        KnowledgeReferenceScope::User("stored source user"),
                        Some("stored source ctx"),
                        &[],
                        &["a"],
                    ),
                    _ => r.clone(),
                }
            })
            .collect();
        sdk::replace(&f, patched).await;
        authorize!(server, f, ctx, revision, issuer, read);
        let reader = open(&read, &f, Some("mount β")).await;
        let before = image(f.source.directory().as_path());
        let keys = server.count("/keys");
        let result = reader
            .references(query(Some("a"), Some(memory())), limits())
            .await
            .unwrap();
        let out = result.outgoing.unwrap();
        assert_eq!(result.focused_source.as_ref(), Some(&out.results[0].source));
        let zero_edges = reader
            .references(query(Some("b"), None), limits())
            .await
            .unwrap();
        let focus = zero_edges.focused_source.as_ref().unwrap();
        assert_eq!(focus.slug, "b");
        assert_eq!(
            focus.namespace.as_ref().unwrap().scope,
            KnowledgeReferenceOwnedScope::Project(other)
        );
        assert_eq!(
            focus.namespace.as_ref().unwrap().context.as_deref(),
            Some("source ctx")
        );
        assert!(focus.witness.is_some());
        let zero_page = zero_edges.outgoing.unwrap();
        assert!(zero_page.complete);
        assert_eq!(zero_page.total, 0);
        assert!(zero_page.results.is_empty());
        assert!(zero_page.next_cursor.is_none());
        assert_eq!(out.total, 9);
        assert_eq!(out.results.len(), 9);
        assert!(out.complete);
        assert_eq!(result.incoming.unwrap().total, 2);
        assert!(result.coverage.knowledge_source_complete);
        assert!(result.coverage.inbound_other_sources_not_qualified);
        assert!(result.warnings.is_empty());
        assert!(
            out.results
                .iter()
                .all(|e| e.source.namespace.as_ref().is_some_and(|ns| ns.scope
                    == KnowledgeReferenceOwnedScope::Project(other)
                    && ns.context.as_deref() == Some("source ctx"))
                    && e.source.witness.is_some())
        );
        assert_eq!(
            out.results
                .iter()
                .map(|e| (e.provenance, e.ordinal))
                .collect::<Vec<_>>(),
            vec![
                (KnowledgeReferenceProvenance::Authored, 0),
                (KnowledgeReferenceProvenance::Authored, 1),
                (KnowledgeReferenceProvenance::Authored, 2),
                (KnowledgeReferenceProvenance::Authored, 3),
                (KnowledgeReferenceProvenance::Authored, 4),
                (KnowledgeReferenceProvenance::WikiLink, 0),
                (KnowledgeReferenceProvenance::WikiLink, 2),
                (KnowledgeReferenceProvenance::WikiLink, 3),
                (KnowledgeReferenceProvenance::WikiLink, 4)
            ]
        );
        assert!(
            matches!(&out.results[0].payload,KnowledgeReferencePayload::Authored{target,relation,field} if target.namespace.scope==KnowledgeReferenceOwnedScope::User("owner β".into())&&target.id=="same-id"&&relation==" supports "&&field.as_deref()==Some(""))
        );
        assert!(
            matches!(&out.results[5].target_observation,KnowledgeReferenceObservation::KnowledgeHead{slug,..} if slug=="b")
        );
        for kind in [
            ReferenceTargetKind::Task,
            ReferenceTargetKind::Session,
            ReferenceTargetKind::Knowledge,
        ] {
            let context = if kind == ReferenceTargetKind::Knowledge {
                Some("target ctx")
            } else {
                None
            };
            assert_eq!(
                reader
                    .references(
                        query(
                            None,
                            Some(target(
                                kind,
                                LogicalScope::User("owner β"),
                                "same-id",
                                context
                            ))
                        ),
                        limits()
                    )
                    .await
                    .unwrap()
                    .incoming
                    .unwrap()
                    .total,
                1
            );
        }
        assert_eq!(
            reader
                .references(
                    query(
                        None,
                        Some(target(
                            ReferenceTargetKind::Knowledge,
                            LogicalScope::User("owner β"),
                            "same-id",
                            Some("wrong ctx")
                        ))
                    ),
                    limits()
                )
                .await
                .unwrap()
                .incoming
                .unwrap()
                .total,
            0
        );
        let user = reader
            .references(query(Some("user-source"), None), limits())
            .await
            .unwrap()
            .outgoing
            .unwrap();
        assert!(
            matches!(&user.results[0].payload,KnowledgeReferencePayload::WikiLink{target} if target.namespace.scope==KnowledgeReferenceOwnedScope::User("stored source user".into())&&target.namespace.context.as_deref()==Some("stored source ctx"))
        );
        assert_eq!(reader.observation().version.as_str(), "exact-version");
        assert_eq!(reader.observation().context_alias, Some("mount β"));
        let KnowledgeHeadRead::Published { heads, .. } =
            reader.heads(heads_limits()).await.unwrap()
        else {
            panic!()
        };
        assert_eq!(heads.len(), 3);
        assert_eq!(image(f.source.directory().as_path()), before);
        assert_eq!(server.count("/keys"), keys);
        assert_eq!(server.count("/userinfo"), 0);
        assert_eq!(server.count("/token"), 0);
    });
}

#[test]
fn two_continuations_and_held_generation_survive_actual_later_sdk_commit() {
    native_test(async {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let revision = f.provider(&server, &ctx);
        let issuer = f.issuer(&ctx);
        let refs = [authored(), authored()];
        let pages = [page("z", &[]), page("a", &["z", "a", "z"])];
        let fields = [
            fields("z", KnowledgeDocumentState::Active, &[]),
            fields("a", KnowledgeDocumentState::Active, &refs),
        ];
        f.seed(&issuer, &ctx, &pages, Some(&fields)).await;
        authorize!(server, f, ctx, revision, issuer, read);
        let reader = open(&read, &f, None).await;
        let generation = reader.observation().generation.clone();
        let mut q = query(Some("a"), Some(memory()));
        q.page_size = n(1);
        let first = reader.references(q, limits()).await.unwrap();
        let focused_source = first.focused_source.clone().unwrap();
        let mut oc = first.outgoing.as_ref().unwrap().next_cursor.clone();
        let mut ic = first.incoming.as_ref().unwrap().next_cursor.clone();
        let mut outgoing = first.outgoing.unwrap().results;
        let mut incoming = first.incoming.unwrap().results;
        let newgen = sdk::replace(&f, vec![]).await;
        assert!(newgen > generation.version);
        while oc.is_some() || ic.is_some() {
            let q = ImportedKnowledgeReferencesQuery {
                outgoing: oc.as_ref().map(|_| "a"),
                incoming: ic.as_ref().map(|_| memory()),
                page_size: n(1),
                outgoing_cursor: oc.as_ref(),
                incoming_cursor: ic.as_ref(),
            };
            // Both foci form the binding; an exhausted direction is retained without its cursor.
            let q = ImportedKnowledgeReferencesQuery {
                outgoing: Some("a"),
                incoming: Some(memory()),
                ..q
            };
            let result = reader.references(q, limits()).await.unwrap();
            assert_eq!(result.focused_source.as_ref(), Some(&focused_source));
            let out = result.outgoing.unwrap();
            let inc = result.incoming.unwrap();
            if oc.is_some() {
                outgoing.extend(out.results);
                oc = out.next_cursor;
            }
            if ic.is_some() {
                incoming.extend(inc.results);
                ic = inc.next_cursor;
            }
        }
        assert_eq!(outgoing.len(), 5);
        assert_eq!(incoming.len(), 2);
        assert_eq!(
            outgoing
                .iter()
                .map(|e| (e.provenance, e.ordinal))
                .collect::<Vec<_>>(),
            vec![
                (KnowledgeReferenceProvenance::Authored, 0),
                (KnowledgeReferenceProvenance::Authored, 1),
                (KnowledgeReferenceProvenance::WikiLink, 0),
                (KnowledgeReferenceProvenance::WikiLink, 1),
                (KnowledgeReferenceProvenance::WikiLink, 2)
            ]
        );
        assert_eq!(reader.observation().generation, generation);
        let fresh = open(&read, &f, None).await;
        assert_eq!(fresh.observation().generation.version, newgen);
        let old = first_cursor(&reader).await;
        let mut q = query(Some("a"), Some(memory()));
        q.page_size = n(1);
        q.outgoing_cursor = Some(&old);
        invalid_cursor(fresh.references(q, limits()).await.unwrap_err());
        assert_eq!(
            fresh
                .references(query(None, Some(memory())), limits())
                .await
                .unwrap()
                .incoming
                .unwrap()
                .total,
            0
        );
    });
}
async fn first_cursor(reader: &CapturedImportedKnowledge<'_>) -> ImportedKnowledgeReferencesCursor {
    let mut q = query(Some("a"), Some(memory()));
    q.page_size = n(1);
    reader
        .references(q, limits())
        .await
        .unwrap()
        .outgoing
        .unwrap()
        .next_cursor
        .unwrap()
}

#[test]
fn cursor_selection_alias_and_data_fields_are_checked_without_new_authority() {
    native_test(async {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let revision = f.provider(&server, &ctx);
        let issuer = f.issuer(&ctx);
        let refs = [authored(), authored()];
        f.seed(
            &issuer,
            &ctx,
            &[page("a", &[])],
            Some(&[fields("a", KnowledgeDocumentState::Active, &refs)]),
        )
        .await;
        authorize!(server, f, ctx, revision, issuer, read);
        let reader = open(&read, &f, None).await;
        let cursor = first_cursor(&reader).await;
        let alias = open(&read, &f, Some("alias β")).await;
        assert_eq!(
            alias.observation().generation,
            reader.observation().generation
        );
        let mut q = query(Some("a"), Some(memory()));
        q.page_size = n(1);
        q.outgoing_cursor = Some(&cursor);
        assert!(matches!(
            alias.references(q, limits()).await,
            Err(ImportedKnowledgeReferenceError::InvalidCursor)
        ));
        let mut encoded = serde_json::to_value(&cursor).unwrap();
        encoded["extra"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ImportedKnowledgeReferencesCursor>(encoded).is_err());
        let encoded = serde_json::to_value(&cursor).unwrap();
        assert!(
            serde_json::from_value::<ImportedKnowledgeReferencesCursor>(encoded["cursor"].clone())
                .is_err()
        );
        for (path, value) in [
            (
                vec!["binding", "0"],
                serde_json::json!(encoded["binding"][0].as_u64().unwrap() ^ 1),
            ),
            (vec!["cursor", "direction"], serde_json::json!("incoming")),
            (vec!["cursor", "offset"], serde_json::json!(usize::MAX)),
            (
                vec!["cursor", "binding", "uri"],
                serde_json::json!("file:///other"),
            ),
            (vec!["cursor", "binding", "version"], serde_json::json!(0)),
            (
                vec!["cursor", "binding", "selection"],
                serde_json::json!({"native":{"project_id":"01KSH1CRFFG8Z74B5ZS78WW808","selected_lance_branch":""}}),
            ),
            (
                vec!["cursor", "binding", "outgoing"],
                serde_json::json!("other"),
            ),
        ] {
            let mut json = encoded.clone();
            if path == vec!["binding", "0"] {
                json["binding"][0] = value;
            } else {
                let mut at = &mut json;
                for key in &path[..path.len() - 1] {
                    at = &mut at[*key];
                }
                at[path[path.len() - 1]] = value;
            }
            let changed: ImportedKnowledgeReferencesCursor = serde_json::from_value(json).unwrap();
            let mut changedq = q;
            changedq.outgoing_cursor = Some(&changed);
            let error = reader.references(changedq, limits()).await.unwrap_err();
            assert!(matches!(
                error,
                ImportedKnowledgeReferenceError::InvalidCursor
                    | ImportedKnowledgeReferenceError::Query(
                        KnowledgeReferenceQueryError::InvalidCursor
                    )
            ));
        }
        let mut oversized = encoded.clone();
        oversized["cursor"]["binding"]["uri"] = serde_json::json!("β".repeat(40000));
        let oversized: ImportedKnowledgeReferencesCursor =
            serde_json::from_value(oversized).unwrap();
        let mut oversizedq = q;
        oversizedq.outgoing_cursor = Some(&oversized);
        invalid_cursor(reader.references(oversizedq, limits()).await.unwrap_err());
        let mut changedlimits = limits();
        changedlimits.batch_rows = std::num::NonZeroU32::new(2).unwrap();
        let continued = reader.references(q, changedlimits).await.unwrap();
        assert!(continued.outgoing.is_some());
        let mut physicallimits = limits();
        physicallimits.native = graphit_core::native_document_file::NativeDocumentFileLimits::new(
            32768, 32768, 128, 32768,
        )
        .unwrap();
        assert!(reader.references(q, physicallimits).await.is_ok());
        let mut ordinary = encoded.clone();
        ordinary["cursor"]["offset"] = serde_json::json!(0);
        let ordinary: ImportedKnowledgeReferencesCursor = serde_json::from_value(ordinary).unwrap();
        let mut ordinaryq = q;
        ordinaryq.outgoing_cursor = Some(&ordinary);
        assert_eq!(
            reader
                .references(ordinaryq, limits())
                .await
                .unwrap()
                .outgoing
                .unwrap()
                .results[0]
                .ordinal,
            0
        );
        let foreign = f.foreign_registry();
        assert!(matches!(
            open_imported_knowledge(
                &read,
                ImportedKnowledgeInput {
                    registry: &foreign,
                    ..input(&f, None)
                },
                open_limits()
            )
            .await,
            Err(ImportedKnowledgeReadError::InvalidRegistryScope)
        ));
        let missing = ArtifactId::new("other", f.registry_limits_projection()).unwrap();
        let other = ConcreteVersion::new("another", f.registry_limits_projection()).unwrap();
        assert!(matches!(
            open_imported_knowledge(
                &read,
                ImportedKnowledgeInput {
                    id: &missing,
                    version: &other,
                    ..input(&f, None)
                },
                open_limits()
            )
            .await,
            Ok(ImportedKnowledgeOpen::RegistryAbsent)
        ));
        let mut entry = f.entry();
        entry.installations[0].original_source = SourceRef::hub_cache(
            &f.paths,
            "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap(),
            ArtifactType::Knowledge,
            f.id.clone(),
            None,
            f.artifact_version.clone(),
            f.registry_limits_projection(),
        )
        .unwrap();
        {
            let mut writer = f
                .registry
                .writer(
                    ctx.remaining().unwrap().expect("finite fixture deadline"),
                    ctx.cancellation(),
                )
                .unwrap();
            writer.put_metadata(entry).unwrap();
        }
        assert!(matches!(
            reader.references(q, limits()).await,
            Err(ImportedKnowledgeReferenceError::Reader(
                ImportedKnowledgeReadError::SourceSelectionChanged
            ))
        ));
    });
}

#[test]
fn incomplete_empty_and_removed_are_honest_without_filesystem_or_network_writes() {
    native_test(async {
        for state in ["incomplete", "empty", "removed", "projected-empty"] {
            let server = Server::new().await;
            let cancel = Cancellation::default();
            let ctx = context(&cancel);
            let f = Fixture::new(&ctx);
            let revision = f.provider(&server, &ctx);
            let issuer = f.issuer(&ctx);
            match state {
                "empty" => f.seed(&issuer, &ctx, &[], None).await,
                "incomplete" => {
                    f.seed(&issuer, &ctx, &[page("a", &["unknown"])], None)
                        .await
                }
                "removed" => {
                    f.seed(
                        &issuer,
                        &ctx,
                        &[page("a", &[])],
                        Some(&[fields("a", KnowledgeDocumentState::Removed, &[])]),
                    )
                    .await
                }
                _ => {
                    f.seed(
                        &issuer,
                        &ctx,
                        &[page("a", &[])],
                        Some(&[fields("a", KnowledgeDocumentState::Active, &[])]),
                    )
                    .await
                }
            };
            authorize!(server, f, ctx, revision, issuer, read);
            let before = image(f.source.directory().as_path());
            let keys = server.count("/keys");
            let reader = open(&read, &f, None).await;
            let incoming = reader
                .references(query(None, Some(memory())), limits())
                .await
                .unwrap();
            assert!(incoming.focused_source.is_none());
            assert_eq!(incoming.incoming.unwrap().total, 0);
            if state == "incomplete" {
                assert!(!incoming.coverage.knowledge_source_complete);
                assert_eq!(incoming.warnings.len(), 1);
                let result = reader
                    .references(query(Some("a"), None), limits())
                    .await
                    .unwrap();
                let focus = result.focused_source.as_ref().unwrap();
                let KnowledgeReferencesWarning::MissingCurrentProjection { source } =
                    &result.warnings[0];
                assert_eq!(focus, source);
                assert_eq!(focus.namespace, None);
                assert_eq!(focus.witness, None);
                let out = result.outgoing.unwrap();
                assert!(!out.complete);
                assert_eq!(out.total, 0);
            } else {
                assert!(incoming.coverage.knowledge_source_complete);
                assert!(incoming.warnings.is_empty());
            }
            if state == "empty" || state == "removed" {
                assert!(matches!(
                    reader.references(query(Some("a"), None), limits()).await,
                    Err(ImportedKnowledgeReferenceError::Query(
                        KnowledgeReferenceQueryError::UnknownSource
                    ))
                ));
            } else if state == "projected-empty" {
                let result = reader
                    .references(query(Some("a"), None), limits())
                    .await
                    .unwrap();
                let focus = result.focused_source.as_ref().unwrap();
                assert_eq!(focus.slug, "a");
                assert_eq!(
                    focus.namespace.as_ref().unwrap().scope,
                    KnowledgeReferenceOwnedScope::Project(
                        "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap()
                    )
                );
                assert!(focus.witness.is_some());
                let out = result.outgoing.unwrap();
                assert!(out.complete);
                assert!(out.results.is_empty());
            }
            drop(reader);
            let again = open(&read, &f, None).await;
            assert!(
                again
                    .references(query(None, Some(memory())), limits())
                    .await
                    .is_ok()
            );
            drop(again);
            assert_eq!(image(f.source.directory().as_path()), before);
            assert_eq!(server.count("/keys"), keys);
            assert_eq!(server.count("/userinfo"), 0);
            assert_eq!(server.count("/token"), 0);
        }
    });
}

#[test]
fn late_sdk_witness_state_removed_and_duplicate_reject_the_whole_page() {
    native_test(async {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let revision = f.provider(&server, &ctx);
        let issuer = f.issuer(&ctx);
        let pages = [page("a", &["z"]), page("z", &[])];
        let fields = [
            fields("a", KnowledgeDocumentState::Active, &[]),
            fields("z", KnowledgeDocumentState::Removed, &[]),
        ];
        f.seed(&issuer, &ctx, &pages, Some(&fields)).await;
        authorize!(server, f, ctx, revision, issuer, read);
        let rows = sdk::rows(&f).await;
        let first = sdk::row(&rows, "a");
        let late = sdk::row(&rows, "z");
        for corruption in ["witness", "state", "removed", "duplicate"] {
            let mut columns = late.columns().to_vec();
            match corruption {
                "witness" => columns[1] = Arc::new(StringArray::from(vec!["changed late title"])),
                "state" => columns[16] = Arc::new(UInt8Array::from(vec![9])),
                "removed" => columns[15] = first.column(15).clone(),
                _ => {}
            };
            let changed = if corruption == "duplicate" {
                first.clone()
            } else {
                RecordBatch::try_new(late.schema(), columns).unwrap()
            };
            sdk::replace(&f, vec![first.clone(), changed]).await;
            let reader = open(&read, &f, None).await;
            for q in [query(Some("a"), None), query(None, Some(memory()))] {
                let error = reader.references(q, limits()).await.unwrap_err();
                assert!(error.source().is_some());
                assert!(matches!(
                    error,
                    ImportedKnowledgeReferenceError::Query(KnowledgeReferenceQueryError::Data(
                        KnowledgeIndexError::InvalidField(_) | KnowledgeIndexError::DuplicateSlug
                    ))
                ));
            }
        }
    });
}

#[test]
fn actual_registry_auth_capture_and_original_failures_keep_typed_first_cause() {
    native_test(async {
        for action in [
            "registry-before",
            "registry-flight",
            "auth",
            "capture",
            "cancel-flight",
            "deadline",
        ] {
            let server = Server::new().await;
            let cancel = Cancellation::default();
            let initial = context(&cancel);
            let f = Fixture::new(&initial);
            let revision = f.provider(&server, &initial);
            let issuer = f.issuer(&initial);
            f.seed(
                &issuer,
                &initial,
                &[page("a", &["a"])],
                Some(&[fields("a", KnowledgeDocumentState::Active, &[])]),
            )
            .await;
            let ctx = if action == "deadline" {
                context_deadline(&cancel, Instant::now() + Duration::from_secs(2))
            } else {
                context(&cancel)
            };
            authorize!(server, f, ctx, revision, issuer, read);
            let reader = open(&read, &f, None).await;
            let keys = server.count("/keys");
            let result = match action {
                "registry-before" => {
                    f.remove_registration(&ctx);
                    reader.references(query(Some("a"), None), limits()).await
                }
                "registry-flight" => {
                    let (result, ()) =
                        tokio::join!(reader.references(query(Some("a"), None), limits()), async {
                            f.remove_registration(&ctx)
                        });
                    result
                }
                "auth" => {
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
                    reader.references(query(Some("a"), None), limits()).await
                }
                "capture" => {
                    let root = f.source.directory().as_path();
                    std::fs::rename(root, root.with_extension("old")).unwrap();
                    std::fs::create_dir(root).unwrap();
                    reader.references(query(Some("a"), None), limits()).await
                }
                "cancel-flight" => {
                    let (result, ()) =
                        tokio::join!(reader.references(query(Some("a"), None), limits()), async {
                            cancel.cancel()
                        });
                    result
                }
                _ => {
                    tokio::time::sleep(
                        ctx.remaining().unwrap().expect("finite fixture deadline")
                            + Duration::from_millis(1),
                    )
                    .await;
                    reader.references(query(Some("a"), None), limits()).await
                }
            };
            let error = result.unwrap_err();
            if !action.starts_with("registry") {
                assert!(error.source().is_some());
            }
            match action {
                "registry-before" | "registry-flight" => assert!(matches!(
                    error,
                    ImportedKnowledgeReferenceError::Reader(
                        ImportedKnowledgeReadError::SourceSelectionChanged
                    )
                )),
                "auth" => assert!(matches!(
                    error,
                    ImportedKnowledgeReferenceError::Reader(ImportedKnowledgeReadError::Auth(
                        AuthError::Denied
                    ))
                )),
                "capture" => assert!(matches!(
                    error,
                    ImportedKnowledgeReferenceError::Reader(ImportedKnowledgeReadError::Capture(_))
                )),
                _ => assert!(matches!(
                    error,
                    ImportedKnowledgeReferenceError::Reader(ImportedKnowledgeReadError::Auth(
                        AuthError::Request
                    )) | ImportedKnowledgeReferenceError::Query(
                        KnowledgeReferenceQueryError::Data(KnowledgeIndexError::Request(_))
                    )
                )),
            }
            assert_eq!(server.count("/keys"), keys);
            assert_eq!(server.count("/userinfo"), 0);
            assert_eq!(server.count("/token"), 0);
        }
    });
}

#[test]
fn long_utf8_content_and_raw_edges_preserve_admitted_query_boundaries() {
    native_test(async {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let revision = f.provider(&server, &ctx);
        let issuer = f.issuer(&ctx);
        let title = "β".repeat(256);
        let mut a = page("a", &["", "b"]);
        a.title = Some(&title);
        let mut b = page("b", &[]);
        b.title = Some(&title);
        let fields = [
            fields("a", KnowledgeDocumentState::Active, &[]),
            fields("b", KnowledgeDocumentState::Active, &[]),
        ];
        f.seed(&issuer, &ctx, &[a, b], Some(&fields)).await;
        let mut rows = sdk::rows(&f).await;
        for row in &mut rows {
            let mut cols = row.columns().to_vec();
            cols[2] = Arc::new(StringArray::from(vec!["β".repeat(131072)]));
            *row = RecordBatch::try_new(row.schema(), cols).unwrap();
        }
        sdk::replace(&f, rows).await;
        authorize!(server, f, ctx, revision, issuer, read);
        let reader = open(&read, &f, None).await;
        let exact = limits();
        let all = reader
            .references(query(Some("a"), None), exact)
            .await
            .unwrap();
        let edge = &all.outgoing.unwrap().results[0];
        assert_eq!(edge.ordinal, 1);
        assert!(all.coverage.knowledge_source_complete);
        invalid_query(
            reader
                .references(query(None, None), limits())
                .await
                .unwrap_err(),
            "focus",
        );
        let mut q = query(Some("a"), None);
        q.page_size = n(101);
        invalid_query(
            reader.references(q, limits()).await.unwrap_err(),
            "page_size",
        );
        invalid_query(
            reader
                .references(
                    query(
                        None,
                        Some(target(
                            ReferenceTargetKind::Task,
                            LogicalScope::User("user"),
                            "id",
                            Some("ctx"),
                        )),
                    ),
                    limits(),
                )
                .await
                .unwrap_err(),
            "target_context",
        );
        assert!(matches!(
            reader.references(query(Some("../a"), None), limits()).await,
            Err(ImportedKnowledgeReferenceError::Query(
                KnowledgeReferenceQueryError::Data(KnowledgeIndexError::InvalidField("slug"))
            ))
        ));
        invalid_query(
            reader
                .references(query(Some(""), None), limits())
                .await
                .unwrap_err(),
            "identifier",
        );
        let mut qlimit = limits();
        qlimit.content.max_identifier_bytes = n(2);
        invalid_query(
            reader
                .references(query(None, Some(memory())), qlimit)
                .await
                .unwrap_err(),
            "identifier",
        );
    });
}

#[test]
fn actual_sdk_read_failure_and_full_source_tuple_change_are_not_partial_results() {
    native_test(async {
        for fault in ["sdk", "source-tuple"] {
            let server = Server::new().await;
            let cancel = Cancellation::default();
            let ctx = context(&cancel);
            let f = Fixture::new(&ctx);
            let revision = f.provider(&server, &ctx);
            let issuer = f.issuer(&ctx);
            f.seed(
                &issuer,
                &ctx,
                &[page("a", &["a"])],
                Some(&[fields("a", KnowledgeDocumentState::Active, &[])]),
            )
            .await;
            authorize!(server, f, ctx, revision, issuer, read);
            let reader = open(&read, &f, None).await;
            let result = if fault == "sdk" {
                let root = f.source.directory().as_path().join("index.lance");
                let data: Vec<_> = image(&root)
                    .into_keys()
                    .filter(|p| {
                        p.starts_with("data") && p.extension().is_some_and(|e| e == "lance")
                    })
                    .collect();
                assert!(!data.is_empty(), "real SDK data files required");
                for path in data {
                    std::fs::remove_file(root.join(path)).unwrap();
                }
                reader.references(query(Some("a"), None), limits()).await
            } else {
                let (result, ()) =
                    tokio::join!(reader.references(query(Some("a"), None), limits()), async {
                        let mut entry = f.entry();
                        entry.installations[0].original_source = SourceRef::hub_cache(
                            &f.paths,
                            "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap(),
                            ArtifactType::Knowledge,
                            f.id.clone(),
                            None,
                            f.artifact_version.clone(),
                            f.registry_limits_projection(),
                        )
                        .unwrap();
                        let mut writer = f
                            .registry
                            .writer(
                                ctx.remaining().unwrap().expect("finite fixture deadline"),
                                ctx.cancellation(),
                            )
                            .unwrap();
                        writer.put_metadata(entry).unwrap();
                    });
                result
            };
            let error = result.unwrap_err();
            if fault == "sdk" {
                assert!(
                    matches!(
                        &error,
                        ImportedKnowledgeReferenceError::Query(KnowledgeReferenceQueryError::Data(
                            KnowledgeIndexError::Store(_)
                        ))
                    ),
                    "{error:?}"
                );
                assert!(error.source().unwrap().source().unwrap().source().is_some());
            } else {
                assert!(matches!(
                    error,
                    ImportedKnowledgeReferenceError::Reader(
                        ImportedKnowledgeReadError::SourceSelectionChanged
                    )
                ));
            }
        }
    });
}
