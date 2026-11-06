---
title: Rust ranking and inference context
description: Pure ranking primitives over caller-owned vectors and scores; distinct from the configured product reranker.
type: specification
updated: 2026-10-29
references:
  - target:
      type: task
      id: tsk-55ac
    relation: implements
  - target:
      type: task
      id: tsk-f845
    relation: derived_from
---

# Rust ranking and inference context

This reference describes pure ranking functions in `graphit-ai` and their execution context.
The configured Rust search reranker is a separate consumer, documented in
[AI Models](../guides/ai_models.md) and the [rerank integration](rust_search_rerank_consumer.md).
A cosine helper does not replace a query/document cross-encoder.

The library receives scores or vectors already produced by a caller. It does not
load a model, generate embeddings, resolve a profile, grant authority, contact a
provider, or activate the search rerank stage. Provider orchestration, local GGUF execution and streaming have their own contracts;
this helper alone does not deliver those paths.

## Context and policy

`InferenceContext::new(&cancellation, deadline)` borrows the original
`graphit_core::file_lock::Cancellation` and retains the original absolute
`std::time::Instant`. Copies of the context preserve both. `cancellation()` and
`deadline()` expose these same values to other inference consumers. `check()`
reports `Cancelled` before `Deadline` when both conditions hold.

`RankingLimits::new(max_candidates, max_dimensions, max_vector_values)` requires
positive capacities representable as bounded allocations. There are no defaults.
The caller selects limits appropriate to the actual candidate set and vector
space. `max_vector_values` includes the query and every document for cosine.
These are logical work and output capacities, not hard RSS or scheduler limits.

Operations check context during validation and vector loops, before and after
sorting, and before returning output. Sorting performs finite work under the
candidate cap; cancellation does not preempt the operating system or interrupt
the standard-library sort in the middle of a comparison. No operation resets a
deadline, changes the signal, retries, or returns partial results on failure.

## Ordering scores

`rank_order(indices, scores, limits, context)` returns a permutation of **input
positions**. Indices and scores must have equal length; scores must be finite.
The permutation includes every position exactly once. The caller applies it to
its candidate data and reads each score from its original position.

Ordering uses descending numeric score, ascending caller index, then ascending
original position. Duplicate indices remain distinct; `+0.0` and `-0.0` tie
numerically. The complete tiebreak permits an in-place unstable sort without a
stable-sort scratch allocation. Input slices remain unchanged, including in error.

For indices `[4, 2, 2]` and scores `[0.0, -0.0, 1.0]`, the returned positions are
`[2, 1, 0]`. This preserves each candidate and its score without cloning caller
metadata or dropping a subset. A disabled rerank stage should avoid producing
scores or calling a ranker; activation belongs to its consumer.

## Cosine scores

`similarity_scores(query, documents, dimensions, limits, context)` returns one
score per document in input order. Dimensions are explicit and positive. All
vectors must have that exact width, finite `f32` values and a nonzero norm.
The function accumulates dot products and norms in `f64`; it does not normalize
or mutate the supplied vectors. No model identity or width is inferred.

For query `[1, 0]` and documents `[[0, 1], [1, 0], [2, 0], [-1, 0]]`, scores are
`[0, 1, 1, -1]`. Large finite and subnormal `f32` values are accumulated in `f64`.

An empty document list returns `[]` without validating or computing the query
vector. It still checks the context, positive dimensions and dimension cap.
Empty ordering inputs similarly return an empty permutation under a valid context.
The provider consumer must check vector/candidate cardinality before this function
if it is associating vectors returned by an external API with a candidate set.

## Errors and recovery

`RankingError` distinguishes context cancellation/deadline, invalid policy,
limit, cardinality, dimensions, non-finite values, zero norm and allocation failure.
Diagnostics contain static categories and no candidate text, vector contents,
credentials or provider payload. Context errors remain available as an error source.

Correct malformed vectors or score sets before retrying. A limit requires a new
caller-selected policy or smaller explicit work; cancellation requires a new
operation, not a replacement signal inside the existing one. On any error the
caller retains its original candidates and can report the failure without treating
first-stage ordering as a successfully reranked result.

## Verification and integration boundaries

Sources: `crates/graphit-ai/src/context.rs` and `src/ranking.rs`. The focused suite
is `cargo xtask cargo test --locked -p graphit-ai --test integration ranking::`. It exercises permutations,
signed-zero and duplicate-index ties, cosine extremes, limits, invalid vectors,
shared cancellation, original deadlines and interruption during vector work.
`cargo xtask cargo clippy --locked -p graphit-ai --test integration --no-deps -- -D warnings`
checks the integration target. See [AI integration tests](../development/ai-tests.md)
for module filters and platform boundaries.

An explicit ignored fixture measures 128 cosine-plus-order operations over 64
documents of 256 dimensions: `cargo xtask cargo test --locked -p graphit-ai --test integration
ranking::measure_ranking_fixture -- --ignored --exact --nocapture`. This measures the pure library,
not an authenticated provider, a search integration, GPU inference or a gain over
another implementation. Process CPU/RSS readings include the fixture harness.

On Linux x86_64 in the coordinated 2026-10-09 window, the focused suite passed
11 behavior tests. The measurement was run separately on the already compiled
debug test binary: 128 samples took 126.371005 ms in total, with a sample p95 of
1.339133 ms. The fixture process used 0.12 s user CPU and 0.00 s system CPU at the
measurement tool's resolution, with 3,072 KiB peak RSS including its harness.
These observations describe this fixture and build only; they are not a hard
memory bound or a production latency target. Strict Clippy
passed. The library and documentation harnesses contained no additional tests.

Direct embedding adapters consume this same context and the existing bounded HTTP
transport. Knowledge and AST require bounded batches and model/generation identity
from their eventual embedding producer; their Lance consumers remain separately
owned. Records requires typed completion outcomes distinguishing observed telemetry
from an unobserved CLI exit, with finite telemetry and original cancellation/deadline.
These requirements do not turn a selected model or profile DTO into authority.
