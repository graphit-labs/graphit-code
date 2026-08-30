# The Dream Now Reports, in Its Output, What the Session Failed to Do

Two loose ends of the dream, both with the same shape: the information existed, but existed in
the wrong place — the daemon's log — so the failure repeated every cycle looking like success.

## 1. Session that used no tool at all

The warning `WARNING — the agent completed without using any tool` went to the daemon's log,
which is exactly where someone expecting the artifact does not look. The LLM call had already
been spent, no file had been written, and nothing in the exit path said why.

Now the report opens with a `## ⚠️ No artifacts were produced` section — **before** the
agent's output, because it states that what follows is probably not what was requested.

The probable cause is NAMEABLE, which is what makes this useful instead of just noise: several
CLIs require their own flag before editing a file without confirmation; that flag is
`ai.agent_args`, and it is empty by default on purpose — it differs per CLI, changes between
releases, and grants real authority. To name the cause without duplicating CLI selection logic,
`ai.StreamResult` now carries `Binary` and `AgentArgsConfigured`: only the `ai` package knows
which CLI was chosen and what it received.

And it remains a **hypothesis**, written as such. With `ai.agent_args` already configured, the
report acknowledges this and does NOT send anyone to fix what is already correct — a
well-configured CLI can simply decide the session didn't need a tool.

The neighboring path got the same treatment: a client without agentic mode cannot edit any file
at all, which is an even harder fact, and it too only showed up in the log.

## 2. Duplicates across batches are not detected

`batchMemories` splits the corpus by character budget so that a store larger than the context
window is analyzed instead of silently truncated. The price, which the code comment already
admitted: two duplicate memories in distinct batches are never compared against each other.

What was missing was for the report to admit it. Without that, it says "nothing to do" about a
pair it never looked at together — completeness that the pass did not achieve.

`ConsolidationReport.Batches` counts the calls, and `CoverageNote()` only speaks up when there
is more than one. The note travels to `ConsolidationOutcome` through the same path `AIFailed`
already used, and is printed **even when actions were found**: "three duplicates resolved" and
"the pass did not look across its own batches" are both true, and omitting the second makes the
first sound like full coverage.

Written as a limit OF THE PASS, not as a finding about the corpus — a test guarantees it never
claims "no duplicates," because no comparison was made.

## Similarity-based batching — done, after a fair challenge

The first version of this log said batching by similarity was "the right solution and the more
expensive one," and left it for later. The Engineer asked why, "since memory is a wiki and the
wiki has embeddings in the database." He was right, and the answer is that **the expensive part
was already paid for**: `~/.graphit/wiki/memory/project/<id>/wiki.db` has 138 memories and 138
vectors of 768 dimensions, and `WikiEmbedTargets` includes both memory wikis, so the daemon
keeps them up to date. I had classified the cost without measuring it.

Done, then:

- `WikiDB.VectorsBySource()` reads the vectors via the join `chunks_vec_map → chunks →
  chunks_vec`, decoding the little-endian blob that `sqlite_vec.SerializeFloat32` writes. It
  lives in the `wiki` package because that package owns the schema.
- `loadMemoryVectors(wikiDir)` keys by ID: the wiki writes the source as `<ID>_…_.md`.
- `orderBySimilarity` is a greedy nearest-neighbor chain — O(n²), microseconds for hundreds of
  memories. It is not optimal clustering and doesn't need to be: the only required property is
  that near-duplicates end up **adjacent**, and a duplicate is by definition the nearest
  neighbor of the other. Deterministic start from the smallest ID, otherwise the same corpus
  would batch differently on every run — and coverage that changes between runs is coverage you
  cannot reason about.

Degrades instead of breaking: scope not yet embedded, or a memory without a vector, keeps
arrival order.

Verified against this repository's real corpus: 138 vectors loaded, lossless ordering (138 of
138).

**And `CoverageNote` is still reported.** Cutting a chain still separates the two memories on
either side of the cut — similarity improves a lot where the cut falls, it doesn't eliminate the
issue. Announcing it as solved would be the same false completeness this whole log is about.

## What's left for later

The titles-only pass, which would cover exactly the pairs separated right at batch boundaries.
Still not done — and still announced by the coverage note.

## Progress Log

- 2026-08-16 — Diagnosis in the report for both artifact-less paths, and a coverage note for
  batched consolidation. Six tests: cause named, cause NOT attributed when the config already
  exists, healthy session with no warning, silent note for a single batch, note present and
  well-worded for multiple batches, and the note reaching the audit markdown.
- 2026-08-16 (same session, later) — Similarity-based batching implemented, on top of
  embeddings that already existed. I had recorded the cost without measuring it, and the
  Engineer called it out. Five more tests: near-duplicates end up adjacent, deterministic
  ordering, degradation without vectors, a memory without a vector doesn't disappear, and the ID
  recovered from the source filename. Full suite green, `make lint` 0 issues.
