---
title: Rust PROJECT hook context
type: reference
status: partial
updated: 2026-10-29
references:
  - target: {type: task, id: tsk-ddeb}
    relation: implemented_by
  - target: {type: task, id: tsk-01eb}
    relation: derived_from
  - target: {type: task, id: tsk-cd05}
    relation: implemented_by
  - target: {type: task, id: tsk-b9fb}
    relation: implements
  - target: {type: task, id: tsk-3180}
    relation: implements
  - target: {type: task, id: tsk-3125}
    relation: uses
  - target: {type: task, id: tsk-d6e4}
    relation: uses
  - target: {type: knowledge, id: Shared_native_host_document_transactions}
    relation: derived_from
  - target: {type: task, id: tsk-1c5c}
    relation: implements
  - target: {type: task, id: tsk-ec11}
    relation: implements
  - target: {type: task, id: tsk-dd19}
    relation: implements
  - target: {type: task, id: tsk-a9ae}
    relation: uses
  - target: {type: task, id: tsk-be6e}
    relation: relates_to
  - target: {type: task, id: tsk-a855}
    relation: uses
  - target: {type: task, id: tsk-1f4a}
    relation: uses
  - target: {type: task, id: tsk-be9c}
    relation: uses
  - target: {type: task, id: tsk-9d92}
    relation: supports
  - target: {type: task, id: tsk-7549}
    relation: implemented_by
  - target: {type: task, id: tsk-0529}
    relation: relates_to
  - target: {type: task, id: tsk-cab0}
    relation: relates_to
  - target: {type: task, id: tsk-16e4}
    relation: implemented_by
  - target: {type: task, id: tsk-633b}
    relation: uses_api
  - target: {type: task, id: tsk-c623}
    relation: uses_api
  - target: {type: task, id: tsk-0264}
    relation: uses_api
  - target: {type: task, id: tsk-b9b4}
    relation: uses_api
---

# Rust PROJECT hook context

App assembles readonly PROJECT context for explicit native host boundaries. It consumes captured caller inputs, actual PROJECT read admissions and the native Memory and Task stores, then returns the existing Runtime codec's complete `EncodedOutput`. The authenticated Source caller and `_session-hook` CLI transport are implemented under tsk-1c5c. PROJECT hook installation through the Rust CLI and shared native writer is implemented under tsk-3180. Kiro V3 support follows the official contract and controlled Graphit integrations under be6e. Tests and acceptance require no vendor executable, login, account, plan or remote model catalogue. Library and local HTTP/CLI fixtures retain their actual scope and do not establish vendor event execution or model perception.

## Native host invocation and lifecycle

The maintained PROJECT invocation is `graphit [--project-version <version>] _session-hook --engine <host> --event <semantic-event> --project-dir .`. The executable resolves through PATH; the host supplies the installation through the supported branded `GLOBAL_DIR` environment variable, or the ordinary native default. Generated commands contain no resolved executable, installation or checkout paths. The hosts are `codex`, `claude`, `cursor`, `opencode`, `kiro`, `qwen`, `kimi` and `antigravity`. The CLI family `kiro` selects only the current `NativeBinding::KiroV3`; it does not infer or retain an old engine. Semantic event names and supported combinations follow the [single Runtime codec matrix](agent_runtime_context.md). Invalid combinations fail before stdin body processing, daemon startup or context effects.

`project_hook_install::client::native_command` composes the same quoted invocation for hook and plugin publishers; the existing Kiro publisher consumes it too. The native input reader consumes one bounded JSON object under the caller's Original deadline/cancellation. It selects only `cwd`/`hook_event_name` where provided, Cursor `workspace_roots`, or Antigravity `workspacePaths`. It ignores forward-compatible unrelated fields and all host session/conversation IDs. A captured project must match the host workspace; ambiguous roots are an error. Project resolution uses the existing Graphit lock discovery, without Git. IDs from the host never become a Graphit Task session or confer authority.

Source authenticates the actual Runtime/Local caller and retains exact live project/version and physical captures. Context-capable events preserve the existing PROJECT, explicit Task and verified Native USER admissions and assembler. It validates the codec's actual delivery channel. For OpenCode the child stdout carries bounded text back to the plugin: the real context or compaction callback must apply that text to its own output. It is not a synthetic command-hook payload or proof that a callback executed.

Quiet-only events return their exact native empty/object/stop payload before Native profile resolution or PROJECT Memory, Task and USER corpus admissions. Supported `Stop`/`SessionEnd` with Hooks enabled additionally require the existing `Configure` permission before looking up the exact retained `ProjectRescanHandle`. Dispatch uses the existing supervisor wake/coalescing mechanism. The Source consumer dispatches to that actual owner on Linux, Windows and macOS, preserving Configure and the exact project/root/version selection. Native Windows/macOS execution remains a separate qualification from the Linux evidence below. Success means the refresh hint was accepted, not that indexing has finished; a missing or stopped owner is an error. It creates no new scheduler, polling, ledger, reminder or quota, and never completes or releases a Task/Session. Disabled Hooks produce no refresh effect.

## Explicit managed-role recipient

The default `_session-hook` recipient is `coordinator`. `--recipient role`
selects the genuine managed-run Role transport and accepts current Kiro V3
`SessionStart` or `UserPromptSubmit`. Profile names and environment values do
not select a Role or confer authority.
The CLI receives the already captured HubResourcePolicy from central routing,
including command/environment overrides, and borrows the existing Original
through connection, call, cleanup and output. It creates no synthetic Incoming
MCP request or replacement deadline. Negotiated transport cleanup is retained by
the proxy owner through caller abandonment and joined on shutdown; request
failure takes precedence over cleanup failure.

Missing, partial or invalid Role credentials and denied selection fail without
Root fallback. The Coordinator branch denies any Role bearer/reference/selection
presence before Root credential loading, including empty transport values.
The Role server verifies the exact credential reference and scope before any
context or Quiet response. Scout/Scribe output includes only complete mandatory
PROJECT Memory; Tracker, Hooks off or Memory off returns empty stdout after
checks. No USER, Task session or Coordinator mandates are composed on this path.
Native codec limits and single-text/write/flush requirements still apply.

Role profile commands are precomposed via SDKs before publication. See [managed-role profiles](managed_role_context.md) and [Role MCP](rust_role_mcp.md). Controlled Graphit integrations do not prove installed vendor events or model perception.

## Install PROJECT hooks

The active Rust daemon must retain the PROJECT and version and authenticate the CLI from the same private installation. Run from that PROJECT:

```text
graphit hooks install --binding kiro-v3 --profile coordinator.md
graphit --project-version main hooks install --binding claude
graphit coordinator install --binding opencode
graphit coordinator remove --binding opencode
graphit hooks remove --binding kiro-v3 --profile coordinator.md
```

The binding selects the current native schema explicitly. Profile is required only for Kiro: the installer updates an existing complete Coordinator Markdown file under .kiro/agents, preserving its body and foreign fields. It rejects Role profiles, managed-role provenance and Role MCP transport. Other hosts use the native command document; OpenCode uses actual plugin callbacks. Supported Stop/SessionEnd quiet hooks dispatch Configure-authorized rescan, without context reads. Codex SessionEnd follows its documented maximum native timeout of three seconds; the original request and authentication TTL remain unchanged. Coordinator installation publishes the bundled Markdown instructions, the supported native profile, MCP and hooks. Antigravity publishes documented workspace MCP and PreInvocation; custom profile MCP is not claimed or generated because the public CLI schema does not define the object fields.

Defaults are SessionStart and UserPromptSubmit for Codex, Claude, Qwen and Kiro; sessionStart for Cursor; PreInvocation for Antigravity; UserPromptSubmit for Kimi; context and compaction for OpenCode 2. OpenCode dispatches documented session.idle and session.deleted notifications to quiet Configure-authorized rescan. The former tool.execute.after callback is removed with OpenCode 1.

Kimi has one global hook configuration at $KIMI_CODE_HOME/config.toml and MCP at $KIMI_CODE_HOME/mcp.json, defaulting to ~/.kimi-code. CLI captures this directory once; Source retains the actual parent and installation owner. Recovery does not rediscover HOME. Global hook and MCP invocations omit fixed project/version arguments, resolve each native cwd through existing project locks, and retain per-project references to the single hook. A host session ID is never a Graphit task session or authority.

Source authenticates the real Runtime caller, selects the retained project tuple and authorizes Configure and PublishedFilesystem before publication. Native directory captures, scope, authority and the same Original are rechecked around effects. Existing Core/Hub writers own collision checks, pending transactions, cancellation and recovery. A later failure reports earlier effects without rollback; foreign values are preserved and identical foreign tuples are not adopted. No compatibility importer, second journal or data migration is provided.

The command builder uses the branded Graphit executable name and typed argv. PROJECT selectors are relative to the host workspace root; global hooks retain their cwd-based selection. Managed Role and Live launches prepend the captured native executable directory to the child PATH and supply the installation locator in memory, even when Source itself started with an explicit flag and no such environment variable. Those resolved values are never rendered into the native artifacts. Unix command hooks use shlex serialization; OpenCode receives program and argv directly. No credential, Memory/Task body or live context is persisted. Agent instructions remain Markdown templates. Unsupported native shell platforms, invalid paths and invalid schemas fail explicitly. Linux is the exercised platform; this result does not certify Windows or macOS execution.

Native input consumes its bounded payload through EOF under the original deadline and cancellation. On Unix an open FIFO uses the nonblocking Tokio pipe receiver, allowing expiry to terminate and join the CLI while the host retains its writer. Regular files use the file reader; an interactive terminal supplies no payload. The non-Unix stdin fallback has not been qualified for this open-pipe case.

Successful CLI output reports SDK hook effects and a static failure category. Public errors omit private paths, commands, credentials and context; actual causes remain linked internally. Limits apply to individual payloads and documents. Shutdown retains and joins the publication worker, preserves the first failure, and retires only the captured runtime/discovery owner. No process quotas, artificial accounting or vendor login/plan checks are added.

Implementation: [publisher](../../crates/graphit-app/src/project_hook_install.rs), [CLI client](../../crates/graphit-app/src/project_hook_install/client.rs), [shared HTTP owner](../../crates/graphit-app/src/managed_roles/http.rs), [foreground lifecycle](../../crates/graphit-app/src/foreground_source.rs) and [affected journeys](../../crates/graphit-app/tests/project_hook_install.rs). Historical pre-V3 qualification is recorded in tsk-3180; fixture ACP peers prove the actual Rust factory/transport boundary, not installed Kiro hook execution or model perception. These Linux fixtures do not establish native Windows/macOS execution. Vendor installation, authentication or model access is not a test or acceptance prerequisite.

## Root and managed-role boundary

The actual managed-role launcher emits transient `GRAPHIT_ROLE_BEARER`, `GRAPHIT_ROLE_REFERENCE` and `GRAPHIT_ROLE_SELECTION` inputs for its owned child. The default Coordinator branch of `_session-hook` refuses Root loading when any such transport input is present, before Source discovery or the Root credential reader. The new hook installation client also refuses that Root administrative path. Partial, malformed and non-UTF8 values cannot turn this denial into access. Presence is never treated as Role authentication, an issuer grant or a trusted Role name.

Coordinator calls without that transient factory context retain the existing authenticated Source journey. The Coordinator denial itself does not deliver Role context or forward parent ReadMemory to Tracker. The explicit `--recipient role` branch delivers mandatory PROJECT Memory through the authenticated Role transport described above. The four Scout/Tracker dual-engine cases under `tsk-b9fb` (Role11) remain historical component evidence; current V3 hooks require their affected checks. Installed Kiro's propagation of the factory environment, vendor events and model perception are not established by a controlled ACP peer. They are outside the current test and acceptance prerequisites; the Graphit adapter and its real authorization/transport/lifecycle remain covered by controlled integrations.

## Root human activity ingress

The Foreground shares its existing activity slot with Source before cloning or serving on Linux, Windows and macOS. A supported Coordinator UserPromptSubmit with Hooks enabled counts activity after authenticating the current Runtime/Local caller and rechecking Original, the physical capture and exact live project/root/version, including revalidation after acquiring the activity slot. Source emits only Instant::now() to the matching existing sender, before Memory reads or context rendering. A later bootstrap failure preserves this accepted timestamp; cancellation, denial or retirement before the pulse produces no pulse.

This effect needs no Graphit SessionId, Task lookup, ReadTask or Task creation. Native and Graphit session identities remain distinct; explicit Task recovery requires its own real producer/admission and is not inferred from MCP/ACP/run IDs or recency. Native IDs are observations, never grants. The human adapter does not need to equal the Codex proposer binding.

Role ingress, AgentSpawn, SessionStart, Task mutation, DATA records and filesystem batches do not count as this human event. Disabled Hooks or a missing/mismatched Dream target produces no pulse. Root retains the supervisor for filesystem producers and removes filesystem forwarding to this human channel. Dream receives no Task/session DTO, actor, human prompt, credential or extra permission.

The Foreground composition is now qualified in the local Linux fixture. The durable Human10 log records `foreground_dream_native_fold_delete_and_real_idle_activity` PASS1 in 25.62 s using the already compiled binary: actual Source listener, CLI entry and RuntimeCaller, Task disabled and Hooks enabled, filesystem without a human pulse, authenticated UserPromptSubmit reaching the existing channel, native fold/delete and a second idle iteration, then normal joined shutdown. The durable drain09 log separately records retry/activity-during-native PASS1 in 33.61 s (2m59 build and queue) and Knowledge queued-cancel/noop/watcher PASS1 in 7.60 s (0.47 s compile). Memory10 PASS1 in 20.78 s is preserved in tsk-9d92 IP103 with the same binary and no new compilation; Memory09 ran zero tests and is not a PASS. These composition proofs complement the unchanged 11 Source tests and overlapping CLI06; their counts remain separate. Earlier startup Request failures and the 18.03 s first observation remain historical evidence. The narrow cancellation classification preserves the same Original and only known pre-publication interruption, without expanding timeout, effects or SDK uncertainty. This closes the human consumer composition scope, while NativeSession/GraphitSession recovery, LC and native Windows/macOS execution remain separate obligations. Vendor installation, authentication and model access are outside the test prerequisites. Missing temporary logs never require replay solely to reproduce a log.

## Authenticated Source and CLI caller

For the default Coordinator recipient, the running Source service must belong to the same private installation as the CLI and retain the selected project and version. The CLI reads the existing Source discovery record and runtime credential; it does not start the service or create a credential. The explicit Role recipient uses the transient factory credentials and Role proxy described above rather than loading the Root credential.

```text
graphit _session-hook --engine kiro --event SessionStart
graphit _session-hook --engine kiro --event UserPromptSubmit
graphit _session-hook --engine kiro --event SessionStart --recipient role
graphit --project-version main _session-hook --engine kiro --event UserPromptSubmit
```

The engine and semantic event select the current explicit host contract; unsupported combinations fail. Kiro selects only current V3. The version must match a retained Source selection. `--project-version` is a global option. Generated PROJECT hooks use `--project-dir .`; the CLI resolves this relative selector through the current project boundary before validating the native host payload and constructing the absolute `NativePath` sent to Source. Global hooks omit the selector and use the host's discovered current project. Paths use `PathBuf` internally and canonical Unicode strings on the wire; non-Unicode input fails explicitly, without lossy conversion or native unit arrays. Path, version, event and current-directory inputs are selectors; they grant no access. Source selects among retained project tuples using literal path and version, without a Git lookup.

The internal local MCP tool `graphit_session_hook` accepts closed fields `engine`, `event`, `project_dir`, `project_version` and optional `task_context`. Unknown fields, invalid types and explicitly null optional values fail. It requires actual runtime or local HTTP credentials authenticated through the current `DaemonAuth`. Project capture, version and read admission use the daemon's installation and the same original request.

Source resolves the selected project's module switches for each hook request, using the actual Global and PROJECT ConfigStore layers under the same Original. Distribution, Environment and Command layers remain captured. The request retains its resolved ModuleSettings through rendering; later requests read then-current configuration. This consumer uses the same real Core producers on all supported hosts. The local qualification runs on Linux; macOS and Windows execution remain separate requirements. The CLI includes module keys in its captured environment, so `GRAPHIT_MODULES__HOOKS=false` and `GRAPHIT_MODULES__MEMORY=false` participate in the normal configuration precedence.

With Hooks disabled, successful execution returns exactly empty stdout without Memory or Task admission or I/O. With Memory disabled, context renders without Memory admission or I/O. With both enabled, Source obtains actual PROJECT ReadMemory admission and invokes the assembler below, including the complete mandatory selection through EOF. Disabled modules still retain authentication and physical project revalidation.

Aliases come from the maintained templates enabled in any retained project and existing Brand name APIs. The selected project's switches determine which mandates render. Names do not prove availability of the corresponding tools in the Main MCP catalog.

Source owns blocking operations in its existing `JoinSet`; cancellation, deadline and service stop prevent delivery, and shutdown joins owned work. Before returning the complete payload it rechecks caller authentication, physical project, any USER admission and Original. `EncodedOutput::into_bytes()` moves into `String::from_utf8` and becomes one MCP text block, without copying record bodies.

The CLI uses existing discovery and authenticated `connect_source` runtime headers. It cancels and joins the connection before delivery, checks Original, requires a successful single text block, then uses `write_all` and `flush`. It adds no JSON wrapper or newline. Write and flush failures propagate. No full response is written on a failed service result or invalid Original; an operating-system write failure can follow a partial stdout write and is reported honestly.

Absent discovery requires restoring the Source service for that installation. Invalid event requires a supported form above. Project/version selection errors require a retained tuple. Authentication or Memory permission errors require an authorized caller; changing selectors does not create permission. Cancellation, expiry, stop and malformed responses fail the command.

Implementation: [CLI routing](../../crates/graphit-app/src/cli.rs), [CLI transport](../../crates/graphit-app/src/session_hook_entry.rs), [Source caller](../../crates/graphit-app/src/source_server/project_hook.rs), [discovery](../../crates/graphit-app/src/source_entry.rs) and [authenticated connection](../../crates/graphit-app/src/ast_source.rs).

The historical Linux caller qualification passed five Source HTTP/entry tests (`source-tests-12`) and one central CLI parser/environment test (`cli-tests-03`), with no failures or ignored cases. They exercise supported V2/V3 pairs, real permissions, opposite project switches and alias coverage, exact CLI bytes, write/flush failures, canceled Original, HTTP cancellation/deadline and owned shutdown join. The earlier two-case run and lib/bin compilation remain historical evidence. Current scoped App lib/bin Clippy passed with warnings denied (`strict-04`, locked/offline dependencies). These local results do not qualify installed native Kiro execution or the full Main MCP catalog; those remain separate integration work.

## Caller contract

[The implementation](../../crates/graphit-app/src/project_hook_context.rs) exposes:

```rust
pub async fn render_project_hook(
    original: &RequestContext<'_>,
    input: ProjectHookInput<'_>,
    memory: Option<(&ProjectReadAdmission<'_>, MemoryStoreReadLimits)>,
    task: Option<(&ProjectReadAdmission<'_>, TaskStoreReadLimits)>,
) -> Result<EncodedOutput, HookContextError>
```

| Input field | Meaning |
|---|---|
| `task_context` | Optional borrowed `TaskContextSelection` with explicit Graphit session and task IDs |
| `binding`, `event` | Caller-selected `NativeBinding` and semantic `HookEvent` |
| `modules` | Borrowed captured `ModuleSettings` |
| `audience` | `MandateAudience` selects coordinator/delegate policy text; explicit Task context requires Coordinator |
| `variables` | Borrowed `InstructionVariable` values from captured template slots and Brand naming |
| `instruction_limits` | Caller-supplied `InstructionLimits` |
| `output_limits` | Caller-supplied `OutputLimits` |

Selecting a binding or audience grants no authority. The caller supplies the original request and, when Memory is enabled, its current recipient's actual PROJECT ReadMemory admission. Selected, enabled Task recovery additionally requires that recipient's actual ReadTask admission for the same captured project and version. The assembler creates no issuer, grant, role, run or Auth factory. See [Project record admission](project_record_admission.md).

The Root PROJECT renderer accepts the explicit host/event pairs in the [Runtime output matrix](agent_runtime_context.md). It uses the existing codec's channel and accepts_context metadata; an unsupported pair fails before corpus I/O. Context-capable events use the native JSON, text or plugin callback channel. Observation-only events return native quiet output before Native USER verification, Memory or Task admission. The managed Role transport remains restricted to current Kiro V3 SessionStart and UserPromptSubmit; that separate barrier does not restrict Root PROJECT hosts.

The original request's remaining lifetime is checked at entry. Hooks disabled and observation-only output return Quiet without corpus I/O or variable rendering. Source dispatches supported Stop/SessionEnd synchronization through the exact retained project supervisor only after Configure authorization; success means the rescan was accepted, not that indexing completed. Hooks enabled rejects the reserved variables `module_mandates`, `project_memory_state`, `project_memory_json`, `project_task_state`, `project_task_json`, `user_memory_state` and `user_memory_json`. Explicit Task selection validates the recipient, duplicate IDs and captured collection limit even when Hooks is disabled.

## PROJECT Memory

Memory disabled requires no admission and reads no corpus. Memory enabled without an admission returns `MissingMemoryAdmission`. The caller must supply `project_version` as Text matching `admission.project()?.version.version()`; missing, differently typed or mismatched values fail before the read. With Memory disabled, the version notice uses the caller's captured value.

`NativeMemoryStore::list` uses the same original snapshot, the actual admission and `mandatory: Some(true)`. The producer returns the complete current selection through EOF under its read limits. No pagination or truncation is introduced. See [Native Memory store](native_memory_store.md).

| State | Meaning |
|---|---|
| `disabled` | Captured module settings disable Memory; no corpus was read |
| `no_corpus` | No published PROJECT corpus for the admitted project version |
| `empty` | Published corpus has no current mandatory matches |
| `loaded` | Published corpus has one or more mandatory matches |

`empty` describes the mandatory selection, not necessarily an empty entire corpus. JSON contains only `id`, `revision`, `title`, `body`, `classification`, `mandatory` and `important`. Classification is `fact`, `skill`, `tension`, `decision`, `convention` or `correction`. Actors, timestamps, paths, tags and references are excluded.

The public PROJECT/Task renderer has no USER admission input and keeps its existing USER-not-loaded notice. Source composes USER through the private authenticated boundary below. Explicit Task recovery states that recovery is observational and no claims were acquired, renewed, released or changed. Record text is data, not a new tool grant or system instruction. The function performs no Task claim, heartbeat, release or delivery ledger mutation.

## Coordinator USER Memory

With Hooks and Memory enabled, the Source Coordinator path uses the current Runtime caller's real Native phase on Linux, Windows and macOS. The initial native_provider lookup may report legitimate absence (NoActive or valid Local mode); that result and an authenticated Local origin produce unavailable without USER admission or corpus I/O. Disabled gates perform no Native verification or USER read. A selected invalid/stale profile or a failure after a Native binding was captured fails the whole request. Concurrent NoActive or WrongMode during authenticate_native is a failure, never absence. There is no flag or profile snapshot selecting a USER principal.

Source retains the same caller, verifier, issuer, Original and native identity through capture, reads, rendering and final rechecks. It derives the literal verified issuer/subject, uses the installation's actual owner and CapturedPublicationScope::user, then obtains the real UserReadAdmission requiring ReadUser and ReadMemory. PROJECT and optional Task admissions come from that same caller and Original with their own scopes and pins. USER has no PROJECT version. Runtime roots are intentionally different; comparing paths or brand names cannot establish a shared issuer.

The private formatter input borrows UserMemoryListRead after Source has authorized and read it. The existing public render_project_hook API remains PROJECT/Task only; it accepts no independently supplied USER admission or principal DTO. The private formatter denies a USER selection for a delegate audience. The Role transport never supplies that input, and Role ceilings still deny ReadUser. This consumer grants neither Role::Dream access nor USER Fenced writes.

NativeUserMemoryStore::list uses the same Original, actual admission and mandatory: Some(true), with the captured Memory policy. It reads the complete current selection through EOF in the SDK's canonical order. It introduces no top-k, paging, recency selection, repair, index or mutation. JSON uses the same borrowed Memory projection as PROJECT: id, revision, title, body, classification, mandatory and important. USER and PROJECT data remain distinct.

| USER state | Meaning |
|---|---|
| disabled | Memory is disabled; no Native authentication or USER read |
| unavailable | Authenticated Local origin or legitimate initial Native absence; no USER read |
| no_corpus | Actual Native USER read found no published corpus |
| empty | Published current mandatory selection is empty |
| loaded | Complete current mandatory selection is present in USER JSON |

A published empty corpus and a corpus containing only nonmandatory records both yield empty. Neither unavailable nor a real read failure becomes no_corpus. Only loaded emits USER JSON, and record text remains data rather than authority. Every supported hook request reauthenticates and rereads current data; an update is visible on the next prompt without a new static profile or persistent context snapshot.

JsonWriter, InstructionLimits and OutputLimits bound the whole escaped UTF-8 payload. Oversize, incomplete/corrupt corpus, permission denial, invalid Native authentication, cancel, deadline or failed recheck fail the whole delivery without useful context prefix. Source keeps its existing worker ownership and joined shutdown; the CLI keeps its existing cancellation, cleanup, single-text and write/flush rules. An already accepted human activity pulse remains accepted if a later USER bootstrap fails. Auth/Core/Records rechecks do not claim Auth+storage linearization or a global generation across USER/PROJECT/Task.

Implementation: [Source](../../crates/graphit-app/src/source_server/project_hook.rs), [borrowed USER projection](../../crates/graphit-app/src/project_hook_context/user.rs), [focused Source tests](../../crates/graphit-app/src/source_server/project_hook/tests/user_context.rs), [USER admission](user_record_admission.md) and [native USER store](native_user_memory_store.md). Linux controlled Broker/JWT/HTTP/CLI fixtures do not establish installed vendor execution or native macOS/Windows behavior.

NativeUserMemoryStore/store and the Source verifier cache are available to this consumer on Linux, Windows and macOS. The Source hook uses the actual Native authentication, verified USER identity, captured scope and admitted store read on each supported system. Disabled gates, authenticated Local and legitimate initial absence retain their existing states; selected invalid Native bindings and store failures remain errors. No unsupported-host fallback or fabricated USER absence replaces an available producer. The recorded functional qualification remains Linux; Windows/macOS compiler checks and native execution retain their separate platform scope under tsk-0529 and tsk-cab0.

## Explicit Coordinator Task recovery

A caller that already knows the relevant Graphit IDs may supply them transiently:

```text
graphit _session-hook --engine kiro --event UserPromptSubmit --task-session 00000000000000000000000001 --task-id 00000000000000000000000002
```

The example IDs are synthetic valid identifiers, not records to create. Repeat `--task-id` to select additional tasks in caller order; it requires `--task-session`. Omitting task IDs recovers only the selected session. The Source form is:

```json
{"engine":"kiro","event":"UserPromptSubmit","task_context":{"session_id":"00000000000000000000000001","task_ids":["00000000000000000000000002"]}}
```

The nested object is closed and non-null, uses the SDK's canonical nonzero ULID Deserialize, defaults omitted `task_ids` to an empty array and rejects duplicates or more entries than the captured `ContentLimits.max_collection_entries`. Unknown fields, nulls, invalid types and old `ses-/tsk-` aliases fail. IDs select data; they grant no access. The CLI rejects Task selection with `--recipient role` before discovery or Root credential loading. Static profiles receive no session IDs, task IDs or live snapshot.

No selection means no Task admission or read and no Task section. With selection and Task disabled, the context reports `PROJECT Task state: disabled` and reads no Task store. With Task enabled, Source obtains real `admit_authenticated_read(..., ProjectReadModule::Task)` under the same authenticated Runtime/Local caller, Original, installation, live PROJECT and version. Its Task read limits come from the existing captured Task policy. Enabling a module supplies no grant.

The [Task child](../../crates/graphit-app/src/project_hook_context/task.rs) calls `read_session(SessionGetInput::current(id))` and `read_task(TaskGetInput::current(id))` for exactly the explicit IDs. It emits `SelectedSessionRead` and ordered `TaskGetRead` values as `{session,tasks}`. The complete current demand, strategy, specifications, checks and checkpoints remain present, together with creation, the validated last transition and producer start cursors. History and Task comments are null (unloaded), with no top-level event collection. Selecting record IDs chooses current work context, not a complete audit. Relevant history/comments are recovered through the existing explicit Get pages; complete audit/export traverses the selected generation through EOF. Public read models contain no claim token; write receipts are never serialized. When Memory is also enabled, the consumer compares both admitted PROJECT owners, physical owner roots, runtime roots, brand keys, version literals and canonical SDK lance_branch() selections before reading; different PROJECTs with equal version text fail as ProjectScopeMismatch. This rejects mixed inputs and grants no authority. Auth already validates each version scope against that admission’s live owner root, so the consumer does not repeat that comparison or compare full VersionContext provenance. Equal version text can select different physical datasets when captured defaults differ; the consumer compares the existing SDK lance_branch() result, without reimplementing that mapping or requiring equal defaults. Selected readers keep their own real pins and validate creation and the last transition; this does not add cross-getter or Memory/Task atomicity.

A missing session, missing task or task belonging to a different session fails the whole request. SDK errors retain their causes. Selection does not discover work by recency, heads, MCP/ACP session or managed run ID, and reading changes no record or claim. The existing human pulse precedes bootstrap: a later Task denial or read failure does not erase an already accepted timestamp.

Both SessionStart and UserPromptSubmit read current selected records. The [official trigger list](https://kiro.dev/docs/hooks/types/) lists no Compact event. Recovery after [compaction](https://kiro.dev/docs/compaction/) therefore uses the next UserPromptSubmit and the caller's same explicit Graphit selection. Repeated-prompt tests update a real Task checkpoint between requests; they do not claim vendor compaction execution or model perception.

This opt-in Task path creates no automatic host-to-Graphit session association and forwards neither Task nor USER context to managed Role hooks. Mandatory USER context for the Coordinator itself is delivered under `tsk-16e4`; Native Coordinator USER Dream is delivered under `tsk-ddeb`. Their reads and admissions remain separate: explicit Task selection grants no USER access or Fenced write authority. This opt-in consumer does not complete its parent, N4, the migration umbrella or the coordination session.

## Compiled policies and slots

Enabled Task, Memory, AST, Hub and Knowledge mandates are rendered separately and wrapped with their catalog tags, in that order. Audience changes only Task text. The API uses compiled `PolicyTemplateSet` candidates; it receives no overrides and performs no template or configuration discovery. `PolicyPrompt::ProjectHookContext` includes the maintained Header, project-version notice, Router/preamble and CoreInvariant plus the new PROJECT notice. See [Rust policy instructions](../architecture/rust-policy-instructions.md).

The caller supplies actual metadata `brand_display`, `brand_marker`, `project_version`; router roles `role_name_scout`, `role_name_tracker`, `role_name_scribe`; and each enabled `skill_name_task/memory/ast/hub/knowledge`. Effective MCP slots are:

| Mandate | MCP variable names |
|---|---|
| Memory | `mcp_tool_memory_search/source/update/insert/query` |
| Task coordinator | `mcp_tool_task_session_get/list/create/claim`, `mcp_tool_task_get/query/create` |
| Task delegate | `mcp_tool_task_get/search/progress/comment_add` |
| AST | `mcp_tool_ast_source/search/schema/query/fts_schema/fts_query` |
| Hub | `mcp_tool_cluster_projects`, `mcp_tool_hub_projects/search/show`, `mcp_tool_config_get` |
| Knowledge | `mcp_tool_wiki_source`, `mcp_tool_knowledge_search/query` |

Slash-separated suffixes denote individual variables, not literal slot names. Shared query references use `mcp_tool_references_query` and `mcp_tool_references_reconcile` when included. Aliases are derived from maintained template slots and Brand name APIs; the assembler does not infer brand prefixes or create grants. Missing required values remain renderer errors. Internal `module_mandates` is TextList; the state and JSON slots are Text. Inserted JSON, record bodies and rendered mandates are never evaluated as templates again.

## Bounds, authority and failure

Every stage retains the original request. Memory uses `MemoryStoreReadLimits`, which charge all current input rows including those filtered out. The complete mandatory selection through EOF and final actual Auth recheck remain unchanged.

Task uses the existing `TaskStoreReadLimits` and selected current read models, with history/comments unloaded. The bounded writer serializes these borrowed details without copying their bodies. JSON serialization borrows the SDK Memory slice through `SerializeSeq`, without an intermediate vector of row views or copied bodies. A private writer checks Original and checked length arithmetic before reserving or appending each serialized chunk. Conversion to String moves the UTF-8 buffer. The JSON cap is the minimum of instruction `max_variable_bytes`, instruction `max_output_bytes`, output `max_context_bytes` and output `max_output_bytes`; ties retain this order.

Overflow retains `InstructionError::LimitExceeded(VariableBytes/OutputBytes)` or `OutputError::LimitExceeded` according to the selected cap. Failed reservation retains `InstructionError::Allocation`, and cancellation/expiry retains the original request error. The first typed failure survives Serde's I/O transport signal. Other serialization failures retain the actual JSON cause. `write_all` never retries persistent cancellation, including empty chunks; Original is checked again after serialization.

This guard bounds escaped JSON bytes, not the remaining aggregate renderer budget. The renderer still charges names, other values and template overhead afterward. Each mandate and the final template use the supplied `InstructionLimits`, including template/value/output byte bounds, fuel and recursion. `OutputContext` retains Original and two positive per-payload byte caps; readonly `encode(..., &context)` serializes once through its bounded byte-vector writer, without message quota or aggregate counters. Repeated payloads apply those caps independently without replacing Original. These logical limits do not guarantee a hard RSS cap or preemptive cancellation.

After encoding, every real Memory and selected Task admission is rechecked, then the original request's remaining lifetime is checked again. Either failure prevents the payload from being returned. The caller consumes `EncodedOutput` through the actual official channel; [codec output](agent_runtime_context.md) establishes neither model perception nor BootstrapDelivered, a receipt or lifecycle completion.

`HookContextError` preserves `Error::source()` for Request (I/O lifetime/cancellation), Auth, boxed MemoryStoreError and TaskStoreError, InstructionError, OutputError and JSON serialization causes. Local MissingMemoryAdmission, MissingTaskAdmission, InvalidTaskSelection, MissingTaskSession, MissingTask, TaskSessionMismatch, TaskRecipientDenied, ReservedVariable, UnsupportedEvent, ProjectScopeMismatch and ProjectVersionMismatch have no nested cause. Wrapper Debug/Display report only category; they never format record bodies, variable values, payloads or SDK diagnostics. Consumers exposing nested diagnostics must apply their own appropriate handling. All failures return an error without a partial encoded payload.

## Verification boundary

Prior receipts in this section retain their original Source/API scope. Changed
current V3 hooks and profiles require their fresh affected checks; none of the
older component receipts establishes current installed-host delivery.

The installer is registered in the existing Xtask test selector catalog and pre-push workspace coverage. Its portable project command is:

```text
cargo xtask test graphit-app/project_hook_install
```

The existing runner executes the `graphit-app` integration target `project_hook_install` with locked dependencies and propagates a failing test status. Its Linux run passed all four cases with no ignored tests. The affected Source signal and late-startup cleanup cases also passed, preserving exit 130 and functional assertions. Their readonly fixture consumes SDK-published branch rows, creates missing temporary AST ancestors through the private filesystem SDK and explicitly disables the AST producer to prevent a rescan from replacing that input. These cases qualify Source selection and lifecycle rather than AST publication. The direct unserved-owner shutdown case passed as well. These historical pre-V3 qualification results and timings belong to tsk-3180.

The tsk-3180 Linux installer suite exercises the actual Auth/Core/Hub APIs in temporary PROJECTs and the compiled Rust CLI against its actual foreground daemon. Its ACP peer consumes the real factory-issued context and the real run-owned MCP service; it is a transport fixture. The installed `kiro-cli 2.28.0 agent validate --path` attempt used an isolated HOME and current directory and returned exit 1, `You are not logged in`. This establishes only the isolated fixture's authentication state and no successful native configuration validation; it does not establish the normal account's login state. No personal profile, credential, trust or settings were altered. That failed vendor validation remains historical, without an installed-event, environment-forwarding or model-perception prerequisite for current acceptance.

`crates/graphit-app/tests/project_hook_context.rs` uses a physical Agent captured and revalidated through Core, parsed through Hub, registered by authenticated fixture administration with the observed content digest, and admitted through the real Auth SDK. Native Memory operations use a temporary isolated installation and Lance corpus. Renderer alias values are text fixtures, not authority or host-installation proof.

Focused commands use Cargo and rustfmt on PATH:

The package-specific `graphit-app` test profile sets `debug = 0`,
`incremental = false` and `strip = "debuginfo"` to reduce artifact growth.
Dependency profiles, assertions and overflow checks retain their existing
settings; use the same Cargo commands.

```sh
cargo xtask cargo test --locked --offline -j4 -p graphit-app --test project_hook_context
cargo test --locked --offline -j4 -p graphit-runtime --test policy_instructions
cargo xtask cargo clippy --locked --offline -j4 -p graphit-app --lib --test project_hook_context --no-deps -- -D warnings
rustfmt --edition 2024 --check crates/graphit-app/src/project_hook_context.rs crates/graphit-app/tests/project_hook_context.rs crates/graphit-runtime/src/policy_instructions.rs crates/graphit-runtime/tests/policy_instructions.rs
```

The focused Linux run passed four App tests and eight Runtime policy tests without failures or ignored cases. App coverage includes all three supported plaintext pairs, full mandatory selection through one-row batches, distinct PROJECT/version corpora, both audiences, actual SDK revocation/expiry and causal bounds/request failures. Exact commands and logs are recorded in ec11 checks. Current Kiro V3 acceptance follows the documented adapter and controlled integrations, without a vendor executable or account prerequisite; these results do not certify Windows/macOS execution or model perception.

The corrective `tsk-dd19` unit suite passed three tests: exact escaped UTF-8 output and four limiting caps, first-cause/prefix preservation, and Original cancellation/expiry before append. Its actual Auth/Lance App integration suite passed five tests without failures or ignored cases, including admitted escaped-body JSON overflow without an encoded output. These three unit and five integration tests also passed after the a9ae Output ABI change and Root caller integration. Current strict App checks passed in the covering lib/all-test target command; ec11's earlier result is historical qualification, not proof of this correction. The focused unit command is:

```sh
cargo xtask cargo test --locked --offline -j4 -p graphit-app --lib project_hook_context::tests::
```

The dd19 runs precede a9ae Runtime producer changes. Current-source requalification passed one Runtime writer test, nine functional Output tests, three App writer tests and five actual Auth/Lance App integration tests. Runtime strict clippy passed; current App strict checks passed in the covering lib/all-test target pass; The producer Task records now retain their qualified results. These outcomes do not certify installed Kiro journeys.

## Authorial Coordinator publication

The product now authors `graphit/coordinator` version `1.0.0` in
[AGENT.md](../../templates/agent-instructions/coordinator/AGENT.md) and its
[canonical prompt](../../templates/agent-instructions/coordinator/prompt.md).
It is a Root agent with the effective Source MCP catalog, distinct from the
three managed roles. Its static policy contains no live Memory, Task, USER,
credential or standing-context snapshot. The strict instruction renderer
supplies captured names and the server namespace; the Hub descriptor parser
and native renderer produce the target format. Root profiles omit model pins
and automatic approval rules.

The new caller is `graphit coordinator install --binding <binding> --source-dir <artifact-directory>`;
Kiro V3 is current; there is no Coordinator preview selector. It requires the
active authenticated Rust daemon and a retained, trusted PROJECT. The consumer
captures the authorial directory and verifies its two files against the
compiled product source before constructing actual Core source/overlay/Derived
provenance. `ScopedCoordinator::publish_derived` owns the complete native
profile. Both current hooks are composed with actual `KiroHookPlan` values and
`KiroProfileRuntime` before rendering and whole-profile publication, including
the complete fingerprint. A shared-field transaction cannot edit a whole-file
ArtifactEntry. The existing shared SDK separately publishes the PROJECT MCP
document; no standalone hook is published afterward. This path uses Configure and PublishedFilesystem authority, with the
actual PROJECT/Original rechecks, causal errors and known partial effects. A confirmed
V3 profile reports both inline hooks immediately, including when later
MCP publication or cleanup fails. A pending SDK publication without confirmation
does not imply a profile or hook effect.

Historical temporary PROJECT publisher receipts are recorded in `tsk-be6e`.
Changed V3 rendering and hooks require fresh component and installed-host checks. Its generated MCP
command is the branded executable on PATH with `mcp --stdio`, without a fixed
checkout or version. The host supplies the installation environment; Codex
publishes its supported `env_vars` allowlist for the branded `GLOBAL_DIR` name.
These locators supply no permission. Native Kiro V3 SessionStart and UserPromptSubmit still require their
separate installed-host qualification. Source component tests and controlled
ACP peers do not prove those events or model perception.

The new publisher integration target is registered in the existing Xtask
selector catalog and pre-push workspace coverage. Its project command is:

```text
cargo xtask test graphit-app/project_coordinator_install
cargo xtask test graphit-app/mcp_source
```

The prior registration compiled and the historical publisher target passed all four
cases without ignored tests through the existing runner. The cases use actual Auth/Core/Hub publication, compiled CLI,
Source MCP and Role-issued Memory in temporary PROJECTs. They cover both
bindings, published Root argv, source guards, foreign MCP preservation and
known profile effects when a later shared-document collision fails. The
controlled ACP peer supplies transport only and does not certify Kiro.
Native SIGTERM retains Interrupted/exit 130 after owned joins, consistent
with the CLI and existing Source executable tests. An HTTP recipient closing
before publication effects cancels and joins that request while Source keeps
serving authenticated callers. A later foreground SIGTERM still requires
exactly exit 130 after drain and credential withdrawal. A real producer or
cleanup failure retains its own failing cause.

The installed `kiro-cli 2.28.0` validation of a profile published in a private
PROJECT, using normal HOME, returned exit 1, `You are not logged in`. A V2
chat ingress attempted an automatic login flow and was interrupted with
exit 130; no completed login was observed. No new login, credential copying,
trust change, model override or personal configuration edit is authorized.
This is the observed authentication limitation of that normal CLI execution;
it does not infer state for other hosts/accounts. AgentSpawn, UserPromptSubmit
and model perception remain necessary and unqualified.

## Current installed Kiro V3 qualification

Current acceptance follows the [official V3 ACP contract](https://kiro.dev/docs/cli/v3/acp-migration/), [hook schema](https://kiro.dev/docs/hooks/), [triggers](https://kiro.dev/docs/hooks/types/) and [command actions](https://kiro.dev/docs/hooks/actions/), with functional controlled integrations of the Graphit producers. No agent test requires a vendor executable, login, account, plan, remote catalogue or model response. The installed Kiro journey and its exclusive tracing/probe helpers have been removed from the maintained target; it is not retained as an ignored acceptance condition. Controlled ACP peers, the compiled Graphit CLI/MCP/HTTP journeys, authorization, cancellation and owned joins remain covered.

## Historical installed Kiro V2 qualification

Kiro V2 is superseded and has no current adapter or test path. Use the maintained V3 profile and hook contracts.

## Native withdrawal

`hooks remove --binding <binding>` removes owned native tuples and MCP fields while preserving foreign content. Kiro external profiles still require --profile. A complete profile owned by Core/Hub cannot be edited through this shared-field path: use `coordinator remove --binding <binding>`, which uses the original Hub installation target and artifact identity to remove the profile and its inline hooks, then the native hook/plugin and MCP ownership. Operation is explicit in the closed HTTP request; there is no old request compatibility path.

Kimi removes only the selected ProjectId reference. SDK command_hook_has_references retains global MCP while another project uses any of the event tuples; after the last reference, SDK writers remove only owned hooks/MCP leaves and preserve unrelated TOML/JSON fields. The captured global parent remains the recovery target. Removal does not discover a new HOME, revoke project grants, complete tasks or remove project data. Failures report real partial effects and retain their causal category.

The earlier automatic full-detail hook equality check is superseded by the current-state recovery contract. Revised checks preserve caller order, explicit Session membership, missing/denied/cancelled failures, complete current specification/checkpoint content, no claim tokens or ownership mutation, and actual producer cursors for explicit pages. Historical full-output qualification does not qualify this changed boundary; affected native MCP/CLI hook tests must pass on its new emission.
