---
references:
  - target: {type: task, id: tsk-7240}
    relation: specified_by
  - target: {type: task, id: tsk-a542}
    relation: uses
  - target: {type: task, id: tsk-0264}
    relation: uses
  - target: {type: task, id: tsk-08bd}
    relation: uses
  - target: {type: task, id: tsk-3ddd}
    relation: uses
  - target: {type: task, id: tsk-9ccd}
    relation: derived_from
  - target: {type: task, id: tsk-0137}
    relation: supports
  - target: {type: task, id: tsk-795b}
    relation: relates_to
  - target: {type: task, id: tsk-8c29}
    relation: relates_to
  - target: {type: task, id: tsk-cab0}
    relation: relates_to
---

# Rust Now PROJECT head sources

Audience: Records, Data, Runtime and Broker maintainers. This page defines the
domain projections introduced by `tsk-7240`. Knowledge's metadata reader, Task
activity and native Memory have Main/Linux evidence. Current consumer behavior
and qualification scopes belong to the [project reader](rust_runtime_project_now.md)
and [Now publisher](rust_runtime_now_publisher.md) contracts. The active-corpus
Native integration remains open in `tsk-cab0`; producer evidence alone does not
qualify that journey or native Windows/macOS execution.

The maintained [Workspace Now contract](design_system/workspaces.md#workspace-now)
requires all project contributors, active sessions and tasks, recent PROJECT
Memory and Knowledge, sorted before limiting, authoritative session counts
and visible individual source failures. The source contracts below implement
the data requirements without borrowing claim authority from presence or the
selected CLI agent.

## Producers and current evidence

| Sections | Producer and API | Actual status and scope |
|---|---|---|
| Tasks and sessions | Records `NativeTaskStore::observe_heads(original, ProjectReadAdmission, TaskStoreReadLimits) -> Result<TaskHeadObservation, TaskStoreError>` | `tsk-08bd` COMPLETED r17, all six checks Passed; actual Source in `task_store/observation.rs`. Foundation `tsk-0264` COMPLETED r18 alone lacks this activity/isolation outcome. See the [native Task contract](native_task_store.md) for scope and remaining product work. |
| Memory | Records `NativeMemoryStore::observe_heads(original, ProjectReadAdmission, MemoryHeadReadLimits) -> Result<MemoryHeadRead, MemoryStoreError>` | Exported Source `tsk-3ddd`, Main/Linux integration10, library45, five doctests, strict Records Clippy and owned formatting Passed. Contract `tsk-9ccd` COMPLETED r11 and fixed Core birth `tsk-8b90` complete. See [native Memory contract](native_memory_store.md). |
| Knowledge | Data `read_native_knowledge_heads(original, ProjectReadAdmission, KnowledgeHeadReadLimits)` | `tsk-a542` records the original Main/Linux producer evidence. The current reader in `knowledge_index.rs` projects eight metadata and reference-witness columns, validates document state and references, and returns lightweight heads; it does not read full document bodies. |

Records already depends on Data. A Data compositor importing Records would
introduce a cycle. Runtime owns composition and can use Records plus Data when
its owner composes the actual dependency/API changes. `tsk-0137` remains the
project/version mapping unit; it does not become a Now implementation. This page grants no ownership of Runtime
source or manifests. `tsk-8c29` remains a caller/infrastructure contract, not
a substitute domain head reader. Reuse it only where its actual API is needed;
its whole completion is not a prerequisite of every native read.

## Admission, namespace and lifetime

Obtain real module Read admissions for Task, Memory and Knowledge from the
actual original request and selected PROJECT/version. Task and sessions share
one Task admission and pin. Keep the original `Arc<SnapshotContext>` and the
borrowed handles throughout each operation, with module/request/scope/brand/
version/captured-path checks at the producers' actual boundaries and final
delivery. Actor, module classification, SDK generation and serialized IDs are
observations; none reconstructs authority.

Module denial does not become an empty section. Count only the complete set
that the actual module admission permits. USER Memory is excluded; a canonical
logical reference to USER is not a USER read. Broker publisher ownership is
not the owner of a Task or session record. The selected agent is a CLI context
and cannot filter project contributors or fabricate authorship.

Task/session share one actual SDK generation. Memory and Knowledge have their
own pins. A common request does not establish a cross-corpus atomic generation.
Remote links use logical instance/project/version/record identities and the
consumer's established route contract. They cannot contain a corpus URI,
checkout path, physical branch name or private recovery key.

## Activity and authoritative counts

`TaskActivityHead` contains canonical `id`, `session_id`, title, status,
`Ownership`, optional `ProgressObservation`, revision and created/updated
epoch-millisecond times. `SessionHead` adds the same actual ownership and
progress observation to its ID/title/status/revision/update time and
completed/total linked-Task counts.

`Ownership` carries actual owner, claim epoch, claimed/heartbeat/lease expiry
times. The domain codec validates their relationships. Work is active only
when status is `in_progress`, owner is present and the lease is strictly later
than the snapshot's checked UTC epoch-millisecond observation time. Equality
is expired; absent lease is inactive. Do not infer a lease from owner, creation
time, a WebSocket connection or an instance publisher. Reads do not expire,
claim, renew or repair records.

Progress in heads is only typed kind (`Progress` or `Checkpoint`), sequence and
observation time. Raw summary, next_step and checkpoint bodies remain in full
domain reads. Remote Now's final allowlist excludes raw progress; even typed
observations must be explicitly chosen by the consumer's final DTO rather
than serialized through the whole Records struct.

Compute session counts from the complete same-pin current Task set, before
filtering active tasks or limiting any section. Cancelled tasks contribute to
neither total nor completed; each other linked task contributes to total and
only completed contributes to completed. A valid session with no non-cancelled
linked tasks has 0/0. Missing or invalid
Task data cannot be represented as 0/0. Canonical Task-to-session linkage and
global current-ID uniqueness are domain invariants.

## Memory and Knowledge metadata

Memory's actual current predicate is `kind=head`, `superseded=false` and
`tombstone=false`; its projected fields are ID, literal title, recorded last
writer and checked UTC epoch-millisecond update time. Initial authorship is
not replaced with the requester, selected agent or Git identity. Creation
audit/history/reference bodies never enter this projection.

Knowledge's actual `KnowledgeHead` contains literal slug/title and optional
`SystemTime` update value. Its reader checks the full physical schema and
projects `slug`, `title`, `updated_at`, `source_path`, `compilation_digest`,
`resolved_wiki_targets`, `document_state` and `reference_projection`.
`decode_head_batch` validates the reference witness and namespace, skips removed
documents and returns only the lightweight head. Document bodies, recipes and
embeddings are not projected. Dates preserve their native
signed UTC nanoseconds and None stays unknown; no author is invented. The
consumer must preserve precision in its chosen wire representation, including
values outside JavaScript's safe integer range, before that DTO becomes ready.

Knowledge and Memory return typed NoCorpus only for valid captured absence.
An empty existing publication retains its generation and returns zero heads.
Schema, request, path, Auth or SDK failures remain errors. No generic recovery
fallback, body scan, cache of the first request, repair or publication occurs
during a Now refresh.

## Complete reads, sorting and partial failure

Consume each producer to its complete EOF and final check. Do not apply an
SDK limit of 20 or truncate the Task set used for session counts. On successful
sections, filter activity/current records, compute total, sort update time
descending and use canonical ID/slug ascending as the stable tie, then take
20. Unknown Knowledge update values sort last. has_more is total greater than
the returned item count. A request or file limit failure is unavailable, not a complete
truncated set or empty success.

Preserve the producers' concrete file, field and batch validations. Knowledge's
head reader uses native document limits, a positive batch size and a slug byte
limit; these do not impose an aggregate corpus quota. Continue to EOF before
sorting and limiting the presentation. Do not add resource reservations,
cumulative accounting or a shared quota framework to compose these readers.

Task's published observation has independent
`Result<Vec<TaskActivityHead>, Box<TaskStoreError>>` and
`Result<Vec<SessionHead>, Box<TaskStoreError>>` fields. A row-local session
presentation/progress failure preserves a valid Task section after EOF.
A Task head failure makes Tasks unavailable and session counts unavailable.
Schema/stream/Auth/request/capture failures, malformed or duplicate identities
and missing Task-to-session links fail the entire Task/session observation;
they are not row-local errors. Existing catalogue/read_view contracts are not
weakened to emulate this behavior.

Independent successful Memory/Knowledge sections remain usable after a
Task/session source error while their own original admissions remain valid.
A cancelled or expired original request cannot deliver a snapshot. Final
delivery must not retain data whose used admission has become invalid.

`tsk-795b` still owns the exact transport envelope, finite error codes,
unavailable count representation, fresh project ACL/subset rules and stream
ordering/reconnect contract. A failed section must not invent total=0 or
has_more=false: its counts are unavailable. The final nullable/variant wire
shape must be closed by that owner before Source. Preserve detailed causes
locally, and map only approved finite codes remotely. Never serialize SDK
errors, paths, credentials, tokens, raw progress/next_step, body/audit,
embeddings or private USER data through an unrestricted domain struct.

## Consumer readiness and verification

Keep native producer evidence separate from its Runtime/Broker consumers.
The implemented project reader and publisher compose the domain APIs above;
their maintained contracts record current transport, authorization and lifecycle
behavior. Reuse valid evidence from unchanged code, and execute affected
regressions when behavior changes. No new refinement task or repetition of a
producer suite is required solely because an aggregate delivery remains open.

Relevant composition coverage includes all contributors,
lease equality/expiry/absent-owner cases, same-pin completed/cancelled totals,
session-local failure with successful tasks, global corruption/late Auth
failure, current/superseded/tombstone Memory, recorded historical author,
Knowledge absent/empty/corrupt/None/nanosecond extremes, more than 20 records
with stable ties and totals, A/B and version isolation, denied-module counts,
concurrent publication, cancellation and metadata-only projection. Each module
and aggregate must retain honest partial/error semantics.

Runtime/App owns the five-second mounted refresh, no-store request/response,
in-flight deduplication, old snapshot while refreshing, stale/retry state and
abort/ignore after navigation or context change. Broker owns real authorized
two-instance HTTP/socket/reconnect fixtures. CLI owns its mature TTY/NDJSON/
pipe behavior and original cancellation contract under `tsk-0137`. No UI was
changed by this domain contract, and the producer tests cannot qualify those journeys,
Kiro, Windows/macOS or end-to-end product validation.
