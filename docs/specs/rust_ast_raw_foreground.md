---
title: Rust AST Raw foreground Source
description: Native profile selection and per-file streaming of Raw Source in the project supervisor.
doc_type: technical
references:
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-393d}
    relation: implements
  - target: {type: task, id: tsk-994a}
    relation: consumes
  - target: {type: task, id: tsk-2297}
    relation: implements
---

# Rust AST Raw foreground Source

The foreground integrates Raw Source into the existing project supervisor. The
[private job](../../crates/graphit-app/src/foreground_ast_sources.rs) takes a fresh
service Original and authenticates the real runtime caller for PROJECT ReadAst
and WriteAst. Background rescans use cancellation without an HTTP duration cap;
startup and HTTP requests retain their own deadlines. Credentials and grants
retain their actual expiration and rechecks. There is no context rotation or new TTL.

The [profile loader](../../crates/graphit-app/src/ast_profile_loader.rs) consumes
45 actual shipped YAML assets and the existing decoder/catalog. Installed
versioned runtime defaults replace matching compiled stock languages. User and
project layers preserve catalog precedence, merge semantics and provenance.
`ast.queries_dir`, `ast.grammar`, allow/deny and exclusive routing use actual
configuration and effective profile selectors, including suffixes and basenames.
Profiles are data, not filesystem or authentication grants. Invalid profile
files report exact owned code/span/field/indices diagnostics. Source bodies do
not enter a profile or corpus cache.

Selection applies the captured AST defaults, project `.astignore` and local `.gitignore` through the Core matcher. This repository's versioned [`.astignore`](../../.astignore) excludes the migrated generated Tree-sitter `parser.c`/`parser.c.inc` files, the ANTLR C++ generated directory and the three exact Vite outputs copied by `scripts/build-rust-ui.mjs` into `crates/graphit-app/assets/ui/` (`index.html`, `logo.svg`, `favicon.svg`). Maintained grammars, actions, wrappers, authored scanners and React/TypeScript/UI sources remain eligible. The concrete selected-file limit still rejects an oversized authored file; generated output is excluded by its project rules rather than by raising that limit. Both rule families use standard Git traversal: an excluded parent directory is pruned, and an unrelated negation cannot reopen it. Reinclusion requires permitted parents, such as `generated/*` with `!generated/driver.rs`, as described in the [watcher contract](rust_daemon_watchers.md#observação-nativa-e-ignore).

[Native selection](../../crates/graphit-core/src/artifact_store/native_source/selection.rs)
retains paths, lengths, hashes, directory/control/file witnesses and its original
request, with `retained_bytes()==0`. Each selected FILE is opened from that owner,
read completely, checked against its captured length and SHA, and decoded as UTF-8.
The [reader](../../crates/graphit-core/src/artifact_store/source_reader.rs) validates
same-request bytes without restoring a reader from data or reopening another path.
A bounded channel of one batch provides SDK backpressure; the producer and SDK
future are joined locally while genuine borrowed authority stays live. FILE and
body owners drop before the next batch. No corpus body vector is collected.

Directory and ignore-control witnesses are metadata, not a corpus-wide handle
cache. The reader owns its birth-root and opens only the ancestor chain required
by current enumeration, FILE IO or revalidation. Each relative reopen checks the
original physical observations for every prefix and leaf. Active Windows chains
keep their no-delete handles until IO completes; recapture keeps the first
selection alive without retaining another descriptor per visited directory.
Filesystem errors, including descriptor exhaustion, remain causal job errors;
no empty publication, raised process limit or new corpus quota replaces them.

Directory lookup uses a shared per-capture native-path index while preserving inventory positions and traversal order. Selected files retain their existing native path order through an ordered map; the same ordering finds selected ancestors without crossing every directory with every selected file. Metadata paths are shared with the witness, and the active-chain IO checks above still validate each prefix and leaf.

Each FILE chunk checks its own native root/ancestor/leaf binding, same descriptor, Original and applicable present or absent ignore controls. It does not rescan unrelated FILES or sibling controls. Raw retains full selection/content revalidation after stream preparation and before commit, and its existing checks after commit. Thus a changed sibling may leave a local FILE readable while the final whole-source check rejects publication.

Raw uses the existing `source_schema`: literal UTF-8 path and nullable source.
Empty text is available; NULL means text not stored; an absent path is different.
The fixed corpus is `Installation/ast/<ProjectId>/source.lance`, selected through
the actual project version's SDK branch. Full rescans Create or overwrite; path
changes merge UpdateAll/InsertAll and DeleteIf limited to the affected paths.
When the corpus is absent, the first publication captures the complete authorized
selection even if its watcher event contains only changed paths. This includes
an incremental event after a failed initial full scan: unchanged FILEs must enter
the first image too. Named branches retain the empty Main birth and receive their
complete selected branch image. The existing parent checks and exclusive rename
reject a competing corpus birth without adopting or replacing it. Actual parent
directories created before a later capture failure remain in the failure receipt.
Delete and rename remove old rows. A matching indexed source image is a noop,
without a prepared commit or a new recovery key. NULL cannot establish unchanged
physical content, so a real refresh is retained when text is not stored.

[SDK streaming](../../crates/graphit-data/src/lance_module_store.rs) reaches EOF
before preparation succeeds. Errors late in a stream publish no partial manifest.
The [branch store](../../crates/graphit-data/src/lance_branch_store.rs) preserves
zero retries, duplicate-key integrity, keys, strict CAS and recovery. Native births,
materialization and commits are separate effects. Actual created directories,
precommit keys, observed materialization/version and the original cause remain
in the job error. Once commit starts it is awaited even during cancellation.
There is no automatic rollback or retry, and corrupt storage is not absence.

Opt-in debug tracing in the existing `index` span records only the logical
`project_id`, selected `project_version` and finite Raw phase labels. It records
no paths, file bodies, credentials, SQL, hashes or per-file counters. These phases
distinguish the existing preparation boundaries:

- `stream_eof`: the normal producer dropped its sender after delivering its
  batches. This does not mean the SDK finished consuming or preparing them.
- `sdk_prepare_returned`: the SDK preparation future returned, observed separately
  from the producer/SDK join. This may be success or error; it proves neither a
  successful preparation nor a published commit. The direct empty Main birth for
  a new named branch emits this phase without a channel-producer EOF event.
- `prepublish_recheck_begin` and `prepublish_recheck_end`: the prepared result
  reached, then completed, the existing whole-content selection revalidation,
  profile, admission and directory checks before publication. Content revalidation
  recaptures and hashes the selected files; these guards keep their original order.

The coarse `prepare` phase includes both stream/SDK work and that later recheck
block. A last `prepare` event therefore does not locate a pending future. An
absent completion event does not prove a deadlock, and process CPU activity does
not attribute work to Raw when other jobs share the process. An error from one
side of the join can await the other side before reaching the existing causal
error report. These observations do not justify removing physical fences,
changing Original/cancellation behavior or inventing a retry.

The supervisor retains a pending full Raw reconciliation after a failed full scan
or delta, including when Main already exists. Its next genuine watcher batch or
rescan captures the complete selection again and clears the obligation only after
Raw succeeds. It does not schedule another batch or replay a prepared commit.
An error after publication retains the known effects; fresh authorization and
Original checks still apply to the next reconciliation. Knowledge and Memory
receive the original batch flags and paths, independently of this Raw promotion.

The semantic/Graph job has a separate pending obligation. A new Raw publication,
first admission, changed profile/configuration controls, or changed model-resource
identity requires recomposition. A successful Raw no-op does not by itself trigger
NN merge, graph extraction or publication: ordinary unsupported-file events and
Memory CRUD rescans keep existing Raw/NN/Graph generations when those inputs are
unchanged. Model readiness and reload still arrive through ordinary rescans; the
supervisor compares a weak reference to the previous immutable execution resource,
so it detects replacement or removal without keeping the old model alive. This
reference caches neither authorization nor corpus data.

Raw or semantic failure retains that semantic obligation for the next genuine
watcher batch or externally requested rescan, even if Raw then returns no change.
Only successful semantic completion without cancellation clears it. No timer,
self-generated retry, prepared-effect replay or new resource quota is involved.
Knowledge and Memory still process their own work independently.

Graph publication recovers an interrupted publication only when its key and
destination match the selected AST graph, under the same captured writer. Core
preserves the previous published entry and records an incomplete owned stage
durably before releasing that pending operation. Pending work for another key
or destination is left intact; publication then follows the ordinary failure
contract. No catalogue repair or `forget` is performed by the AST producer.

[Explicit PROJECT controls](../../crates/graphit-daemon/src/watcher.rs) traverse
protected ignore ancestors only for the configured config file and query-directory
prefix. Their changes cause a rescan before ordinary ignore filtering. The same
supervisor remains to observe configuration even if Ast begins disabled. The job
checks fresh ModuleSettings, so disabled Ast performs no Raw publication.
Access events, including closing a control file after reading it, are excluded
before reload as well as before ordinary delivery. Reads of profiles and controls
therefore do not feed the rescan queue.

The same native watcher also observes the installation's global configuration,
USER query directory and the query directory of the compiled runtime version.
Only changes to those controls request a coalesced rescan; other runtime versions,
Raw corpus writes and profile reads do not. Missing control directories are
observed through their nearest existing directory and parent, so creating them
can install the corresponding native watch without polling or an installation scan.
These paths and events are notification hints. They grant no access: each job
still captures, authenticates and validates the actual configuration, profiles and FILEs.

On a rescan the App rereads global and PROJECT configuration and updates the
watcher's relative control directories and ignore scopes when they have changed.
Changing `ast.queries_dir` therefore retargets native observation even beneath
ignored ancestors. The handle is weak, requires the same canonical checkout and
rejects malformed paths or a stopped owner. It adds no independent lifecycle;
the existing supervisor cancels callbacks and joins its watcher. Disabled Ast
still observes configuration and retargets controls, while publishing no Raw.
ReadAst continues to govern reads of an existing Raw image. NN freshness,
inference, entity graph publication and the full migration remain separate work.
The semantic endpoint resolves current Ast/Embedding settings for the one
authenticated and admitted PROJECT selection. It preserves command, environment
and distribution layers while rereading global and PROJECT configuration. Ast
disabled returns `model_disabled`; enabling it does not manufacture an absent
semantic producer or lazily activate Memory.
AST components are eventually consistent. Search captures only the selected
native Lance corpus; Source and the Icebug graph can advance independently.
A missing graph or Source does not prevent lexical or compatible-vector search.
Responses do not publish or compare cross-component generation identifiers.
Rows retain their public identity, source association and coordinates, while
vector candidates and rerank text come from that same corpus snapshot. Rerank
uses published projections or lexical text, without requiring an exact Source
span. A genuinely absent candidate is handled individually. Source requests
still verify the requested file/span rather than serving unrelated text.
Browser joins independent graph results by public identity, keeping physical
Ladybug IDs scoped to their own query. Authorization, query-specific pagination,
cancellation and deadlines remain enforced.

The hot reload changes above are implemented under `tsk-2297`. Two native cases
passed missing control-directory birth, current runtime/Access exclusion,
protected-ignore retarget and malformed/foreign/stopped handle rejection. Two
App cases passed automatic runtime/USER/global updates in two PROJECTs, sibling
runtime isolation, global selector mapping, disable/enable, actual Source HTTP
and a second profile edit after retargeting beneath `target/`. A selected-module
case passed fresh global/PROJECT settings, command/environment precedence and
cancellation. The earlier `tsk-393d`/SDK evidence below is retained without replay.

## Qualification status

The Core selection focal passed 32 cases, including actual profile basename
selection, zero body retention, per-FILE validation, request mismatch and changed
content. Both explicit PROJECT watcher controls and control-read exclusion passed.
The three foreground fixtures passed actual HTTP/Source, create/change/rename/delete,
named/default branches, custom profile layers, empty text, ignore, late invalid UTF-8,
noop, NULL, current module settings, denied PROJECT and cancellation. Five loader
fixtures passed stock routes, overlays, configured allow/deny, explicit/exclusive
routing and invalid-profile diagnosis/revalidation. Current strict library Clippy
passed for Core, daemon and App after concurrent Data/Records corrections.
A postcommit cancellation
preserves the real key, observed SDK version,
cause and retained foreground owner for explicit cleanup. Installing user profiles
does not relax the private artifact namespace requirement.
The [SDK stream fixtures](../../crates/graphit-data/tests/lance_source_stream.rs) passed
five Linux cases and strict Clippy. Windows/macOS are unrun. Parser, M, Auth and Broker
producer suites are reused rather than replayed.
