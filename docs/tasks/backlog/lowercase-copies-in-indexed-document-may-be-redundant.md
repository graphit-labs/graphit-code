# Lowercase copies in the indexed document can be redundant with the engine tokenizer — the tuning scan needs to measure this

# Lowercase copies in the indexed document may not pay for their place

Opened on 2026-08-23 when porting the AST index to LanceDB
(`internal/ast/search_lance.go`, function `entityBody`). **It is not a defect** — it is a part of a
measured configuration whose justification was not isolated.

## O fato

`entityBody` assembles the indexed document with, among other things, **four** variants of the
identifier:

```
evictOldestStaged evict Oldest Staged evictoldeststaged evict oldest staged …
```

that is: the name, the split shape, the name in lowercase and the split shape in lowercase.

This exactly reproduces the `documentText` that the tuning scan measured
(`internal/lancestore/probe_floor_lancedb_test.go`, `TestSearchTuningSweep`), and was maintained by
this: **deviating from a measured setting without re-measuring is the error that the scan itself exists
to stop it.**

## Why suspect them

The default tokenizer of the engine **already normalizes to lowercase at the time of indexing**. If this is
true here, the two lowercase copies add no terms at all-they just **double the
frequency** of the same terms throughout the document.

Folding TF evenly across all documents is not catastrophic (BM25 saturates TF, and the effect on
most of it is canceled when it is uniform), but:

- inflates the document size, which is the denominator of the BM25 length normalization —
  so documents with long identifiers are penalized more than they should;
- inflates the inverted index without new recall;
- and masks the actual effect of the bag of grams, which is the part that actually buys recall of
  truncation.

What to do

1. Add to `TestSearchTuningSweep` two variants of `documentText`: one **without**
   `lower(name)` and `lower(split)`, and one without any of the four duplicates (only name + split). 2. Compare strict/recall/voids against current setting. If tie, **remove copies** —
   smaller document with the same result is strictly better. 3. Directly confirm that the lowercase default tokenizer: index `FooBar` and query `foobar`
   without any lowercase copies in the document. If you get married, the redundancy is proven.

## How to know it worked

- The scan has one line for each variant, and the chosen one is justified by number.
- If copies come out, `TestLanceBothWritePathsProduceTheSameDocument` need to have their assertions
  verbatim — he states `"evict Oldest Staged"` and `"evict oldest staged"` on purpose,
  exactly so that this change appears in a test instead of passing in silence.
