use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_knowledge::{
    document_communities::{
        DocumentCommunities, DocumentCommunityError as E, classify_document_communities,
    },
    references::{PageReferences, ReferenceError, ReferenceGraph, ReferenceLimits},
};
use std::{
    error::Error,
    io,
    num::NonZeroUsize,
    time::{Duration, Instant},
};

fn original() -> SnapshotContext {
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
fn limits() -> ReferenceLimits {
    ReferenceLimits {
        max_identifier_bytes: NonZeroUsize::new(64).unwrap(),
    }
}
fn classify(ctx: &SnapshotContext, pages: &[PageReferences<'_>]) -> DocumentCommunities {
    let graph = ReferenceGraph::build(ctx, pages, limits()).unwrap();
    let before = graph.stats(ctx).unwrap();
    let report = classify_document_communities(ctx, &graph).unwrap();
    assert_eq!(graph.stats(ctx).unwrap(), before);
    report
}

#[test]
fn empty_single_and_known_isolated_pages_have_no_groups() {
    let ctx = original();
    for pages in [
        vec![],
        vec![page("a", "A", &[])],
        vec![page("a", "A", &["missing"]), page("b", "B", &[])],
    ] {
        assert!(classify(&ctx, &pages).groups.is_empty());
    }
}

#[test]
fn directed_duplicate_self_reciprocal_and_dangling_links_share_one_simple_pair() {
    let ctx = original();
    let plain = [page("a", "A", &["b"]), page("b", "B", &[])];
    let expected = classify(&ctx, &plain);
    assert_eq!(expected.groups.len(), 1);
    assert_eq!(expected.groups[0].members, ["a", "b"]);
    assert_eq!(expected.groups[0].label, "A");
    assert_eq!(expected.groups[0].cohesion, 1.0);
    let noisy = [
        page("b", "B", &["a", "a", "b"]),
        page("a", "A", &["a", "b", "ghost", "", "b"]),
    ];
    let graph = ReferenceGraph::build(&ctx, &noisy, limits()).unwrap();
    assert_eq!(graph.stats(&ctx).unwrap().broken_edges, 1);
    assert_eq!(graph.stats(&ctx).unwrap().links, 3);
    assert_eq!(
        classify_document_communities(&ctx, &graph).unwrap(),
        expected
    );
    assert_eq!(
        graph
            .broken_links(&ctx, NonZeroUsize::new(1).unwrap())
            .unwrap()[0]
            .target,
        "ghost"
    );
}

#[test]
fn disjoint_cliques_are_complete_and_ties_use_lexical_members() {
    let ctx = original();
    let pages = [
        page("z", "Z", &["x", "y"]),
        page("b", "B", &["a", "c"]),
        page("alone", "Alone", &[]),
        page("a", "A", &["b", "c"]),
        page("y", "Y", &["x", "z"]),
        page("c", "C", &["a", "b"]),
        page("x", "X", &["y", "z"]),
    ];
    let report = classify(&ctx, &pages);
    assert_eq!(report.groups.len(), 2);
    assert_eq!(report.groups[0].members, ["a", "b", "c"]);
    assert_eq!(report.groups[0].label, "A");
    assert_eq!(report.groups[1].members, ["x", "y", "z"]);
    assert_eq!(report.groups[1].label, "X");
    assert!(report.groups.iter().all(|g| g.cohesion == 1.0));

    // The bridge changes total degree, while each community remains a clique.
    let mut bridged = pages;
    bridged.iter_mut().find(|p| p.slug == "c").unwrap().targets = &["a", "b", "x"];
    let report = classify(&ctx, &bridged);
    assert_eq!(report.groups.len(), 2);
    assert_eq!(report.groups[0].members, ["a", "b", "c"]);
    assert_eq!(report.groups[0].label, "C");
    assert_eq!(report.groups[1].members, ["x", "y", "z"]);
    assert_eq!(report.groups[1].label, "X");
    assert!(report.groups.iter().all(|g| g.cohesion == 1.0));
}

#[test]
fn chain_labels_use_degree_and_density_is_rounded() {
    let ctx = original();
    let pages = [
        page("a", "A", &["b"]),
        page("b", "Middle", &["c"]),
        page("c", "C", &[]),
    ];
    let report = classify(&ctx, &pages);
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].members, ["a", "b", "c"]);
    assert_eq!(report.groups[0].label, "Middle");
    assert_eq!(report.groups[0].cohesion, 0.67);
}

#[test]
fn configured_sdk_runs_and_input_permutations_keep_exact_case_and_unicode() {
    let ctx = original();
    let canonical = [
        page("A", "", &["a", "é"]),
        page("a", "lower", &["A", "é"]),
        page("é", "accent", &["A", "a"]),
        page("x", "X", &["y"]),
        page("y", "Y", &[]),
    ];
    let expected = classify(&ctx, &canonical);
    assert_eq!(expected.groups[0].members, ["A", "a", "é"]);
    assert_eq!(expected.groups[0].label, "A");
    assert_eq!(expected.groups[1].members, ["x", "y"]);
    for rotation in 0..canonical.len() {
        let mut pages = canonical.to_vec();
        pages.rotate_left(rotation);
        pages.reverse();
        // Reversed target order and additional reciprocity do not change the simple graph.
        pages.iter_mut().find(|p| p.slug == "A").unwrap().targets = &["é", "a", "A", "a"];
        pages.iter_mut().find(|p| p.slug == "y").unwrap().targets = &["x", "x"];
        for _ in 0..4 {
            assert_eq!(classify(&ctx, &pages), expected);
        }
    }
}

#[test]
fn report_owns_its_metadata_after_input_and_graph_are_dropped() {
    let report = {
        let ctx = original();
        let a = String::from("a");
        let b = String::from("b");
        let title = String::from("Owned label");
        let targets = [b.as_str()];
        classify(&ctx, &[page(&a, &title, &targets), page(&b, "B", &[])])
    };
    assert_eq!(report.groups[0].members, ["a", "b"]);
    assert_eq!(report.groups[0].label, "Owned label");
}

#[test]
fn request_identity_cancellation_and_deadline_keep_real_causes() {
    let cancel = Cancellation::default();
    let ctx = SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel.clone());
    let graph = ReferenceGraph::build(&ctx, &[], limits()).unwrap();
    let foreign = original();
    let error = classify_document_communities(&foreign, &graph).unwrap_err();
    assert!(matches!(
        error,
        E::References(ReferenceError::DifferentRequest)
    ));
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<ReferenceError>()
            .is_some()
    );
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    let error = classify_document_communities(&expired, &graph).unwrap_err();
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<io::Error>()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );
    cancel.cancel();
    let error = classify_document_communities(&ctx, &graph).unwrap_err();
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<io::Error>()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
}
