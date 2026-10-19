---
title: "Persisted record relationships"
description: "Typed relationships across Task, Session, Memory and Knowledge, including authoring, queries and recovery."
content-type: reference
audience: developers, ai-agents
references:
  - target: {type: task, id: tsk-96c8}
    relation: implements
  - target: {type: task, id: tsk-265d}
    relation: implements
  - target: {type: task, id: tsk-6286}
    relation: requires_contract
  - target: {type: task, id: tsk-8e7b}
    relation: requires_contract
  - target: {type: task, id: tsk-85c4}
    relation: requires_contract
  - target: {type: task, id: tsk-c62d}
    relation: requires_contract
  - target: {type: task, id: tsk-9613}
    relation: requires_contract
  - target: {type: task, id: tsk-fb6e}
    relation: requires_contract
  - target: {type: task, id: tsk-c623}
    relation: requires_contract
  - target: {type: task, id: tsk-5995}
    relation: uses
  - target: {type: task, id: tsk-41d1}
    relation: requires_contract
  - target: {type: task, id: tsk-c471}
    relation: requires_contract
  - target: {type: task, id: tsk-17a7}
    relation: requires_contract
  - target: {type: task, id: tsk-55e8}
    relation: requires_contract
  - target: {type: task, id: tsk-72ce}
    relation: requires_contract
  - target: {type: task, id: tsk-633b}
    relation: requires_contract
  - target: {type: task, id: tsk-319e}
    relation: requires_contract
  - target: {type: task, id: tsk-12df}
    relation: requires_contract
  - target: {type: task, id: tsk-9a83}
    relation: requires_contract
  - target: {type: task, id: tsk-74cc}
    relation: requires_contract
  - target: {type: task, id: tsk-29fb}
    relation: requires_contract
  - target: {type: task, id: tsk-cd41}
    relation: uses
  - target: {type: task, id: tsk-b777}
    relation: specified_by
  - target: {type: task, id: tsk-6cf9}
    relation: requires_contract
  - target: {type: task, id: tsk-8e5d}
    relation: requires_contract
  - target: {type: task, id: tsk-7fc2}
    relation: requires_contract
  - target: {type: task, id: tsk-bd00}
    relation: uses
  - target: {type: task, id: tsk-16a8}
    relation: derived_from
  - target: {type: task, id: tsk-9983}
    relation: uses
  - target: {type: task, id: tsk-c8e9}
    relation: uses
  - target: {type: task, id: tsk-2f62}
    relation: uses
  - target: {type: task, id: tsk-8c07}
    relation: uses
  - target: {type: task, id: tsk-9d61}
    relation: uses
  - target: {type: task, id: tsk-3514}
    relation: uses
  - target: {type: task, id: tsk-f02f}
    relation: uses
  - target: {type: task, id: tsk-d7c3}
    relation: implements
  - target: {type: task, id: tsk-ef75}
    relation: uses_api
---

# Persisted record relationships

Graphit stores relationships between engineering records so agents and engineers can inspect provenance, dependencies and reuse across tasks, sessions, memories and knowledge pages. A relationship is data in the database. Writing an identifier in a description or rendering a Markdown link does not create one.

## Write explicit references

When a record relies on another record, the agent must resolve its real ID and send `references` in the write input. The source is the record being created or updated. Each reference contains:

| Field | Meaning |
| --- | --- |
| `target.type` | `task`, `session`, `memory` or `knowledge` |
| `target.id` | Actual Task/Session/Memory ID or Knowledge page slug |
| `target.scope` | `project` or `user`; defaults to the source scope |
| `target.scope_id` | Logical project or user identity; defaults only within the source scope |
| `target.context` | Knowledge context; inherits only within the same source namespace |
| `relation` | Semantic relation, for example `supports`, `implements`, `derived_from` or `relates_to` |
| `field` | Optional provenance within the source record |

Illustrative fragment for a Task write, after resolving the memory ID:

```json
{
  "references": [
    {
      "target": {"type": "memory", "id": "<verified-memory-id>"},
      "relation": "supports"
    }
  ]
}
```

Send the complete intended explicit list when changing relations. Omit `references` to preserve existing links; send `references: []` to clear explicit links. Read the current list before replacing it. Structural Task links (`session_id`, `parent_id`, `depends_on`) remain managed by their existing fields and are not cleared by an empty explicit list.

Task create/revise/progress/comment/check/completion and related text mutations, Session create/revise/checkpoint/completion and related text mutations, Memory insert/update, and Task batch operations accept structured references. The Memory HTTP create/update endpoints accept the same field. Existing claim, revision and scope checks still apply. A relationship never grants access to its target.

For Knowledge, put the same structure in the authoritative Markdown's existing YAML frontmatter:

```yaml
references:
  - target:
      type: memory
      id: <verified-memory-id>
      scope: project
      scope_id: <logical-project-id>
    relation: derived_from
```

Indexing validates the complete batch before changing the compiled corpus. Invalid types, missing IDs and a non-list `references` value fail validation. Omission preserves already indexed authored references; `references: []` clears them. Keep references in the source for reproducible indexing into a new database. Existing wiki cross-links are persisted separately as `wiki-link` relations with `field: wiki-links`.

## Read and analyze

`graphit_references_query` reads current persisted edges from the authorized project's Task, Memory and Knowledge stores. It supports `source_type`/`source_id`, `target_type`/`target_id`, `relation`, source `scope`/`scope_id`/`context`, and cursor pagination. For backlinks, filter the target; `target_scope`, `target_scope_id` and `target_context` disambiguate its namespace. `include_user: true` also reads the current authenticated caller's personal memory; it is false by default. Imported Knowledge comes only from the project's registered contexts.

The result contains `relations.results`, optional `relations.next_cursor`, `complete` and `warnings`. Qualify identities when different scopes or contexts can contain the same ID. A stored target may be unavailable or outside the current read scope; its identity remains usable for analysis. The UI only attaches a navigation URL after resolving an exact authorized target. It shows outgoing references and incoming backlinks, and marks unresolved targets without inventing a local link.

The UI endpoint is `GET /api/references?project_dir=...&kind=task&id=...`, with optional `scope` and `context`. It reads stored relationships; opening or refreshing a viewer does not create them. Knowledge targets use the selected context and a `page` query parameter to open the actual page.

## Storage and recovery

`record_relations` uses the same schema in each source module's authorized LanceDB store. Each row records typed source and target identities, scope/context, relation, field, origin, source revision and source snapshot hash. Knowledge's containing store supplies its project/import context namespace when composing queries; the index remains portable.

A complete immutable generation, including an empty-generation marker, is merged in one relation-table commit. Readers match generations against authoritative current heads before choosing current edges. Older replayed generations cannot restore cleared links. Task and Session carry explicit reference lists in their existing durable snapshots. Memory writes the relation generation before committing the matching head; Knowledge validates metadata before corpus changes. These are separate table commits, not a cross-table transaction. Failed or missing projections are reported as incomplete instead of silently presenting stale edges.

`graphit_references_reconcile` replays known structured metadata and existing structural links in writable project stores; `include_user` optionally includes the caller's memory. It does not infer references from prose, alter record bodies, or rewrite imported Knowledge. It reports module failures individually and can be repeated. Normal Knowledge indexing also repairs missing projections on unchanged-content paths. Memory reconciliation writes only relation projections, so it cannot replace a concurrently edited memory head.

Existing records without explicit references retain their content. Task structural links and existing wiki cross-links can be projected; arbitrary mentions require an explicit author update. Relation maintenance may remove generations older than a marker matching the authoritative committed head; it keeps that marker and any newer generation awaiting a head commit. Knowledge runs this maintenance during indexing; Task and Memory run it in their scheduled store maintenance. Each module then compacts and prunes relation-table versions according to its retention setting. Deletion removes the source from current reads; references from other records can remain as unresolved provenance.

Native Knowledge packages carry their indexed relationship metadata along with their corpus. They do not include the referenced Task or Memory record bodies. A target remains subject to the consumer's own access and namespace resolution.

## Implementation and validation

The storage contract lives in `internal/relations`; authorized composition and filtering live in `internal/references`. Producers are Task, Memory and Wiki services. MCP tools and UI handlers share those services. Agent skill and lifecycle mandate generators include the explicit write contract, examples and query route.

Native storage tests cover reopens, omission versus clearing, stale-generation replay, invalid metadata, Memory body edits, Knowledge reconciliation and namespace isolation. MCP schema tests assert the nested typed write contract and agent instructions; UI/API tests verify exact authorized navigation and backlinks.

## Rust authored-reference projection

`graphit_records::reference_projection::project_authored_references` validates
borrowed reference DTOs and returns one `AuthoredReferenceUpdate`: `Preserve` for
an omitted field, or `Replace(Vec<LogicalReference>)` for a sequence. An empty
sequence explicitly clears authored references. Null, a non-sequence, or an
`InvalidShape` entry fails the complete operation. The caller represents optional
null/non-string values as `InvalidShape`; `None` means omission only. Order and
duplicates remain intact. Structural links are outside this update.

The input includes the original `SnapshotContext`, `SourceNamespace` and an
`AuthoredReferencesField`. Entry slices and text owners have separate lifetimes;
the result borrows text and namespace strings, not the temporary candidate slice
or request. `LogicalScope` uses the actual Core `ProjectId` or a nonempty opaque
user subject. Explicit project IDs use Core's canonical nonzero ULID parser and
retain its error cause. User subjects are never installation identities.

Target types are exactly `task`, `session`, `memory`, `knowledge`; scopes are
exactly `project`, `user`, with case preserved. Omitted scope inherits the source
kind. Omitted scope ID inherits only when the chosen kind equals the source kind;
another kind requires an explicit ID. Knowledge context inherits only when the
fully resolved scope kind and ID equal the source namespace. A foreign namespace
does not inherit context. Explicit knowledge context must be nonempty; other
target types reject context. Source contexts also must be nonempty when present.

Target IDs and relations are nonempty literal strings. They are not trimmed,
normalized or constrained to a closed relation vocabulary. Optional `field`
provenance is preserved, including an empty string. All four target kinds accept
either logical scope, even for unresolved targets. This pure projection performs
no lookup, authorization, domain-state query, parser adaptation, I/O or storage
mutation. The consumer enforces access and publication in its own real boundary;
a logical reference supplies no authority.

The original request is checked before shape/source validation, for every entry
and before returning success. Request cancellation/deadline errors retain their
`io::Error` and `Error::source()`, as do project-identity errors. No context or
deadline is created or retained. Knowledge's syntax-arena adapter and Data's
publication consumer remain separate deliveries; this kernel does not claim
their integration.

Run `cargo test -p graphit-records --test reference_projection --locked` for
shape, namespace, literal, error and ownership fixtures. The module's compile-fail
doctest verifies that output cannot outlive its text owner. These checks qualify
the pure Rust projection, independently of storage/MCP/UI integration.

## Rust Task/Session persistence and query: native Linux qualification

The completed Source-free contract `tsk-b777` defines Records SDK `tsk-6cf9`, App route `tsk-8e5d` and remaining FullNoLoss refinement `tsk-7fc2`. The [native references Source](../../crates/graphit-records/src/task_store/references.rs) is implemented and Linux-qualified by the affected native SDK, library and protocol suites, Records doctests and scoped strict Clippy. The App route and cross-source refinement remain separate planned deliveries. The qualified pure `tsk-bd00` kernel and preserved `tsk-16a8` FullNoLoss history do not qualify this persistence/query implementation. Earlier service/MCP/GET and separate-table recovery descriptions remain their own contract, not a delivered Rust HTTP route. Current [Task models](../../crates/graphit-records/src/task_store/model.rs) require references/reference_generation and exports provide the direct native query. The existing [strict Task writer](../../crates/graphit-records/src/task_store/write.rs) and [Data prepare_merge](../../crates/graphit-data/src/lance_branch_store.rs) are its real single-delta/CAS/Confirmed substrate; no new Data API is proposed.

### Linux-qualified native SDK publication

`tsk-6cf9` Source adds a borrowed AuthoredReferencesField before limits to create_task/create_session and the thirteen actual native text/lifecycle mutators, including comment, revise/check/supersession/link/flag, progress/checkpoint, completion/cancellation and release. No public revise_session or old overload is invented. Use the existing kernel with the admission’s actual PROJECT SourceNamespace, context None and same Original. Validate the whole input before clones, birth, prepare or SDK effects; omitted preserves, [] clears only authored links, and null/nonsequence/InvalidShape rejects. Owned TaskReference preserves all four logical target kinds, project/user scope, Knowledge context, literal ID/relation/field, order and duplicates; references grant no access.

TaskStoreWriteLimits gains explicit TaskReferencesInputLimits { max_entries: NonZeroUsize, max_text_bytes: NonZeroUsize }, mapped by the caller from existing ContentLimits.max_collection_entries/max_input_bytes. Charge entry count and checked supplied string bytes before kernel/retention; overflow/cap is an error, not a prefix. Core ArtifactLimits includes the CopyLimits for concrete filesystem operations; authored reference bounds remain in TaskReferencesInputLimits. No unlimited default, new global knob, ledger or RSS guarantee is introduced.

Required Task/Session snapshot fields are references Vec and reference_generation, without defaults for old snapshots or a converter/rebuild. Generation equals the published domain revision, including a claim/renewal that actually publishes. Claim/heartbeat take no authored patch and preserve state; already-owned no-publication returns create no marker. A reference-only changed revise follows the existing owner/fence/CAS/lease/reason guards but preserves specification/check statuses. It has an explicit branch rather than swallowing planner NoChange; total no-op keeps its existing semantics. Store-local creation/comment/lifecycle audit retains normalized Preserve versus Replace identity, so create/comment retry compares omission versus [] before Existing. Historical replay never reapplies an old patch; the pure TaskComment kernel is unchanged.

Each changed publication includes a typed Row::ReferenceProjection, head and creation/event/comment in one existing strict commit in the same Task index.lance. The projection key binds source kind/ID/revision; its payload derives PROJECT/version from capture and carries the complete ordered edges, including a real empty marker. Task order is session, optional parent, dependencies in snapshot order, then authored order; structural literals are session/parent/dependency with field None. Session has authored edges only. Structural|Authored provenance, origin and duplicates remain explicit. Canonical typed payload/header/all 30 columns are decoded and re-encoded for equality; public_search for this family is {}. This new family passed the affected native Task SDK/library/protocol qualification, including typed-corpus regression coverage. There is no second table/writer/commit, new deletion/retention API, repair fallback or generation counter. Existing SDK failure/recovery key and Confirmed version/cause remain causal.

### Linux-qualified same-pin query

The implemented async NativeTaskStore::references(original: &Arc<SnapshotContext>, admission: &ProjectReadAdmission<'_>, query: TaskReferencesQuery, limits: TaskStoreReadLimits) returns Result<TaskReferencesRead, TaskStoreError>. Query focus is TaskReferenceRecord::Task(TaskId) or Session(SessionId), with page_size: NonZeroUsize and separate outgoing_cursor/incoming_cursor. The reader consumes TaskStoreReadLimits directly: native file policy and scanner batch size, without aggregate corpus quotas. One actual ReadTask/Original/native capture and SDK pin produce both pages, with rechecks before/after awaits, batches and delivery; no birth/materialization/reconcile/repair or separate get recapture.

Read all consulted typed heads/current markers/edges through EOF before paging. Use current SDK filters/projections and batched scanners, consuming to EOF and dropping irrelevant data. The reader derives the expected reference projection during its first current-head scan, dropping descriptions, checks and history. Each current marker is compared with that captured projection and immediately releases it; it does not reopen the head once per marker. Same-pin point reads obtain only selected page edges. The retained state contains current observations, temporary expected reference projections and selected pages, without collecting corpus bodies or history. Validate the batch schema and decode consulted rows with the canonical Serde decoder, including nonselected sources needed for incoming completeness; cancellation, Auth, malformed rows and storage errors never become healthy partial results. Validate current marker edges against real bounded authored head state, not revision alone. A missing current marker reports incomplete with an authorized source warning and never selects an older generation; tampered/foreign/duplicate/inconsistent consulted markers fail. Historical/orphan valid rows are not current and cannot resurrect cleared links. Full-corpus FTS/index preflight still validates all rows, including noncurrent projection payloads.

TaskReferencesRead distinguishes NoCorpus, missing focus (MissingTask/MissingSession errors) and Published with observed TaskGeneration, focus_revision, both pages as Box<TaskReferencePage>, coverage and warnings. Boxing is transparent in JSON. Results retain logical source/target namespace, origin/provenance/field/ordinal/source_revision/reference_generation and the actual observed query pin, not a fabricated precommit/publication version. Empty complete edges differ from a missing projection. Outgoing order is structural then authored; incoming is Task sources then Session, canonical domain ID then ordinal. Duplicates remain. Closed opaque cursors bind exact NativePath, ProjectId, project_version, SDK pin, focus, direction and ordinal offset; no cursor is authority. Changing those bindings invalidates the whole request. Page size may vary within 1..100, with 20 as host default; offset greater than total is invalid, equal to total is an empty complete page. A cursor with NoCorpus is invalid; without a cursor NoCorpus remains explicit.

Coverage is sources Task/Session PROJECT within the exact selected corpus. A valid focus outgoing page may be complete even when other source corpora or rich targets are unqualified. Incoming/global source coverage remains explicitly incomplete while Memory/Knowledge/USER/import sources are pending. Rich-target resolution is independent: only exact same-project/context-None/current NativePath/version Task/Session IDs observed in validated same-pin heads receive minimal title/status resolution under ReadTask. Other modules, user/import/foreign/context and opaque or absent IDs retain unresolved identity, without inferred access, title or href. Missing rich target does not erase a real edge or mean its source projection is incomplete.

### Literal-target Task backlinks: Linux-qualified

The earlier `tsk-9a83` delivery added the exported asynchronous
NativeTaskStore::reference_backlinks(original:&Arc<SnapshotContext>,
admission:&ProjectReadAdmission<'_>,query:TaskReferenceBacklinksQuery,
limits:TaskStoreReadLimits)->Result<TaskReferenceBacklinksRead,TaskStoreError>
in the existing [references module](../../crates/graphit-records/src/task_store/references.rs).
Source is present with **23 distinct Linux functional cases covered by successful
runs: 14 SDK (nine legacy +five new) and nine library**. The SDK run passed 13 and
failed only the new Auth fixture (117.94s); correction used actual ReadMemory
denial and the same admin-parent-issued ReadTask-only bearer for revocation.
The corrected focal case passed one (0.29s), completing 14-case coverage on
unchanged product Source. Library nine passed (17.52s): eight previous names
plus one new duplicate-old SDK replacement case; its pure closed-serialization/
generation unit is included here, not presented as SDK execution. Records
**six doctests passed separately**. Strict attempt01 passed (5.07s) before the
last test-only Auth-fixture edits; strict attempt02 (`.build/rust-records-references/task-backlinks/strict-attempt-02.log`)
passed exit0 in 2m09s including the normal queue. Owned four-file rustfmt
--edition2024 --config skip_children=true --check and diff --check exited zero.
The final Source manifest is `.build/rust-records-references/task-backlinks/source-final.json`.
The 23+6 scope above is historical qualification of `tsk-9a83`. Only its
Auth fixture changed after compile03; subsequent reader cleanup is qualified
separately below.
Previous failed probe/compile/fixture logs remain
preserved. The pre-API probe failed one (2.19s), then passed one (3.87s), but
repeats a named case and adds zero. Historical `tsk-6cf9` receipts remain separate.
This scope excludes the full 52-case TaskStore suite, other Task-domain/Auth
Source, USER/Knowledge/Memory/App/global FullNoLoss, Windows/macOS and performance.

TaskReferenceBacklinksQuery contains target:TaskReferenceTarget,
page_size:NonZeroUsize (1..100) and cursor:Option<TaskReferenceBacklinksCursor>.
TaskReferenceTarget contains target_kind:TaskReferenceKind, scope:TaskReferenceScope,
id:String and context:Option<String>; context is valid only for Knowledge.
The selector accepts literal identities in all four target kinds, without
requiring the target to be a native Task/Session focus. Actual ReadTask admits
only the PROJECT Task/Session **source**. A target selector/cursor is not a
grant or body lookup; rich resolution stays limited to authorized own-project
Task/Session heads at the same pin. Other targets remain unresolved.

TaskReferenceBacklinksRead is NoCorpus or
Published{generation:TaskGeneration,incoming:Box<TaskReferenceBacklinksPage>,
coverage:TaskReferencesCoverage,warnings}. The page contains
results:Vec<TaskReferenceEdge>,total,complete,next_cursor. Output remains
Task/Session PROJECT source coverage, not global incoming completeness.
The existing NativeTaskStore::references, TaskReferencesQuery/Read, writes and
schema are unchanged by this extension.

Closed Serde query/target/cursor shapes reject unknown fields. The new cursor
family is incoming-only and binds root NativePath, ProjectId, selected logical
version, actual SDK pin/physical branch/inherited target, literal target and offset.
Scanner batch size may change between pages; it is a read hint, not cursor identity.
TaskStoreReadLimits has only opaque NativeDocumentFileLimits and batch_rows;
it has no aggregate read_max_rows/read_max_text_bytes fields. Native configuration
is rechecked per call, not serialized through a getter or treated as authority.
page_size may vary within 1..100; stale/foreign family or bindings and invalid
offsets reject. Original/admission and typed target/cursor validation precede
capture and NoCorpus; cursor plus NoCorpus is invalid. The HTTP consumer applies
its actual request/output byte policy separately from the corpus reader.


Historical `tsk-d7c3` removed output counters and serialization previews across
Memory, USER Memory and Task/Session queries. Its affected 40-case qualification
and failure logs remain historical evidence; they do not qualify the later
reader cleanup. Correction `tsk-96c8` removes TaskReferencesLimits,
ReferenceBudget, the counting-only JSON seed, and quota fields from cursors.
The Task writer's actual single-input reference limits remain. Memory and USER
reader cleanup belongs to their separately owned delivery.

Read all heads and all markers through full EOF, including canonical
old/orphan rows and duplicate/key checks, with bounded uniqueness metadata and
current-head observations and their expected reference projections. Validate schema
and typed JSON directly; release each expected projection after comparing its
marker, retaining selected pages without historical edges or corpus bodies. Necessary page totals and
offsets remain, while artificial corpus rows/edges/text accounting is removed. Missing
current markers yield authorized incomplete warnings, never adoption of an
older marker, repair or healthy global-complete output. Recheck the Original,
ReadTask and native guards at actual SDK/batch/delivery boundaries; causal
errors remain whole-query errors.

The canonical historical marker.project_version labels its publication origin.
It may differ from the selected logical query version in a legitimate inherited
or forked pin. ProjectId, exact current-head revision/reference_generation/edges,
actual pin/branch and cursor selection remain authoritative; no independent
origin-authority metadata is asserted. The minimal existing-reader correction
belongs causally to `tsk-9a83` qualification and does not rewrite the historical `tsk-6cf9`
receipt. App/USER/Knowledge/full NoLoss and non-Linux product proof remain
outside this Source stage.

### Planned App route and FullNoLoss continuation

`tsk-8e5d` implements POST /api/tasks/references in the existing opt-in Foreground Task boundary, using the exact native selection type and input { kind: task|session, id, page_size?, outgoing_cursor?, incoming_cursor? }, with unknown fields rejected. Omitted page_size means20; null/zero/>100 is invalid. Existing cookie/Origin/JWT/same-origin, fresh PROJECT ReadTask, same Original and physical capture remain required; selection/cursor is not authority. GET is not an alias (405), and absent opt-in remains404. Call one native references query for both pages; map current policy input/query limits and bound the actual response JSON before delivery. Only same-pin authorized Task/Session observations may receive encoded explorer/session links preserving exact selection. Foreign or unresolved targets get no invented local URL or extra body lookup. Planned HTTP/API delivery does not qualify browser/IAB or global coverage.

`tsk-7fc2` retains every outstanding FullNoLoss requirement of `tsk-16a8`: Memory’s own current-generation/projection/delta/read admission; Knowledge authored metadata and actual wiki-links with registered/import namespaces; actual USER identity/admission/capture and include_user policy; imported read-only availability; and authorized App composition. No ReadTask substitution, fake empty source or inferred target grant is allowed. Distinguish unrequested, pending/inaccessible, incomplete projection and actual error without unauthorized existence disclosure. Cross-corpus observations/cursors must state real consistency and staleness, not global transaction atomicity. Owners must close actual API/schema/query/error/limits contracts before Source; the Task SDK does not depend backwards on its App/browser consumer. FullNoLoss remains necessary, not canceled or passed by this first bounded corpus.

#### Current Memory read policy

PROJECT/USER Memory reference facades now take `MemoryStoreReadLimits` directly.
They consume heads/current markers through EOF without aggregate row/edge/text
quotas. Canonical keys, both JSON copies, current revision/reference generation,
owner/provenance and live admissions remain validated. Closed cursor identity
binds root/owner/generation/foci/direction/offset; batch size and lexical query
policy are not cursor bindings. `max_query_bytes` protects only one encoded
lexical query; input/per-row write/native file guards stay separate. Historical
Memory receipts below describe their original policy and scope. Task reference
limits and Knowledge producer contracts remain separate owned interfaces.

## Planned cross-source producers and coverage

The following contracts originate in the Source-free refinement `tsk-7fc2`; registration and planning review do not qualify implementation. PROJECT Memory Source under `tsk-29fb` passed 93 distinct functional tests (SDK68 +library25), with six Records doctests separately and scoped strict/fmt0; the remaining planned extensions retain their separate readiness gates.

| Producer/refiner | Planned scope and current boundary |
|---|---|
| `tsk-29fb` | PROJECT Memory Source is written/exported with 93 distinct Linux functional passes (SDK68:60 existing +8 new, plus library25). The current library run includes the actual Confirmed/revoke reference-query assertion without adding a function. Scoped strict attempt02 passed exit0 after the preserved test-only type_complexity failure; eight reference cases passed again as a subset of SDK68. Six Records doctests passed separately (four ordinary/two compile_fail), with owned edition2024 formatting/diff0. This qualifies the native PROJECT Memory reference Source on Linux. Actual ReadMemory and one pin bind outgoing/incoming, per-domainrevision markers with existing reference_generation, complete EOF/current-head matching, causal bounds and closed cursors. The historical corrected packet at `.build/rust-records-references/crosssource-contract/project-memory-corrected.json` (artifact unavailable in this checkout) and [Memory API contract](native_memory_store.md#project-memory-reference-projection-and-query-linux-qualified) specify pre-kernel input bounds and result-family exclusions, without claiming physical FTS payload bytes absent. This scoped functional evidence does not qualify global/App/USER/Knowledge/live Dream/Windows/macOS behavior. |
| `tsk-9a83` | The [literal-target backlinks Source](#literal-target-task-backlinks-linux-qualified) has 23 distinct Linux functional cases covered by successful runs (SDK14 +library9), plus six Records doctests separately. Current strict02, owned edition2024 formatting/diff and final Source-manifest checks passed; native Linux backlinks qualification retains that bounded 23+6 scope after the test-only Auth-fixture correction. The qualified `tsk-6cf9` focus remains Task/Session; the extension changes query scope, not writes/schema. Pre-API probe overlap adds zero. |
| `tsk-74cc` | USER prerequisites/refinement `tsk-633b`, `tsk-5995` and `tsk-74cc` are complete; Core `tsk-319e` is also complete. The [USER owner/layout data Source](native_user_memory_store.md) in `tsk-9613` is Linux-qualified for 13 distinct data/layout/causal-guard cases (USER6 +PROJECT7) and scoped strict/fmt/diff0, without CRUD/Auth/caller/semantic or new doctest qualification. The [ordinary USER store/search Source](native_user_memory_store.md#native-user-apis-source-implemented-validation-pending), `tsk-c623`, is Linux-qualified for 115 distinct functional passes (USER SDK15/library5 +PROJECT SDK68/library27), final scoped strict0 and owned fmt/diff0. Six Records doctests passed separately (four ordinary/two compile_fail), without addition to the 115 functional cases; see the [ordinary USER evidence](native_user_memory_store.md#c623-linux-qualification-and-evidence-boundaries). Fenced/automatic `tsk-c62d`, reference query `tsk-fb6e`, semantic `tsk-85c4`, daemon `tsk-8e7b` and App `tsk-6286` remain separate deliveries; Source presence gives no global/source-coverage PASS. The ordinary USER receipt does not substitute for the separate fb6e query qualification below or qualify global/consumer delivery. |
| `tsk-fb6e` | The [USER reference-query Source](native_user_memory_store.md#user-reference-query-implemented-sdk-qualification-pending) has 51 distinct Linux functional passes: USER SDK24/library5 +PROJECT SDK10/library write-boundary12; six Records doctests passed separately. The remaining Native/Web fixture/no-hit oracle passed on current Source03 without Product Source changes; earlier failures remain audited. Current Source03 strictfinal03 passed exit0 (11.04s) after coordinator space recovery; strictfinal02 ENOSPC remains audited. The historical query receipt at `.build/rust-records-references/user-memory-query/validation.json` (artifact unavailable in this checkout) records this native Linux qualification, with no replay added to the 51 distinct count. Published.coverage is Box<UserMemoryReferencesCoverage> with unchanged JSON. Actual UserReadAdmission/Original, canonical USER source scope, one default pin, complete marker EOF and boxed bounded pages remain. Canonical row_key prefix candidates reject tampered kind through full decoding; genuine missing marker still yields incomplete warning. Closed cursors bind NativePath/owner/generation/foci/six public limit parameters; only own live same-scope Memory receives rich ID/title. Other sources remain unqualified, and the sealed c623 ordinary 115+doc6 receipt is separate. |
| `tsk-41d1` | Knowledge refinement remains open. Completed producers are `tsk-9983` (authored metadata), `tsk-c8e9` (schema18/state/witness and complete first publisher), `tsk-2f62` (locator contract) and `tsk-8c07` (read-only imported heads/page). Native reference query `tsk-9d61` has Linux Native/Auth/SDK focal validation (Data9 + Knowledge1, scoped strict and private-cursor compilefail); [its contract](rust_knowledge_reference_query.md) preserves raw authored/Wiki provenance, stored namespaces, coverage and sealed continuations. [Imported query](rust_imported_reference_query.md) `tsk-3514` consumes the completed native query/imported reader. [Existing-document republication](rust_knowledge_document_republication.md), `tsk-f02f`, is Linux-qualified by 31 distinct functional cases (14 new +17 existing module cases), scoped strict/fmt/diff and cold NoFault review, with real ReadKnowledge + WriteKnowledge, Full18 preservation and CAS; the repeated Auth extension adds zero cases. The separate App C5 consumer remains unqualified by these library tests. [Authored first publication](rust_knowledge_authored_first_publication.md), `tsk-6101`, is Linux-qualified by 28 distinct functions (Data5 + Knowledge3 + SDK phases3 +17 affected index cases), strict/fmt/diff and cold NoFault review. It creates a mixed Some/None image exclusively; omitted metadata remains incomplete and explicit [] is known empty. The legacy complete-publisher extension adds zero distinct cases. [Explicit materialization](rust_knowledge_branch_materialization.md), `tsk-2437`, has 12 distinct Linux functional passes (Data5 + private SDK phases4 + Knowledge2 +1 existing republication-boundary regression), scoped strict/fmt/diff and cold NoFault review. It materializes the captured inherited version after both real Knowledge admissions, expected comparison and Full18 EOF; confirmed effects retain their actual generation and cause. Insert/remove/reactivate remain refinement. The refiner does not qualify those journeys. |
| `tsk-12df` | Root/App FullNoLoss composition refinement under `tsk-c471`, dependent on `tsk-7fc2`. It is distinct from the Task-only API `tsk-8e5d`; Foreground composition remains App-owned and planned. |

Each source uses its own TargetRead admission and Original, selected namespace and one actual SDK pin. include_user defaults to false; true requires delivered USER availability/admission. Outgoing completeness concerns the requested source, incoming completeness concerns every requested source, and rich-target resolution is a separate authorized dimension. Cross-corpus composition states individual observations and staleness, without a global atomic snapshot, ReadTask substitution or a healthy empty result for pending/denied/error sources. Exact namespace is preserved for equal IDs in distinct projects/users/import contexts. `tsk-17a7` WEB-A6/T5 and all retained FullNoLoss requirements remain mandatory; these plans do not qualify them.

Native Linux qualification for `tsk-6cf9` passed **70 distinct functional tests: Task SDK47, TaskStore library15 and TaskProtocol8**. Records doctests passed **six cases separately** (four ordinary/two compile_fail). Locked no-run compilation and scoped Clippy `--lib --test task_store --test task_protocol --no-deps -- -D warnings` exited zero; focused rustfmt/diff checks passed. Repeated and focal runs are included, not added to these counts. Proof includes real foreign ProjectId and midscan credential revocation, SDK-rejected duplicate source with unchanged pin, and an actually published malformed canonical projection causing whole-query failure. Confirmed proof uses actual SDK publication followed by a production boundary cause and fresh query; it does not inject SDK lost acknowledgement or establish atomic Auth/SDK fencing. Failure history is retained in `.build/rust-records-references/task-sdk/evidence`; the final receipt location is `.build/rust-records-references/task-sdk/validation.json`.

App `tsk-8e5d`, FullNoLoss `tsk-7fc2`, Root integration gates, UI/live Runtime, Windows/macOS and performance remain outside this qualification. The R7 App mapper unit reference_failure_mapping_preserves_request_status_and_confirmed_cause passed **one case separately**, qualified by the Root/core_writer receipt `tsk-cd41`; scoped App/Daemon strict checks also passed externally. Role tests and rustdoc from that receipt are not added to the native70/Records doctest6 counts. The Broker caller compiled with `--test broker_records --no-run --locked` (exit0); ignored Docker Runtime fixtures remain unqualified and supply no product proof. These native results do not imply Task lifecycle completion or Foreground delivery.

### Task reference reader cleanup

`tsk-96c8` forwards TaskStoreReadLimits directly to both native reference readers
and the existing Task browser consumer. It retains complete EOF, schema/JSON,
current markers, old/orphan validation, namespaces and projection integrity.
Cursor bindings retain source identity, generation, focus/target and direction;
scanner tuning no longer invalidates a continuation. The App retains its closed
input and actual encoded-response bounds. Current Linux qualification passed
**34 distinct affected functional tests**: Records library9, SDK references9 and
backlinks5, plus App parser3 and browser8. The browser quota-only branch was
retired; its real response/request byte-bound assertions still pass. Scoped
Records and App scoped Clippy passed, together with owned formatting/diff
checks. The current receipt is
`.build/rust-task-reference-read-policy/validation.json`, separate from the
earlier producer qualifications above. App lint targets the affected library;
its unit/browser consumer fixtures compiled and passed. The broader `--tests`
attempt exposed an unrelated retirement fixture mismatch, tracked by its owner.

Memory corpus/reference-ledger removal is qualified under `tsk-265d`; [the current read-policy section](native_memory_store.md#current-read-policy-qualification) separates current reader results from historical receipts. Task reference cleanup belongs to its separate `tsk-96c8` producer and consumer evidence.


## Native Foreground browser composition

The native browser composition keeps one query, admission, generation and cursor family per requested source. It does not flatten Task, Memory, USER or Knowledge DTOs, synthesize physical emptiness from zero matches, or treat a logical target as an authorization lookup. The [native browser reference contract](rust_foreground_record_references.md) defines own-module discovery, readonly imported readers, typed POST navigation descriptors and the pending functional qualification. Full browser navigation remains an independent acceptance requirement; backend metadata alone does not prove it.
