---
references:
  - target: {type: task, id: tsk-59c7}
    relation: implements
  - target: {type: task, id: tsk-9a83}
    relation: requires_contract
  - target: {type: task, id: tsk-6cf9}
    relation: requires_contract
  - target: {type: task, id: tsk-3e62}
    relation: specified_by
  - target: {type: task, id: tsk-dd92}
    relation: implemented_by
  - target: {type: task, id: tsk-add4}
    relation: implemented_by
  - target: {type: task, id: tsk-9748}
    relation: simplified_by
  - target: {type: task, id: tsk-1962}
    relation: uses
  - target: {type: task, id: tsk-4f78}
    relation: uses
  - target: {type: task, id: tsk-631e}
    relation: uses
  - target: {type: memory, id: 01M4NJBW5NQWNE24J3BW3G07MF, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: derived_from
---

# Native Task textual and semantic search

Audience: Records consumers and Data/Auth/Core/Task maintainers. **Projection/index publisher `tsk-dd92` is implemented and Linux-qualified: 35 distinct functional tests, six Records doctests separately, scoped strict/fmt0. Search/cursor `tsk-add4` is implemented and Linux-qualified: 45 distinct functional tests, six Records doctests separately and scoped strict/fmt0.** Investigation `tsk-3e62` closed the Source-free design; planning checks do not prove product behavior. Projection, schema/encoder, read preflight, index publication and recovery have executed functional/strict evidence in the bounded native producer scope below. These are historical producer receipts; current cleanup `tsk-9748` qualifies its affected paths separately below. This page describes native Records contracts, their Role consumer, and the first-corpus CLI/Source MCP/HTTP integration delivered in `tsk-59c7`. Browser-specific journeys retain their own integration evidence.

The [current NativeTaskStore](native_task_store.md) provides authoritative Task reads, comments and a title-substring catalogue. FTS discovery is a distinct operation. After discovering a TaskId, use the selected `read_task`/Task Get for the authoritative current record. Request ordered history and comments explicitly through its generation-bound pages when needed; a search hit or unloaded section is not a substitute for that read. Complete audit/export traverses those section cursors through EOF. Search mode `fts` is model-free and remains the default. Modes `semantic` and `hybrid` use the selected embedding service and the incremental vectors described below.

## First corpus and application consumers

The productive writer initializes the derived JSON FTS index before its first
merge, using the same acquired Core writer, captured pin, Original and admission.
It never reacquires that writer. Missing index metadata triggers complete typed
EOF validation and an explicit Create publication; Ready skips index publication,
and subsequent writes merge once while maintaining the public projection. An
incompatible index remains an error. Queries never initialize or repair it.

This internal derived-state step preserves the writer's actual authority,
including CoordinateTask Session mutations; it does not manufacture ordinary
Task administration. Session rows still project `{}`. Explicit
`create_lexical_index` and recovery retain their own ordinary Task admission.

Index creation and the record merge are separate SDK publications. A failure
after index publication retains its real generation and recovery key as
`LexicalIndexConfirmed`; this alone does not confirm a Task or Session mutation.
If the record merge was also confirmed, its `Confirmed` result remains outermost.
Uncertain commits keep the actual SDK recovery key. No rollback, retry or model
inference is implied.

`graphit task search` and Source `graphit_task_search` expose the same reader. For example:

```sh
graphit task search --text nebula --page-size 1 --top-k 10
```

Source MCP arguments are `{selection, input}`; authenticated native HTTP uses
`POST /api/tasks/native/search` with the same body. Selection identifies a retained
PROJECT and literal version, granting no authority. `input` is the native
`TaskLexicalQuery`: mode (`fts` by default), text, filter, positive page_size/top_k and optional cursor.
Filters are status, session_id, flagged and blocked. Continue with the returned
cursor under the same query and generation; a different selection or stale pin
is rejected. The host supplies its captured individual query-byte bound.

The Source/CLI response is `{operation:"search",result:<TaskLexicalSearchRead>}`.
Its native outcomes remain NoCorpus, PublishedEmpty and Published, with the
existing ranked hits, total_hits and next_cursor. The Role endpoint preserves
its existing direct native result and permission policy; its special search
tool is advertised once. These searches exclude Session text, strategies,
checkpoints and old specifications. The default FTS mode requires no embedding configuration.

Task `tsk-59c7` records the current integration qualification: real CLI creation
without an FTS seed, then CLI/MCP/HTTP pages, branch/project isolation, revision
and comments. Existing publisher/reader receipts below retain their historical
scope; they are not substituted for these product journeys.

## Incremental semantic and hybrid modes

The same Task search operation accepts `mode: "fts" | "semantic" | "hybrid"`.
Omission selects FTS; configuring a model does not change that default. Examples:

```sh
graphit task search --mode semantic --text 'recover interrupted publication' --page-size 10 --top-k 20
graphit task search --mode hybrid --text 'recover interrupted publication' --page-size 10 --top-k 20
```

The same `input` is accepted by `graphit_task_search`, native HTTP
`POST /api/tasks/native/search`, authenticated Browser `POST /api/tasks/search`, and managed
Role search. The Task Explorer search control uses the Text/Semantic/Hybrid selector,
retains filters and paged record navigation, and opens the exact Task dossier after a hit.
The table preserves the producer's unique Task ranking and each winning match location
and score, with links to specification/comment sections. `total_hits` counts unique
eligible Tasks before the top-k cap, as in the native reader.
With an empty query it keeps the ordinary Task catalogue. Search errors remain visible;
semantic service failures do not switch to a title-only catalogue. Each retains its existing response envelope. FTS does not acquire
an embedding service, resolve model metadata, or infer a query. Semantic and
hybrid require `modules.embedding`; disabled or unavailable services produce
explicit failures instead of silently executing another search mode.

The daemon reuses its project supervisor and provider-neutral embedding resources.
Mutations of the public Task projection wake that supervisor through its coalesced record-only notification. Claims, heartbeats and status-only changes do not request inference. The notification does not walk or reparse AST/Knowledge sources. It cancels only background inference, leaving an in-flight source pass alive; only source/control events preempt that pass. Interrupted inference tails resume from their persisted corpora. Memory mutations use the same record-only wake. The worker captures one Task snapshot with `capture_embeddings`, consumes
`TaskEmbeddingCorpus::next_documents` through EOF, and selects only missing or
incompatible public projections. It infers finite batches outside the writer,
and attaches nullable Arrow `List<Float32>` vectors to the same authoritative Task dataset. Derived rows hold association metadata and a typed vector; they do not duplicate public source text or store vectors as JSON. It does not embed private
fences, claims, actor metadata, session strategy or audit history. There is no second
Task corpus or authoritative local mirror for S3. Local, Broker and direct inference
use the existing service admissions and query/document roles.

Fingerprints bind the public content and vector-space identity: model/revision identity,
dimensions and the Task document recipe. Claims, leases, timestamps and other changes
that leave the public projection unchanged reuse the existing vector. A changed
specification invalidates only its incompatible association; unchanged current
specifications and comments retain their compatible vectors. Historical specifications
remain outside the search corpus. Attachment rechecks current public text
under the writer, so inference completed against an obsolete document is not applied
as the current vector.

Semantic queries use the SDK cosine-distance kernel to rank eligible current vectors already published, even while other
documents await embedding. Hybrid queries retain lexical candidates without vectors
and fuse their ranks with the semantic candidates. Both preserve Task IDs, matching
specification/comment locations, status/session/flagged/blocked filters, and pagination.
Continuation retains the compatible query vector, so later pages do not infer it again. It binds the selected mode and semantic query identity in addition to the
project, version, snapshot, filter and query. Model/recipe changes require new vectors;
incompatible vectors are excluded rather than mixed with the new space.

The producer still reads the current authoritative Task graph for filtering and
validation. A search hit does not replace Task Get, revision checks or authorization. CPU loops check cancellation and deadlines per record;
authority and filesystem/transport bindings are revalidated before and after SDK
batches, at EOF and before delivery, rather than reopening those checks for each
candidate/vector. FTS reads current Task graph projections once, then validates
only matching typed sources and their Session/CommentAdded bindings in grouped
reads of the same snapshot. It does not load creation records or replay unrelated
Task/Session lifecycle history. Hybrid reuses the graph already captured by its
semantic path. These changes remove redundant I/O, decoding and retention;
latency claims require a corresponding focal measurement.
Historical Linux FTS receipts below do not qualify this newer semantic delivery or
native platforms that have not been executed.

## Deliveries and current implementation evidence

The publisher and reader delivery leaves have parent `tsk-8c6a` in `ses-c98a`; they are siblings of the investigation, not its children. Investigation completion does not require their implementation. Records owns `task_store/public_search.rs`, `lexical_index.rs`, `lexical_search.rs`, narrow schema/encoder/mutation/model/error/export changes and their tests. [public_search.rs](../../crates/graphit-records/src/task_store/public_search.rs) and [lexical_index.rs](../../crates/graphit-records/src/task_store/lexical_index.rs) are actual implemented Source; [lexical_search.rs](../../crates/graphit-records/src/task_store/lexical_search.rs) and [lexical_model.rs](../../crates/graphit-records/src/task_store/lexical_model.rs) now implement the reader and its exported query/filter/cursor/hit/read types. Publisher qualification does not qualify that reader or downstream App consumers. Root owns shared manifests/locks and downstream composition; Auth, Data, Core, Gantt and catalogue ownership remains separate.

Existing [rows.rs](../../crates/graphit-records/src/task_store/rows.rs) serializes `Stored<T>` with a private fence, so whole payload indexing is unsafe. Existing [read.rs](../../crates/graphit-records/src/task_store/read.rs) validates Task/Session histories and comments and provides the catalogue. [Data index publication](../development/native_json_fts_index.md) and [SDK qualification](../development/lance13_fts_index_qualification.md) are delivered primitives. Their qualification does not prove Task mutation coverage or Task search. The decided specification and leaf coverage are preserved in `.build/rust-records-task/fts-contract/contract.md` and `.build/rust-records-task/fts-contract/planning.json`.

## Public corpus and schema

The implemented column `public_search` is non-null Utf8 containing JSON produced by a pure allowlist encoder in `rows::batch`:

| Physical family | Exact public JSON fields |
|---|---|
| Current Task head | `title:string`, `description:string`, `check_text:[string]`, `evidence:[string]`. |
| Task comment | `comment_body:string`. |
| Every other family, including Session and history events | `{}`. |

Only the current specification and checks retained in that head are searched, including Superseded checks and their retained verification evidence. Arrays preserve typed check order; absent verification contributes no null or invented evidence. All append-only comment bodies remain eligible. Old specification/event snapshots do not contribute searchable terms. Session content, progress/checkpoints, audit reasons/actors, ownership, claims/fences/tokens, Auth data and idempotency keys are excluded. Public textual values remain literal; the encoder does not redact or reinterpret them.

The projection and current/history/comment rows publish in the **same strict merge**. Index-publication preflight validates exact equality between `public_search` and the typed encoder for every physical family, including `{}`. Search validates that equality for every matching typed source, including unexpected candidate families, before returning any results. Malformed, forged or extra projection content in consulted rows fails explicitly. The payload remains the authoritative typed history; public_search is a derived index input, not a second store of truth.

A corpus without the new column is `SchemaMismatch` before read, mutation or index publication. There is no migration, converter, automatic deletion/rebuild or fallback. The new schema must be qualified with native fixtures; live stores are not manually altered.

## ReferenceProjection family: native Linux qualification

`tsk-6cf9` adds a typed reference_projection family in the same Task dataset, alongside required authored references/reference_generation and normalized audit patch state. Its public_search JSON is {}, so relation IDs/text and storage metadata do not enter the FTS allowlist. [Rows](../../crates/graphit-records/src/task_store/rows.rs) validate typed payload and canonical equality of all 30 columns; full typed-corpus index-publication preflight includes current and noncurrent reference projections. Search does not audit unrelated reference/history rows; any unexpected family returned as a lexical candidate is rejected, and reference queries validate their consulted projections.

The [native references query](record_relations.md#rust-tasksession-persistence-and-query-native-linux-qualification) is separate from FTS: same-pin outgoing/incoming, complete markers versus incomplete missing markers, namespace/rich-target dimensions and closed cursors. This family and native query passed the `tsk-6cf9` Linux qualification: 70 distinct functional tests (Task SDK47, TaskStore library15, TaskProtocol8), six Records doctests separately, locked no-run compilation and scoped strict Clippy. Historical publisher35/reader45 receipts below retain their original scopes and are not added to this count. No App/HTTP references route or global FullNoLoss delivery is inferred.

`tsk-9a83` adds [literal-target reference backlinks](record_relations.md#literal-target-task-backlinks-linux-qualified)
as a separate Task/Session PROJECT source query. Its Source and exported DTOs
are present with 23 distinct Linux functional cases covered by successful runs
(SDK14 +library9), plus six Records doctests separately passed. The sole SDK
Auth-fixture failure was corrected test-only and passed focally on unchanged
product Source; current strict02, owned edition2024 skip_children formatting/
diff and final Source-manifest verification passed after the final fixture
edits. Native backlinks are Linux-qualified in this 23+6 scope. The existing FTS schema, reference projection payload
and public_search={} stay unchanged; references never become lexical candidates.
The historical 70+6 receipt is not added; the overlapping pre-API probe adds zero.
This proof excludes the full 52-case TaskStore suite and other domains. The historical marker
publication-version label can differ from an authorized inherited/forked query
selection; exact current head/ProjectId/pin/branch binding remains required.
No App/USER/Knowledge/global FullNoLoss delivery is inferred.

## Native APIs and authority

create_lexical_index and resolve_lexical_index are implemented and qualified in the Linux publisher scope. search_lexical and its query/filter/ranking/cursor/outcomes below are implemented and qualified in the bounded Linux reader scope. Methods are asynchronous and return `TaskStoreError` on failure:

| Method | Inputs and result |
|---|---|
| `create_lexical_index` | Original `&Arc<SnapshotContext>`, `&ProjectWriteAdmission`, `TaskStoreWriteLimits` → `TaskLexicalIndexCreation`. |
| `resolve_lexical_index` | Original, WRITE admission, `&DeltaRecoveryKey`, write limits → `DeltaResolution`. |
| `search_lexical` | Original, `&ProjectReadAdmission`, `TaskLexicalQuery`, `TaskStoreReadLimits`, `max_query_bytes:NonZeroUsize` → `TaskLexicalSearchRead`. |

Explicit index administration and resolution require **ProjectWriteOperation::Task**; OwnTask and CoordinateTask do not administer the corpus. The internal writer initializer preserves the admitted record operation rather than exposing index administration. Search requires ProjectReadModule::Task. Every admission is checked against the same original Arc, physical PROJECT capture and selected version/branch. Rechecks occur before/after public SDK awaits, batches, rows and delivery; this does not promise interruption inside an SDK call or continuous atomic Auth/SDK fencing.

Publication retains complete typed-corpus preflight at one captured pin, including creation/history continuity, comment bijection and projection consistency. Ordinary search instead reads current graph projections for readiness/filters, then consumes every FTS candidate through EOF. Each matching Task/comment payload and its public projection are validated together; matching comments also validate their actual CommentAdded event and attribution, and matching Tasks/comments require their current Session. Grouped reads remain on the same pin and do not reopen a fresh catalogue or selected record. Unrelated creation/event/reference rows are not audited by a search. Full history validation remains in index publication and explicit history/audit consumers; a healthy current search does not certify the integrity of unconsulted history. This supersedes the former universal history preflight requirement for search.

Readonly search never births, materializes, repairs or refreshes the corpus/index. Resolution is observational: under fresh Task WRITE admission, Writer and guard it observes the SDK key/manifest and corpus binding, without scanning Task bodies, history or public projection. Its signature takes write limits.

`search_lexical` accepts `max_query_bytes:NonZeroUsize` directly. It bounds
one literal query's UTF-8 bytes and its five-path SDK expansion, using checked
arithmetic before expansion allocation or native corpus I/O. Exceeding either
size returns `TaskStoreError::Invalid("Task FTS query byte limit")`. The index
publisher needs no query bound. The readers no longer charge aggregate corpus
rows or text; they preserve schema, canonical payload/public-projection validation,
full EOF and actual native file/path guards. `TaskStoreReadLimits` retains native
limits and `batch_rows`; batch size is an SDK hint.

## Explicit index publication and recovery

Publication acquires the original Task writer without birth or inherited materialization during preflight. Valid absence returns `TaskLexicalIndexCreation::NoCorpus` without creating a directory, corpus, branch or index. An existing incomplete Task directory remains an error. For an actual publication, materialize the captured inherited target and use Data `prepare_json_fts_index("public_search", "task-public-json-content-v1", JsonFtsIndexMode::Create)`, then strict commit against the current writer pin. An empty SDK index may be published. An occupied name retains the SDK error; there is no replacement or refresh controller.

Success is `Published(TaskLexicalIndexPublication { generation:TaskGeneration, recovery_key:DeltaRecoveryKey })`. A known commit followed by recheck failure is `TaskStoreError::LexicalIndexConfirmed { publication:Box<_>, cause:Box<_> }`, preserving generation/version/key. SDK conflict or uncertain commit is the real `Commit(DeltaCommitFailure)` with its original cause and key. None of these observations implies rollback or automatic replay.

Resolution uses a fresh original and actual WRITE admission, validates key URI and physical branch before corpus I/O and invokes the real captured SDK resolver. Absence or a still-inherited target returns Unresolved. Resolution never births, materializes, retries publication or reapplies mutation. Auth and physical binding remain required; a recovery key is not authority.

## Qualified native query

`TaskLexicalQuery` contains `text:String`, `filter:TaskSearchFilter`, `page_size:NonZeroUsize`, `top_k:NonZeroUsize` and optional `TaskSearchCursor`. Filter fields are optional status, session_id, flagged and blocked. Conditions combine against the current head before collapse/cap. None adds no condition; boolean false selects the complement. Blocked is exactly Open and not ready, evaluated using the full captured graph. There is no additional title-substring filter.

Expand literal semicolon-separated segments across the five JSON paths `title`, `description`, `check_text`, `evidence`, `comment_body` using path,str,value. Blank text/segments and byte-limit excess are rejected before expansion. Commas and newlines remain literal values, not a query language. The Data policy is JSON/simple/lower_case with no stemming, stopwords, ASCII folding or token-length filter; OR and fuzziness zero. There is no custom parser, ranking threshold or semantic cutoff.

When Task heads exist, the same-pin `JsonFtsIndexState("public_search")` must be Ready before querying. Missing returns `Invalid("Task FTS index missing")` and Incompatible returns `Invalid("Task FTS index incompatible")`, never healthy NoMatch or flat-text fallback; SDK errors retain their causal Branch/Store/Commit errors. No LexicalIndexUnavailable variant is added. Complete valid input with zero Task heads, including a corpus containing only valid Sessions, returns PublishedEmpty without FTS and needs no index. The reader also captures the Task directory before accepting NoCorpus: a present Task directory without index.lance is IncompleteCorpus, not validated absence.

The SDK candidate stream projects typed source columns together with `_score`, sets no SDK limit/offset and consumes every batch through EOF. Validate each candidate's kind, canonical payload/projections, key/TaskId, head revision or comment ID/sequence against the same-pin graph and its grouped Session/CommentAdded bindings, reject physical duplicates and require finite scores before retention. Late schema, projection, binding, SDK, Auth, cancellation or deadline errors invalidate the whole observation; no healthy prefix is returned.

Choose the maximum SDK score per TaskId. A physical tie prefers the head, then the comment with the lowest sequence. Sort unique eligible tasks by score DESC and canonical TaskId ASC, then apply top_k. `total_hits` counts all distinct eligible hits before the cap. A `TaskLexicalHit` has current `TaskSummary`, finite score and location `Specification { revision }` or `Comment { comment_id, sequence }`; it adds no excerpt/body or authority claim.

| Read outcome | Meaning |
|---|---|
| NoCorpus | Validated native absence; no generation or birth. |
| PublishedEmpty { generation } | Complete valid corpus with zero Task heads. |
| Published { generation, total_hits, results, next_cursor } | Complete ranked result; empty results and zero hits are NoMatch. |
| Error | Index unavailable, corruption, malformed projection/candidate, changed binding, limit, SDK or authority/request failure; never healthy empty. |

## Cursor and authoritative retrieval

The closed TaskSearchCursor binds actual project/root, logical version/physical branch, full TaskGeneration, literal text, all filters, page_size, top_k and offset into the already deduplicated/capped result. It does not reuse or change Core, catalogue or Gantt cursors and grants no authority. It is not claimed authentic or tamper-proof. SearchBinding uses the local `#[serde(with = "closed_generation")]` adapter for generation: exactly version, physical_branch and inherited_target, with deny_unknown_fields for nested continuation data. The shared TaskGeneration type and its wire representation remain unchanged; this local closed codec adds no authority or authenticity guarantee.

Changing the query, model/recipe, scope or corpus binding rejects continuation. Offset must be positive, less than capped length and a multiple of page_size; overflow or forged bounds is invalid. A cursor on NoCorpus or PublishedEmpty is InvalidCursor. Each new request must present a fresh actual admission; the old admission cannot authorize it. The admitted store reopens the cursor's exact historical version with the existing SDK APIs, so background embedding commits and ordinary writes do not invalidate pages or change their ranking, totals or heads. A new search without a cursor observes the new data. Missing historical data is an explicit error, never a fallback to latest.

Named branches also bind the SDK's branch provenance. An inherited default snapshot can continue after its selected child materializes only when the SDK proves the direct parent/fork relationship; subsequent cursors retain that child's provenance. Cursor fields never choose a URI or authorize another branch/project. Page_size may exceed the remaining capped results, and the last page has next_cursor=None. Each continuation validates its consulted sources and current graph at the historical pin; pages do not mix generations or replay unrelated lifecycle history.

## Historical producer qualification

Publisher `tsk-dd92` is **Linux-qualified**: `.build/rust-records-task/fts-source/affected-task-store.log` passed **31 tests** (26.58 s), including eight new SDK cases and the active-fence fixture; `.build/rust-records-task/fts-source/affected-lib-initial.log` passed **four tests**, two new strict-key/known-cancellation SDK cases and two existing cases. These are **35 distinct functional cases**. Focal-7 passed eight overlapping cases and is not added again. `.build/rust-records-task/fts-source/doc.log` passed **six cases separately**, four ordinary/two compile_fail. `.build/rust-records-task/fts-source/strict-2.log` `--lib --test task_store --no-deps -- -D warnings` exited zero (6.44 s); owned fmt of nine paths and scoped diff exited zero. Durations are not benchmarks. Reader/cursor `tsk-add4` has its distinct qualified evidence below. Publisher results do not replace reader proof.

The qualified publisher fixtures exercise create, revise, verify, supersede, comment, claim/heartbeat/release, links/flags, completion and cancellation with real Auth/fence/revision checks. SDK queries find new head terms, exclude revised-away head terms, preserve comments and exclude Session/events/coordination terms. A physical Ready inspector alone proves neither mutation coverage nor Task search. SDK append qualification or Memory Merge/delete qualification cannot replace this Task proof. A concrete SDK failure is reported to Root for a minimal producer contract; it does not authorize fallback or a speculative refresh framework.

The historical fourth focal had 5 PASS/1 fixture failure for the expected schema-error cause, before the final passing SDK cases. Initial strict failed type_complexity; the CompleteHistory type alias fixed that lint without behavior change, and strict-2 passed. Existing shared library-test fixture unused-method warnings remain preserved outside the scoped strict target; no foreign Source edits or warning suppression are claimed. The receipt `.build/rust-records-task/fts-source/validation.json` brings together commands, logs, results, Source hashes and failure history. Qualification does not imply lifecycle completion or App delivery. This qualification excludes App, Windows/macOS and performance.

Reader `tsk-add4` functional results passed on Linux: `.build/rust-records-task/fts-search/affected-task-store.log` **38 PASS/0 failures/0 ignored** (69.32 s), and `.build/rust-records-task/fts-search/affected-lib-2.log` **seven PASS/0 failures/0 ignored** (3.14 s), three new private boundaries plus four prior cases. This is **45 distinct functional cases**, including **ten new reader cases: seven SDK and three private boundaries**. The seven focal SDK cases overlap and are not added again. `.build/rust-records-task/fts-search/strict-2.log` exited zero (4.72 s); owned fmt and whitespace passed. `.build/rust-records-task/fts-search/doctest.log` exited zero: four ordinary + two compile_fail = **six cases separately**, 12.79 s compilation and 1.81 s documentation execution. The native reader is **Linux-qualified**; historical publisher35/doc6 and overlapping focal runs are not added to the current reader45/doc6. The technical receipt location is `.build/rust-records-task/fts-search/validation.json`; lifecycle and App delivery are outside this evidence.

The initial reader strict attempt failed E0583 for a missing real Data module; its owner added that module, and the retry passed. `.build/rust-records-task/fts-search/strict-initial.log` preserves the failure. No foreign Source edits, warning suppression, App product proof or non-Linux/performance claim follows from this reader qualification.

The actual SDK focal observed batch sizes `[7]` with batch hint1: all seven candidates arrived in one batch. This does not prove multiple FTS batches; the reader must consume every batch through EOF regardless of that hint. The private cancellation fixture cancels the real Original at the second/final delivery check after page construction through the production before composition. It is a real request boundary, not SDK interception or atomic Auth/SDK proof.

Reader fixtures must cover spec-only/evidence-only/comment-only, multiple physical matches deduplicated before top_k, ranking/ties/filters, all EOF despite batch hints, noMatch versus NoCorpus/PublishedEmpty, changed/forged cursor binds, fresh-request continuation, branch/project isolation, revoke/cancel/deadline, individual query/expansion bounds and late corruption of consulted sources. Unrelated historical corruption must remain detectable by explicit Task/history reads without forcing a universal audit in current search. Private boundary compositions must be labelled as such, without claiming SDK interception.

Current affected qualification commands:

```text
cargo test --locked --offline -p graphit-records --lib task_store::lexical_
cargo test --locked --offline -p graphit-records --test task_store task_lexical_
cargo clippy --locked --offline -p graphit-records --lib --test task_store --no-deps -- -D warnings
```

Run the affected FTS filters and owned formatting after this reader-policy change;
schema/encoder behavior is unchanged. Historical full TaskStore/doctest results
above retain their original scope and are not a required replay for this cleanup.

The existing RoleTool Task search consumer supplies
`RoleToolPolicy.task_max_query_bytes`, keeping its captured individual text bound.
The native index-publication fixture supplies only TaskStoreWriteLimits. TaskPolicy
and other downstream adapters remain separate interfaces; this API cleanup adds
no new wire operation or compatibility layer.

`tsk-9748` removes TaskSearchLimits, ScanLimits and aggregate row/text charge
passes from the active FTS reader and index publisher. Its current affected test
and lint receipt is `.build/rust-task-fts-read-policy/validation.json`; historical
publisher/reader counts above are preserved separately. Current Linux
qualification passed **22 distinct affected functional cases**: six native
library FTS boundaries, fifteen SDK index/search cases, and one actual
RoleTool socket consumer. Scoped Records library/TaskStore-target and App
library Clippy passed, together with owned formatting and diff checks.
Earlier resolution/foreign-lint failures remain in the current receipt history;
no passing functional suite was replayed for those corrections.

The historical consumer integration mapped LexicalIndexConfirmed to App write_confirmed. In the current first-writer integration, Now still maps it to Unavailable; [App Failure::store](../../crates/graphit-app/src/task_server.rs) now returns index_confirmed with its actual index_publication generation/recovery_key, recursively converted cause and committed=true. A derived index alone never announces a confirmed Task mutation. Outer Confirmed CRUD and uncertain commit recovery keys remain distinct. Failure::status and the router are unchanged, so write_confirmed retains HTTP 500. These arms preserve native known-commit information; they do not implement App FTS or qualify the reader. The producer's 35 functional tests and six Records docs are unchanged. The `.build/rust-records-task/fts-source/app-consumer-2.log`, `cargo check --locked -p graphit-app --lib`, exited zero (1 min 57 s) after the Hub owner corrected a foreign free-function self error. The initial exit101 is retained in the receipt history. This proves compilation of the ceded App/Now arms against the current dependencies, not App FTS/runtime or CLI/MCP/HTTP delivery.
