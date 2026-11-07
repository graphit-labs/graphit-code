---
title: Local Role MCP toolbox
type: reference
status: partial
updated: 2026-11-06
references:
  - target: {type: task, id: 01M5C6B85MCWH5HX932YVEKN3Z}
    relation: implements
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-e6b2}
    relation: uses
  - target: {type: task, id: tsk-51f2}
    relation: relates_to
  - target: {type: task, id: tsk-ef75}
    relation: relates_to
  - target: {type: task, id: tsk-b912}
    relation: implements
  - target: {type: task, id: tsk-ef62}
    relation: implements
  - target: {type: task, id: tsk-69f2}
    relation: uses
  - target: {type: task, id: tsk-be6e}
    relation: implements
  - target: {type: task, id: tsk-01eb}
    relation: derived_from
  - target: {type: task, id: tsk-b9fb}
    relation: implements
  - target: {type: task, id: tsk-9f05}
    relation: implements
  - target: {type: task, id: tsk-eec8}
    relation: implements
  - target: {type: task, id: tsk-6cf9}
    relation: uses
  - target: {type: task, id: tsk-cd41}
    relation: implements
  - target: {type: task, id: tsk-2005}
    relation: derived_from
  - target: {type: task, id: tsk-8f4d}
    relation: uses
  - target: {type: task, id: tsk-2f38}
    relation: relates_to
  - target: {type: task, id: tsk-abb4}
    relation: supports
  - target: {type: task, id: tsk-112f}
    relation: implements
---

# Local Role MCP toolbox

`graphit_app::role_tool_server::RoleToolServer` is a composable rmcp/Axum
producer for genuine run-owned Role credentials. Its router serves `/mcp`;
the current daemon mounts it at `/roles/mcp`. It shares the actual
DaemonAuth issuer and retained Runtime project tuples. It creates no Runtime
key, profile, Role definition, grant or alternate daemon. Runtime, Local and
browser credentials cannot substitute for a Role credential.

## Structured MCP output

Structured Role tools accept optional top-level strict boolean `ai_optimized`,
alongside `selection` and `input`. Omitted/`true` returns TOON text without a
JSON `structuredContent` duplicate; `false` selects JSON for machine consumers.
The JSON-RPC frames, arguments and catalogue remain JSON. AST Source and
SessionHook retain their textual payloads. ModuleSkill, Knowledge Source and
Role Source and Live MCP AST Source return structured envelopes whose original
content strings remain
unchanged. See [the shared MCP contract](mcpstdio_module.md#structured-output-and-errors).

## Role hooks at current Kiro V3 boundaries

`<brand>_session_hook` uses the same strict `{selection?, input}` envelope.
Input is `{engine:"kiro", event:"SessionStart"|"UserPromptSubmit"}`. The removed `v3` field and legacy event aliases are rejected.
The existing issuer verifies bearer, exact Role reference, installation, physical
PROJECT and version before interpreting the recipient as data. Selectors, role
names, module switches and reference decoding never confer authority.

Scout and Scribe read the complete mandatory PROJECT Memory selection through
NativeMemoryStore and real ProjectRecordAuthority admission. Output is one native
plaintext payload containing version, corpus state and canonical current rows;
no Task or USER data is requested. Tracker returns authenticated empty output
without Memory admission. Hooks or Memory disabled also yield empty output only
after actual authentication and scope checks. NoCorpus and a published empty
selection remain distinct; store, permission, cancellation and encoding errors
are errors rather than healthy Quiet.

The request reloads actual Global and PROJECT Core configuration while retaining
captured Command, Environment and Distribution layers. Memory composition uses
the shared bounded writer with the bootstrap prompt; the native output codec
uses the captured positive content limits. The existing native job owner checks
Original, live selection and grant again before delivery. Role hooks accept
only current `NativeBinding::KiroV3` and those two canonical events.

Implementation: [handler](../../crates/graphit-app/src/role_tool_server/session_hook.rs),
[catalog](../../crates/graphit-app/src/role_tool_server/catalog.rs) and
[transport](../../crates/graphit-app/src/role_proxy.rs).
Run the registered guard with `cargo xtask test graphit-app/role_hook_`.
The compiled journey is registered separately as
`cargo xtask test graphit-app/role_hook_v3_executable`. The library and integration selectors support affected validation; both are covered
by the pre-push workspace test gate. No push is initiated by these checks.

## Inputs and request lifetime

The owner supplies SourcePolicy, effective ConfigSnapshot and existing SDK
policies for Memory, Task lexical search, Knowledge, document rendering and
instructions. TaskPolicy comes from the same HubResourcePolicy used by
ForegroundTaskConfig, independently of BrowserConfig. It retains the SDK batch
default, content bounds, request deadline, projection, native-file and
publication limits. Task authored-reference input bounds come from the same
ContentLimits collection and input-byte limits; omitted references use the
SDK Omitted variant. No new quota or reservation is introduced.

Requests carry a sensitive Authorization bearer header,
`x-graphit-auth-origin: role` and bounded JSON `x-graphit-role-reference`.
Optional `x-graphit-role-selection` is bounded UTF-8 Selection JSON with project_dir,
project_id and/or project_version. These are selectors, never authority; they
select only tuples retained by Runtime. project_dir is a literal Unicode path string. Without it, list_tools uses current.
Calls without explicit selectors use the same header/default. Explicit call
selectors remain subject to the actual run grant. Ambiguous siblings fail.

Arguments use the strict envelope `{selection?, input?}`; each tool schema
describes its input. Unknown fields or closed-enum values fail. No actor,
principal, grant or issuer is accepted as an input DTO.

Each call owns one fresh RequestContext and real installation/project captures.
Task protocol operations delegate their existing executor without outer
verification or a second Original. Task lexical search and references use their
native readers under the Role admission. Other operations retain the same grant/request through admission,
SDK awaits and final recheck. List is a separate authenticated request and
filters tools with those same permission classes. Hidden direct calls fail.

Startup compares the configured installation with Auth using its canonical root
and retains that resolved installation for later Global reads. Brand checks and
physical installation/request revalidation remain required; another installation
is rejected even when its policy otherwise matches.

Native jobs remain owned through cancellation and shutdown. A cancelled
shutdown waiter leaves the JoinSet with the component; the daemon drains it
before Auth teardown. The Role HTTP owner retains the first observed actual undelivered
I/O/join/close error with its kind and source while continuing every join and
retained close. A delivered `RunOutcome.failure` remains a handled request; it
does not poison global shutdown. Confirmed Task/Memory effects retain causal delivery
errors. A successful Memory write followed by a failed final check returns
write_confirmed with actual Memory ID/revision and cause, not an invented SDK
manifest version or rollback. Transport loss proves no rollback.

## Implemented handlers

Names derive from validated Brand. This table uses graphit.

| Tools | Producer and authorization |
|---|---|
| session_hook | Verified run-owned Role; Scout/Scribe PROJECT Read(Memory), authenticated Tracker/module-off empty output; Kiro V3 SessionStart/UserPromptSubmit |
| task_get/list/query/session_get/session_list | Task protocol reads, Read(Task); query is typed ReadView, not arbitrary SQL |
| task_comment_add, task_progress | Existing fenced writes, WriteOwnTask; no coordinator claim reuse |
| task_search | Native same-pin FTS, literal query/filter/cursor, Read(Task) |
| task_references | Native Task/Session source references and independent page cursors, Read(Task); targets confer no authority |
| memory_get/source/list/mandatory/important/history/search | PROJECT NativeMemoryStore reads, Read(Memory); actual lexical index |
| memory_insert/update/delete/mark_mandatory/unmark_mandatory/promote/demote | NativeMemoryStore, WriteMemory, revision/idempotency checks |
| knowledge_list | Complete admitted native Knowledge heads |
| wiki_source | Selected native page after EOF validation and document_source rendering |
| knowledge_id | Pure default ID from KnowledgeSourceIdentity::from_relative_path(path, None, current scalar limit); Read(Knowledge), no document/corpus access or write |
| knowledge_schema | Actual producer physical Knowledge schema under Read(Knowledge); no row scan/count implied |
| knowledge_search | Same-pin native lexical Knowledge search and borrowed Markdown preview, Read(Knowledge) |
| knowledge_references | Native Knowledge source edges, literal backlinks and incomplete coverage, Read(Knowledge); no USER/foreign target resolution |
| wiki_log | Actual document synchronization transactions for the admitted PROJECT/version, Read(Knowledge), paginated without corpus bodies |
| ast_source | inspect_source_role with local index/physical Source, Read(Ast) |
| ast_schema, ast_query, ast_search | Admitted PROJECT graph/SDK pin with Read(Ast); readonly Cypher, FTS and shared-service semantic/hybrid search |
| module_skill | Read(selected module), captured overrides, shared MiniJinja renderer |

`knowledge_id` takes `{input:{path:<relative Unicode string>}}` in the normal
selection envelope and returns exactly `{id}`. It uses the publisher's default
identity factory and never opens the supplied path. It needs no Knowledge corpus,
model, external account or provider. Absolute/escaping/NUL paths and existing
content limits are rejected. The output is ordinary data, not a PROJECT grant;
explicit frontmatter IDs remain the publisher's input and are not overwritten.
Catalog filtering, run/selection verification, Knowledge admission and the final
rechecks are the same as other Knowledge handlers. `ai_optimized` stays top-level:
omitted/true yields TOON and false yields JSON, with no JSON-only outputSchema.
`mcp_tool_knowledge_id` is resolved by the existing dynamic Brand template expansion.

The native Role catalog selects handlers on Linux, Windows and macOS, using the corresponding native producers.

Memory preserves absence, generation, current metadata, line coordinates and
immutable before/after history. Knowledge distinguishes NoCorpus, PublishedEmpty,
PageAbsent and Found; only the selected page body is retained. Error responses
classify authentication, request, input, SDK storage and confirmed effects
without dumping protected files or tokens.

## Role PROJECT AST reads

The native Role catalog on Linux, Windows and macOS exposes `graphit_ast_schema`, `graphit_ast_query` and
`graphit_ast_search` under the same closed `{selection?, input}` envelope.
The selected PROJECT/version must be retained by Runtime and admitted by the
actual Role grant. A selector, installed context name or Runtime credential
cannot supply that permission. These tools consume the current PROJECT graph;
they accept no imported `context` field.

Schema takes an empty input. Query takes `query` and an optional `parameters`
object, reusing the existing native readonly Cypher executor and typed result
encoding. Schema and query retain the actual `PreparedRead` and captured graph
in the server's owned blocking job. They neither open embedding resources nor
route through Runtime/Local authentication.

Search takes `query`, optional `mode` (`fts`, `semantic`, `vector`, `hybrid`),
`k` (default 10) and `metric` (`cosine`, `l2`, `dot`, default cosine). Mode defaults
to semantic, as in the shared search input. FTS uses the actual `EntitySearch`
pin and never reads the model slot. Semantic/hybrid use the daemon's existing
shared slot and `capture_role_service` from the actual grant, preserving the
same Original. No model/tokenizer is created per Role and no RemoteRequest is
granted implicitly. The common NN and RRF kernel remains shared with the other
graph consumers.

Responses use `{kind:"graph", result:...}`. Search preserves graph/search/raw
generations and public hit metadata without retaining corpus bodies. A missing
search index returns `index_absent`; absent model resources return
`model_unavailable` for semantic/vector. Hybrid can still return available FTS
results with `vector_status:"model_unavailable"`. Invalid input, failed native
read, revoked permission, cancelled Original or inactive selection remain errors.
The Role retains its admission and SDK pin through post-read checks; pin checks
also cover the shared kernel's early lexical/component-unavailable returns.

Example query payload:

```json
{"input":{"query":"MATCH (n:Function) WHERE n.name = $name RETURN n.name AS name","parameters":{"name":"main"}}}
```

The implementation is in [the Role AST consumer](../../crates/graphit-app/src/role_tool_server/ast.rs)
and [its actual MCP fixtures](../../crates/graphit-app/src/role_tool_server/tests/ast.rs).

## Role native references

On the native Role endpoint for Linux, Windows and macOS, `task_references` reads native Task/Session edges under Read(Task).
`knowledge_references` reads the selected PROJECT's Knowledge edges under
Read(Knowledge). Both use the source module's actual `ProjectReadAdmission`,
current generation, closed cursors and the same Original. They preserve native
no-corpus, incomplete, zero-match and published states. They do not open a USER,
foreign PROJECT or imported target merely because an authored edge names it.
A literal incoming target searches only the admitted source corpus; unresolved
targets remain unresolved. This is the Role contract, not a missing global
resolver. The separately authorized Root/Browser reference navigation was
qualified by 51f2/ef75; it does not expand a delegated Role grant.

The Task input is the native `TaskReferencesQuery`; `page_size` is required and
between 1 and 100. Keep returned cursors unchanged:

```json
{"input":{"focus":{"task":"00000000000000000000000001"},"page_size":1}}
```

Knowledge accepts an outgoing slug, an incoming logical target, or both, with
independent outgoing/incoming cursors:

```json
{"input":{"outgoing":"alpha","page_size":1}}
```

```json
{"input":{"incoming":{"target_kind":"memory","scope":{"user":"other-owner"},"id":"mem-foreign"},"page_size":1}}
```

These payloads are exercised by the [actual Role MCP reference fixtures](../../crates/graphit-app/src/role_tool_server/tests/references.rs). The [adapter](../../crates/graphit-app/src/role_tool_server/references.rs) calls the native readers directly and preserves their finite errors; it adds no resolver, index or cache. No consumer here requests automatic USER/foreign/imported target resolution; reading such a target remains a separate operation with its own admission.

## Role Knowledge lexical search

`graphit_knowledge_search` uses the existing `{selection?, input}` envelope.
The closed input requires `query`; `top_k` defaults to 0 (all matches in
pages), `page_size` to 20 (1–100), and `preview` to false. `cursor` is
optional, opaque and non-null. Queries must be nonempty after trimming, contain
no NUL and fit the captured scalar-byte limit. Semantic and hybrid modes are
not accepted by this lexical tool.

Every call verifies the actual Role grant and admits PROJECT Read(Knowledge).
The Data capture and Knowledge lexical/preview bridge consume the same Original,
selection and generation pin. Continuations bind the real Actor identifier,
PROJECT identity, native root and version, query and captured limits; they
confer no authority. Each continuation readmits the current grant. Selection,
run and Original are rechecked before delivery.

A fresh absent corpus returns `kind: "no_corpus"`. Published responses distinguish
`published_empty`, `no_hits` and `hits`, with `generation`, `total`, `hits` and
`next_cursor`. Each hit preserves the SDK `focused_source` metadata and lexical
score. With previews disabled, bodies are not retained. A preview contains only
`text`, `source` (summary or body), `range` and `truncated`, rendered by the
borrowed Markdown preview producer within the captured byte limits. The Role
transport's selected-format output cap still applies.

Changed caller/query/limits produce `invalid_cursor`; a stale pin or cursor
against an absent corpus produces `generation_changed`. Invalid input, denial,
preview/storage failure, cancellation, deadline and retired selection remain
errors rather than empty hits or useful partial output. A preview failure uses
`knowledge_preview_failed` and preserves its available causal I/O category.
The lexical path opens no NN/model resources.

## module_skill

Modules are closed to ast, hub, task, memory and knowledge. A companion must
match an exact maintained SkillTemplateSet reference. The physical capture
helper shared with managed_roles reads PROJECT rules, GLOBAL rules, explicit
installed Hub skill and compiled defaults in precedence order. A Hub selection
also requires Read(Hub), actual registry entry and NativeBinding; absent,
ambiguous and out-of-root selections fail. Captures are revalidated after render.

Variables contain Brand tool names and effective knowledge.docs_dir (standard
docs default when absent). No unprovided live fact, secret or filesystem loader
is available to an override. Mentioning a maintenance tool in a general skill
does not grant access or publish an absent handler. Unknown modules/references
and modules outside the grant are denied before protected template reads.

The Role reader supplies the text variable `tool_transport: role`. The AST entry
and impact companion select a Role branch with the actual `{selection?, input}`
payloads, search `k` and nested Core Source selectors. Omission selects the Root
branch; no argument aliases or payload migration are introduced. The affected
fixture renders both Role assets and executes their six JSON examples through
the actual MCP handlers. Runtime renderer checks cover both brands and the
unchanged Root default. The six actual handler calls passed in42o and the seven
Runtime renderer cases passed in41z; this is distinct from physically publishing
all five skills and ten companions.

Memory and Knowledge also select their Role entry/companion branches. Memory
uses `input.text`, nested `filter.exclude_mandatory` and `input.id/selectors`;
Knowledge uses `input.query/top_k/page_size/preview/cursor` and
`input.slug/selectors`. ModuleSkill uses `input.module/reference`. These guides
do not advertise USER scope, Root query fields, imported contexts or maintenance
authority. Their seven Memory and original nine Knowledge JSON examples are
exercised by [the Role instruction consumers](../../crates/graphit-app/src/role_tool_server/tests/instructions.rs),
including an authorized Scribe capture and native reference reads. Memory
examples passed in44f (4.42s), those nine Knowledge examples in44l (58.21s) and
the two Root/default and Role renderers in44d (0.06s each). All ten current
Knowledge JSON examples, including synchronization history, passed in45h
(70.12s); its Runtime renderer passed in45k (0.05s). No physical CP1 publication
is inferred from these handler and renderer checks.

## Knowledge document synchronization history

`graphit_wiki_log` reads committed document synchronization transactions under
Read(Knowledge), using the same real PROJECT/version admission as Knowledge
source and search. Role input is `{"input":{"limit":10}}` with an optional
opaque `cursor`; Root MCP uses `{"history":{"limit":10},"project_dir":...}`
with its actual NativePath selector. CLI exposes `knowledge log --limit 10`
and optional `--cursor` / `--project-dir`. Inputs are closed and each page
captures and rechecks current authority. No imported, USER or foreign corpus
is opened through this reader.

Entries contain the actual SDK version, UTC timestamp and added/updated/removed slug lists. The DTO omits corpus URI and storage credentials, distinguishes no corpus from a published history with no entries and preserves finite cursor or storage failures.

## Installed Hub metadata and FILE content

Role Hub readers use `Read(Hub)` and the closed `{selection?, input}` envelope.
`hub_list` accepts `scope: project|global_installation`, optional literal
`artifact_type`/`id`/`version`, a positive `limit` and optional `offset`;
`hub_show` requires scope, kind, ID and exact version. They read the actual
PROJECT identity-bound or installation owner-bound registry. PROJECT entries
respect the current Role binding/shared target. GlobalNative and logical
GlobalCache are explicit installation metadata, not access to another PROJECT.

The safe metadata projection contains only artifact_type, id, version,
producer, target, origin, alias, entry_name and display metadata. It exposes
neither SourceRef/root/backend nor credentials, hashes or storage topology.
An absent literal version returns `artifact_not_installed`; no latest value
is inferred. Remote FILE metadata independently requires current origin and
producer authorization before delivery. Native metadata does not open corpora.

`hub_content` accepts `{scope,artifact_type,id,version,path}` and reads one
installed artifact text file. It rejects AST/Knowledge and absolute/traversing
paths. Resources must match their canonical SourceRef derived from the admitted
PROJECT; a local link must remain inside that same PROJECT. The source producer
must match that PROJECT. The actual ProjectReadAdmission supplies Core's held-root
CapturedProjectDocumentDirectory, which reads only the selected file and rechecks
the captured chain. It does not scan a tree or require a network grant. Registered
source data alone cannot open another PROJECT, runtime control or arbitrary disk.

Remote FILE content separately captures a current Role service and requires the
explicit RemoteRequest permission; Read(Hub) never implies it. Standard managed
role ceilings remain unchanged. The real custom-Agent registration path can
issue an attenuated HubArtifact grant with Read(Hub)+External after an actual
AdminParent registers its observed Agent body/hash. Broker STSv3 or standalone
ACL independently authorizes the producer. The owned Core cache receipt must
match the exact installed SourceRef. Its Core verification captures the same
installation-only publication scope as the cache producer, using the current
request; the authentication realm's additional trusted PROJECT roots are not
that cache receipt's scope. Missing/changed cache or installation metadata is an
error, with no hydration or credential fallback. Both local and
remote IO share the existing bounded UTF8/FILE output codec; Root FILE routes
retain their service checks.

`hub_projects` reuses the current authenticated catalog client with explicit
transport/provider, optional mutually exclusive id/name_prefix and bounded
pages. Its returned PROJECT IDs/names grant no PROJECT/content access.
`hub_search` requires an explicit observed producer PROJECT and literal
kind/id/version filters. It reads streaming manifest pages only, retains all
declared versions and optional latest, and never infers latest or hydrates
payloads. Both use the actual verified Role service, shared verifier/client,
current remote authority and native cursor/revision kernels. ReadHub alone
does not grant remote effects. Broker and Standalone identity/provider mode
rules, current admission/Original checks, restart and causal errors remain
those of the existing production kernels.

`hub_type_path` accepts only `{artifact_type,name}` under `input`. It uses the
current Role binding verified by the issuer and the existing ProjectionPlanner
preflight; it neither inspects nor creates filesystem destinations. Its eight
fields are artifact_type, binding, target, name, placement, relative path,
node_kind and agent_schema. A null path distinguishes dynamic context from
kinds with no physical destination. Agent schema is the binding's exact SchemaID,
not a JSON schema body. Unsupported artifact kinds fail causally. This advice
confers neither installation state nor write permission and reveals no private
root or runtime path. The [type-path adapter](../../crates/graphit-app/src/role_tool_server/hub/types.rs)
and its [MCP consumers](../../crates/graphit-app/src/role_tool_server/tests/hub/types.rs)
passed the actual MCP cases for SDK layouts and closed inputs (47h), verified
binding and tamper rejection (47h), and cancellation/deadline/revocation/output
bounds (48f). No filesystem publication or vendor account is needed for advice.

[Role Hub handlers](../../crates/graphit-app/src/role_tool_server/hub.rs),
[content adapter](../../crates/graphit-app/src/role_tool_server/hub/content.rs)
and [actual MCP journeys](../../crates/graphit-app/src/role_tool_server/tests/hub.rs)
cover these consumers. Installed metadata and local Core content passed46b
(1.47s/0.58s); their unchanged results are reused. Remote FILE, projects and
search passed the actual Broker journey48h (33.51s), including producer isolation,
cache integrity and the active `broker.s3_disabled` denial. Affected App/Auth/AI/Hub
library and consumer strict lint passed48i (33.41s). The existing Linux gate
prepares its owned Broker fixture; hosted CI and other platforms were not run.
Generic configuration/cluster APIs are not promised by this delegated Role
contract. The active local CLI uses Core ConfigStore/ConfigSnapshot through
[config_entry](../../crates/graphit-app/src/config_entry.rs), including secret
redaction; Role Skill only consumes its already captured snapshot. No Role
catalog/template consumer requires exporting global settings or a private
project roster under ReadHub. Root/CLI/SDK consumers remain independent and
are neither removed nor newly qualified by this scope reconciliation. This
Hub slice alone does not complete N4.

## Scope and remaining integration

Scout, Tracker and Scribe retain current ceilings. Coordination and Task
lifecycle are not exposed by this delegated endpoint. USER Memory, imported Role
Source, other Hub adapters and vendor launch journeys
remain separate producers; this unit does not close N4. Remote Presence denial
without RemoteRequest is finite and does not invalidate local tools. RoleNow
library 2f38 is delivered separately; its MCP attachment belongs to the consumer.

The active daemon captures RoleToolPolicy from its effective ConfigSnapshot,
Hub content/publication policies and existing native-file bounds before serving.
It cancels and joins this owner before issuer teardown. RoleProxy forwards stdio
MCP to /roles/mcp with the transient `GRAPHIT_ROLE_BEARER` and
`GRAPHIT_ROLE_REFERENCE` pair. The child may also receive non-authority
`GRAPHIT_ROLE_SELECTION` and `GRAPHIT_ROLE_BINDING` selectors. Missing credentials
fail closed; this mode never reads the Runtime key as a fallback.

For `mcp --stdio`, the CLI selects only the Role proxy whenever its captured
process environment contains any `GRAPHIT_ROLE_` field. The explicit `--role`
selector uses the same path. Both require a non-empty transient bearer/reference
pair before PROJECT discovery; the existing proxy performs credential decoding and
the Role endpoint verifies authority. `--live-user`, `--project-dir` and an explicit
`--project-version` conflict with this transport. Missing or invalid Role inputs never select Root bootstrap or
Runtime authentication; reading the same daemon's endpoint metadata remains the
Role transport's discovery step.

`GRAPHIT_ROLE_BINDING` optionally selects a closed `NativeBinding` value for the
`x-graphit-role-binding` header before the proxy requests the Role catalogue.
The normal daemon binding applies when the field is absent. The header is a
selector and grants no permission; the issuer verifies it against the actual
run credential, reference and request scope on each Role request.

Coordinator installation publishes the branded executable name through PATH
with neutral server argv `mcp --stdio`, retaining no machine-specific paths or
static PROJECT/version flags in MCP configuration. Custom installations require
the branded `GLOBAL_DIR` variable in the native host environment. Codex's typed
stdio plan forwards that name with `env_vars`; the field is not emitted for
other bindings. Managed Role launches supply the captured installation and
native executable directory through the child environment, without persisting
resolved values into the profile. The Codex builtin also forwards the four
`GRAPHIT_ROLE_*` transport names (BEARER, REFERENCE, SELECTION, BINDING), plus
`GRAPHIT_LIVE_MCP_URL`, `GRAPHIT_LIVE_MCP_BEARER` and
`GRAPHIT_LIVE_WEB_SESSION`. Together with the branded `GLOBAL_DIR`, these are
eight names, never credential values. Normal Root launches have no transient
Role/Live credentials. Partial Role credentials fail closed through the existing
Role selector, and Live retains its explicit `--live-user` path. The generic
SDK continues to accept other valid environment names for user-owned MCPs.
An ordinary Root launch
resolves PROJECT and the current version from its CWD through the existing
checkout maintenance path. An inherited Role launch uses the verified private
selection and Role proxy instead. Neither mode requires a separate server
configuration, and the installation locator confers no authority.
See [the publication factory](../../crates/graphit-app/src/project_coordinator_install.rs)
and [its compiled consumer](../../crates/graphit-app/tests/project_coordinator_install.rs).

The launcher selects the maintained Process or ACP adapter using the captured native binding and the current RoleIssuer run. Kiro V3 keeps its explicit inline Role proxy and uses `acp --agent-engine=v3` plus `_meta.kiro.modeId`. Other hosts reuse the configured Graphit MCP server; transient private Role ENV selects the Role endpoint without Root fallback. OpenCode 2 selects the published primary- capable profile through its advertised ACP mode, then permits Code Mode and only the configured Graphit namespace; nested tools retain native permission checks and issuer authorization. Kimi uses its actual Markdown profile with print mode, not an ignored `--agent-file web` combination. None of these profiles contains the bearer or confers PROJECT authority. See [current Run transport](managed_role_context.md#run-owned-native-transports). Stop accepts only an empty application payload or the observed typed truncated-body EOF, with no payload chunk before it; delivered deadline failure does not poison shutdown.

Generated PROJECT Role hooks resolve the branded executable through PATH and
`--project-dir .` from the host workspace. They retain the explicit Role recipient;
missing or invalid factory credentials cannot select Root. Recipient EOF before
publication cancels and joins that request without stopping Source or publishing
the profile. Service stop before publication also cancels and joins the owned
worker; closing its HTTP recipient during that drain does not create an operation
failure. Foreground SIGTERM returns 130 when no real operation or cleanup failure
occurred. A real failure retains its cause.

The managed launcher now prepares Scout/Scribe mandatory PROJECT context
under the run's own verified bearer and actual ReadMemory admission before
starting the native child. The HTTP owner captures current PROJECT/version and
module switches per request, with the existing Memory read limits, and retains
that frame through the run. Memory-disabled projects omit the prefix without
a corpus read. The next request resolves current Global/PROJECT configuration;
operational membership does not grant authority. This is a transient first-prompt
consumer, not a new MCP handler or permission.

The complete current mandatory selection uses the existing seven-field row
projection and preserves `no_corpus` versus `published` (including an empty
selection). It contains no USER, Task, session correlation or actor context;
static profiles carry no live payload. The whole composed prompt fits captured
content.max_input_bytes or fails before child launch, following the existing
run-close/residual ownership path. See
[managed launcher context](managed_role_context.md).

The Kiro Role renderer now accepts the typed Tracker only with its explicit
run-owned MCP plan. Current V3 launches `acp --agent-engine=v3` and selects the
actual profile through NewSession mode metadata. The Markdown renderer receives
both actual runtime hook plans before publication. It keeps filesystem writes
and native shell denied, allows the explicit MCP namespace, and disables inherited
MCP configuration and Powers. The generic renderer still rejects readonly inspection-shell for Kiro
without that Role/MCP composition; Codex and Cursor profiles retain their
existing policy. Rendering never grants Git access. Tracker omits Memory under
its existing ceiling, using the role in the actual verified credential reference.
The earlier eec8 negative qualification is historical. The 9f05 Linux consumer passed authenticated native SDK/MCP checks and
compiled-launcher controlled ACP journeys; installed vendor/model execution remains separate.

The historical 112f parser receipt used CLI 2.28.0 and `--v3 acp`; it does not
qualify the current argv. CLI 2.31.1 help inspection supports explicit
`acp --agent-engine=v3`, which the current consumer uses. Installed authentication,
event delivery and model execution remain separate checks.

## Readonly Git inspection

The Role catalog exposes `graphit_git_status` and `graphit_git_diff` only when
the actual run credential authorizes `InspectGit`. The same permission is
checked before opening the selected physical PROJECT, at the reader checkpoints
used by SDK callbacks and protected passes, and again before delivery.
Scout and Scribe cannot use these tools under their current ceilings. PROJECT
selection never widens a run's scope; its logical version is not matched against
Git branch or HEAD. HEAD is read only to compare the staged index.

Use `{"input":{}}` for status or tracked worktree diff. Use
`{"input":{"comparison":"staged"}}` for HEAD-tree-to-index diff. Unborn HEAD
uses the SDK's empty tree. Unknown fields are rejected; callers cannot supply a
program, command, environment, working directory, remote or arbitrary revision.
Status reports separate `tree_index` and `index_worktree` facts. Untracked files
are status facts and do not appear as tracked worktree patches. Staged facts
include the actual old/new entry kinds and identify type changes. An unmerged
index fails both diff comparisons explicitly. Gitlinks carry actual index/checked-out commit
identities and nested worktree status; they are not blob patches.

Paths and unified hunk bytes are always base64, preserving non-UTF8 data. Diff
entries identify text, binary or gitlink. Text hunks use the SDK's unified-diff
renderer. Binary entries report the actual source/destination sizes. Gitlink
identities are explicit fields; their zero blob sizes do not measure commits.
A transition between a gitlink and a blob or symlink fails the whole diff with
`Unsupported`, rather than reporting a blob object as a commit.

The consumer uses `gix` 0.89.0 with SHA-1 and SHA-256 support. It opens only the
supplied captured PROJECT. It removes filter drivers and diff command/textconv
keys from the SDK's private in-memory configuration before inspection. This
never writes `.git/config`, personal configuration or the index and invokes no
Git process, external driver or remote operation. Repository content is an
observational SDK read, not an atomic snapshot of concurrently changing Git
files. The original request and physical authority are rechecked before return.

The synchronous SDK status producer uses scoped workers. Its submodule adapter
uses that same producer instead of the SDK's high-level iterator, whose early
drop can detach workers. The blocking producer remains in the existing jobs
owner. Transport drop, service stop or deadline cancels the same request and SDK
flag; the owner awaits its join before shutdown finishes. Cancellation is
cooperative; native scans, allocations and tree conversion are not hard RSS or
preemption guarantees.

The existing content limits bound collection, path scalars, blob input and the
whole serialized output. Required output is serialized once into a checked
writer; excess or allocation failure discards the response. There is no
truncation or successful prefix. Failure diagnostics expose a static category
and IO kind, never repository paths, bodies, credentials or raw SDK messages.

SDK-only receipts are under
`.build/rust-runtime-git-inspection-contract/`: native03 covers the synchronous
producer for SHA-1/SHA-256; native04 additionally observes actual dirty nested
submodules. Configured clean/textconv helpers never ran and main/nested index
and configuration bytes stayed unchanged. These receipts do not qualify Auth,
MCP, cancellation, the launcher or an installed vendor. Historical 9f05 consumer
checks and receipts are maintained separately in
`.build/rust-runtime-tracker-git-9f05/` and its Task. Historical native05 passed four focused cases (three native SDK/Auth/MCP and one renderer) in 8.90 seconds. Its renderer receipt predates the current V3 profile port; unchanged SDK/Auth/MCP evidence retains its original scope.
Historical Executable02 passed four Scout/Tracker × V2/V3 journeys through the compiled CLI,
daemon and stdio Role proxy in 6.93 seconds, with a controlled ACP peer.
Bootstrap02 passed three cases in 6.40 seconds; affected cancellation and physical
binding checks passed. App strict lint includes tests. These Linux component
receipts do not qualify an installed model, other operating systems or broader
Task/session/USER context delivery.

Run the registered focused guard with
`cargo xtask test graphit-app/git_inspection_`. The same library filter belongs
to the existing test selector catalog; its selector uses the final filter segment.
A full Rust function path is not the selector. Linux execution passed four
cases; Windows and macOS execution remain unqualified.

## Verification

The receipts below precede the current V3-only port. They preserve the Role
authority and lifecycle evidence in their recorded scope; they are not fresh
qualification of the changed V3 consumer, profile or installed host.

Four current Linux Role socket cases passed, covering genuine Scout/Tracker/Scribe
grants, native Task/Memory lexical indexes, selected Knowledge, local AST, physical
PROJECT/GLOBAL/Hub/default skills, peer selection, denied coordinator fence,
revocation/expiry and cancellation/EOF/confirmed Memory effects. The focused
Reference error mapping, affected formatting, the public SDK example and affected
App/Daemon strict lint passed. These qualify the Role toolbox. Historical receipts under
`.build/rust-managed-roles-20261011/role-toolbox/` are absent in this checkout;
the completed cd41/abb4 Tasks retain their qualification evidence. Current
consumer/regression receipts are under `.build/rust-runtime-role-bootstrap-eec8/`.
No supplier suite,
performance campaign, vendor launch or other-OS execution is implied.

The historical launcher packet added three renderer checks, two RoleProxy/cancellation socket cases,
one ACP V2/V3 socket case and one compiled CLI case covering both engines.
The executable case checks the Source payload, confirmed run close, no bearer in
native profiles, rejection without transient credentials and owned daemon cleanup.
Affected App library, binary and executable-test strict lint and formatting passed.
The initial executable failure exposed a Tokio task created before entering its
runtime; the Role CLI now performs discovery inside the retained runtime.

## Implementation

[Role endpoint](../../crates/graphit-app/src/role_tool_server.rs),
[transient stdio proxy](../../crates/graphit-app/src/role_proxy.rs),
[run owner](../../crates/graphit-app/src/managed_role_run.rs),
[shared Task policy](../../crates/graphit-app/src/foreground_tasks.rs) and
[actual socket cases](../../crates/graphit-app/src/role_tool_server/tests.rs).
See [managed install/sync and launcher scope](managed_role_context.md).

## Current Runtime selections and wake handles

Role call/list and Git consume the owner's LiveProjectSelections per request,
retain the selected tuple and check current membership before capture/effect
and delivery. Actual Role verification and per-operation admissions still
apply. A switch does not retarget a run's grant; removal cannot become access
to a sibling PROJECT. Task dispatch uses its real live-selection consumer.

The Source/Role rescan attachment now accepts publication of the owner's
current handle Arc through shared handlers. The Role state uses an RwLock;
an admitted write clones the Arc in a short scope before awaiting the store,
then matches identity, checkout and version exactly. Empty projections remain
valid. The hint supplies no permission and only wakes the actual owner after
Ok or Confirmed; reads and pre-effect errors do not wake it. Poisoned state
fails before the write, and known committed metadata survives late denial.

Global Hub reads compare the retained physical installation directory with the
existing registry parent under the same Original, then use
`HubRegistry::load_captured`. Registry paths and SourceRef metadata keep their
logical spelling. Selected Hub skills capture their source directory before
checking confinement and reading templates; the source directory, registry
selection and ReadHub grant are rechecked before delivery. Existing native
consumer bounds and Hub document bounds apply to their respective reads.
