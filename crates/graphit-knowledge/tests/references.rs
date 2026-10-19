use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_knowledge::references::{
    BrokenLink, Direction, PageReferences, ReferenceError, ReferenceGraph, ReferenceHit,
    ReferenceLimits, ReferenceStats,
};
use std::{
    io,
    num::NonZeroUsize,
    time::{Duration, Instant},
};

fn cap(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

fn limits() -> ReferenceLimits {
    ReferenceLimits {
        max_identifier_bytes: cap(100),
    }
}

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}

fn page<'a>(slug: &'a str, title: &'a str, targets: &'a [&'a str]) -> PageReferences<'a> {
    PageReferences {
        slug,
        title,
        targets,
    }
}

fn error<T>(result: Result<T, ReferenceError>) -> ReferenceError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("expected a reference error"),
    }
}

fn request_error<T>(result: Result<T, ReferenceError>, kind: io::ErrorKind) {
    match error(result) {
        ReferenceError::Request(error) => assert_eq!(error.kind(), kind),
        other => panic!("expected {kind:?}, got {other}"),
    }
}

#[test]
fn resolved_corpus_preserves_dangling_destinations_and_normalizes_links() {
    let ctx = context();
    let pages = [
        page("alpha", "Alpha", &["beta", "ghost", "ghost", "alpha", ""]),
        page("beta", "", &["alpha"]),
        page("lonely", "Lonely", &[]),
    ];
    let graph = ReferenceGraph::build(&ctx, &pages, limits()).unwrap();
    assert_eq!(
        graph.stats(&ctx).unwrap(),
        ReferenceStats {
            pages: 3,
            links: 3,
            inbound_pages: 2,
            orphans: 1,
            broken_edges: 1,
        }
    );
    assert_eq!(graph.orphans(&ctx, cap(10)).unwrap(), ["lonely"]);
    assert_eq!(
        graph.broken_links(&ctx, cap(10)).unwrap(),
        [BrokenLink {
            source: "alpha",
            target: "ghost"
        }]
    );
    assert_eq!(
        graph
            .neighbors(&ctx, "alpha", Direction::Outbound, 1, cap(10))
            .unwrap(),
        [
            ReferenceHit {
                slug: "beta",
                title: Some("beta"),
                depth: 1
            },
            ReferenceHit {
                slug: "ghost",
                title: None,
                depth: 1
            },
        ]
    );
    assert_eq!(
        graph
            .neighbors(&ctx, "beta", Direction::Inbound, 1, cap(10))
            .unwrap(),
        [ReferenceHit {
            slug: "alpha",
            title: Some("Alpha"),
            depth: 1
        },]
    );
}

#[test]
fn every_broken_source_target_pair_is_reported() {
    let ctx = context();
    let pages = [
        page("beta", "B", &["ghost", "ghost"]),
        page("alpha", "A", &["ghost"]),
    ];
    let graph = ReferenceGraph::build(&ctx, &pages, limits()).unwrap();
    assert_eq!(
        graph.broken_links(&ctx, cap(2)).unwrap(),
        [
            BrokenLink {
                source: "alpha",
                target: "ghost"
            },
            BrokenLink {
                source: "beta",
                target: "ghost"
            },
        ]
    );
    assert_eq!(graph.stats(&ctx).unwrap().broken_edges, 2);
}

#[test]
fn traversal_has_fixed_direction_and_minimum_depth() {
    let ctx = context();
    let pages = [
        page("a", "A", &["b"]),
        page("b", "B", &["c"]),
        page("c", "C", &["a", "ghost"]),
    ];
    let graph = ReferenceGraph::build(&ctx, &pages, limits()).unwrap();
    for depth in 1..=3 {
        let hits = graph
            .neighbors(&ctx, "a", Direction::Outbound, depth, cap(10))
            .unwrap();
        let expected = [
            ReferenceHit {
                slug: "b",
                title: Some("B"),
                depth: 1,
            },
            ReferenceHit {
                slug: "c",
                title: Some("C"),
                depth: 2,
            },
            ReferenceHit {
                slug: "ghost",
                title: None,
                depth: 3,
            },
        ];
        assert_eq!(hits, expected[..depth as usize]);
    }
    let inbound = graph
        .neighbors(&ctx, "a", Direction::Inbound, 3, cap(10))
        .unwrap();
    assert_eq!(
        inbound,
        [
            ReferenceHit {
                slug: "c",
                title: Some("C"),
                depth: 1
            },
            ReferenceHit {
                slug: "b",
                title: Some("B"),
                depth: 2
            },
        ]
    );
    assert!(matches!(
        error(graph.neighbors(&ctx, "ghost", Direction::Inbound, 1, cap(10))),
        ReferenceError::UnknownPage
    ));
}

#[test]
fn fork_does_not_turn_outbound_walk_into_inbound_walk() {
    let ctx = context();
    let pages = [
        page("a", "A", &["b"]),
        page("b", "B", &[]),
        page("c", "C", &["b"]),
    ];
    let graph = ReferenceGraph::build(&ctx, &pages, limits()).unwrap();
    assert_eq!(
        graph
            .neighbors(&ctx, "a", Direction::Outbound, 2, cap(10))
            .unwrap(),
        [ReferenceHit {
            slug: "b",
            title: Some("B"),
            depth: 1
        },]
    );
    assert_eq!(
        graph
            .neighbors(&ctx, "b", Direction::Inbound, 2, cap(10))
            .unwrap(),
        [
            ReferenceHit {
                slug: "a",
                title: Some("A"),
                depth: 1
            },
            ReferenceHit {
                slug: "c",
                title: Some("C"),
                depth: 1
            },
        ]
    );
}

#[test]
fn output_order_does_not_depend_on_input_order() {
    let ctx = context();
    let first = [
        page("a", "A", &["c", "b", "ghost"]),
        page("b", "B", &["ghost"]),
        page("c", "C", &[]),
    ];
    let second = [
        page("c", "C", &[]),
        page("b", "B", &["ghost"]),
        page("a", "A", &["ghost", "b", "c", "b"]),
    ];
    let a = ReferenceGraph::build(&ctx, &first, limits()).unwrap();
    let b = ReferenceGraph::build(&ctx, &second, limits()).unwrap();
    assert_eq!(a.stats(&ctx).unwrap(), b.stats(&ctx).unwrap());
    assert_eq!(
        a.broken_links(&ctx, cap(10)).unwrap(),
        b.broken_links(&ctx, cap(10)).unwrap()
    );
    assert_eq!(
        a.orphans(&ctx, cap(10)).unwrap(),
        b.orphans(&ctx, cap(10)).unwrap()
    );
    assert_eq!(
        a.neighbors(&ctx, "a", Direction::Outbound, 3, cap(10))
            .unwrap(),
        b.neighbors(&ctx, "a", Direction::Outbound, 3, cap(10))
            .unwrap()
    );
}

#[test]
fn individual_utf8_identifier_limits_accept_then_reject_one_less() {
    let ctx = context();
    let pages = [page("é", "é", &["é", "", "ghost"])];
    let exact = ReferenceLimits {
        max_identifier_bytes: cap(5),
    };
    let graph = ReferenceGraph::build(&ctx, &pages, exact).unwrap();
    assert_eq!(graph.stats(&ctx).unwrap().links, 1);
    let reduced = ReferenceLimits {
        max_identifier_bytes: cap(4),
    };
    assert!(matches!(
        error(ReferenceGraph::build(&ctx, &pages, reduced)),
        ReferenceError::LimitExceeded("identifier bytes")
    ));
    let unicode = [page("é", "", &[])];
    ReferenceGraph::build(
        &ctx,
        &unicode,
        ReferenceLimits {
            max_identifier_bytes: cap(2),
        },
    )
    .unwrap();
    assert!(matches!(
        error(ReferenceGraph::build(
            &ctx,
            &unicode,
            ReferenceLimits {
                max_identifier_bytes: cap(1),
            }
        )),
        ReferenceError::LimitExceeded("identifier bytes")
    ));
}

#[test]
fn duplicate_identity_fails_without_overwriting() {
    let ctx = context();
    let duplicate = [page("a", "A", &[]), page("a", "Other", &["b"])];
    assert!(matches!(
        error(ReferenceGraph::build(&ctx, &duplicate, limits())),
        ReferenceError::DuplicatePage
    ));
}

#[test]
fn identifiers_are_validated_for_pages_targets_and_queries() {
    let ctx = context();
    let good = [page("a", "A", &[])];
    let graph = ReferenceGraph::build(&ctx, &good, limits()).unwrap();
    for invalid in ["", "..", "..escape", "a/b", "a\\b", "a\n", "a\0"] {
        assert!(matches!(
            error(ReferenceGraph::build(
                &ctx,
                &[page(invalid, "", &[])],
                limits()
            )),
            ReferenceError::InvalidIdentifier
        ));
        if !invalid.is_empty() {
            assert!(matches!(
                error(ReferenceGraph::build(
                    &ctx,
                    &[page("a", "", &[invalid])],
                    limits()
                )),
                ReferenceError::InvalidIdentifier
            ));
        }
        assert!(matches!(
            error(graph.neighbors(&ctx, invalid, Direction::Outbound, 1, cap(10))),
            ReferenceError::InvalidIdentifier
        ));
    }
    let excessive = "x".repeat(101);
    assert!(matches!(
        error(graph.neighbors(&ctx, &excessive, Direction::Outbound, 1, cap(10))),
        ReferenceError::LimitExceeded("identifier bytes")
    ));
    assert!(matches!(
        error(graph.neighbors(&ctx, "missing", Direction::Outbound, 1, cap(10))),
        ReferenceError::UnknownPage
    ));
    for depth in [0, 4, u8::MAX] {
        assert!(matches!(
            error(graph.neighbors(&ctx, "a", Direction::Outbound, depth, cap(10))),
            ReferenceError::InvalidDepth
        ));
    }
}

#[test]
fn output_limits_fail_whole_query_and_allow_exact_result_count() {
    let ctx = context();
    let orphan_pages = [page("a", "A", &[]), page("b", "B", &[])];
    let orphans = ReferenceGraph::build(&ctx, &orphan_pages, limits()).unwrap();
    assert_eq!(orphans.orphans(&ctx, cap(2)).unwrap(), ["a", "b"]);
    assert!(matches!(
        error(orphans.orphans(&ctx, cap(1))),
        ReferenceError::LimitExceeded("results")
    ));
    let broken_pages = [page("a", "A", &["ghost1", "ghost2"])];
    let broken = ReferenceGraph::build(&ctx, &broken_pages, limits()).unwrap();
    assert_eq!(broken.broken_links(&ctx, cap(2)).unwrap().len(), 2);
    assert!(matches!(
        error(broken.broken_links(&ctx, cap(1))),
        ReferenceError::LimitExceeded("results")
    ));
    assert!(matches!(
        error(broken.neighbors(&ctx, "a", Direction::Outbound, 1, cap(1))),
        ReferenceError::LimitExceeded("results")
    ));
    assert_eq!(
        broken
            .neighbors(&ctx, "a", Direction::Outbound, 1, cap(2))
            .unwrap()
            .len(),
        2
    );
    assert_eq!(broken.stats(&ctx).unwrap().links, 2);
}

#[test]
fn cancellation_prevents_build_and_every_existing_graph_query() {
    let ctx = context();
    let pages = [page("a", "A", &["ghost"])];
    let graph = ReferenceGraph::build(&ctx, &pages, limits()).unwrap();
    ctx.cancellation().cancel();
    request_error(
        ReferenceGraph::build(&ctx, &pages, limits()),
        io::ErrorKind::Interrupted,
    );
    request_error(graph.stats(&ctx), io::ErrorKind::Interrupted);
    request_error(graph.orphans(&ctx, cap(10)), io::ErrorKind::Interrupted);
    request_error(
        graph.broken_links(&ctx, cap(10)),
        io::ErrorKind::Interrupted,
    );
    request_error(
        graph.neighbors(&ctx, "a", Direction::Outbound, 1, cap(10)),
        io::ErrorKind::Interrupted,
    );
}

#[test]
fn expired_original_deadline_is_not_renewed_by_a_new_context() {
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    request_error(
        ReferenceGraph::build(&expired, &[], limits()),
        io::ErrorKind::TimedOut,
    );
    let ctx = SnapshotContext::new(
        Instant::now() + Duration::from_secs(1),
        Cancellation::default(),
    );
    let pages = [page("a", "A", &[])];
    let graph = ReferenceGraph::build(&ctx, &pages, limits()).unwrap();
    std::thread::sleep(
        ctx.deadline()
            .expect("finite fixture deadline")
            .saturating_duration_since(Instant::now()),
    );
    request_error(graph.stats(&context()), io::ErrorKind::TimedOut);
    request_error(graph.orphans(&ctx, cap(10)), io::ErrorKind::TimedOut);
    request_error(graph.broken_links(&ctx, cap(10)), io::ErrorKind::TimedOut);
    request_error(
        graph.neighbors(&ctx, "a", Direction::Outbound, 1, cap(10)),
        io::ErrorKind::TimedOut,
    );
}

#[test]
fn equal_deadline_and_signal_are_not_identity() {
    let ctx = std::sync::Arc::new(context());
    let impostor = SnapshotContext::new(
        ctx.deadline().expect("finite fixture deadline"),
        ctx.cancellation().clone(),
    );
    let pages = [page("a", "A", &[])];
    let graph = ReferenceGraph::build(&ctx, &pages, limits()).unwrap();
    assert!(matches!(
        error(graph.stats(&impostor)),
        ReferenceError::DifferentRequest
    ));
    assert!(matches!(
        error(graph.orphans(&impostor, cap(10))),
        ReferenceError::DifferentRequest
    ));
    assert!(matches!(
        error(graph.broken_links(&impostor, cap(10))),
        ReferenceError::DifferentRequest
    ));
    assert!(matches!(
        error(graph.neighbors(&impostor, "a", Direction::Inbound, 1, cap(10))),
        ReferenceError::DifferentRequest
    ));
    let same = std::sync::Arc::clone(&ctx);
    assert_eq!(graph.stats(&same).unwrap().pages, 1);
    assert_eq!(graph.orphans(&ctx, cap(10)).unwrap(), ["a"]);
}

#[test]
fn empty_corpus_is_valid_producer_input_and_self_only_page_is_orphan() {
    let ctx = context();
    let empty = ReferenceGraph::build(&ctx, &[], limits()).unwrap();
    assert_eq!(empty.stats(&ctx).unwrap(), ReferenceStats::default());
    assert!(empty.orphans(&ctx, cap(1)).unwrap().is_empty());
    let pages = [page("self", "", &["self"])];
    let graph = ReferenceGraph::build(&ctx, &pages, limits()).unwrap();
    assert_eq!(graph.orphans(&ctx, cap(1)).unwrap(), ["self"]);
    assert!(
        graph
            .neighbors(&ctx, "self", Direction::Outbound, 3, cap(1))
            .unwrap()
            .is_empty()
    );
}
