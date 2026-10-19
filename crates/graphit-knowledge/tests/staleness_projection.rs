use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::knowledge_document_projection::*;
use graphit_knowledge::{
    references::{Direction, ReferenceError, ReferenceLimits},
    staleness::{RootCause, StalePage, StaleReason, StalenessError, StalenessLimits},
    staleness_projection::*,
};
use std::{
    error::Error,
    io,
    num::NonZeroUsize,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn n(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}
fn time(value: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(value)
}
fn request() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}
fn input_limits() -> KnowledgeDocumentProjectionLimits {
    KnowledgeDocumentProjectionLimits {
        max_documents: n(100),
        max_observations: n(100),
        max_rows: n(1_000),
        max_field_bytes: n(10_000),
        max_text_bytes: n(1_000_000),
    }
}
fn limits() -> StalenessProjectionLimits {
    StalenessProjectionLimits {
        references: ReferenceLimits {
            max_identifier_bytes: n(100),
        },
        staleness: StalenessLimits {
            max_pages: n(100),
            max_graph_edges: n(100),
            max_observations: n(100),
            max_path_bytes: n(100),
            max_metadata_bytes: n(10_000),
            max_results: n(100),
        },
    }
}
fn recipe(max: usize) -> RecipeFields<'static> {
    RecipeFields {
        id: AUTHORED_RECIPE_ID,
        parameters: Some(AuthoredParameters {
            fallback_title: "Fallback",
            kind_on_absent: "page",
            max_summary_bytes: n(max),
        }),
    }
}
fn page<'a>(slug: &'a str, path: &'a str, targets: &'a [&'a str]) -> AuthoredPageFields<'a> {
    AuthoredPageFields {
        slug: Some(slug),
        title: Some(slug),
        body: Some("body"),
        kind: Some("page"),
        summary: Some("summary"),
        description: None,
        summary_truncated: false,
        source_path: path,
        physical_digest: Some(PhysicalSourceDigest32::from_bytes([9; 32])),
        compilation_digest: CompilationDigest32::from_bytes([1; 32]),
        recipe: recipe(8),
        updated_at: Some(time(100)),
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: targets,
    }
}
fn observation<'a>(path: &'a str, digest: u8) -> CompilationObservationFields<'a> {
    CompilationObservationFields {
        path,
        state: CompilationObservationState::Present {
            compilation_digest: CompilationDigest32::from_bytes([digest; 32]),
            recipe: recipe(8),
        },
    }
}
fn corpus() -> [AuthoredPageFields<'static>; 6] {
    [
        page("a", "docs/a.md", &["b", "b", "missing", "a", ""]),
        page("b", "docs/b.md", &["c"]),
        page("c", "docs/c.md", &[]),
        page("d", "docs/d.md", &["e"]),
        page("e", "docs/e.md", &[]),
        page("z", "docs/z.md", &[]),
    ]
}
fn observations() -> [CompilationObservationFields<'static>; 6] {
    [
        "docs/a.md",
        "docs/b.md",
        "docs/c.md",
        "docs/d.md",
        "docs/e.md",
        "docs/z.md",
    ]
    .map(|path| observation(path, 1))
}
fn removed_c() -> [CompilationObservationFields<'static>; 6] {
    let mut values = observations();
    values[2].state = CompilationObservationState::Removed;
    values
}
fn error(
    projection: &PreparedKnowledgeDocumentRows<'_, '_>,
    context: &SnapshotContext,
    limits: StalenessProjectionLimits,
) -> StalenessProjectionError {
    assess_projection(context, projection, time(200), limits)
        .err()
        .expect("expected failure")
}
fn request_error(error: StalenessProjectionError, kind: io::ErrorKind) {
    let first = error.source().expect("typed cause");
    let io = first
        .source()
        .expect("original io cause")
        .downcast_ref::<io::Error>()
        .unwrap();
    assert_eq!(io.kind(), kind);
}

#[test]
fn real_projection_builds_normalized_graph_and_sparse_changed_report() {
    let context = request();
    let documents = corpus();
    let mut observed = observations();
    observed[0] = observation("docs/a.md", 2);
    let projection = prepare(&context, &documents, &observed, input_limits()).unwrap();
    let assessment = assess_projection(&context, &projection, time(200), limits()).unwrap();
    let graph = assessment.graph(&context).unwrap();
    let stats = graph.stats(&context).unwrap();
    assert_eq!(stats.pages, 6);
    assert_eq!(stats.links, 4);
    assert_eq!(stats.broken_edges, 1);
    let outgoing = graph
        .neighbors(&context, "a", Direction::Outbound, 1, n(10))
        .unwrap();
    assert_eq!(
        outgoing.iter().map(|hit| hit.slug).collect::<Vec<_>>(),
        ["b", "missing"]
    );
    assert_eq!(
        assessment.stale_pages(&context).unwrap(),
        [StalePage {
            slug: "a",
            since: time(200),
            reason: StaleReason::SourceChanged { path: "docs/a.md" }
        }]
    );
    assert!(std::ptr::eq(
        outgoing[0].slug.as_ptr(),
        documents[0].resolved_wiki_targets[0].as_ptr()
    ));
}

#[test]
fn explicit_removal_propagates_inbound_and_missing_observation_is_an_error() {
    let context = request();
    let documents = corpus();
    let observed = removed_c();
    let projection = prepare(&context, &documents, &observed, input_limits()).unwrap();
    let assessment = assess_projection(&context, &projection, time(200), limits()).unwrap();
    let dependency = StaleReason::Dependency {
        root_slug: "c",
        root_cause: RootCause::SourceRemoved { path: "docs/c.md" },
    };
    assert_eq!(
        assessment.stale_pages(&context).unwrap(),
        [
            StalePage {
                slug: "a",
                since: time(200),
                reason: dependency
            },
            StalePage {
                slug: "b",
                since: time(200),
                reason: dependency
            },
            StalePage {
                slug: "c",
                since: time(200),
                reason: StaleReason::SourceRemoved { path: "docs/c.md" }
            },
        ]
    );
    let missing = [
        observed[0],
        observed[1],
        observed[3],
        observed[4],
        observed[5],
    ];
    let incomplete = prepare(&context, &documents, &missing, input_limits()).unwrap();
    assert!(matches!(
        error(&incomplete, &context, limits()),
        StalenessProjectionError::Staleness(StalenessError::UnobservedSource)
    ));
}

#[test]
fn configuration_drift_uses_compilation_digest_and_never_physical_digest() {
    let context = request();
    let documents = [page("a", "docs/a.md", &[])];
    let mut observed = [observation("docs/a.md", 2)];
    if let CompilationObservationState::Present { recipe, .. } = &mut observed[0].state {
        *recipe = crate::recipe(16);
    }
    let projection = prepare(&context, &documents, &observed, input_limits()).unwrap();
    let assessment = assess_projection(&context, &projection, time(200), limits()).unwrap();
    assert!(matches!(
        assessment.stale_pages(&context).unwrap()[0].reason,
        StaleReason::SourceChanged { .. }
    ));
    observed[0].state = CompilationObservationState::Present {
        compilation_digest: CompilationDigest32::from_bytes([1; 32]),
        recipe: recipe(16),
    };
    let same = prepare(&context, &documents, &observed, input_limits()).unwrap();
    assert!(
        assess_projection(&context, &same, time(200), limits())
            .unwrap()
            .stale_pages(&context)
            .unwrap()
            .is_empty()
    );
    let mut changed_physical = documents;
    changed_physical[0].physical_digest = Some(PhysicalSourceDigest32::from_bytes([3; 32]));
    let same = prepare(&context, &changed_physical, &observed, input_limits()).unwrap();
    assert!(
        assess_projection(&context, &same, time(200), limits())
            .unwrap()
            .stale_pages(&context)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn age_uses_expiry_boundary_and_existing_since_only_when_a_cause_exists() {
    let context = request();
    let mut documents = [page("a", "docs/a.md", &[])];
    documents[0].stale_after = Some(Duration::from_secs(50));
    let observed = [observation("docs/a.md", 1)];
    let projection = prepare(&context, &documents, &observed, input_limits()).unwrap();
    let assessment = assess_projection(&context, &projection, time(200), limits()).unwrap();
    assert_eq!(
        assessment.stale_pages(&context).unwrap(),
        [StalePage {
            slug: "a",
            since: time(150),
            reason: StaleReason::Age
        }]
    );
    assert!(
        assess_projection(&context, &projection, time(149), limits())
            .unwrap()
            .stale_pages(&context)
            .unwrap()
            .is_empty()
    );
    documents[0].updated_at = Some(time(150));
    let projection = prepare(&context, &documents, &observed, input_limits()).unwrap();
    assert_eq!(
        assess_projection(&context, &projection, time(200), limits())
            .unwrap()
            .stale_pages(&context)
            .unwrap()[0]
            .since,
        time(200)
    );
    documents[0].updated_at = Some(time(100));
    documents[0].stale_since = Some(time(120));
    let projection = prepare(&context, &documents, &observed, input_limits()).unwrap();
    assert_eq!(
        assess_projection(&context, &projection, time(200), limits())
            .unwrap()
            .stale_pages(&context)
            .unwrap()[0]
            .since,
        time(120)
    );
    documents[0].stale_after = None;
    let projection = prepare(&context, &documents, &observed, input_limits()).unwrap();
    assert!(
        assess_projection(&context, &projection, time(200), limits())
            .unwrap()
            .stale_pages(&context)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn source_change_precedes_age_without_waiving_future_timestamp_validation() {
    let context = request();
    let mut documents = [page("a", "docs/a.md", &[])];
    documents[0].stale_after = Some(Duration::from_secs(50));
    let observed = [observation("docs/a.md", 2)];
    let projection = prepare(&context, &documents, &observed, input_limits()).unwrap();
    assert_eq!(
        assess_projection(&context, &projection, time(200), limits())
            .unwrap()
            .stale_pages(&context)
            .unwrap(),
        [StalePage {
            slug: "a",
            since: time(200),
            reason: StaleReason::SourceChanged { path: "docs/a.md" }
        }]
    );
    for future_update in [true, false] {
        let mut invalid = documents;
        if future_update {
            invalid[0].updated_at = Some(time(201));
        } else {
            invalid[0].stale_since = Some(time(201));
        }
        let projection = prepare(&context, &invalid, &observed, input_limits()).unwrap();
        assert!(matches!(
            error(&projection, &context, limits()),
            StalenessProjectionError::Staleness(StalenessError::InvalidTimestamp)
        ));
    }
}

#[test]
fn duplicates_invalid_identifiers_and_paths_preserve_kernel_errors() {
    let context = request();
    let documents = [page("a", "docs/a.md", &[]), page("a", "docs/b.md", &[])];
    let observed = [observation("docs/a.md", 1), observation("docs/b.md", 1)];
    let projection = prepare(&context, &documents, &observed, input_limits()).unwrap();
    let duplicate = error(&projection, &context, limits());
    assert!(matches!(
        duplicate,
        StalenessProjectionError::References(ReferenceError::DuplicatePage)
    ));
    assert!(
        duplicate
            .source()
            .unwrap()
            .downcast_ref::<ReferenceError>()
            .is_some()
    );
    let documents = [page("a", "docs/a.md", &[])];
    let observed = [observation("docs/a.md", 1), observation("docs/a.md", 1)];
    let projection = prepare(&context, &documents, &observed, input_limits()).unwrap();
    assert!(matches!(
        error(&projection, &context, limits()),
        StalenessProjectionError::Staleness(StalenessError::DuplicateObservation)
    ));
    for bad in ["../a", "a/b", "a\nb"] {
        let documents = [page(bad, "docs/a.md", &[])];
        let observed = [observation("docs/a.md", 1)];
        let projection = prepare(&context, &documents, &observed, input_limits()).unwrap();
        assert!(matches!(
            error(&projection, &context, limits()),
            StalenessProjectionError::References(ReferenceError::InvalidIdentifier)
        ));
    }
    for bad in ["../a.md", "docs//a.md", "C:/a.md"] {
        let documents = [page("a", bad, &[])];
        let observed = [observation(bad, 1)];
        let projection = prepare(&context, &documents, &observed, input_limits()).unwrap();
        assert!(matches!(
            error(&projection, &context, limits()),
            StalenessProjectionError::Staleness(StalenessError::InvalidPath)
        ));
    }
    let documents = [page("a", "docs/a.md", &["bad/path"])];
    let observed = [observation("docs/a.md", 1)];
    let projection = prepare(&context, &documents, &observed, input_limits()).unwrap();
    assert!(matches!(
        error(&projection, &context, limits()),
        StalenessProjectionError::References(ReferenceError::InvalidIdentifier)
    ));
}

#[test]
fn real_kernel_limits_allow_equality_and_reject_one_below() {
    let context = request();
    let documents = corpus();
    let observed = removed_c();
    let projection = prepare(&context, &documents, &observed, input_limits()).unwrap();
    let exact = StalenessProjectionLimits {
        references: ReferenceLimits {
            max_identifier_bytes: n(7),
        },
        staleness: StalenessLimits {
            max_pages: n(6),
            max_graph_edges: n(4),
            max_observations: n(6),
            max_path_bytes: n(9),
            max_metadata_bytes: n(114),
            max_results: n(3),
        },
    };
    assert_eq!(
        assess_projection(&context, &projection, time(200), exact)
            .unwrap()
            .stale_pages(&context)
            .unwrap()
            .len(),
        3
    );
    type BoundCase = (&'static str, fn(&mut StalenessProjectionLimits), bool);
    let cases: [BoundCase; 7] = [
        (
            "identifier bytes",
            |v| v.references.max_identifier_bytes = n(6),
            true,
        ),
        ("pages", |v| v.staleness.max_pages = n(5), false),
        ("graph edges", |v| v.staleness.max_graph_edges = n(3), false),
        (
            "observations",
            |v| v.staleness.max_observations = n(5),
            false,
        ),
        ("path bytes", |v| v.staleness.max_path_bytes = n(8), false),
        (
            "metadata bytes",
            |v| v.staleness.max_metadata_bytes = n(113),
            false,
        ),
        ("results", |v| v.staleness.max_results = n(2), false),
    ];
    for (boundary, change, references) in cases {
        let mut small = exact;
        change(&mut small);
        let failure = error(&projection, &context, small);
        let actual = match failure {
            StalenessProjectionError::References(ReferenceError::LimitExceeded(actual))
                if references =>
            {
                actual
            }
            StalenessProjectionError::Staleness(StalenessError::LimitExceeded(actual))
                if !references =>
            {
                actual
            }
            other => panic!("wrong limit error for {boundary}: {other}"),
        };
        assert_eq!(actual, boundary);
    }
}

#[test]
fn original_request_cancellation_expiry_and_foreign_context_reach_real_apis() {
    let cancellation = Cancellation::default();
    let context = SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        cancellation.clone(),
    );
    let documents = corpus();
    let observed = observations();
    let projection = prepare(&context, &documents, &observed, input_limits()).unwrap();
    let assessment = assess_projection(&context, &projection, time(200), limits()).unwrap();
    let foreign = request();
    assert!(matches!(
        error(&projection, &foreign, limits()),
        StalenessProjectionError::Projection(KnowledgeDocumentProjectionError::DifferentRequest)
    ));
    assert!(matches!(
        assessment.graph(&foreign),
        Err(StalenessProjectionError::Projection(
            KnowledgeDocumentProjectionError::DifferentRequest
        ))
    ));
    assert!(matches!(
        assessment.stale_pages(&foreign),
        Err(StalenessProjectionError::Projection(
            KnowledgeDocumentProjectionError::DifferentRequest
        ))
    ));
    cancellation.cancel();
    request_error(
        error(&projection, &context, limits()),
        io::ErrorKind::Interrupted,
    );
    request_error(
        assessment.graph(&context).err().unwrap(),
        io::ErrorKind::Interrupted,
    );
    request_error(
        assessment.stale_pages(&context).err().unwrap(),
        io::ErrorKind::Interrupted,
    );

    let deadline = Instant::now() + Duration::from_secs(1);
    let expiring = SnapshotContext::new(deadline, Cancellation::default());
    let projection = prepare(&expiring, &documents, &observed, input_limits()).unwrap();
    let assessment = assess_projection(&expiring, &projection, time(200), limits()).unwrap();
    std::thread::sleep(
        deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
    );
    request_error(
        error(&projection, &expiring, limits()),
        io::ErrorKind::TimedOut,
    );
    request_error(
        assessment.graph(&expiring).err().unwrap(),
        io::ErrorKind::TimedOut,
    );
    request_error(
        assessment.stale_pages(&expiring).err().unwrap(),
        io::ErrorKind::TimedOut,
    );
}

#[test]
fn empty_projection_is_a_valid_logical_assessment() {
    let context = request();
    let projection = prepare(&context, &[], &[], input_limits()).unwrap();
    let assessment = assess_projection(&context, &projection, time(200), limits()).unwrap();
    assert!(assessment.stale_pages(&context).unwrap().is_empty());
    assert_eq!(
        assessment
            .graph(&context)
            .unwrap()
            .stats(&context)
            .unwrap()
            .pages,
        0
    );
}
