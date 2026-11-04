use graphit_ai::{
    context::{ContextError, InferenceContext},
    ranking::{RankingError, RankingLimits, rank_order, similarity_scores},
};
use graphit_core::file_lock::Cancellation;
use std::time::{Duration, Instant};

fn limits() -> RankingLimits {
    RankingLimits::new(128, 1024, 132_096).unwrap()
}
fn context(cancellation: &Cancellation) -> InferenceContext<'_> {
    InferenceContext::new(cancellation, Instant::now() + Duration::from_secs(5))
}

#[test]
fn permutation_keeps_every_candidate_score_and_numeric_tie() {
    let cancel = Cancellation::default();
    let indices = [4, 2, 2, 1, 8, 0, 2];
    let scores = [0.0, -0.0, 0.0, -1.0, 1.0, -0.0, 0.0];
    let expected = [4, 5, 1, 2, 6, 0, 3];
    let payloads = ["a", "b", "c", "d", "e", "f", "g"];
    for _ in 0..8 {
        let order = rank_order(&indices, &scores, limits(), context(&cancel)).unwrap();
        assert_eq!(order, expected);
        let mut positions = order.clone();
        positions.sort_unstable();
        assert_eq!(positions, (0..payloads.len()).collect::<Vec<_>>());
        assert_eq!(
            order.iter().map(|&i| payloads[i]).collect::<Vec<_>>(),
            ["e", "f", "b", "c", "g", "a", "d"]
        );
        assert_eq!(scores[order[0]], 1.0);
        assert_eq!(scores[order[6]], -1.0);
    }
}

#[test]
fn invalid_order_leaves_inputs_unchanged() {
    let cancel = Cancellation::default();
    let indices = [2, 0];
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let scores = [value, 1.0];
        assert_eq!(
            rank_order(&indices, &scores, limits(), context(&cancel)),
            Err(RankingError::NonFinite)
        );
        assert_eq!(indices, [2, 0]);
        assert_eq!(scores[0].to_bits(), value.to_bits());
    }
    assert_eq!(
        rank_order(&indices, &[1.0], limits(), context(&cancel)),
        Err(RankingError::Cardinality)
    );
}

#[test]
fn cosine_preserves_document_order_and_inputs() {
    let cancel = Cancellation::default();
    let query = [1.0, 0.0];
    let documents = vec![
        vec![0.0, 1.0],
        vec![1.0, 0.0],
        vec![2.0, 0.0],
        vec![-1.0, 0.0],
    ];
    let before = documents.clone();
    assert_eq!(
        similarity_scores(&query, &documents, 2, limits(), context(&cancel)).unwrap(),
        [0.0, 1.0, 1.0, -1.0]
    );
    assert_eq!(documents, before);
    assert_eq!(query, [1.0, 0.0]);
}

#[test]
fn f64_accumulation_handles_large_and_subnormal_f32_vectors() {
    let cancel = Cancellation::default();
    for vector in [[f32::MAX, f32::MAX], [f32::from_bits(1), f32::from_bits(1)]] {
        let scores =
            similarity_scores(&vector, &[vector.to_vec()], 2, limits(), context(&cancel)).unwrap();
        assert!(scores[0].is_finite());
        assert!((scores[0] - 1.0).abs() < 1e-14);
    }
}

#[test]
fn invalid_vectors_never_return_partial_scores() {
    let cancel = Cancellation::default();
    for (query, documents, error) in [
        (vec![1.0], vec![vec![1.0, 0.0]], RankingError::Dimensions),
        (vec![1.0, 0.0], vec![vec![1.0]], RankingError::Dimensions),
        (vec![0.0, 0.0], vec![vec![1.0, 0.0]], RankingError::ZeroNorm),
        (
            vec![1.0, 0.0],
            vec![vec![1.0, 0.0], vec![0.0, 0.0]],
            RankingError::ZeroNorm,
        ),
        (
            vec![f32::NAN, 0.0],
            vec![vec![1.0, 0.0]],
            RankingError::NonFinite,
        ),
        (
            vec![1.0, 0.0],
            vec![vec![1.0, 0.0], vec![f32::INFINITY, 0.0]],
            RankingError::NonFinite,
        ),
        (
            vec![1.0, 0.0],
            vec![vec![f32::NEG_INFINITY, 0.0]],
            RankingError::NonFinite,
        ),
    ] {
        assert_eq!(
            similarity_scores(&query, &documents, 2, limits(), context(&cancel)),
            Err(error)
        );
    }
}

#[test]
fn policies_reject_zero_and_unrepresentable_capacities() {
    for (candidates, dimensions, values) in [
        (0, 1, 1),
        (1, 0, 1),
        (1, 1, 0),
        (usize::MAX, 1, 1),
        (1, usize::MAX, 1),
        (1, 1, usize::MAX),
    ] {
        assert!(matches!(
            RankingLimits::new(candidates, dimensions, values),
            Err(RankingError::InvalidPolicy)
        ));
    }
}

#[test]
fn exact_caps_pass_and_plus_one_fails() {
    let cancel = Cancellation::default();
    let policy = RankingLimits::new(1, 2, 4).unwrap();
    assert_eq!(
        rank_order(&[0], &[1.0], policy, context(&cancel)).unwrap(),
        [0]
    );
    assert_eq!(
        rank_order(&[0, 1], &[1.0, 2.0], policy, context(&cancel)),
        Err(RankingError::Limit)
    );
    assert_eq!(
        similarity_scores(&[1.0, 0.0], &[vec![1.0, 0.0]], 2, policy, context(&cancel)).unwrap(),
        [1.0]
    );
    assert_eq!(
        similarity_scores(
            &[1.0, 0.0],
            &[vec![1.0, 0.0], vec![1.0, 0.0]],
            2,
            policy,
            context(&cancel)
        ),
        Err(RankingError::Limit)
    );
    let values = RankingLimits::new(2, 2, 3).unwrap();
    assert_eq!(
        similarity_scores(&[1.0, 0.0], &[vec![1.0, 0.0]], 2, values, context(&cancel)),
        Err(RankingError::Limit)
    );
    assert_eq!(
        similarity_scores(&[], &[], 3, policy, context(&cancel)),
        Err(RankingError::Limit)
    );
    assert_eq!(
        similarity_scores(&[], &[], 0, policy, context(&cancel)),
        Err(RankingError::Dimensions)
    );
}

#[test]
fn empty_inputs_do_not_require_vectors_but_still_check_context() {
    let cancel = Cancellation::default();
    assert!(
        rank_order(&[], &[], limits(), context(&cancel))
            .unwrap()
            .is_empty()
    );
    assert!(
        similarity_scores(&[f32::NAN], &[], 2, limits(), context(&cancel))
            .unwrap()
            .is_empty()
    );
    cancel.cancel();
    assert_eq!(
        rank_order(&[], &[], limits(), context(&cancel)),
        Err(RankingError::Context(ContextError::Cancelled))
    );
    assert_eq!(
        similarity_scores(&[], &[], 2, limits(), context(&cancel)),
        Err(RankingError::Context(ContextError::Cancelled))
    );
}

#[test]
fn copied_context_preserves_original_deadline_and_shared_signal() {
    let cancel = Cancellation::default();
    let deadline = Instant::now();
    let original = InferenceContext::new(&cancel, deadline);
    let copied = original;
    assert!(std::ptr::eq(copied.cancellation(), &cancel));
    assert_eq!(copied.deadline(), deadline);
    assert_eq!(
        rank_order(&[0], &[1.0], limits(), copied),
        Err(RankingError::Context(ContextError::Deadline))
    );
    cancel.clone().cancel();
    assert_eq!(original.check(), Err(ContextError::Cancelled));
}

#[test]
fn nonempty_vector_work_honors_cancelled_and_expired_contexts() {
    let query = [1.0, 0.0];
    let documents = [query.to_vec()];
    let policy = RankingLimits::new(1, 2, 4).unwrap();
    let cancel = Cancellation::default();
    assert_eq!(
        similarity_scores(&query, &documents, 2, policy, context(&cancel)),
        Ok(vec![1.0])
    );
    cancel.cancel();
    assert_eq!(
        similarity_scores(&query, &documents, 2, policy, context(&cancel)),
        Err(RankingError::Context(ContextError::Cancelled))
    );
    let uncancelled = Cancellation::default();
    let expired = InferenceContext::new(&uncancelled, Instant::now());
    assert_eq!(
        similarity_scores(&query, &documents, 2, policy, expired),
        Err(RankingError::Context(ContextError::Deadline))
    );
}

#[test]
fn diagnostics_include_only_static_categories() {
    let error = RankingError::NonFinite;
    assert_eq!(error.to_string(), "ranking value is not finite");
    assert_eq!(format!("{error:?}"), "NonFinite");
    let source = RankingError::Context(ContextError::Deadline);
    assert_eq!(source.to_string(), "inference deadline exceeded");
    assert!(std::error::Error::source(&source).is_some());
}

#[test]
#[ignore = "optional ranking throughput measurement"]
fn measure_ranking_fixture() {
    let cancel = Cancellation::default();
    let query = vec![1.0; 256];
    let documents = vec![vec![1.0; 256]; 64];
    let policy = RankingLimits::new(64, 256, 65 * 256).unwrap();
    let indices: Vec<_> = (0..64).rev().collect();
    let mut samples = Vec::new();
    let total = Instant::now();
    for _ in 0..128 {
        let start = Instant::now();
        let scores = similarity_scores(&query, &documents, 256, policy, context(&cancel)).unwrap();
        let order = rank_order(&indices, &scores, policy, context(&cancel)).unwrap();
        assert_eq!(order[0], 63);
        samples.push(start.elapsed().as_nanos());
    }
    let wall = total.elapsed().as_nanos();
    samples.sort_unstable();
    println!(
        "{{\"fixture\":\"cosine-and-rank-64x256\",\"samples\":128,\"wall_ns\":{wall},\"p95_ns\":{}}}",
        samples[121]
    );
}
