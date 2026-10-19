---
references:
  - target: {type: task, id: tsk-da58}
    relation: specified_by
  - target: {type: task, id: tsk-a684}
    relation: implemented_by
---

# Memory Module Specification

Memory preserves durable project facts, decisions, conventions, corrections and reusable procedures.
It is authored data, not a compiled wiki. This contract describes the current Rust Records services,
[Browser](ui_dashboard.md), search and background jobs. The lower-level specifications retain
field and storage details: [native store](native_memory_store.md),
[search](native_memory_search.md), [embeddings](native_memory_embeddings.md) and
[USER storage](native_user_memory_store.md).

## Scopes and storage

| Scope | Authority and identity | Versioning |
|---|---|---|
| PROJECT | Verified permission for the stable project ID | Native Lance branch for the selected project version; Git snapshot integration when installed |
| USER | Verified issuer/subject plus installation binding, under the USER admission | Independent of project branches and Git |

The local PROJECT dataset is `<global>/projects/<project-id>/memory/index.lance`.
USER uses `<global>/users/<principal-key>/runtime/memory/index.lance`. The principal key comes from
verified identity, not a username query parameter or an `anonymous` substitute. A path, CORS
policy, `unit.id` or PROJECT read grant does not authorize another user's Memory.

When the active provider's valid discovery enables S3, resolve the authorized PROJECT/USER
`memory` root and open its Lance dataset directly. Obtain scoped temporary credentials through
the existing STSv3 contract; retain valid credentials and renew when needed. Do not use a local
mirror, write/upload queue or fallback on authentication/STS/S3 failure. Valid discovery explicitly
disabling remote storage can select local. New operations capture current committed heads while
already captured readers can retain snapshots. See [S3](../guides/s3-and-ui-network.md).

Each corpus holds records, revision history, references, lexical state and nullable compatible
vectors in the same dataset. PROJECT versions share native fragments until writes diverge.
No per-version copied dataset, `lineages` registry, Markdown source directory or compiled Memory
wiki is involved. There is no table deletion/recreation or permanent importer when model width
changes; incompatible semantic associations are replaced through normal indexing while authored
history is preserved. [Storage layout](../architecture/storage_layout.md) defines maintenance.

## Record schema and revision chain

The public record contains canonical ID, positive revision, title, Markdown body, classification,
independent `mandatory`/`important` flags, tags, authored references, reference generation, actor
attribution and creation/update timestamps. The library distinguishes authored input, current
record, creation event, revision events and tombstone state. Wire names/encodings come from the
actual consumer codec, not from an old `MemoryRecord` struct or a generic wiki page.

Classification describes meaning; importance influences curated retrieval and mandatory means
unconditional full recall at the defined agent bootstrap boundary. Neither flag grants access or
implies the other. Explicit typed references create relations; mentioning another ID in prose
alone does not. Omitted reference patches preserve, `[]` clears, and a supplied list replaces
explicit references; malformed or unknown fields are rejected.

## Writes, concurrency and history

Create uses a stable idempotency key and returns the producer's confirmed record. Updates/deletes
use the expected current revision and authenticated write admission. Stale revisions or competing
changes produce conflicts rather than last-writer-wins overwrite. A successful mutation advances
its real revision and retains immutable creation/history; removal makes the current record
unavailable without erasing the authorized audit history.

Do not infer rollback from timeout or a lost response. Inspect current state/history and reported
confirmed/uncertain effects before retry. Cancelled or expired Original contexts cannot continue
into a new IO or delivery step. Recovery uses existing producer operations; direct table editing
is not a public API.

PROJECT Git hooks retain immutable commit tags for Memory and Task, restore corresponding state
on checkout and merge independent family changes against their merge base. Conflicting changes
preserve both sides and fail explicitly; USER Memory is never included. No atomic transaction
spans Git and both datasets. See [Git records](git_records.md).

## Search, incremental embeddings and reuse

FTS needs no embedding service. Semantic search requires the selected service/vector space;
hybrid combines lexical and vector channels while retaining lexical candidates without vectors.
The current filters distinguish classification, literal tags, mandatory, important and exclusion
of already loaded mandatory records. Returned match location/revision identifies whether the
match came from current or explicitly eligible historical content. Search does not turn a
historical/deleted match into a live `get` result.

Writes publish authored/textual state immediately. Background inference selects only missing or
incompatible public inputs, computes batches outside the writer, and attaches results after
rechecking identity/content fingerprint/model recipe. Compatible unchanged records reuse their
vectors; partial committed semantic coverage is queryable while the rest advances. Changing a
body or vector space makes the old association ineligible. No whole-corpus inference or external
semantic mirror is required for each edit. `modules.embedding=false` preserves CRUD/FTS and
prevents embedding preparation/loading/inference. Rerank is a separate AST/Knowledge concern.

Unconfigured/failed requested services return explicit errors; they never silently impersonate
FTS. Broker provider inference is Broker-only; Direct and Local selections cannot bypass it.
See [retrieval](../guides/retrieval_architecture.md) and [model identity](ai_engine.md).

## Initial recall protocol

Agent lifecycle adapters load the current mandatory selection at the configured bootstrap
boundary. The agent reads all mandatory content before contextual recall, then uses search and
selected exact records for the current question. Contextual queries can exclude mandatory rows
already loaded. Static skills/agent files contain normative policy only, never copied private
runtime memories, credentials or a standing-context snapshot.

Persist only non-sensitive, project-inherent reusable knowledge in PROJECT. A transient request,
progress report, unconfirmed hypothesis or personal detail is not a durable shared fact. Task
retains execution evidence; Memory supplements it with confirmed reusable conclusions.

## Consumers and presentation

The Browser Memory Explorer uses its verified actor for catalog/detail/history and CRUD HTTP
operations; it is not a wiki adapter. Markdown rendering is a view of authored record fields,
not another authoritative store. Current head/detail/history and conflict behavior must stay
consistent across the same provider/project/version selection.

The current Root MCP catalog exposes `graphit_memory_search` and `graphit_memory_index`.
Do not assume unadvertised CRUD/source/mandatory tool names exist because an older guide listed
them. Role/Live catalogs and Browser APIs have their own admitted surfaces; inspect the actual
catalog and [MCP reference](../guides/mcp_tools_reference.md). CLI currently exposes `memory index`.

`memory index` ensures the lexical index and configured semantic maintenance; `--refresh` explicitly
replaces lexical index state. Reads do not perform an implicit global rebuild. Native maintenance
folds/compacts through SDK APIs and respects branches, Git tags and retained snapshots. It does
not delete a remote corpus when a local checkout stops using it.

## Implementation map

- `graphit-records/src/memory_store/`: current records, revisions, validation and native storage.
- `graphit-records/src/memory_store/embeddings/`: captured inputs, associations and searches.
- `graphit-app/src/record_storage.rs`, `native_storage_credentials.rs`: provider/credentials.
- `graphit-app/src/foreground_memory/`, `foreground_memory_jobs/`: productive CRUD/search/jobs.
- `graphit-app/src/browser_auth/`: verified Browser admission; `ui/src/components/memory/`: views.

Tests qualify their exact boundary. Linux/native GGUF evidence does not imply unexecuted
Windows/macOS, another vector recipe or a remote storage configuration has been validated.
