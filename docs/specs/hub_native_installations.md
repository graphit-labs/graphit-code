---
title: Rust native installation identity
type: reference
updated: 2026-11-07
references:
  - target: {type: task, id: tsk-9a88}
    relation: implements
  - target: {type: task, id: tsk-ad05}
    relation: supports
  - target: {type: task, id: tsk-3ddb}
    relation: derived_from
  - target: {type: task, id: tsk-84f3}
    relation: derived_from
  - target: {type: task, id: tsk-1833}
    relation: derived_from
---

# Rust native installation identity

This is the current `graphit-hub` installation contract, consumed by the Rust CLI/MCP,
configuration, hooks and Browser paths described in the [caller contract](hub_consumer_contract.md).
A library fixture alone does not qualify every native platform or host. The current Kiro binding is
`NativeBinding::KiroV3`, serialized as `kiro-v3`. No launcher version, file
extension or ACP protocol selects a binding. There is no old-registry reader
or data migration.

## Selection and ownership

`NativeBinding` has eight canonical values: antigravity, claude, codex, cursor,
kiro-v3, opencode, qwen and kimi. Select Kiro with `kiro-v3`;
`claude-code` normalizes to `claude`; the retired Gemini CLI and its alias are rejected.
Case-insensitive input parsing is separate from strict canonical serialized
identity. `AdapterId` represents eight host families, including one Kiro family.

`AgentMembership` preserves order and deduplicates binding identity.
`Selection` returns `binding` and `origin`. Flag → Command → Project
may select outside membership. Ambient Environment → Global → Distribution →
OpenCode is used when membership is empty or contains it; otherwise the first
member wins. Batch uses a flag singleton, ordered membership, or the single
resolver. Invalid winners fail without fallback. Membership grants no ownership
or authority; selection reads only the captured configuration.

`ProjectionRequest::target()` derives the closed `InstallationTarget`. The key
namespace, type and real ID are validated together:

| Surface | Target | Library publication |
|---|---|---|
| Agent, all hosts | `native_binding` | Requires Derived payload and matching RC native schema |
| Kiro Skill | `shared_adapter:kiro` | One family-scoped installation at the native skill directory |
| Kiro Command/Workflow | `native_binding` | Unsupported by the generic Markdown route; requires HK's qualified hook producer |
| Kiro MCP/Power | `native_binding` | Native capability/metadata only; real configuration producer remains pending |
| Global AST/Knowledge/Language | `global_native` | One Native/Original corpus per full ID and concrete version, independent of host |
| Project Rule/AST/Knowledge/Language | `shared_adapter` | Rule uses dynamic context; remaining native capabilities do not imply a physical handler |
| Other hosts' non-Agent surfaces | `shared_adapter` | Existing bounded physical/dynamic/native route, subject to capability errors |

For example, Kiro Agent keys use `kiro-v3`; Skill keys use `kiro`.
An Agent key with namespace `kiro`, or a Skill key with namespace `kiro-v3`, is
invalid. Kiro MCP metadata cannot use the shared family target. A native target
for another host's Skill is likewise invalid. Serialized aliases are rejected.
`InstallationTarget::GlobalNative` serializes as `{"kind":"global_native"}` and
uses the reserved key namespace `global_native`. `adapter()` returns `None` for
this target and `Some` of the actual adapter for host targets. `includes(binding)`
is true for every binding because the corpus identity is independent of host;
it grants no access. `for_artifact(binding, kind)` remains the host projection
API and never returns GlobalNative. Only AST, Knowledge and Language accept a
GlobalNative key. Unknown fields, host selection, aliases and entry names are
rejected for a global corpus. These identities do not certify that native hosts
load or execute the content.

## Native files and publication

Kiro V3 Agent goes to `.kiro/agents/<name>.md` with YAML frontmatter and the
literal instruction body. `KiroProfileRuntime` composes its MCP plan and typed
hook tuples before that single Markdown emission. Profile hooks use `hooks[]`;
standalone hook JSON uses `version: "v1"` with PascalCase triggers. Codex Agent
uses `.codex/agents/<name>.toml`; Codex Skill uses `.agents/skills/<name>`.
Kiro Skill uses `.kiro/skills/<name>`.
Rules have no physical rules directory. Name is alias → nonempty entry name →
real-ID basename; aliases change destinations, not installation identity.

Raw `AGENT.md` copying is unsupported. `publish_derived` takes the real source,
validated RC output plan, selected name and host schema. Agent schema must match
its binding before materialization or item effects. Individual
lexical/capability/schema rejection precedes publication. The registry rejects Original Agent records. Every Derived record requires
`original_selection`. The rendered output selection exists only in the transient
publication plan and is not written into the registry. The original selection names the
actual descriptor file read by Agent rendering, including a noncanonical filename,
or the original root directory for a Skill with its standard `SKILL.md` and assets.
The constructor checks that node against the retained source snapshot; registry
restore validates its safe relative path and permitted kind. Missing selections
and escaped paths are invalid; absent nodes are rejected by real source capture.
These records grant no access: reading the original still requires its real
authority and a captured reader matching the recorded input fingerprint. The
rendered installation may contain JSON or another native output and is never a substitute
for the original Markdown descriptor. Records without `original_selection` are
invalid; no compatibility inference or importer is provided. Rendering and metadata
are not trusted tool grants; actual runtime qualification remains required.
The current library can return `PublicationChange::Unsupported` inside `Ok`;
public applications must normalize that to a failed capability result.

A Kiro Agent has one current binding, native file, source version,
installed content hash and logical ownership. A family-scoped Skill is
one installation; an equivalent repeat is verified unchanged with zero document
replacements. Conflicting
batch requests or another real ID targeting an owned destination fail. Local
incompatible versions of one stable real ID do not coexist by changing aliases;
explicit update replaces that installation. Global format-4 metadata retains independent concrete versions;
remote authorization and hydration remain producer work in `tsk-7a13`.

## Registry, publication and removal

Configured paths use ordinary OS resolution, including directory links, junctions
and other volumes. `ProjectionPlanner::new(destination_root, limits, mode)`
resolves a selected source `Path` and its `NodeKind`; it retains no ancestor
identities or later directory-revalidation contract. A missing registry reads as
empty without creating files. Explicit writers create required parents.

`hub.json` uses strict format **4**, ordered native membership and typed
`Installation.target`, original source, alias/name, backend and payload. JSON
rejects duplicates, nulls, unknown fields and incompatible targets. PROJECT
physical records select a configured checkout using `PublicationSlot::ProjectRoot`;
shared metadata uses `Scope`. Neither value is authority. Global records retain
independent concrete versions. Metadata commits take a short file lock, compare
the previously read document, then replace it atomically. A stale writer receives
`WouldBlock`; reopening preserves another writer's changes.

`InstalledPayload::Original` carries no physical selection binding. Derived
metadata retains the origin, source policy, source fingerprint and original
content selection. `DerivedArtifact` validates selections against real snapshot
nodes and rendered output. It retains no directory chain, generated content copy,
receipt, journal, backup, pending intent or recovery operation.

Publishing derived output uses its validated input selection to plan the logical
destination, without re-enumerating or checking the original source path again.
Fully retained rendered bytes can therefore be installed after the original is
changed or removed. Payload bytes not retained in the plan are still read and
verified when consumed; their absence or corruption remains an error. The current
request authorization, destination ownership checks and metadata CAS still apply.

Publication prepares source content before Core's short catalogue lock. Core
checks its owned installed hash before update/removal. Hub preflights metadata
before publication and saves the logical selection after Core. These two writes
are not a transaction. `PublicationError.effect` is `NotApplied`, `MayHaveApplied`
or `Applied`, preserving Core's actual effect; a Hub save failure after a confirmed
physical change reports `Applied`. No rollback is promised. A normal explicit
rerun can reconcile already installed desired bytes with logical metadata; edits
differing from the desired content remain conflicts.

`PublicationOutcome` contains `change` and `forgotten: Option<ArtifactEntry>`.
`remove` checks the exact typed key and owned content. `forget` detaches that key's
metadata and returns the former entry while preserving physical content. Neither
requires the original source. An unreadable installed copy returns the OS access
error without temporarily changing its permissions. Explicit Forget can release
metadata without reading or deleting that content. `unregister_binding` changes membership alone; it
does not remove a published Agent or shared Skill. Rendered Link mode remains
unsupported; genuine local-source links remain supported.

## Source reference representation

A persisted SourceRef contains one `root`, its origin, producer, artifact type,
logical ID and version. Its effective directory is derived by the same path
resolver used at construction: LocalLink uses the root itself; Resources resolves
the authored artifact beneath the project resource root; HubCache resolves the
canonical package cache using origin, producer and artifact identity. The cache
adds its existing remote-origin/archive digest binding when applicable.
`physical_id` is present only for a cache whose physical ID differs from the logical
ID. There is no second persisted `directory`, alternate path reader or discovery
fallback. Restoration validates serialized paths, identities and
origin combinations without opening the source. Actual access still requires the
caller admission and actual content validation.

## Verification boundaries

`graphit-hub` unit and public integration suites cover real publication,
verified unchanged output, metadata updates, source-free Remove/Forget,
conflicts, host schemas, shared binding references and current source selection.
Shared-document tests preserve native comments and foreign values. The focused
commands are `cargo test -p graphit-hub --lib --tests --offline` and strict Clippy
for the same targets. They do not qualify separately installed native hosts or
prove native Windows/macOS execution on Linux.

Live Knowledge preparation retains the selected Source SDK pin and its physical
manifest observation. PROJECT captures recheck the same Original, admission,
logical scope and branch; installed captures retain their imported admission
and registry tuple. Preparation records a compact selection/generation stamp
without scanning document rows, collecting corpus slugs, opening NN or running
inference. Each actual Knowledge query still validates its own row contents and
authorization. A newer publication does not retarget a retained context pin.

## Live evidence and selected Knowledge generations

The Live preparation stores the exact selected Knowledge tuples with a SHA-256
observation of their documentary Source generation. The Create request's PROJECT
is stored separately as either Published or an observed NoCorpus; it is not added
to the artifact selection list. These observations contain no credentials, reader,
SDK pin or authorization. Embedding models and NN generations do not affect the
documentary stamp.

GET /api/live/sessions/{id}/knowledge/page requires the persisted Live generation,
a literal page slug and, optionally, the complete selected context tuple encoded
as JSON. It reauthenticates the Web principal and admits USER Knowledge and the
actual PROJECT or installed source read independently. It works after Ready,
Failed or Close without fabricating a running turn. A removed selection or changed
Source generation is refused; it never silently opens newer content. Across
restart the same source can be reopened only while its recorded generation remains
available. This is not historical corpus retention.

The response preserves title, canonical slug in path, physical source_path, body
in content, complete context and Live generation. References are a typed union:
refs.kind=native carries KnowledgeReferenceSnapshotRead for PROJECT and remote
pins; refs.kind=imported carries ImportedKnowledgeReferencesRead for a local
installed pin. refs.data preserves focused_source, outgoing/incoming pages,
namespace, witness, coverage, warnings and the SDK cursor unchanged. No body is
copied into the reference payload. Body and references share one admitted Source
pin. More than one matching context returns only candidate metadata, without
candidate bodies or a dashboard fallback.

The read outcomes distinguish absence from a changed preparation:

- An absent page or an unmatched witnessed reference returns 404 not_found.
  An unchanged NoCorpus observation also returns 404 for a page request.
- A complete context tuple outside the session selection is refused with 403;
  fresh USER and source permission failures remain authorization failures.
- A different Live generation, or a freshly observed documentary generation that
  differs from the prepared stamp, returns 409 context_changed. In particular,
  publishing a Source after the default PROJECT was prepared as NoCorpus does not
  make the old preparation read the new page: that request returns 409.
- Reference resolution omits denied or changed destination candidates. Zero
  eligible matches returns 404; multiple matches returns metadata candidates.
  Cancellation, deadline and integrity failures are not converted to absence.

PublishedEmpty is a real published generation, distinct from NoCorpus. Neither
404 nor a matching tuple grants access, triggers indexing or updates a stored
preparation. Reopening a changed corpus requires a newly prepared observation.

Live MCP uses the same admitted source kernel. Its Running, session and last-prompt
gates remain mandatory. Heads, page and search compare the actual returned Source
generation with the persisted stamp after the query, including URI, version,
physical branch and inherited target. This avoids a second Source open and avoids
assuming that a pre-query observation binds a later fresh query to that generation.

Contextual reference navigation resolves on click through the same GET endpoint.
An optional reference JSON contains the literal SDK namespace and a nullable
32-byte witness; page remains the literal target slug. Outgoing targets carry no
destination witness, so null requests an actual witnessed focus with that namespace.
Incoming sources supply their observed witness and require an exact match. These
query values are data and grant nothing. Only persisted selected contexts and the
canonical default PROJECT are considered, each with fresh authority and stamp
validation. Every context is captured once for the click, reference metadata is
read first, and the body is read only for a unique match from its retained pin.
Ambiguity returns metadata candidates. Denied or changed destination contexts are
omitted without hiding literal edges or preventing another admitted destination;
cancellation, deadlines and corrupt source data still fail causally. No eager
reference-by-context table, global discovery or corpus cache is created.
