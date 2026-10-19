---
title: Rust native reranking validation
type: spec
status: linux-library-qualified
updated: 2026-10-18
tags: [rust, ai, rerank, limits]
references:
  - target: {type: task, id: tsk-ea46}
    relation: relates_to
  - target: {type: task, id: tsk-4ec8}
    relation: implements
  - target: {type: task, id: tsk-c976}
    relation: implements
  - target: {type: task, id: tsk-7b3b}
    relation: implements
  - target: {type: task, id: tsk-ad37}
    relation: derived_from
  - target: {type: task, id: tsk-55ac}
    relation: uses
  - target: {type: task, id: tsk-9f12}
    relation: relates_to
  - target: {type: task, id: tsk-b559}
    relation: relates_to
---

# Rust native reranking validation

This reference defines complete scalar reranking results and the limits used by Rust native provider adapters. It is implemented by `crates/graphit-ai/src/native_rerank.rs` and consumed by the Cohere, Voyage, Jina and Graphit adapters and existing ranking API.

## Complete scalar results

`IndexScore { index: usize, relevance_score: f64 }` refers to one document within one request batch. `validate_results(count, rows, limits, context)` requires exactly `count` rows, each index in `0..count` appearing once, and every score finite. It returns scores in input order.

Negative scores and both signed zeros are valid. There is no conversion to `f32`, clipping, cosine substitution or default score. Duplicate/missing/out-of-range indices and nonfinite values return `Shape`; invalid counts/bounds return their typed errors. No partial vector is exposed.

## Explicit work limits and context

`NativeRerankLimits::new(max_candidates, max_batch, max_input_utf8, max_request_json, max_response_json)` requires positive representable bounds and checks representability of candidate buffers. These are configured input/work bounds, not process memory reservations or a promised RSS ceiling.

`validate_input` checks the original context before work, candidate count and checked UTF-8 totals. For a nonempty set, the query counts once alongside all documents. An empty set is valid after the context check. Provider adapters additionally validate escaped JSON and transport response bounds.

All operations preserve `InferenceContext` with the caller's cancellation reference and absolute deadline. Cancellation and deadline remain distinct errors. Allocation failures remain typed; context failures preserve their cause. Neither produces zero scores.

## Existing ordering API

Pass complete scores and caller indices to `rank_order` in `ranking.rs`. It returns positions ordered by descending score, then ascending caller index, then original position. Signed zeros compare as a tie. This preserves the consumer's identity and deterministic tie behavior without interpreting provider scores as probabilities.

The separate `similarity_scores` operation computes embedding similarity and validates shape, finitude and nonzero norms. It is not a replacement for native reranker scores.

## Guarded operations

GuardedRerankError<E> preserves NativeRerankError or an admission cause E alongside the operation NetworkEffect. check_admission surrounds callback invocation with original context checks. SDK score and score_guarded share the same validation and wire pipeline. Guarded adapters borrow configured HeaderMap values; Direct remains responsible for its real lease and Bearer credential, while the active SecretString convenience API retains provider authentication defaults. The model-free Graphit adapter uses the same strict complete-set Results decoding; its Broker consumer owns final ranking/top-N and authorization. A later failure never exposes partial scores or resets a previous MayHaveApplied effect. See [Direct native reranking](rust_direct_native_rerank.md) for the real Auth lease consumer and current qualification.

## Historical functional verification

The recorded affected suite passed 2 native cases plus 8 real Cohere TCP cases. Native cases verify complete validation and integration into `rank_order`, including caller indices and signed-zero ties. Cohere exercises full wire/result sets, explicit batching, malformed responses, limits, HTTP failures, cancellation/deadline and no partial result after a late failure.

See [wire verification](rust_native_rerank_wire.md#affected-verification) for the invocation and evidence scope, and [Cohere contract](rust_cohere_rerank.md) for the current adapter. Documentation changes do not count as a new execution or provider measurement. This historical library evidence does not establish authenticated application cutover or other platforms; the separate Direct consumer reference states its actual scope.

Decisions 566/570 superseded the former per-leaf guard/hash/floor/custody requirements. Existing Task audit records preserve that history; current completion requires actual functional checks and coherent documentation.
