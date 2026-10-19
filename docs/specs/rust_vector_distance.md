---
references:
  - target: {type: task, id: tsk-dec0}
    relation: implements
  - target: {type: task, id: tsk-da58}
    relation: derived_from
  - target: {type: task, id: tsk-4f78}
    relation: uses
  - target: {type: task, id: tsk-631e}
    relation: uses
---

# Exact cosine over variable-width vectors

`graphit_data::vector_distance::cosine_distances`, implemented in [vector_distance.rs](../../crates/graphit-data/src/vector_distance.rs) and exported by [lib.rs](../../crates/graphit-data/src/lib.rs), scores one borrowed Arrow
`List<Float32>` batch using the public Lance 14.0.2 cosine kernel. Each returned
position corresponds to an input row. Null lists represent rows without a
semantic vector and return `None`; non-null vectors must have the query's
positive dimension, non-null finite elements, and a finite SDK distance.
Invalid inputs or a non-finite result fail the whole batch. No partial scores
are returned. A zero vector can produce a non-finite cosine and is rejected
through that result check. Values are neither normalized nor clamped here.

The function borrows the child Float32 slices and allocates only the scores for
this batch. It does not retain a corpus, select a top-k, apply a quality
threshold, build an ANN index, or grant authority. Persisted List vectors can
have model-dependent widths without choosing an artificial schema dimension;
the caller must select a coherent model/configuration/recipe before scoring.
Lance `nearest` requires a fixed inner dimension and is not used by this API.

The caller owns authorization, cancellation, the captured physical dataset,
row/source bindings, fingerprints, complete coverage and EOF. It must withhold
results until all relevant batches are validated, filter eligible source
revisions and collapse logical records before selecting the result count.
The [planned Memory contract](native_memory_embeddings.md) describes that
consumer; this Data API does not qualify Memory or ModelService integration.

## Public API and errors

```text
pub fn cosine_distances(
    vectors: &arrow_array::ListArray,
    query: &[f32],
) -> Result<Vec<Option<f32>>, VectorDistanceError>;
```

The implementation calls `lance_linalg::distance::cosine::cosine_distance`
using borrowed child slices. `VectorDistanceError` distinguishes `EmptyQuery`,
`NonFiniteQuery { element }`, `VectorType`,
`Dimensions { row, expected, actual }`, `NullElement { row, element }`,
`NonFiniteVector { row, element }` and `NonFiniteDistance { row }`.
Row/element positions describe this batch, not a stable corpus identity. A
later invalid vector discards the batch result; outer NULL is distinct from an
invalid child. Model identity, source fingerprints and request checks are the
caller's responsibility.

## Executed Linux scope

The first locked focal suite [vector_distance.rs](../../crates/graphit-data/tests/vector_distance.rs)
executed **eight tests PASS, zero failures/ignored/filtered**: four pure tests
and four real SDK tests. Runtime was 0.09 s; the 6 min 23 s build included queue
and is not a benchmark. Scoped strict Clippy completed **exit 0**, as did
owned formatting. The Clippy build took 16 min 09 s including the warm-cache
queue, not a performance measurement. The library compiled this page through
`include_str!`; there are no new doctests or executable examples, and no
doctest count is inferred from that compilation. Task lifecycle is
separate from these Data/Linux qualification results.

The four pure cases exercise row alignment, slices/offsets and actual
2/3-dimensional inputs; query/late-vector dimension errors; non-Float32 child
and null components; and non-finite query/vector/SDK results, zero and overflow.
Errors after valid rows return no partial scores.

The four SDK cases use isolated real datasets, not live Graphit stores:

- List roundtrip, reopen and complete multi-batch EOF with borrowed Float32
  slices from the same captured pin.
- Strict merge, stale CAS with actual recovery key, Applied/Conflicted and
  foreign-binding outcomes, old-pin preservation, and actual JSON FTS
  inspector Ready plus search after the vector change.
- Named-branch isolation and inherited reads without publication; old pins
  retain the original rows.
- Recompute dimension 2 to 3 using the same List schema, without an artificial
  fixed-width default.

These results qualify the executed Data kernel/SDK behavior only. They do not
qualify Memory capture/attach/query, ModelService, GGUF or another consumer;
other operating systems remain unrun. The scoped suite does not intercept SDK
lost acknowledgements or establish continuous Auth/SDK atomicity.

Commands for this leaf, coordinated with the Root build window:

```sh
cargo test --offline --locked -p graphit-data --test vector_distance -j1
cargo clippy --locked -p graphit-data --lib --test vector_distance --no-deps -- -D warnings
rustfmt --edition 2024 --check crates/graphit-data/src/vector_distance.rs crates/graphit-data/tests/vector_distance.rs
```

Documentation validation here is Source/API/link agreement plus the library's
`include_str!` compilation, not a new doctest suite. Root combines the warm
target and global manifests/locks. No broad workspace gate
or other SDK suite replay is part of this documentation delta.
