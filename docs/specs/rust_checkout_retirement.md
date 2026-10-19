---
title: Explicit local checkout retirement
doc_type: technical
status: source-main-qualified
references:
  - target: {type: task, id: tsk-cab0}
    relation: corrects
  - target: {type: task, id: tsk-2515}
    relation: implements
  - target: {type: task, id: tsk-b9d7}
    relation: derived_from
  - target: {type: task, id: tsk-9a23}
    relation: implements
---

# Explicit checkout retirement

The App component `project_retirement::retire_checkout` removes only an explicitly
selected checkout from [Core's maintained inventory](rust_project_inventory.md).
The UI's existing unregister intent keeps source files. The Foreground
administrative HTTP/CLI ingress below invokes this component; the React
unregister request remains a separate integration.

The input carries a Runtime or Local credential, a `PrivateInstallation` captured
on the incoming `RequestContext`, the expected `ProjectId`, and the exact native
historical checkout path. The component authenticates through the real `DaemonAuth`
and requires ordinary `Permission::Configure` explicitly enabled for that origin.
Registry data, path/ID selectors, Broker inventory credentials and browser PROJECT
JWTs do not authorize this operation. No policy is expanded by calling it.

The installation root and brand must match the running service's live scope. Its
physical installation namespace and recorded binding are revalidated.
`CapturedPublicationScope::revalidate_installation` observes the owner, runtime
root and their captured ancestor identities under the incoming cancellation and
deadline. Runtime-key and Auth-document checks retain their actual private
handles and document observations. Unrelated admitted project directories do not
become a prerequisite for authentication or local metadata cleanup. Full project
scope validation remains mandatory for grants and operations that use a project.
Selection uses the current
inventory snapshot; it does not reopen the historical checkout, inspect Git,
ensure identity or create a missing directory. Exact metadata absence returns
unchanged. Malformed metadata, original cancellation, deadline and I/O failures
remain errors.

For a registered pair, `ProjectSupervisors::stop_checkout(id, path)` removes active
owners across all versions of that exact checkout and matching parked contexts.
It cancels all matching owners before awaiting any join, then awaits their actual
producer and watcher cleanup. Other projects and checkout siblings remain live.
Stop errors retain their causes and prevent inventory mutation. This is not a
universal quiescence receipt: the producer owns cleanup of its resources.

After the joins, the component rechecks the same Original, authenticated source,
Configure permission and installation. It then acquires the existing inventory
writer and calls `remove_checkout`. No file lock crosses an asynchronous join.
Core rereads under its stable lock and atomically publishes the actual metadata
change. Versions, other checkouts, identity and project files remain. A metadata
failure does not restart owners already stopped; there is no transaction or
rollback spanning supervision and the registry. A confirmed Core commit is not
converted into an ambiguous failure by a new post-commit request check.

Cancellation or expiry during cleanup does not abort the joins or renew the
Original. After cleanup, that request error prevents publication. Await the
operation to acknowledge cleanup. Dropping the future does not establish a join
acknowledgement. Parking, offline state, switching and daemon shutdown keep the
historical inventory and do not invoke this explicit operation.

Data-version deletion is separate: Lance named-branch semantics and the actual
Knowledge writer are tracked by `tsk-1e53`. This component never invokes
`remove_version` or deletes data.

## Live administrative ingress

`POST /admin/projects` now enters the running Foreground owner. The administrative
request uses exactly one `Authorization: Bearer ...` and
`x-graphit-auth-origin: runtime`, with a JSON body such as
`{"operation":"retire_checkout","project_id":"<ProjectId>","project_dir":"/project"}`.
`project_dir` uses Core's Unicode `NativePath` JSON string; the shown absolute
path is illustrative, not an installed project. A Windows selector likewise
uses its literal Unicode path string with normal JSON escaping.
Browser Origins are refused. The live operation authenticates Runtime and requires
explicit unconditional Configure permission. The Foreground factory enables that
ordinary permission only for its private Runtime origin, preserving Local/Role/Web
policies; a project access JWT or a selector
never substitutes for it.

The Foreground owns the command until its cleanup is observed. It withdraws the
checkout from operational selection, joins its Now publisher, selected Dream owner
and project supervisors, then invokes the qualified Core retirement component.
Other checkout publishers and supervisors remain owned. After the Core result, it
publishes the complete retained inventory through the configured Broker credential
mode and shared verifier cache. No automatic rollback or retry follows a network
failure. The reply separates `checkout_removed` from `broker_effect`; a failed
Broker admission after local removal does not erase the local effect. Diagnostic
causes stay internal and credentials never enter the response.

A recoverable `dream_join` failure retains the stopped Dream owner for explicit
cleanup retry. Its expected termination does not stop the daemon listener or
sibling services. The retained owner is not evidence of successful cleanup;
the administrative failure and its effects remain observable.

Operational membership contains only project IDs, native paths and versions. It
is distinct from historical inventory and authorization. New Source and Task
requests consult that membership, while admitted native readers retain their own
Original. The compiled `projects retire-checkout --project-id ID --project-dir PATH` client
uses the same route, runtime credential and NativePath codec. It can run outside a
PROJECT checkout and prints the actual JSON effect observation.

The native caller preserves valid JSON replies together with their HTTP status,
including failure replies that record partial effects. Rejections before the
owner may have an empty or non-JSON body. Captured response limits still apply.
Within those limits, the caller reports the
numeric HTTP status and received byte count without including the response body
or credentials in the error. Successful responses still require JSON; an empty
200 or 204 is not a successful retirement acknowledgement.

Task Browser context requests retain one operational snapshot and use its current
project/version for the existing Task admission. They pass that exact tuple to the
reader instead of indexing startup state. Task authorization, response schemas
and Original checks remain required. The removed startup roster and current-index
fields have no replacement fallback; the affected fixtures use the same snapshot
producer and retain it through their actual Auth and I/O operations.

SourceServer also retains only the live roster. Its Now, Role, Hook and Ask
factories borrow the operational snapshot before composition rather than storing
another startup roster/index. A missing active selection is an explicit error;
it cannot select an old checkout. Snapshot Arcs stay alive through borrowed
metadata and I/O without keeping the state lock held.

`projects maintain-checkout --project-id ID --project-dir PATH [--version VERSION]
[--detached-base VERSION] [--select]` observes the real checkout through the Git SDK and registers its capture with the
Core inventory writer. The same observation is reused for registration. An omitted version follows the actual checkout selection; it
does not run Git checkout or import old state. The HTTP operation is
`maintain_checkout`, with the same `project_id` and `project_dir` fields plus
`version`, `detached_base` and `select`. Identity and the native root must match the
explicit selectors. The Core registration retains previously maintained versions.
The response reports `inventory_registered`, `trusted_root`, the observed `version`, and the
separate `broker_effect`.

The owner publishes the complete maintained inventory before admitting a new
Now publisher. The Broker requires that exact project/version to be registered.
The command sends one PUT and preserves its confirmed or uncertain effect if a
later Now admission fails. It does not replay the publication or roll back the
Core registration. An unchanged selection sends no PUT.

After Core confirms registration, the owner passes a real PROJECT scope and
binding to `trust_project_root`. Auth validates Native Runtime, Configure, the
same issuer and Original, and the physical binding before adding the exact root.
This preserves the installation realm, runtime, issuer epoch and verifier cache.
Trust is additive and can preserve historical roots; it neither marks a checkout
active nor grants a PROJECT module or SQL permission. The consumer records the
confirmed effect before a later Original check, so cancellation afterward cannot
hide that effect. Retirement withdraws operational membership and joins jobs
without adding new trust. If the selected Dream checkout was retired, its owner
is joined and the existing factory starts the remaining selected checkout; shared
activity follows that real owner. A later factory failure preserves the known
Core retirement effect.

Native MCP stdio startup submits the same typed maintain operation once before
connecting its Source proxy. The canonical native root is shared by maintenance
and proxy selection; the proxy transport constraints are checked before effects.
The real runtime Configure permission remains
required; a selected checkout does not grant PROJECT access. The owner observes
the checkout first. Equivalent version facts reuse the active context even when
selection provenance differs (explicit argument versus checkout/default). When
the active tuple and requested selection already match,
it returns `unchanged` without Core bootstrap, model startup or Broker PUT.

The running owner replaces its operational snapshot and reconciles its real
supervisors, publishers and selected Dream resources. Unchanged siblings keep
their jobs. Source and Task requests clone one snapshot and retain that tuple for
the operation. The embedding process and model resources are reused; enabling
embeddings when no process exists uses the existing model factory. Global
`daemon.models` changes refresh the running child through authenticated Runtime
control; Source publishes the new selection only after its ACK. Invalid settings
preserve the valid generation. Empty settings clear semantic resources while
retaining the lightweight child control for re-enabling and checkout maintenance.
Removing the last PROJECT closes that child only when no real Browser owner
remains. Browser USER Memory and imported Knowledge can consume the model without
a PROJECT; their owner also observes GLOBAL model controls and joins the child
on service shutdown. Task-domain search remains lexical.
Requests already admitted by the child retain their generation until completion.
No lock is retained across
model or native I/O. Each project callback clones the current model slot at job
admission, so watchers created before embeddings were enabled use the model after
reconciliation. Existing affected watchers receive a rescan through their actual
handles. Cancellation between Knowledge, Raw, semantic and Memory processing
stops admission of the next phase. Knowledge reads the same current optional
model resources before Raw processing. The typed Memory cancellation before
admission can complete quietly;
generic Interrupted, semantic failures and uncertain writes remain errors. New Now publishers retain their own tuple and wait for the
real welcome/first publication; they do not retain the entire project list.

Functional consumer evidence under `tsk-9a23`: `memory-cancel10.log` records
one real CPU-model cancellation/NoCorpus/foreign-checkout test (3.95 s, no skips).
`stdio-live04.log` records the live Foreground owner and SDK MCP stdio handshake,
unchanged maintenance with no Core/trust/Broker effect, canonical checkout
selection, source fallback for an unsupported extension with Ast disabled, and
complete stdio/daemon joins (one test, 3.65 s, no skips).
`live-task-role11.log` records the live Role MCP consumer with actual SDK authority
and local M process: scoped Task reads on main, the selected feature version and
the remaining sibling, rejection of stale/retired selection, and complete MCP
and Foreground joins (one test, 6.37 s, no skips). Ast and Knowledge are explicitly
disabled only in this focused Task fixture. Build-directory waiting is separate
from these test runtimes. These qualify the affected consumers; the complete
Broker inventory/Now/Source journey is recorded separately below.
The roster-retention cleanup also passed the existing live Hook reload consumer
in `source-live-retention-hook02.log` (one test, 0.02 s after 1m29 compilation).
Both consumers qualify the SourceServer roster-retention cleanup; the Role build
included 7m00 of compilation and waiting before the measured test runtime.

The Task consumer uses the existing Role MCP endpoint and a tool-owned Role
issued through the real SDK administrative authority. Switching the selected
version does not retarget an existing Role scope; a new credential must refer to
the selected version. The Role catalog exposes Task reads and own-task
comments/progress; it does not expose Session creation. This lifecycle consumer
uses the existing scoped Task read, preserving that capability boundary.
Retirement removes eligibility for new requests without
changing admitted readers or granting the remaining project any permissions.
This consumer does not replace the separate standard Root Task CLI/MCP
composition tracked by G1. The Broker fixture uses an access token signed by its
test issuer and real JWKS verification; it does not qualify BrokerSigner or PKCE
login.

The complete multi-project Broker journey passed in `live-inventory09.log`
under `tsk-9a23`: one test, 148.42s of execution after 45.62s compilation/queue.
It preserves real add/select and complete PUT/GET inventory, third-project Now,
literal version switch, exact checkout retirement, withdrawal from Source/Now,
and continuing sibling/historical registry assertions. This qualifies that live
lifecycle integration; React unregister remains separate. Compilation failures
before a test starts remain build failures, not functional evidence.

The Source owner corrected the Ast-disabled fallback and qualified Web, native
MCP/CLI and the independent Graph gate in the focused `graph-source-ast-off`
cases under `tsk-9d92`. The unchanged `live-inventory07.log` now passes its first
disk source request, keeping Knowledge enabled, then fails during Broker
inventory write at `insert_parent` with Deadline. The failure-only diagnostic
observes elapsed 2543 ms and initially remaining 1808 ms; pool size 2/idle 1 is
sampled after unwinding, not proof of a database lock or its cause. Its reply
preserves
confirmed Core/trust effects and the Broker's `MayHaveApplied` observation;
it is not replayed or treated as success. Its targeted PostgreSQL diagnosis
contains PID/state/wait type/last-query class only, without raw SQL values or
credentials. These query classes are not correlated with the request's operation
phase. The separate Knowledge error after document publication observed
in `live-task-role07.log` remains a failed cancellation case for the productive
Knowledge path; the focused Role qualification does not supersede it.

The two-second deadline in Inventory07 came from this normal-journey fixture's
explicit Broker publish/read HTTP configuration. Its bootstrap Original had a
600-second deadline; administrative requests separately preserve their finite
Hub policy deadline (120 seconds in this fixture). It was not an access-token TTL or a demonstrated pool
shortage. The fixture now uses the same explicit 30-second request timeout as
the existing compiled native Broker integration. Production configuration and
the separate short-deadline/cancellation negatives are unchanged. The changed
Inventory08 journey passed the inventory publication stage: its response
observed `broker_effect=confirmed` and retained Core/trust effects. It then failed
at `now_publication` with `Now publisher Remote(Superseded)` and HTTP 503 instead
of the required 200. That failure may come from an existing sibling publisher or
the newly admitted publisher; the shared readiness join does not identify which
one. The Broker contract preserves publishers of retained tuples across a full
inventory update; the generic wire error also covers inactive/expired sessions,
so the failure does not prove the PUT invalidated a publisher. The test ran
134.77s after 76s of compilation/queue time. Both Inventory07 and Inventory08
remain recorded as failed.

The normal Inventory09 fixture aligns only its advertised heartbeat with the
60-second interval already used by the qualified normal subscription fixture
([Runtime subscription](rust_runtime_now_subscription.md)). This journey tests
add/switch/retire and consumers, without a four-second freshness SLO. It does not
change production configuration or token TTL; Broker configuration requires an
explicit heartbeat rather than supplying a production default. Separate signed
expiry cases retain their four-second setting and valid evidence. Only a fresh
publish renews publisher liveness with Auth, PROJECT and registration rechecks;
no standalone keepalive, Ping renewal or old-snapshot replay was introduced.

Inventory09 also reports separate Knowledge publication failures: main fails
at DataFirst Image/BeforeCommit with Request InvalidData; feature/live fails at
Writer/BeforeCommit with Artifact Create AlreadyExists. Those producer errors
remain necessary Knowledge work. The passing lifecycle assertions do not prove
those Knowledge publications succeeded or replace their cancellation/recovery
qualification.

Filesystem batches no longer emit Dream human-activity timestamps. The removed
supervisor callback has no replacement; explicit human ingress remains separate.
The affected producer/coalescing, filesystem, parking/sibling and deletion tests
passed four cases in `fs-activity-removal-daemon01.log` (0.07 s). The existing
Role Memory rescan and Knowledge/Project retirement consumers passed one case
each in `fs-activity-removal-consumers01.log` (9.76 s, 6.19 s and 0.11 s).
The affected Daemon/App libraries and Project/Knowledge retirement targets also
passed scoped Clippy with warnings denied in `fs-activity-removal-strict01.log`.

## Qualification

The same component is composed on Linux, macOS and Windows through the existing
Core inventory and Daemon supervisor APIs. Platform composition is tracked by
`tsk-cab0`; the executed results below remain Linux evidence, and do not assert
native Windows or macOS execution.

Current Main contains the qualified component and focused native fixtures. The initial
App matrix passed 12 cases and failed the missing physical checkout case because
installation authentication revalidated unrelated project receipts. That result
is preserved in `app-tests-second.log`; it does not qualify the corrected contract.
The corrected current Main passes 13 App cases, 2 Daemon selective-stop cases,
and 23 selected Core scope/Auth-document cases. They cover real Runtime and Local
callers, exact history, absent physical checkout without recreation, sibling
versions/files, all cancellations before joins, partial cleanup/metadata failures,
installation/runtime replacement, Original cancellation/deadline and poison.
The 23 Core cases are the `installation_scope` (5), `runtime_scope` (4),
`bounded_scope` (1), `installation_binding` (12) and
`installation_capture_and_store` (1) filters. The initial obsolete immediate-Busy
fixture failure remains historical in `core-auth-installation.log`; current
causal waiting/poison fixtures pass.

Formatting and diff checks passed. Strict Clippy (`-D warnings`) passed for
Core/Daemon libraries and tests, plus the App library and this test target. No UI/CLI, Data
retirement, Windows or macOS execution is claimed. The installation contracts are
[Runtime keys](runtime_key.md) and [private Auth documents](private_auth_document.md).
