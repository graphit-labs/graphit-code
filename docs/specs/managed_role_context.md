---
title: Managed role context
type: reference
status: partial
updated: 2026-10-16
references:
  - target: {type: task, id: tsk-b912}
    relation: supports
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
  - target: {type: task, id: tsk-abb4}
    relation: implements
  - target: {type: task, id: tsk-cd41}
    relation: implements
  - target: {type: task, id: tsk-bdf5}
    relation: implements
  - target: {type: task, id: tsk-49b5}
    relation: implements
  - target: {type: task, id: tsk-f5c9}
    relation: derived_from
  - target: {type: task, id: tsk-5a55}
    relation: derived_from
  - target: {type: task, id: tsk-9bd4}
    relation: relates_to
  - target: {type: task, id: tsk-a855}
    relation: relates_to
  - target: {type: task, id: tsk-b96c}
    relation: relates_to
  - target: {type: knowledge, id: Native_host_bindings_during_the_Rust_rewrite}
    relation: derived_from
  - target: {type: task, id: tsk-112f}
    relation: implements
---

# Managed role context

Runtime and Hub integrators use this composer to build the default Scout,
Tracker and Scribe bodies with a validated Core Brand and the original request.
The composer is available in the main Rust Runtime. Its Linux functional tests,
role doctests and actual Hub-descriptor integration passed. Native host execution
has separate integration checks.

## Runtime hooks in published Kiro V3 profiles

The managed-role publisher precomposes actual `SessionStart` and
`UserPromptSubmit` plans and passes both through `KiroProfileRuntime` to the
native Markdown renderer before ToolOwned publication. Commands select the
Rust executable, installation, PROJECT and version and include `--recipient role`.
They contain no bearer, live Memory or Task snapshot. Existing shell quoting
and platform policy apply. The output fingerprint covers the final profile,
including its MCP configuration and both inline hooks.

The launcher supplies transient Role transport credentials to the child.
`_session-hook --recipient role` consumes them through the actual
RoleProxy/RoleToolServer. A profile alone does not authenticate its hooks.
The Coordinator entry denies inherited Role transport inputs before Root loading.

Each hook delivers only current mandatory PROJECT Memory for Scout or Scribe.
Tracker and disabled Hooks/Memory return authenticated Quiet without Memory
admission. The bootstrap retains its actual prompt and captured run settings;
later hook requests resolve current Global/PROJECT configuration. Static role
bodies remain free of runtime snapshots. See [Role MCP](rust_role_mcp.md) and
[PROJECT hooks](rust_project_hook_context.md). Historical `tsk-b9fb` controlled
ACP/CLI receipts do not qualify current V3 hooks or installed Kiro/model perception.

## Composition and ownership

The public entrypoint is:

```rust,ignore
let role = compose_role(ManagedRole::Scout, &brand, &original_snapshot)?;
```

It returns Result<StaticRole, RoleError>. ManagedRole is the closed enum Scout,
Tracker, Scribe. StaticRole owns its name and generic body; role(), name(),
body(), summary(), modules(), read_only() and allows_shell() expose its metadata.
There is no arbitrary-body constructor, mutable body, Clone, Debug or serde
support. Composition accepts no live Memory/Task content, concrete project,
session/task IDs or personal instructions.

The function borrows the actual SnapshotContext and checks it before building
and before returning. It creates no fresh deadline, cancellation or request
identity. A cancelled original returns Cancelled; an expired original returns
DeadlineExceeded. Failed work drops its private strings and returns no partial
StaticRole. Repeated calls retain no batch state or admission counters.

A validated Brand and a fixed suffix produce <brand>-scout, <brand>-tracker or
<brand>-scribe. Names must fit Hub's 64-byte ASCII limit; 65 bytes are rejected
before allocating output. There is no truncation or alias. The normative body is rendered from the central Markdown sources through the
shared MiniJinja instruction renderer and its bounded output writer.
Fixed positive template/value/output limits and SDK fuel/recursion apply to this
default factory; they are logical limits, not whole-process RSS or preemptive time
bounds. See the template reference for their exact values and counting rules.

RoleError has static Display messages and these categories:

| Error | Meaning |
|---|---|
| InvalidName | The combined name does not fit its 64-byte limit |
| Cancelled | The original request was cancelled |
| DeadlineExceeded | The original request deadline elapsed |
| Allocation | Reserving an owned name/vector/output buffer failed |
| Request(kind) | Another category returned by the actual Core request check |
| Instruction(kind) | The template stage rejected input, a logical limit or an SDK operation; classified without authored content |

## Default bodies and descriptor intent

Bodies refer to <brand>_module_skill and the matching <brand>-<module> skills.
They require current project instructions, skills before first use, narrow module
MCP retrieval and non-sensitive project facts with logical identity and relative
paths. Runtime snapshots and mandatory-memory titles or bodies are loaded at the
proper lifecycle boundary; they are never embedded in these static files.

| Role | Responsibility and modules | read_only | allows_shell |
|---|---|---|---|
| Scout | Recall and locate without edits; Memory, Task, AST, Knowledge, Hub | true | false |
| Tracker | Read affected callers, tests and docs; AST, Knowledge, Task; report impact | true | true |
| Scribe | Transcribe only decided authorized content; Task and Memory | false | false |

These flags express descriptor intent. Actual issuer policy and native host
capabilities decide whether the descriptor may be rendered or executed; metadata
issues no authentication, tool permission or MCP grant.

The current [Role MCP toolbox](rust_role_mcp.md#role-project-ast-reads) composes PROJECT schema, readonly Cypher and FTS/semantic/hybrid search under actual Read(Ast) admission. Its closed envelope is `{selection?, input}`; the AST skill entry and companion select the matching branch using `tool_transport: role`. The omitted variable retains the Root default. Search shares the daemon's model slot and uses a Role service capture, never a Runtime credential or an implicit RemoteRequest grant.

The same `tool_transport: role` context selects Memory/Knowledge Markdown with
the actual closed reader payloads. A source edge is not a target grant and the
Memory guide offers writes only to a run with WriteMemory. The new
[instruction consumer](../../crates/graphit-app/src/role_tool_server/tests/instructions.rs)
executes their examples through real handlers. Its seven Memory and nine
Knowledge examples, native reference readers and two Root/default/Role renderers
passed in44f/44l/44d; their receipt records that scope separately from unchanged
AST, physical instruction publication and remaining Hub/history work.

The common protocol reserves acceptance, checkpoints, claims, revisions,
completion and lifecycle to the coordinator. A delegate reports anchored
findings or transcribes expressly decided content; it does not acquire a
coordinator claim or release another worker's ownership. It ends its turn without
a waiting loop. A performer already in that role works directly, not recursively.
Separate helpers require an explicit applicable bounded role assignment. If the
host cannot run the role separately, the coordinator performs that work directly
under its ordinary ownership.

## Hub integration and native boundaries

Pass the returned name, summary, borrowed body and metadata to the actual Hub
RequestContext::managed_agent. Retain that Hub request and borrow its original
snapshot for composition. StaticRole must remain alive while a descriptor borrows
its body. Descriptor creation and generic body tests do not establish native
rendering, installation, tool grants or model delivery.

Host policy constraints remain with Hub/Auth producers. In particular, do not
change Tracker intent or invent grants to bypass a native policy rejection.
Overrides, live Memory/Task assembly, actual agent SourceRefs, credentials,
installation/removal, hooks/dispatch and ownership release have separate
producer/consumer contracts. Kiro V3 is the sole current binding for the Runtime front. This leaf does not qualify its installed native journey.

## Validation and sources

The six functional tests cover deterministic default/alternate brands, generic
content and coordinator-only protocol, names at 64/65 bytes, cancellation and
expiry after successful composition, and the actual Hub descriptor using the
same captured snapshot. The six role compile-fail examples cover arbitrary-body
injection and private body/clone/debug/serde boundaries. No obsolete import is
used as the reason for a compile-fail pass.

Run `cargo test --locked -p graphit-runtime --test roles`,
`cargo test --locked -p graphit-runtime --doc roles`, scoped rustfmt and
`cargo clippy --locked -p graphit-runtime --lib --test roles -- -D warnings`.
On 2026-10-10 the main workspace passed all six functional role tests, six role
compile-fail doctests, strict Clippy for the affected library/test targets and
scoped rustfmt of roles.rs, tests/roles.rs and lib.rs. Tests used the actual
three-argument Core constructor and real Hub descriptor producer. The retired
quota-only tests were removed. Native Windows/macOS execution is not proved here.
Kiro acceptance uses the documented adapter and controlled integrations; vendor
installation, login, account, plan or remote catalogue is not a prerequisite.

Source: [roles.rs](../../crates/graphit-runtime/src/roles.rs),
[tests/roles.rs](../../crates/graphit-runtime/tests/roles.rs), and the Root-owned
[exports](../../crates/graphit-runtime/src/lib.rs), whose two retired role reexports
were removed from the main workspace. Native layout
contracts are in
[native host bindings](../development/native-host-bindings.md).

## Centralized Markdown sources

The default-only role factory now renders the normative Markdown assets through the captured instruction stage. Role identity/modules/intent remain Rust data; summaries, common protocol, header and role guidance are Markdown under `templates/agent-instructions/`. The three-argument API, immutable StaticRole and original request boundary remain intact. Override capture and runtime dispatch still belong to their separate consumers. See [Rust agent instruction templates](../architecture/rust-agent-instruction-templates.md) for selection, variables, native literals, limits and current validation scope.

## Managed install and sync

`graphit roles install` and `graphit roles sync` connect to the current daemon
using its physical runtime key. They do not start a replacement daemon when
discovery is absent or busy. The `/admin/roles` route requires the current Runtime
caller, its live AdminParent and explicit ManageAccessGrants, Configure and
PublishedFilesystem permissions. Local/browser callers receive no administrative
authority from this route. Each operation retains one original request and the
Runtime-produced checkout/project/version tuple.

The raw candidates are selected in Project, Global, Hub, default order. Project
files live at `<checkout>/<brand-directory>/rules/roles/{role}.md`, plus
`roles/header.md` and `protocol.md`; global files use `<installation>/rules/`
with the same logical names. `--rule ROLE=ID@VERSION` selects an installed Rule
from the actual global Hub registry and its unique installation target including
the selected binding. Rule targets use the shared adapter identity supplied by
InstallationTarget::for_artifact, while agents remain binding-specific. Missing exact
optional candidates permit fallback; malformed selected content, denied access,
changed captures and cancellation do not. Hub candidates use the same selective
physical document observations; the role entry is required, and absent optional
header/protocol includes remain observed until publication. No whole-tree capture
or retained-path requirement is applied to those absent optional files. The default templates can be included
explicitly with `defaults/roles/{role}.md`. `--skill-alias MODULE=NAME` and
`--module-skill NAME` are effective naming inputs, never permission grants.

The raw composer is `role_instructions::render_role`; it reconstructs borrowed
candidate references rather than copying every Markdown body. Role identity and
permissions remain Rust data. Configuration uses the existing issuer replacement
semantics: equal tool-owned definitions preserve the live reference; changed
definitions revoke the earlier generation. Owned native publication uses the
Core writer, recovery and physical ownership checks. Repeated equal content is
Unchanged, unrelated files survive, and a foreign file at the owned name is
refused. Terminal output preserves prior role effects if a later role fails.

Codex preserves readonly instructions and the official shell-tool field; its
parent sandbox remains in effect. Cursor carries readonly intent with shell
enabled but cannot guarantee Scout without shell. Kiro
V3 uses its current Markdown schema; an unsupported role
returns an error with any prior publication effects. No host limit is relaxed to
make a role appear functional.

Runtime template tests (three new cases plus six existing role cases), the
Tracker native encoding case, two owned publisher cases and both Auth parent/
configuration cases passed on Linux for this delivery. The compiled App CLI fixtures also passed: two actual daemon/CLI scenarios
(1.72 seconds combined in the earlier run) and the isolated installed-Hub Rule
scenario (0.56 seconds after correcting optional-file capture). Auth and the selected App/Hub/Runtime targets passed strict Clippy. The final
App-only check after selective Hub capture also passed (exit 0). This is installation/configuration qualification: Task, Memory and
Knowledge MCP execution and retained role-credential dispatch remain separate
work under N4. A native file alone does not prove a role can execute those tools.

Implementation: [managed_roles.rs](../../crates/graphit-app/src/managed_roles.rs),
[compiled consumer tests](../../crates/graphit-app/tests/managed_roles.rs),
[raw composer](../../crates/graphit-runtime/src/role_instructions.rs) and
[owned publisher](../../crates/graphit-hub/src/native/role_publication.rs).

## Local Role MCP tools

The run-owned credential can use the composable [Role toolbox](rust_role_mcp.md)
with the same daemon issuer and retained project tuples. Task reads and fenced
own-task comments/progress, native Memory reads/writes/FTS, Knowledge heads,
selected page reads and [lexical search](rust_role_mcp.md#role-knowledge-lexical-search),
local AST Source, and captured module skills share the
actual Role admission and Original. The catalogue only exposes implemented
handlers allowed by the current grant; direct hidden calls remain denied.

This component does not launch a vendor or close N4. Task coordination, USER
Memory, imported Role Source and absent AST query/search, xref and Hub handlers
remain separate deliveries. Kiro V3 conformance follows the documented transport
and hook contracts without requiring installed vendor/model access. Local RoleNow is its
existing admission producer; attachment to MCP is a consumer. The default
Scout/Tracker/Scribe ceilings remain unchanged.

## Run-owned native transports

The active daemon exposes `/admin/roles/run` and `/roles/mcp`. The selected
`--agent` binding, or the daemon's captured default, chooses the existing CLI
adapter. There is no separate engine flag or daemon-default restriction. Model
selection is optional where the host supports its configured default; an explicit
empty model is invalid. No model name or provider account is fabricated.

`graphit roles run --agent opencode --role scout --prompt 'Locate the sources'`
selects the published Role before the first prompt. Codex, Claude, Cursor, Qwen,
Antigravity, Kiro V3, OpenCode 2 and Kimi use their maintained AI adapters and
Process/ACP owners. The selected native profile or documented instruction
input accompanies each launch. Kiro V3 uses session metadata and OpenCode 2 uses
its advertised ACP mode option with acknowledged selection. Kimi loads its
published Markdown profile through the official print-mode agent-file selector;
its web server ignores that global selection and is not used for Role runs.
The [Kimi print contract](rust_kimi_print.md) covers this native JSONL path; the
web adapter remains the separate authorized Ask/Conversation consumer. OpenCode profiles use
`mode: all`, enabling explicit primary-session selection as well as subagents.
See the official [OpenCode ACP configuration](https://opencode.ai/v2/docs/cli/acp/#config-options)
and [agent modes](https://opencode.ai/v2/docs/agents/#modes).

The current Runtime caller still authorizes ManageAccessGrants, Configure,
PublishedFilesystem and NativeLaunch. Setup reuses the raw candidate capture,
native publisher, definition registration and actual start_run/issue lifecycle.
A different selected binding does not inherit the daemon's authority: the issuer
checks the same selected binding, reference, live parent and retained scope.
Each call preserves Source, project/version, permission and Original checks.

The four private child environment fields are GRAPHIT_ROLE_BEARER,
GRAPHIT_ROLE_REFERENCE, GRAPHIT_ROLE_SELECTION and GRAPHIT_ROLE_BINDING.
The first pair authenticates only through the actual Role issuer. Selection and
binding are untrusted routing inputs until verified with that credential and
current authority. The proxy sends binding only on Role requests. Runtime and
Local callers do not acquire binding-selection authority from that header.
Installed-skill and Hub readers consume the verified effective binding rather
than the daemon's default. No bearer enters argv, native files or diagnostics.

The maintained Graphit MCP server configuration supplies the transport; private
Role environment selects the Role proxy without Runtime fallback. Explicit Role
mode without credentials, partial private credentials, invalid binding and
conflicting Root/Live Web selections fail before Root discovery/authentication.
Kiro's inline Role proxy remains secret-free. Claude and Kimi profile allowlists
include only the configured Graphit MCP namespace alongside their native tools;
Claude references the configured server by name rather than creating a second
transport. OpenCode's Role profile only adds
permission for the namespace derived from its actual MCP plan; it does not embed
a second MCP configuration or claim host permissions from the Graphit grant.
The native permission rules and the issuer's per-call authorization remain
separate. The Role permits OpenCode Code Mode (execute) because its configured
MCP server uses that mode; nested tools still enforce the namespace/native rules,
including shell/edit denial. This is not shell execution or PROJECT authority. Root profiles retain native
approval (ask) for Code Mode and their Graphit namespace; native tool grants and
the generic readonly guard remain unchanged. Tool naming follows the official [OpenCode MCP contract](https://opencode.ai/v2/docs/mcp-servers/#names).

Startup attaches captured configuration, Memory limits and live retained
projects before serving. Missing context is 503 after header validation. Each
HTTP job captures aligned project/version and module settings under its single
Original. Global/Project layers are resolved for that request; later selection or
configuration changes never rebase an already started job. Cancellation, EOF and
service stop cancel and join the same owned worker, including time queued under
the original deadline. No replacement executor, service or administrative
session is created to escape a failed request.

For Scout/Scribe with captured Memory enabled, the first prompt verifies the
run bearer and ReadMemory admission, then reads the mandatory current selection
to EOF. Disabled Memory omits the prefix without corpus I/O. Tracker omits it
using its actual verified Role identity. The transient prefix includes project
version, memory state and public record fields; no USER, actor or Task correlation
is invented. The complete escaped prefix and caller prompt must fit the captured
input bound before launch; it is never truncated. Recipient authority, templates
and Original are rechecked after composition and before/after execution.

Process and ACP cleanup use their real owners. Unknown effects and physical
remaining-process handles remain owned; a known joined failure preserves its
cause without falsely retaining a joined child. The issuer run is closed only
after native owners are joined. A fresh finite cleanup Original reuses the same
retained administrative session, never a new bootstrap or TTL. run_closed means
confirmed issuer closure; siblings remain independent. Shutdown attempts every
retained close and preserves the first observed real failure. Once owned work
and cleanup have been joined, a disconnected recipient only prevents delivery
of the response; it does not turn a completed job into a daemon failure. Actual
worker, cleanup and issuer-close errors still propagate. An error alone does
not prove a remaining resource. Detailed SDK causes stay private, while
public outcomes use finite failure classes and redact bearer/reference.

Implementation: [run owner](../../crates/graphit-app/src/managed_role_run.rs),
[drivers](../../crates/graphit-app/src/managed_role_run/native.rs),
[stdio proxy](../../crates/graphit-app/src/role_proxy.rs), and
[HTTP owner](../../crates/graphit-app/src/managed_roles/http.rs).
