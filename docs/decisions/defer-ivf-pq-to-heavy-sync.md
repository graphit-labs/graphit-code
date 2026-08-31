---
title: Defer IVF-PQ to heavy sync while keeping search publication synchronous
status: accepted
date: 2026-08-31
tags: [ast, lance, ivf-pq, performance]
---

# Defer IVF-PQ to heavy sync while keeping search publication synchronous

## Status

Accepted.

## Context

The AST write phase publishes both the Icebug graph and its Lance search sidecar. A measured 25.667-second Lance rebuild spent 22.914 seconds training IVF-PQ; row ingestion, FTS, B-tree, and bitmap creation together consumed about 2.753 seconds.

The search sidecar cannot be deferred as a whole. Its `files` and `entities` tables are the queryable copy of source text and the basis for textual search. Publishing the graph while leaving those tables stale would make `ast search` inconsistent and `ast source` incorrect or unavailable.

IVF-PQ is different: vectors stored in `entities.embedding` remain queryable by exact scan when no approximate index exists. The binary per-file embedding cache also lets a rebuild restore vectors without model inference.

## Decision

`ast index` and synchronous phase 1 of `sync` will:

1. Publish a new corpus generation with `vector_index: pending` before mutating Lance.
2. Rebuild/update `files` and `entities`, including source text and cached vectors.
3. Create or fold FTS, B-tree, and bitmap indexes.
4. Leave IVF-PQ absent so semantic queries use exact scan.

`sync --heavy`, `ast embed`, and the background embedding cycle own vector finalization. They capture the corpus generation before work, generate only missing embeddings, serialize IVF-PQ builders, and publish `vector_index: ready` only when the captured generation is still current. Work completed for an obsolete generation is discarded. When fewer than 256 vectors exist, exact scan is the finalized ready policy because Lance cannot train IVF-PQ below that floor.

Incremental AST updates remove the previous IVF-PQ before changing entity rows. This keeps pending meaningful and prevents Lance from selecting an approximate index trained for a prior corpus generation.

## Consequences

- The measured synchronous search rebuild falls from 25.667 seconds to about 3.52-3.53 seconds on the project corpus.
- Text search and source retrieval remain coherent with the graph immediately after `ast index`.
- Semantic search remains available during pending through exact scan, with higher query latency on large corpora.
- Heavy work becomes explicit and generation-safe. On the measured corpus, zero-embedding IVF-PQ finalization took 27.8 seconds; a repeated heavy run on the ready generation took 1.6 seconds and did not retrain.
- The `embeds.json` status file and two advisory locks become part of the local search publication protocol.
- A random generation token invalidates vector readiness even when source bytes are unchanged but parser/index behavior changes.
