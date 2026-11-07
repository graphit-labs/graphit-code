---
title: Indexing, incremental updates and versions
description: Progressive availability, compatible embedding reuse, full/incremental indexing, branches and on-the-fly contexts.
content-type: guide
audience: developers
updated: 2026-10-29
previous: getting_started.md
---

# Indexing, incremental updates and versions

This guide explains when newly processed data becomes queryable and what is reused. It covers
local PROJECTs and authorized remote contexts. [Storage layout](../architecture/storage_layout.md)
contains physical paths; [retrieval](retrieval_architecture.md) explains which query to use.

## The four corpora

| Corpus | Authoritative input | Published text/structure | Background work |
|---|---|---|---|
| AST | Current selected checkout files and SCIP/syntax producer | Per-file Source, SCIP entities, lexical rows and Icebug graph | Compatible embeddings and native index maintenance |
| Knowledge | Configured documentation sources and optional README | Compiled pages, metadata, references and lexical rows | Compatible page embeddings and index maintenance |
| Memory | Authenticated CRUD with revisions | Current records and history in the same dataset | Incremental embeddings for eligible unchanged/current inputs |
| Task | Authenticated Task/Session/check/comment operations | Records, lifecycle and searchable public Task projections | Incremental compatible search embeddings |

A single project/corpus dataset represents versions with native Lance branches. Source and AST
entities have different granularities and remain distinct datasets; complete source is not copied
into every entity. Text and vectors within a searchable corpus do not use parallel authoritative
stores. A missing index, an empty corpus, partial coverage and a failed operation are different
states; no result alone proves the project contains no relevant data.

## First indexing of an empty project

1. Maintain the checkout through `graphit setup --agent codex --project-dir .` or the existing
   `projects maintain-checkout` command. Setup uses the real checkout version and does not accept
   a synthetic `--project-version`.
2. The daemon resolves identity, provider, version, ignores and effective parser/document settings.
3. For AST, supported specialized indexers run in Docker. An explicit custom extension route wins;
   the builtin generic syntax fallback produces SCIP when applicable. Compiler output is checked
   against the captured Source and selection before publication.
4. Each completed FILE publishes its own Source/entities/graph state. Those results can be read,
   searched lexically and traversed before the remaining corpus finishes.
5. Each compiled Knowledge file publishes its pages and lexical fields without waiting for the
   corpus or model. A malformed/oversized file returns the concrete diagnostic; it is not proof
   that every other file must be withheld.
6. If embedding is enabled and configured, background batches compute missing vectors and commit
   them progressively. Semantic search uses the valid vectors already published, even while
   coverage is partial. Lexical-only rows remain eligible for FTS/hybrid lexical retrieval.

Publication is progressive, not a transaction encompassing an entire repository. A single query
captures its own coherent publication/snapshot; it must not combine half-written FILE state. A
concurrent query can legitimately observe an earlier captured publication.

The AST worker discovers pending embeddings from an immutable Lance snapshot
without retaining the exclusive publication fence during that scan. Vector-schema
changes remain protected. Before processing a selected FILE, the worker captures
its current publication again and checks its pending state, source fingerprint,
revision and Raw version. A long pending-work scan therefore does not itself hold
back readers, and its snapshot cannot authorize a stale embedding update.

## Updating, renaming and deleting files

The watcher coalesces relevant changes and reprocesses changed files and their genuinely affected
relationships. It reuses unchanged content/compilation fingerprints, parser results and embeddings;
it does not reparse or re-embed every file for one edit. SCIP compiler indexers can have a broader
language/project compilation unit; generic syntax extraction remains per-file. Do not promise
single-file compiler execution for every language.

Changing a FILE withdraws obsolete facts and incompatible semantic eligibility. New lexical/graph
content becomes visible after that FILE publication; a background vector becomes visible only if
its identity and input fingerprint still match the current row. Deleting or renaming removes the
old identity/facts instead of leaving stale semantic hits. Rapid successive edits or cancellation
cannot attach an obsolete result to a newer row. Unrelated Icebug shards and Lance fragments are
shared rather than rewritten as full project copies.

Knowledge compares its physical and compilation fingerprints, source membership and settings.
It compiles changed inputs, updates/removes affected pages and preserves compatible vectors of
unchanged inputs. Recovery of an interrupted first publication follows captured generation/CAS;
it must not treat a concurrently published file as an empty baseline to erase.

## Memory and Task incremental semantics

CRUD commits authored state immediately. The semantic job derives pending work from persisted
rows; it is not a separate authoritative queue. It reuses a vector only when record identity,
public content fingerprint, model identity/revision, dimensions and recipe are compatible.
Task state or claim metadata that does not alter the embedded public input need not cause fresh
inference. Changing embedded content or vector space makes the old vector ineligible.

Searches can use partial published semantic coverage. `fts` needs no embedding model. `hybrid`
keeps lexical candidates that have no vector; `semantic` cannot invent a score for them. An
unconfigured or failed requested embedding service is an explicit error, not a silent switch to
lexical mode. `modules.embedding=false` preserves CRUD/FTS and prevents embedding model work.

## Branches, worktrees and Git commits

The stable `.graphit/project.json` ID identifies a project. Distinct worktrees of that same project
select independent native branches when their project versions differ; equal project/version
selections reach the same corpus. A new branch shares native fragments until writes diverge.
Project versions are not directories of copied datasets. USER Memory is separate from project
branches and is bound to verified identity and installation.

For Git projects, setup/product Git hooks bind PROJECT Memory/Task to immutable Lance tags for
commits. Checkout restores the matching record state, including compatible embeddings; merge
combines independent changes by family and reports divergent edits as conflicts. No transaction
spans Git and both stores. Unsnapshotted record edits must be reconciled, not discarded.
[Git record contracts](../specs/git_records.md) describe recovery and rewrites. Projects without Git
use normal version selection and need no artificial commits or hooks.

## Local, Broker and on-the-fly contexts

Local AST persists Icebug Parquet and queries it through in-memory Ladybug. Hub AST uses the same
query path over authorized S3 ranges; it does not require a `.lbug` download or repository clone.
Installed Knowledge uses its captured native dataset and authorization, without requiring the
producer checkout. Imported artifacts are pinned versions; local edits do not mutate them.

When discovery selects S3, Memory/Task open their authoritative remote tables directly. Credentials
are scoped/renewed by the Broker contract; data travels between Lance and storage. A new operation
observes remote commits under the SDK's guarantees; an existing read can retain its snapshot.
Auth/STS/S3 failure never authorizes a silent local write. See [S3](s3-and-ui-network.md).

An active Broker provider routes embedding and rerank only through Broker endpoints (or explicit
disabled modes). It cannot borrow a local model or direct provider. A Local provider can select
these services independently. Hardware settings affect only actual local inference.

## Verify availability without waiting for all embeddings

```sh
graphit ast search 'checkout' --mode fts --rerank=false
graphit ast schema
graphit ast source Cargo.toml --head 20
graphit knowledge search 'configuration' --mode fts --rerank=false
graphit knowledge source --help
graphit task search --text indexing --mode fts --page-size 10 --top-k 10
```

Use the exact paths, slugs and entity identities returned by discovery. Query the AST schema before
constructing a relationship traversal. Semantic/hybrid reads require the configured embedding
service and valid vectors; they do not wait for full coverage. Rerank is independent and off by
default. [CLI reference](cli_reference.md) and [MCP schemas](../reference/mcp-input-schemas.json)
contain current argument shapes.

`graphit sync` reconciles installed artifacts/agent files; it does not force index completion.
Stop hooks request an asynchronous rescan. To investigate errors, inspect daemon status and the
specific FILE/job diagnostic rather than deleting stores, changing global quotas or repeatedly
restarting an index. Native maintenance respects branch heads, Git tags and retained snapshots.
