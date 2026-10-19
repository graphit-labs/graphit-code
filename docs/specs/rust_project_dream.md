---
title: Rust PROJECT Dream caller
type: specification
status: partial
updated: 2026-10-16
audience: integrators
references:
  - target: {type: task, id: tsk-ddeb}
    relation: implements
  - target: {type: task, id: tsk-04be}
    relation: uses
  - target: {type: task, id: tsk-633b}
    relation: uses
  - target: {type: task, id: tsk-c62d}
    relation: uses
  - target: {type: task, id: tsk-cd05}
    relation: implemented_by
  - target: {type: task, id: tsk-9d92}
    relation: derived_from
  - target: {type: task, id: tsk-6aa2}
    relation: implemented_by
  - target: {type: task, id: tsk-f2ba}
    relation: derived_from
  - target: {type: task, id: tsk-714d}
    relation: uses
  - target: {type: task, id: tsk-74e7}
    relation: uses
  - target: {type: task, id: tsk-e7aa}
    relation: uses
---

# Rust PROJECT Dream caller

The App Foreground contains an opt-in PROJECT Dream caller, implemented in
`crates/graphit-app/src/foreground_dream.rs` and composed by
`ForegroundSource::start_and_publish`. It connects the existing project activity
signal to a serial native Codex proposal, complete recipe validation and
automatic Records application. It uses the existing Auth, Process, Runtime
instructions and Records SDKs rather than a separate executor or watcher.

Status remains partial for the broader integration. Under `tsk-6aa2`, eight
unique Linux caller/SDK tests passed, followed by strict App `--lib` Clippy.
This qualifies the exercised Foreground controller/SDK paths, not the compiled
Main CLI, commercial Codex credentials/live inference, all MCP/host policy
effects, Kiro authentication, Windows or macOS. The delivered producer leaf is tsk-6aa2 COM35; human ingress, parent/session
deliveries and installed-host qualification remain separate.

The [consolidation instructions](../architecture/rust-dream-instructions.md)
describe the pure five-slot prompt. [Rust Dream records](rust_dream_records.md)
defines the paired PROJECT admission and automatic operation boundary.
Neither the prompt nor model text grants authority or proves a Memory effect.

A separate Native Coordinator USER lane is implemented under `tsk-ddeb` in
`foreground_dream/user.rs`. Its integrated Linux functional qualification passed; affected strict qualification remains in the central queue.
The PROJECT evidence and counts on this page do not qualify that lane; its
[separate contract](#coordinator-user-lane) is below.

## Startup and opt-in configuration

The startup, installed Agent and PROJECT prompt sections below describe the
PROJECT lane. The USER lane shares model/program/timer settings but does not
select an installed Agent or PROJECT.

`ForegroundSource::start_and_publish` receives the captured installation,
project bootstrap inputs/current selection, `ConfigSnapshot`, native binding,
environment and startup original. Dream is initialized during Foreground
construction and its Process host/job is started before the shared
`attach_jobs` composition. The existing `ProjectSupervisors` retains filesystem producer supervision,
while Source receives the same human activity slot. Dream does not create a
second watcher or model server.

With `dream.enabled` absent or false, the Dream constructor returns without
selecting an Agent, registering a Dream definition, adding Dream Fenced
permission or installing a Dream ProcessHost. Ordinary Foreground behavior is
preserved. Enabling Dream requires the Codex binding and the async startup
with captured environment. The public synchronous `ForegroundSource::start`
rejects enabled Dream with `Unsupported`; it cannot supply that async startup.
Enabled native Foreground Dream currently requires Linux.

| Config key | Type/requirement when enabled | Default |
| --- | --- | --- |
| `dream.enabled` | boolean opt-in | `false` |
| `dream.agent` | PROJECT only: required nonblank installed Hub Agent ID | none |
| `dream.agent_version` | PROJECT only: optional concrete Hub version, parsed when present | omitted; registry lookup by Agent ID |
| `dream.model` | required nonblank model string, supplied literally | none |
| `dream.program` | required nonblank program, absolute or one PATH component | none |
| `dream.idle_timeout` | positive `u64` seconds, checked against Instant overflow | 7200 |
| `dream.max_duration` | positive `u64` seconds, checked against Instant overflow | 600 |
| `dream.retries` | `u32`; zero permits the initial attempt | 2 |
| `dream.backoff` | positive `u64` seconds, checked against Instant overflow | 30 |

Required strings also reject NUL. The program is resolved once at Dream start
from the captured PATH or retained as an explicit absolute path; relative
paths with multiple components are rejected. The caller does not rediscover
cwd, Git version or environment during each attempt. These timers/retries are
actual caller settings, not a durable ledger, CPU quota or hard RSS guarantee.

## Installed Agent and actual authority

The selected PROJECT paths, ID and version come from the existing Foreground
bootstrap. Runtime Auth authorizes the Hub read; the native registry selects
the requested Agent/version and exactly one installed target that includes
`NativeBinding::Codex`. Missing, ambiguous or incompatible selections fail.

The installed `SourceRef` is data. The caller captures its physical `AGENT.md`
through the existing template reader, checks the allowed project/installation
root, parses the native Agent descriptor, computes the authored SHA-256 and
revalidates the capture. It registers one `RoleDefinition` with `Role::Dream`,
Codex binding and only ReadMemory/WriteFencedMemory permissions. A retained
definition reference is not admission. The authored content is re-captured and
checked against that definition hash before use.

Foreground adds the Runtime administrative Fenced permission only for enabled
Dream; it does not widen local/incoming/manual role permissions. Each attempt
uses actual Runtime authentication, admin parent, RunHandle, issued/verified
role credential and PROJECT ReadMemory/FencedMemory admissions. Both
admissions use the same finite Original, project/version/root and live scope.
No boolean, DTO, role label, prompt, hash or serialized credential substitutes
for those admissions.

## Activity, attempts and prompt input

`ForegroundDream::activity_sender` supplies a sender only for the selected
project ID, version and root. Foreground shares that existing slot with the
Source Coordinator hook. On Linux, a supported configured UserPromptSubmit
with Hooks enabled and the current authenticated Runtime/Local caller emits
`Instant::now()` through `send_replace` after Original, physical capture and
live tuple checks, before bootstrap reads. It is receipt-time activity. The
accepted timestamp remains if later context rendering fails; cancellation or
retirement before the pulse produces no effect.

This signal is independent of Graphit Task/session creation, lookup and
ReadTask. Dream receives only the timestamp, without Task data, actor,
human prompt or extra grants. Native session metadata is observational and
its correlation to a Graphit SessionId remains a separate Runtime/Task
contract. The supported human adapter need not equal the Codex proposer.
Root keeps filesystem supervision for producer work and removes its forwarding
to the human channel; filesystem batches, rescan hints, Task mutations and
Role/DATA events do not invent a human timestamp. The older eight-case
qualification below exercised filesystem activity before this consumer;
its counts are historical, and current Source/composition qualification is
recorded separately under tsk-cd05/tsk-9d92.

The current Foreground Human10 composition case
`foreground_dream_native_fold_delete_and_real_idle_activity` passed in Linux
with the existing native Codex wire fixture, actual Source listener and CLI
hook transport. Task is disabled and Hooks enabled. A filesystem event leaves
the actual human channel unchanged; authenticated UserPromptSubmit advances it
and triggers a second native iteration after idle. Runtime was 25.62 seconds,
using the already compiled binary. Retry/activity-during-native also passed,
including success/new-activity and exhausted-old-attempt rearm. The detailed
composition provenance and earlier failures are recorded below. This is not
commercial-host authentication or full migration qualification.

The job waits until the selected activity instant plus `idle_timeout`. It runs
serially, with an initial attempt and at most `retries` additional iterations,
using captured backoff. A success or noop consumes only its activity generation;
activity received during that operation remains eligible for its own idle wait.
An exhausted pre-effect failure rearms activity received during its attempt only
after confirmed native cleanup and SDK closure of the old Run. A partial
Application keeps its original Run/Actor; activity does not reset that recovery.
Cancellation, unconfirmed cleanup or exhausted retries without new eligible activity returns the actual
job terminal/owner rather than pretending completion.

Each attempt constructs a finite request with the same Core cancellation and
a checked `now + max_duration` deadline. It obtains actual PROJECT scope and
paired admissions, then reads the complete admitted Memory catalogue to EOF.
NoCorpus and published-empty results cause no model request; neither becomes a
fabricated corpus.

The prompt projects only `id`, `revision`, `title`, `body`, `classification`,
`mandatory` and `important`. It does not include authors, paths, USER, Task or
other-domain data. The canonical renderer receives exactly five Text slots:

| Slot | Value |
| --- | --- |
| `dream_session_id` | Fresh attempt identity data, not a Task/session claim |
| `dream_agent` | Captured installed Agent identity |
| `project_id` | Actual selected PROJECT ID |
| `project_version` | Actual captured version |
| `memories_json` | Complete minimized admitted corpus projection |

Rendering uses existing instruction limits and the attempt original. Corpus,
prompt and answer owners are temporary; their contents are not logs or
receipts. Bounds and cooperative checks do not promise preemption or RSS limits.

## Native request and scoped readonly controls

After Agent/source validation and real NativeLaunch authorization, the caller
creates an empty owned temporary cwd outside the selected PROJECT. It copies
no CODEX_HOME, credentials, trust file or personal configuration into it. The
Process Request uses the captured program/environment, that cwd, finite
Zeroizing stdin and the existing Process byte/grace/cleanup limits.

The pinned wire decoder limits each frame to 8 MiB and depth 128. The caller combines these SDK bounds with the captured content policy; aggregate response and Process byte bounds remain explicit.

The actual argument recipe is `exec --json --sandbox read-only --ephemeral
--ignore-user-config --ignore-rules --model <captured model>
--skip-git-repo-check`, with overrides `features.apps=false` and
`features.shell_tool=false`, followed by `-` for stdin. Arguments are literal
Process values, not a shell command assembled from the prompt.

The caller captures only the selected PROJECT `.codex/config.toml` through
the native document reader. It derives actual `mcp_servers` names, rejects
invalid names and emits a bounded inline TOML override that quotes each literal
name and sets its `enabled=false`. An absent/empty focal table emits no override.
It does not infer names from the Agent/Brand, scan globals/personal HOME or
claim that missing project configuration means all MCP servers are absent.

These controls have the scoped policy boundary of `tsk-714d`. They do not prove
all MCP/cloud/host effects are disabled or that inherited managed/system policy
is absent. Readonly rendering alone is not enforcement. The wire pin is Codex
0.160.1, commit `d27764b82f7118f674371e6d6e76271d9d606edb`; it is not an
attestation of the resolved executable's version or commercial authentication.

## Proposal validation and automatic effects

`graphit_daemon::dream::propose` reuses `run_codex` and consuming receipt parts.
The caller accepts an answer only after full completion/END, actual pipe EOF,
successful Process status, complete cleanup and no retained RemainingProcess.
Partial output, evidence unavailable, native error or a failed process does not
become an answer or authorize Memory application. Requested model remains
captured caller evidence, not a model identity reported by the wire.

The entire JSON recipe is prepared before any Memory effect: closed action
shapes, input bounds, actual corpus IDs/revisions, member bounds and no duplicate
IDs. Runtime generates operation keys; the model cannot supply grant, key or
receipt. `actions: []` is an explicit validated noop. Text is a decision proposal,
not toolproof; zero MCP events is not failure for this recipe.

`Application` owns the prepared sequence, cursor and actual receipts. It first
reobserves the same input/key, then calls automatic fold/delete only when no
prior effect is found. It advances after an actual receipt or domain refusal.
Automatic bare delete protects important/mandatory records and the last active
record. Earlier receipts survive a later failure; the sequence is not a global
transaction and no blind uncertain replay occurs.

For retained partial/uncertain application, a retry reissues credentials and
admissions under a fresh finite Original while keeping the same actual
session, RunHandle, role reference, scope and Actor as long as they remain
valid. It uses the retained recipe/keys and reobservation; it does not create a
new Run/Actor to recover an old CAS. Native-only failures without a prepared
application close their run before a fresh proposal attempt is admitted.

## Cancel, joins and Auth cleanup

The Core cancellation reaches the real job/Process operation. Job JoinHandles
remain borrowed through await; shutdown does not remove or abort them before
observing their actual terminal. RemainingProcess is retained and repaired;
pending or observed unconfirmed cleanup retains the owner and temporary cwd. The ProcessHost is shut
down through its real owner; Joining/PendingHost states remain available for
continued cleanup when shutdown is interrupted or incomplete.

Only after actual operation/process joins does the caller release inert
corpus/proposal/application and credential owners. A separate finite cleanup
request uses fresh Runtime authentication and the same admin session/run for
`close_run` when valid. Fresh Auth observation already prunes expired
sessions/runs/scopes/tokens; no new reaper or ledger is introduced.

`Denied`/`Expired` after that cleanup remain distinct causes. Releasing an inert
expired owner does not report formal RunHandle closure. Process join and formal
Auth close are different observations; callback, Drop or exit alone proves
neither universal quiescence nor closure. Foreground retains cleanup owners on
failure and tears down Runtime Auth only after the background jobs/Host joins. Drop requests Core/Host cancellation; explicit shutdown observes the actual joins.

The Foreground composition is now qualified in the local Linux fixture. The durable Human10 log records `foreground_dream_native_fold_delete_and_real_idle_activity` PASS1 in 25.62 s using the already compiled binary: actual Source listener, CLI entry and RuntimeCaller, Task disabled and Hooks enabled, filesystem without a human pulse, authenticated UserPromptSubmit reaching the existing channel, native fold/delete and a second idle iteration, then normal joined shutdown. The durable drain09 log separately records retry/activity-during-native PASS1 in 33.61 s (2m59 build and queue) and Knowledge queued-cancel/noop/watcher PASS1 in 7.60 s (0.47 s compile). Memory10 PASS1 in 20.78 s is preserved in tsk-9d92 IP103 with the same binary and no new compilation; Memory09 ran zero tests and is not a PASS. These composition proofs complement the unchanged 11 Source tests and overlapping CLI06; their counts remain separate. Earlier startup Request failures and the 18.03 s first observation remain historical evidence. The narrow cancellation classification preserves the same Original and only known pre-publication interruption, without expanding timeout, effects or SDK uncertainty. This closes the human consumer composition scope, while NativeSession/GraphitSession recovery, LC and native Windows/macOS qualification remain separate. Installed Kiro authentication and model perception are not prerequisites for controlled Graphit tests. Missing temporary logs never require replay solely to reproduce a log.

## Coordinator USER lane

The Linux implementation under `tsk-ddeb` adds one installation-owned
`ForegroundUserDream` lane, separate from the installed PROJECT `Role::Dream`
job. Controlled native peers exercise the actual controller/application, Source activity and compiled prompt. The integrated USER functional cases passed; affected strict qualification is pending in the central queue.
The existing PROJECT and `tsk-c62d` producer qualifications keep their own
counts and scopes.

### Configuration and startup

The lane consumes the daemon's existing global `ConfigSnapshot`, resolved from
distribution/global/environment/command layers without PROJECT configuration.
It requires `dream.enabled`, the global Memory module gate and the Codex
binding. Shared `Settings` contains only model, program, idle timeout, maximum
duration, retries and backoff. `dream.agent` and `dream.agent_version` are
consumed by the PROJECT plan only. USER does not select a Hub Agent, PROJECT or
logical version, and it does not invent a PROJECT capture for startup.

Startup is an explicit initial attempt even with zero PROJECTs. The initial
channel value `None` denotes startup, not human activity or a human metric.
Subsequent attempts use accepted human timestamps plus idle, with the captured
retry/backoff policy. Success or noop waits for new activity; there is no
periodic sweep, extra scheduler, quota or global map. Activity received during
an attempt remains eligible for its own idle wait.

Only the initial Native-provider lookup treats `NoActive` or Local mode as a
legitimate noop when no application is retained. Active-profile errors, failed
signed-token validation, denial, cancellation and deadline are failures of the
attempt. They cannot become absence or revive a retained application. The
actual complete default USER list includes nonmandatory records and is read to
EOF. `NoCorpus` and published-empty are noops before program resolution, model
request or write admission; they do not materialize or repair storage.

### Native authority, prompt and proposal

The lane authenticates the existing Runtime caller through the actual Native
verifier cache. A verified literal issuer/subject and actual installation
capture select USER storage. Reading requires actual `ReadUser` plus
`Read(Memory)` admissions bound to that same Native caller and Original. The
[Auth Parent factory](user_record_admission.md) supplied by `tsk-04be` combines
ordinary `MutateUser` with administrative `WriteFencedMemory` and retains the
`MemoryFenceRequired` obligation. Administrative `NativeLaunch` is checked for
proposal execution. Credential, caller metadata or a serialized identity grants
nothing; `Role::Dream` continues to deny `ReadUser`.

The compiled `PolicyPrompt::UserDreamConsolidation` has exactly two Text slots:
`dream_session_id` and `memories_json`. Its bounded borrowed projection contains
only `id`, `revision`, `title`, `body`, `classification`, `mandatory` and
`important`. It contains no identity, JWT, PROJECT/version or Task metadata.
Corpus and proposal are data, never authority. The [USER prompt source](../../templates/agent-instructions/policy/prompts/user-dream-consolidation.md)
uses the existing instruction renderer and limits.

The request reuses `native_args` with read-only sandbox, ephemeral execution,
ignored user config/rules and apps/shell disabled, in an independent owned
empty temporary cwd. USER reads no PROJECT Codex config and forwards no MCP or
Role credential. These scoped controls do not attest inherited managed host
policy or commercial vendor behavior. `propose_guarded` shares the existing
`run_codex` path and leaves PROJECT `propose` available. Its concrete `CodexSink`
guard checks the same Original and rechecks the real read/write admissions and
Parent `NativeLaunch` authorization at every event.

The same `perform_user` frame retains caller, verifier, physical capture,
Parent, read/write admissions, Original and temporary cwd until the actual
native operation and any `RemainingProcess` are observed and joined. A cleanup
deadline bounds cleanup observation only. Initial failure remains failure;
partial output or a proposal never authorizes a write. Only a complete,
successful, cleaned-up native answer reaches the existing pure recipe parser.

### Application, accepted activity and shutdown

[USER application](rust_dream_records.md#coordinator-user-application)
uses the actual `tsk-c62d` SDK automatic/reobserve methods.
`UserApplication` retains physical capture/binding, prepared recipe, operation
keys, cursor and receipts as data. Each apply uses legitimate borrowed
admissions, checks that the same physical USER is selected and rejects A-to-B
retargeting. The first attempt at each cursor calls the actual `automatic_*`
SDK method directly; that method already reobserves under its writer before
any new effect, preserving idempotency, CAS and Fenced checks. An in-memory
attempted-operation state selects recovery routing only and grants no authority.
If the call fails, is uncertain or is cancelled, the application retains its
cursor, input/key and earlier receipts. On resumption it calls the actual
`reobserve_*` method with fresh legitimate USER admissions, then calls
`automatic_*` with the same input/key only when reobservation returns `None`.
A receipt or domain refusal advances the cursor and clears the attempted state.
A new application instance replaying the same recipe still relies on the
actual automatic SDK's idempotency; no replacement key is invented for replay.
SDK deadlines and guards are unchanged. References, tags,
classification/flag derivation and protection rules remain the producer's
responsibility. Pure `recipe::prepare` is shared without a new grant, writer
or authority-bearing DTO.

Source attaches one installation USER sender before serving. After existing
caller authentication, Original and active physical PROJECT checks,
`record_human_activity` supplies the same accepted `Instant` to the applicable
PROJECT sender and USER sender. This signal needs no USER read, principal or
TaskSession payload. Later observations come only from accepted human events
in real PROJECTs; zero-PROJECT startup does not promise a new global hook.
The USER lane survives PROJECT selection/switch/retirement independently and
is cancelled and joined by Foreground shutdown before Auth teardown.

### Source evidence and proof limits

The implementation is in [the USER controller](../../crates/graphit-app/src/foreground_dream/user.rs), [Foreground composition](../../crates/graphit-app/src/foreground_source.rs), [Source activity ownership](../../crates/graphit-app/src/source_server.rs), [the accepted pulse](../../crates/graphit-app/src/source_server/project_hook.rs), [guarded native proposal](../../crates/graphit-daemon/src/dream.rs) and [USER application](../../crates/graphit-daemon/src/dream/user.rs). No suite was repeated solely for documentation.

The fixtures use actual AuthStore, signed Ed25519 Broker, Core, SDK and ProcessHost with a controlled native Codex peer. The original 30-second request and 20-second test wait remain unchanged. The two internal Auth identity comparisons now share one checked identity per synchronous barrier, with no cross-stage cache or omitted check. Earlier failures and external diagnostics remain historical; the successful functional run is not a benchmark or vendor authentication/live-inference claim. Current affected strict qualification remains pending in the coordinator's shared build queue.

This source boundary is Linux-only through the actual Native SDK/cache.
No vendor account, model access or commercial credential is a prerequisite for
its Graphit qualification; no Windows/macOS runtime qualification follows.
No personal profile, credential, trust or configuration change is part of this
consumer. Global migration, host and LC obligations remain separate.

## Source and qualification boundary

This section retains the executed PROJECT qualification. It does not report
execution or strict results for the separate Coordinator USER lane above; its evidence is recorded in that section.

| Implementation | Contract |
| --- | --- |
| `crates/graphit-app/src/foreground_dream.rs` | Settings, Agent definition, activity/job, native request, recipe/application and retained cleanup |
| `crates/graphit-app/src/foreground_source.rs` | Async startup, conditional Fenced permission, shared supervisor signal and shutdown composition |
| `crates/graphit-daemon/src/dream.rs` and `dream/recipe.rs` | Proposal/consuming parts and whole recipe/application/reobservation |
| `crates/graphit-app/src/foreground_dream_tests.rs` | Actual Foreground/Hub/Core publication/Auth/Lance/native Process fixtures |

The executed private module is `foreground_source::dream_tests`: **8 passed,
0 failed, 0 ignored, 180 filtered**. These are eight unique tests; nested
scenarios and the separate focal repetition are not additional tests. The
suite took 104.01 s, with 43.16 s build and 147.25 s total command time. Strict
App `--lib` Clippy also exited 0 in 81.26 s. External ANTLR C++ build warnings
occurred without a Rust App lint failure; this was not an all-App integration
gate.

Coverage uses actual Foreground, SDK Hub Agent publication, Auth, Lance and
Native Process: fold/delete/protected/last/noop; foreign/stale/duplicate/
malformed/partial/nonzero/oversize proposals with zero Memory effects; actual
filesystem activity without a model; `slow_first` success and `failed_old`
last-failure/new-activity rearm with retries zero; bounded retry; partial
same-Run/Actor recovery with earlier receipt and late row limit; cancel with
borrowed join; deadline/Auth expiry/Denied distinctions; Host waiter
poll-pending/resumption; and disabled Fenced denial. This does not universally
qualify `PendingHost::repair` or every failure-path join/cleanup.

The executed commands used the coordinated locked/offline target:

```text
cargo xtask cargo test --locked --offline -p graphit-app --lib foreground_dream -j4 -- --test-threads=1
cargo xtask cargo clippy --locked --offline --no-deps -p graphit-app --lib -j4 -- -D warnings
```

Private history in repository-relative
`.build/rust-foreground-dream/validation.json` retains the earlier 6 PASS/2 FAIL
and 7 PASS/1 FAIL attempts and lock-wait cancellation exit 130. These failures
are not erased or added to the final unique-test count. Formatting/diff,
six-file whitespace and six local-link checks passed after the final doc edits.

Command/harness/build costs are separate from operation latency; no
benchmark, hard CPU/RSS limit or vendor inference performance follows from
SDK fixtures. Linux fixture qualification must not be transferred to installed
Codex credentials, Kiro, Windows/macOS, all failure-path joins or Main CLI
cutover. The instruction renderer's tests qualify rendering only.
