# Memory Module Specification

The Memory module stores durable preferences, corrections, decisions, and learned
procedures for AI agents. Raw Markdown is the source of truth; a compiled wiki is
the query surface. Optional S3 publication shares scopes across machines without a
Git repository.

## Scopes and local storage

Memory has two primary scopes:

- `project`: shared architectural facts, workflows, and team conventions, keyed
  by the project's ULID.
- `user`: personal preferences and workstation conventions, keyed by the user
  identity hash.

Imported memory contexts use the same mechanism with a context name. All stores
live once in the global brand directory:

```text
~/.graphit/
├── memory-raw/memory-project-<project-id>/   raw Markdown, source of truth
├── memory-raw/memory-user-<user-id>/         raw Markdown, source of truth
└── wiki/memory/<scope>/<id>/                 compiled searchable wiki
```

Nothing is copied into a project's `.graphit` directory. See
[Storage Layout](../architecture/storage_layout.md).

## Optional S3 backend

Memory shares the Hub's S3 configuration; there is no `memory.repo` or separate
memory bucket key. With `hub.bucket` configured, a scope maps directly to:

```text
s3://<bucket>/<hub.prefix>/memory/<scope>/<id>/
```

A complete explicit credential pair from global Graphit config is passed to the
S3 client. When either explicit value is absent, the AWS SDK provider chain is
used. With no bucket, every memory operation remains available locally and no
objects are uploaded. See
[S3 Credentials and UI Network Configuration](../guides/s3-and-ui-network.md).

### Pull, publish, conflict, and deletion semantics

- `Pull` merges remote objects into the raw directory. It does not mirror-delete
  local files, because an unpublished local memory must survive synchronization.
- `Publish` uploads the raw scope in the background. The CLI and daemon wait for
  pending uploads on shutdown so the last write is not abandoned.
- `RemoveFile` records a local deletion; the next publish removes the matching
  remote object.
- Independent memory IDs do not conflict. Concurrent edits to the same object are
  last-writer-wins.
- Pruning a local scope reclaims local disk and deregisters it; it deliberately
  does not delete the remote prefix another machine may still use.

Object storage has no commit message. Audit history lives in the memory itself:
`revision`, `previous`, and `updated_by` frontmatter identify the chain, and
superseded bodies are archived under the memory history path.

## Memory card structure

Each card is a Markdown file with YAML frontmatter and a structured body:

```yaml
---
title: "Prefer table-driven Go tests"
type: "preference"
tags: ["go", "testing"]
created_at: 2026-08-24T12:00:00Z
updated_at: 2026-08-24T12:00:00Z
important: true
revision: "01..."
updated_by: "user-id"
---

# Prefer table-driven Go tests

## What
Use table-driven cases for related Go scenarios.

## Why
The shared setup makes edge cases visible and keeps assertions consistent.

## How
Name every case and run it with `t.Run`.

## Impact
New tests follow one reviewable pattern.
```

The What/Why/How/Impact sections make the instruction actionable for an agent.

## Shards and compiled wiki

After a successful compile, content-addressed chunk and embedding shards are
mirrored beside the raw scope under `.wiki/shards/` and published with it. Another
machine can reuse those vectors instead of embedding unchanged text again.

The compiled pages and LanceDB index remain local and are rebuilt from raw
Markdown plus valid shards. Imports are additive; a shard is used only when its
source hash matches. The generated search database is never the source of truth.

## Write and consolidation cycle

A normal memory mutation follows this chain:

```text
pull/merge remote → write raw Markdown → compile wiki → mirror shards → publish in background
```

The consolidation cycle can deduplicate related cards, resolve superseded
instructions, promote durable facts, and maintain history. It changes raw cards
first and recompiles the same scope; it never edits the compiled index as primary
data.

## Daemon synchronization

The global memory sync module discovers active scopes from the global memory lock
and watches the `memory-raw/` root. It debounces filesystem changes, recompiles only
the touched scopes, and handles lost-event rescans by recompiling all registered
scopes. The module runs once per daemon because memory storage is global, not once
per project.

Remote failures do not erase the local raw source. They are logged and retried by
later synchronization, while an unconfigured bucket stays intentionally local-only.
