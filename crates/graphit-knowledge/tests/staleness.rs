use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_knowledge::{
    references::{PageReferences, ReferenceError, ReferenceGraph, ReferenceLimits},
    staleness::{
        PublishedPage, RootCause, SourceFingerprint, SourceObservation, SourceState, StalePage,
        StaleReason, StalenessError, StalenessLimits, assess,
    },
};
use std::{
    io,
    num::NonZeroUsize,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn cap(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}
fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn limits() -> StalenessLimits {
    StalenessLimits {
        max_pages: cap(100),
        max_graph_edges: cap(1000),
        max_observations: cap(100),
        max_path_bytes: cap(100),
        max_metadata_bytes: cap(10000),
        max_results: cap(100),
    }
}
fn refs<'a>(slug: &'a str, targets: &'a [&'a str]) -> PageReferences<'a> {
    PageReferences {
        slug,
        title: slug,
        targets,
    }
}
fn graph<'c, 'a>(ctx: &'c SnapshotContext, pages: &[PageReferences<'a>]) -> ReferenceGraph<'c, 'a> {
    ReferenceGraph::build(
        ctx,
        pages,
        ReferenceLimits {
            max_identifier_bytes: cap(100),
        },
    )
    .unwrap()
}
fn page(slug: &str) -> PublishedPage<'_> {
    PublishedPage {
        slug,
        source: None,
        updated_at: None,
        stale_after: None,
        stale_since: None,
    }
}
fn sourced<'a>(slug: &'a str, path: &'a str) -> PublishedPage<'a> {
    PublishedPage {
        source: Some(SourceFingerprint {
            path,
            digest: [1; 32],
        }),
        ..page(slug)
    }
}
fn changed(path: &str) -> SourceObservation<'_> {
    SourceObservation {
        path,
        state: SourceState::Present([2; 32]),
    }
}
fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1000)
}
fn error<T>(result: Result<T, StalenessError>) -> StalenessError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("expected staleness error"),
    }
}
fn dependency<'a>(slug: &'a str, root_slug: &'a str, root_cause: RootCause<'a>) -> StalePage<'a> {
    StalePage {
        slug,
        since: now(),
        reason: StaleReason::Dependency {
            root_slug,
            root_cause,
        },
    }
}

#[test]
fn source_drift_propagates_inbound_and_consistent_publication_clears_old_stale() {
    let ctx = context();
    let graph = graph(
        &ctx,
        &[refs("a", &["b"]), refs("b", &["c"]), refs("c", &[])],
    );
    let mut pages = [page("b"), sourced("c", "c.md"), page("a")];
    let observations = [changed("c.md")];
    let cause = RootCause::SourceChanged { path: "c.md" };
    assert_eq!(
        assess(&ctx, &graph, &pages, &observations, now(), limits()).unwrap(),
        [
            dependency("a", "c", cause),
            dependency("b", "c", cause),
            StalePage {
                slug: "c",
                since: now(),
                reason: StaleReason::SourceChanged { path: "c.md" }
            }
        ]
    );
    for page in &mut pages {
        page.stale_since = Some(UNIX_EPOCH);
    }
    let prior = assess(&ctx, &graph, &pages, &observations, now(), limits()).unwrap();
    assert!(prior.iter().all(|page| page.since == UNIX_EPOCH));
    pages[1].source.as_mut().unwrap().digest = [2; 32];
    assert!(
        assess(&ctx, &graph, &pages, &observations, now(), limits())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn removed_is_explicit_and_missing_observation_fails_the_whole_request() {
    let ctx = context();
    let graph = graph(
        &ctx,
        &[refs("a", &["b"]), refs("b", &["c"]), refs("c", &[])],
    );
    let pages = [page("a"), page("b"), sourced("c", "c.md")];
    let removed = [SourceObservation {
        path: "c.md",
        state: SourceState::Removed,
    }];
    assert_eq!(
        assess(&ctx, &graph, &pages, &removed, now(), limits()).unwrap(),
        [
            dependency("a", "c", RootCause::SourceRemoved { path: "c.md" }),
            dependency("b", "c", RootCause::SourceRemoved { path: "c.md" }),
            StalePage {
                slug: "c",
                since: now(),
                reason: StaleReason::SourceRemoved { path: "c.md" }
            }
        ]
    );
    assert!(matches!(
        error(assess(&ctx, &graph, &pages, &[], now(), limits())),
        StalenessError::UnobservedSource
    ));
}

#[test]
fn cycles_normalized_links_and_dangling_destinations_terminate_with_known_pages_only() {
    let ctx = context();
    let graph = graph(
        &ctx,
        &[
            refs("a", &["b", "a", "b", "ghost"]),
            refs("b", &["c"]),
            refs("c", &["a"]),
        ],
    );
    let result = assess(
        &ctx,
        &graph,
        &[page("a"), page("b"), sourced("c", "c.md")],
        &[changed("c.md")],
        now(),
        limits(),
    )
    .unwrap();
    assert_eq!(
        result.iter().map(|p| p.slug).collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
}

#[test]
fn lexical_root_walks_through_other_direct_roots_without_replacing_their_reason() {
    let ctx = context();
    let graph = graph(
        &ctx,
        &[
            refs("x", &["z"]),
            refs("z", &["a"]),
            refs("a", &[]),
            refs("y", &["a", "z"]),
        ],
    );
    let mut pages = [
        sourced("z", "z.md"),
        page("y"),
        sourced("a", "a.md"),
        page("x"),
    ];
    let mut observations = [changed("z.md"), changed("a.md")];
    let result = assess(&ctx, &graph, &pages, &observations, now(), limits()).unwrap();
    assert_eq!(
        result,
        [
            StalePage {
                slug: "a",
                since: now(),
                reason: StaleReason::SourceChanged { path: "a.md" }
            },
            dependency("x", "a", RootCause::SourceChanged { path: "a.md" }),
            dependency("y", "a", RootCause::SourceChanged { path: "a.md" }),
            StalePage {
                slug: "z",
                since: now(),
                reason: StaleReason::SourceChanged { path: "z.md" }
            }
        ]
    );
    pages.reverse();
    observations.reverse();
    assert_eq!(
        assess(&ctx, &graph, &pages, &observations, now(), limits()).unwrap(),
        result
    );
}

#[test]
fn propagation_exceeds_presentation_depth_three() {
    let ctx = context();
    let graph = graph(
        &ctx,
        &[
            refs("a", &["b"]),
            refs("b", &["c"]),
            refs("c", &["d"]),
            refs("d", &["e"]),
            refs("e", &["f"]),
            refs("f", &[]),
        ],
    );
    let pages = [
        page("a"),
        page("b"),
        page("c"),
        page("d"),
        page("e"),
        sourced("f", "f.md"),
    ];
    let result = assess(&ctx, &graph, &pages, &[changed("f.md")], now(), limits()).unwrap();
    assert_eq!(result.len(), 6);
    assert_eq!(
        result[0],
        dependency("a", "f", RootCause::SourceChanged { path: "f.md" })
    );
}

#[test]
fn output_limits_preserve_chain_and_fresh_cache_validation() {
    let ctx = context();
    let graph = graph(
        &ctx,
        &[refs("a", &["b"]), refs("b", &["c"]), refs("c", &[])],
    );
    let pages = [page("a"), page("b"), sourced("c", "c.md")];
    let mut exact = limits();
    exact.max_results = cap(3);
    assert_eq!(
        assess(&ctx, &graph, &pages, &[changed("c.md")], now(), exact)
            .unwrap()
            .len(),
        3
    );
    exact.max_results = cap(2);
    assert!(matches!(
        error(assess(
            &ctx,
            &graph,
            &pages,
            &[changed("c.md")],
            now(),
            exact
        )),
        StalenessError::LimitExceeded("results")
    ));
    assert!(
        assess(
            &ctx,
            &graph,
            &pages,
            &[SourceObservation {
                path: "c.md",
                state: SourceState::Present([1; 32])
            }],
            now(),
            exact
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn explicit_age_clock_uses_expiry_for_root_and_current_clock_for_dependents() {
    let ctx = context();
    let graph = graph(&ctx, &[refs("a", &["b"]), refs("b", &[])]);
    let updated = now() - Duration::from_secs(10);
    let expiry = now();
    let mut pages = [
        page("a"),
        PublishedPage {
            updated_at: Some(updated),
            stale_after: Some(Duration::from_secs(10)),
            ..page("b")
        },
    ];
    assert!(
        assess(
            &ctx,
            &graph,
            &pages,
            &[],
            expiry - Duration::from_secs(1),
            limits()
        )
        .unwrap()
        .is_empty()
    );
    let exact = assess(&ctx, &graph, &pages, &[], expiry, limits()).unwrap();
    assert_eq!(
        exact,
        [
            dependency("a", "b", RootCause::Age),
            StalePage {
                slug: "b",
                since: expiry,
                reason: StaleReason::Age
            }
        ]
    );
    let later = expiry + Duration::from_secs(20);
    let result = assess(&ctx, &graph, &pages, &[], later, limits()).unwrap();
    assert_eq!(result[0].since, later);
    assert_eq!(result[1].since, expiry);
    pages[0].stale_since = Some(UNIX_EPOCH);
    pages[1].stale_since = Some(updated);
    let result = assess(&ctx, &graph, &pages, &[], later, limits()).unwrap();
    assert_eq!(result[0].since, UNIX_EPOCH);
    assert_eq!(result[1].since, updated);
}

#[test]
fn source_reason_wins_age_but_never_hides_invalid_clock_or_age() {
    let ctx = context();
    let graph = graph(&ctx, &[refs("a", &[])]);
    let base = PublishedPage {
        updated_at: Some(UNIX_EPOCH),
        stale_after: Some(Duration::from_secs(1)),
        ..sourced("a", "a.md")
    };
    for state in [SourceState::Present([2; 32]), SourceState::Removed] {
        let obs = [SourceObservation {
            path: "a.md",
            state,
        }];
        let result = assess(&ctx, &graph, &[base], &obs, now(), limits()).unwrap();
        assert_eq!(result[0].since, now());
        assert_eq!(
            result[0].reason,
            match state {
                SourceState::Removed => StaleReason::SourceRemoved { path: "a.md" },
                _ => StaleReason::SourceChanged { path: "a.md" },
            }
        );
        for bad in [
            PublishedPage {
                updated_at: Some(now() + Duration::from_secs(1)),
                ..base
            },
            PublishedPage {
                stale_since: Some(now() + Duration::from_secs(1)),
                ..base
            },
            PublishedPage {
                stale_after: Some(Duration::MAX),
                ..base
            },
        ] {
            assert!(matches!(
                error(assess(&ctx, &graph, &[bad], &obs, now(), limits())),
                StalenessError::InvalidTimestamp
            ));
        }
        for bad in [
            PublishedPage {
                updated_at: None,
                ..base
            },
            PublishedPage {
                stale_after: Some(Duration::ZERO),
                ..base
            },
        ] {
            assert!(matches!(
                error(assess(&ctx, &graph, &[bad], &obs, now(), limits())),
                StalenessError::InvalidAgeLimit
            ));
        }
    }
}

#[test]
fn clock_before_epoch_is_valid_and_future_metadata_fails_even_on_fresh_pages() {
    let ctx = context();
    let graph = graph(&ctx, &[refs("a", &[])]);
    let before = UNIX_EPOCH - Duration::from_secs(100);
    let published = PublishedPage {
        updated_at: Some(before),
        stale_after: Some(Duration::from_secs(10)),
        ..page("a")
    };
    assert_eq!(
        assess(&ctx, &graph, &[published], &[], UNIX_EPOCH, limits()).unwrap()[0].since,
        before + Duration::from_secs(10)
    );
    for bad in [
        PublishedPage {
            updated_at: Some(now() + Duration::from_secs(1)),
            ..page("a")
        },
        PublishedPage {
            stale_since: Some(now() + Duration::from_secs(1)),
            ..page("a")
        },
    ] {
        assert!(matches!(
            error(assess(&ctx, &graph, &[bad], &[], now(), limits())),
            StalenessError::InvalidTimestamp
        ));
    }
}

#[test]
fn corpus_requires_exact_unique_slug_set_and_preserves_identifier_error_origin() {
    let ctx = context();
    let graph = graph(&ctx, &[refs("a", &[]), refs("b", &[])]);
    for pages in [
        vec![page("a")],
        vec![page("a"), page("b"), page("c")],
        vec![page("a"), page("c")],
    ] {
        assert!(matches!(
            error(assess(&ctx, &graph, &pages, &[], now(), limits())),
            StalenessError::CorpusMismatch
        ));
    }
    assert!(matches!(
        error(assess(
            &ctx,
            &graph,
            &[page("a"), page("a")],
            &[],
            now(),
            limits()
        )),
        StalenessError::DuplicatePage
    ));
    assert!(matches!(
        error(assess(
            &ctx,
            &graph,
            &[page("a"), page("bad/slug")],
            &[],
            now(),
            limits()
        )),
        StalenessError::References(ReferenceError::InvalidIdentifier)
    ));
    let empty = graphit_knowledge::references::ReferenceGraph::build(
        &ctx,
        &[],
        ReferenceLimits {
            max_identifier_bytes: cap(1),
        },
    )
    .unwrap();
    assert!(
        assess(&ctx, &empty, &[], &[], now(), limits())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn raw_counts_and_dangling_graph_edges_obey_exact_limits() {
    let ctx = context();
    let graph = graph(&ctx, &[refs("a", &["b", "ghost"]), refs("b", &[])]);
    let pages = [page("a"), page("b")];
    let obs = [changed("new.md"), changed("other.md")];
    let mut exact = limits();
    exact.max_pages = cap(2);
    exact.max_graph_edges = cap(2);
    exact.max_observations = cap(2);
    assert!(
        assess(&ctx, &graph, &pages, &obs, now(), exact)
            .unwrap()
            .is_empty()
    );
    for (smaller, label) in [
        (
            StalenessLimits {
                max_pages: cap(1),
                ..exact
            },
            "pages",
        ),
        (
            StalenessLimits {
                max_graph_edges: cap(1),
                ..exact
            },
            "graph edges",
        ),
        (
            StalenessLimits {
                max_observations: cap(1),
                ..exact
            },
            "observations",
        ),
    ] {
        assert!(
            matches!(error(assess(&ctx, &graph, &pages, &obs, now(), smaller)), StalenessError::LimitExceeded(boundary) if boundary == label)
        );
    }
    let duplicates = [changed("new.md"), changed("new.md")];
    assert!(matches!(
        error(assess(&ctx, &graph, &pages, &duplicates, now(), exact)),
        StalenessError::DuplicateObservation
    ));
    exact.max_observations = cap(1);
    assert!(matches!(
        error(assess(&ctx, &graph, &pages, &duplicates, now(), exact)),
        StalenessError::LimitExceeded("observations")
    ));
}

#[test]
fn metadata_and_paths_count_raw_utf8_bytes_including_extra_and_duplicate_observations() {
    let ctx = context();
    let graph = graph(&ctx, &[refs("a", &[])]);
    let pages = [sourced("a", "á/a.md")];
    let obs = [changed("á/a.md")]; // 1+7+7 bytes.
    let mut exact = limits();
    exact.max_path_bytes = cap(7);
    exact.max_metadata_bytes = cap(15);
    assert_eq!(
        assess(&ctx, &graph, &pages, &obs, now(), exact)
            .unwrap()
            .len(),
        1
    );
    exact.max_path_bytes = cap(6);
    assert!(matches!(
        error(assess(&ctx, &graph, &pages, &obs, now(), exact)),
        StalenessError::LimitExceeded("path bytes")
    ));
    exact.max_path_bytes = cap(7);
    exact.max_metadata_bytes = cap(14);
    assert!(matches!(
        error(assess(&ctx, &graph, &pages, &obs, now(), exact)),
        StalenessError::LimitExceeded("metadata bytes")
    ));
    exact.max_metadata_bytes = cap(16);
    assert_eq!(
        assess(
            &ctx,
            &graph,
            &pages,
            &[changed("á/a.md"), changed("x")],
            now(),
            exact
        )
        .unwrap()
        .len(),
        1
    );
    exact.max_metadata_bytes = cap(15);
    assert!(matches!(
        error(assess(
            &ctx,
            &graph,
            &pages,
            &[changed("á/a.md"), changed("x")],
            now(),
            exact
        )),
        StalenessError::LimitExceeded("metadata bytes")
    ));
    exact.max_metadata_bytes = cap(21);
    assert!(matches!(
        error(assess(
            &ctx,
            &graph,
            &pages,
            &[changed("á/a.md"), changed("á/a.md")],
            now(),
            exact
        )),
        StalenessError::LimitExceeded("metadata bytes")
    ));
}

#[test]
fn invalid_relative_paths_are_rejected_even_for_unused_observations() {
    let ctx = context();
    let graph = graph(&ctx, &[refs("a", &[])]);
    for path in [
        "",
        "/absolute",
        "C:drive",
        "back\\slash",
        "a//b",
        ".",
        "a/../b",
        "a/./b",
        "a/",
        "a\nb",
        "a\0b",
    ] {
        assert!(
            matches!(
                error(assess(
                    &ctx,
                    &graph,
                    &[page("a")],
                    &[changed(path)],
                    now(),
                    limits()
                )),
                StalenessError::InvalidPath
            ),
            "{path:?}"
        );
        assert!(
            matches!(
                error(assess(
                    &ctx,
                    &graph,
                    &[sourced("a", path)],
                    &[],
                    now(),
                    limits()
                )),
                StalenessError::InvalidPath
            ),
            "{path:?}"
        );
    }
    for path in ["á/b.md", "a..b", "文件/说明.md"] {
        assert!(
            assess(
                &ctx,
                &graph,
                &[page("a")],
                &[changed(path)],
                now(),
                limits()
            )
            .unwrap()
            .is_empty()
        );
    }
}

#[test]
fn original_cancelled_request_is_checked_before_foreign_context_or_bad_input() {
    let ctx = context();
    let graph = graph(&ctx, &[refs("a", &[])]);
    let foreign = context();
    ctx.cancellation().cancel();
    match error(assess(
        &foreign,
        &graph,
        &[sourced("a", "/bad")],
        &[],
        now(),
        limits(),
    )) {
        StalenessError::References(ReferenceError::Request(error)) => {
            assert_eq!(error.kind(), io::ErrorKind::Interrupted)
        }
        other => panic!("{other}"),
    }
}

#[test]
fn foreign_request_cannot_replace_original_identity() {
    let ctx = context();
    let graph = graph(&ctx, &[refs("a", &[])]);
    let foreign = context();
    assert!(matches!(
        error(assess(&foreign, &graph, &[page("a")], &[], now(), limits())),
        StalenessError::References(ReferenceError::DifferentRequest)
    ));
    assert!(
        assess(&ctx, &graph, &[page("a")], &[], now(), limits())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn original_deadline_cannot_be_refreshed_after_graph_capture() {
    let ctx = SnapshotContext::new(
        Instant::now() + Duration::from_secs(1),
        Cancellation::default(),
    );
    let graph = graph(&ctx, &[refs("a", &[])]);
    std::thread::sleep(
        ctx.deadline()
            .expect("finite fixture deadline")
            .saturating_duration_since(Instant::now()),
    );
    let fresh = context();
    match error(assess(&fresh, &graph, &[page("a")], &[], now(), limits())) {
        StalenessError::References(ReferenceError::Request(error)) => {
            assert_eq!(error.kind(), io::ErrorKind::TimedOut)
        }
        other => panic!("{other}"),
    }
}
