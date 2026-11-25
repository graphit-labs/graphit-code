---
title: System contracts and reconstruction map
description: Observable Rust contracts, data and state flows, authority, errors, consumers and implementation owners.
content-type: reference
audience: developers
updated: 2026-10-29
---

# System contracts and reconstruction map

Audience: implementers and integrators reproducing the current Rust product's observable behavior.
This page connects user journeys to concrete contracts; it is not a future architecture or a promise
that every prepared platform artifact has been published. Start with [architecture](architecture_overview.md),
[configuration](../guides/configuration.md), [CLI options](../reference/cli-options.md) and
[MCP schemas](../reference/mcp-input-schemas.json). Detailed linked specifications own their actual
wire/data codecs. Implementing only this overview would omit those required contracts.

## Core entities and identity

| Entity | Identity / scope | Mutability and persistence |
|---|---|---|
| Installation | Native installation identity and captured Brand/global root | Private config/Auth/inventory; cannot be inferred from one client's project path |
| Project | Canonical nonzero ULID in `.graphit/project.json` | Name is mutable metadata; ID owns stores/authorization |
| Checkout | Actual physical project root plus captured Git/version facts | Inventory locator, never corpus identity or access authority |
| Project version | Actual Git branch, detached `git:<SHA>`, or selected non-Git/main version | Native Lance branch within each corpus; graph FILE publication has its own version-key |
| Principal | Verified issuer/subject and applicable installation | Independent of paths, CORS, display names and unit attribution |
| Task / Session / Memory | Canonical domain ID within authorized corpus/version | Positive revisions, creation/history and domain state transitions |
| Artifact | Producer, kind, ID and literal artifact version | Immutable selected package/publication identity, distinct from project version |
| Model space | Protocol/model/revision/dimensions/recipe (plus verified local artifact) | Compatibility boundary; equal width alone is insufficient |
| Captured read | Authorized corpus and native generation/branch/publication | Snapshot consistency, cursor binding and EOF; not fresh authorization forever |

Paths are Unicode JSON strings. Rust uses standard Path/PathBuf; reject non-Unicode/NUL/invalid
relative components explicitly. There is no OS-discriminator/byte array path codec. Current
selectors cannot choose an arbitrary S3 URI, file root, user or credential.

## Configuration and capture

Ordinary precedence is Command > Environment > Project > Global > Distribution, then the
consumer's documented code default for an absent value. Presence includes false/zero/empty;
a wrong winning type fails. Null is not a generic public config value. Whole objects/lists replace
across layers unless a specific consumer explicitly implements another rule. Module booleans
are strict JSON booleans. Mechanical environment names do not imply every key is captured by
every consumer; [the key reference](../reference/configuration-keys.md) identifies that boundary.

Auth provider topology and account secrets are separate from ordinary Config. Agent binding has
specialized membership/explicit selection rules; completion `ai.cli` is separate from agent-file
installation. Model slot selection is `daemon.models`; old model-ID convenience keys are not
active model selection. Secret inspection reports only configured state. Writers lock/reread,
preserve unrelated values and publish atomic replacements; reads create no state.

## Request and response pipeline

1. Capture process/installation inputs, original cancellation and monotonic deadline.
2. Resolve actual identity/provider/account and select retained PROJECT/version or exact artifact.
3. Admit the operation under current permissions. IDs/paths/cursors alone grant nothing.
4. Resolve storage and service topology. Broker credentials/discovery are reused/renewed only in
   their own identity/provider/scope/module binding. Remote failure never creates a local replacement.
5. Capture the relevant native snapshot or validate writer expected revision/head. Execute bounded
   input/IO/native work under Original, admission and concrete concurrency controls.
6. Recheck association and authority before publication/delivery. Record real confirmed/uncertain
   effects if later IO/encoding fails; there is no pretend rollback.
7. Format only at the interface boundary. CLI/HTTP keep JSON; structured MCP uses TOON by default
   and explicit false for JSON, without duplicating the whole payload. Original text stays original.
8. Join owned processes/native work/transport handles on cancellation, EOF or shutdown. Separately
   server-owned Live work has its explicit cancellation lifecycle.

HTTP status, JSON-RPC protocol error, MCP domain `isError`, valid absent/empty result and pending
publication are different states. Do not flatten them to an empty array or retry every error.
Do not include private upstream diagnostics/tokens in public errors.

## Data and versioning invariants

One authoritative Lance dataset exists per project/corpus: AST Source, AST searchable entities,
Knowledge, Memory and Task. Text/vector fields share each searchable corpus. Knowledge and AST
progressively publish completed files before inference/corpus EOF. Memory/Task commit authored
records before background semantic attachment. Valid partial coverage can be searched; FTS/graph
and Source are not blocked by embeddings.

Semantic jobs derive pending work from persisted public rows, compute finite batches outside the
writer and attach only if current identity/content fingerprint/model recipe still matches. Reuse
unchanged compatible vectors across incremental edits/branches. No duplicate pending corpus or
queue is authoritative. Query snapshots/FTS/vector freshness use the SDK's native semantics;
index maintenance is not required to make every new unindexed fragment invisible first.

Branches share fragments and snapshots until writes diverge. PROJECT Task/Memory Git tags bind
observed commits to immutable native snapshots; checkout/merge uses those refs, with explicit
conflicts for divergent family changes. USER Memory never follows Git. Cleanup respects branch
heads, Git tags and retained snapshots. Never reclaim active data by filesystem glob.

Icebug is an immutable Parquet/manifest graph publication with FILE-sized members and stable
ranges; it is distinct from Source and the entity search dataset. In-memory Ladybug reads local
or authorized remote ranges without persistent `.lbug` materialization. Closing readers permits
owned obsolete-shard retirement. No format redesign is implied by moving the global root.

## Lifecycle matrix

| Journey | Owner and normal outcome | Failure/recovery contract |
|---|---|---|
| Setup/maintain checkout | Core identity + App inventory; existing ID retained | Actual effect can precede later failure; no ID replacement or import |
| AST full/incremental | SCIP/compiler or syntax fallback → FILE graph/text publication → semantic batches | Auth/Source/integrity/parent cancellation terminal; only classified unavailable compiler falls back after cleanup |
| Knowledge compilation | Captured source → parser/metadata/desired reconciliation → progressive publication | CAS/concurrent first publication preserves real latest state; no unknown-file adoption |
| Memory CRUD | Records expected revision/idempotency → retained history/text | Stale revision conflicts; tombstone preserves audit; no last-writer-wins |
| Task execution | Session/Task specification → claim → progress/checks → complete/release | Fences/leases and hierarchy gates; cancelled child terminal but not delivered scope |
| Git commit/checkout/merge | Existing registry notifications → native tags/restore/family merge | No distributed atomicity; preserve partial effects and retry explicit recovery |
| Hub publication/install | Authored PROJECT capture/exact package → native writer → origin/version/hash record | User/foreign edits protected; pending/uncertain effects retained; recovery proves ownership |
| General sync | Existing exact Hub installs + package-native roles/skills/Coordinator/hooks/MCP | No remote upgrade or index wait; completed independent steps remain committed |
| Model use | Effective Local purpose → verified slot → demand load → native inference → idle unload | Explicit GPU fails if unavailable; auto reports fallback; disabled purpose causes no model work |
| Browser login | Broker PKCE/state/nonce → JWT validation → opaque HttpOnly session handle → PROJECT admission | Expired challenge starts again; access JWT valid until expiration subject to full checks |
| Service lifecycle | Per-user manager + real Source owner → readiness/discovery → drain | Correct process/descendant cancellation and joined cleanup; no PID-name heuristics |

## Authentication, authorization and storage

Validate JWT trusted signature, permitted algorithm, issuer, exact audience/resource/client binding,
expiry/not-before/temporal requirements and token use. Preserve PKCE/state/nonce and ID token checks
in login/refresh. Discovery/JWKS cache supports key rotation; no mandatory UserInfo/introspection
per request. UserInfo can fill missing profile fields only. Revocation/account/claim changes can
remain unseen until access expiration; do not invent a second TTL/blacklist to mask that decision.
PROJECT authorization is separate and current at consequential operations.

STSv3 uses fixed scope/module and current verified PROJECT or USER identity. Broker returns bucket,
region, endpoint, prefixes, expiry and restricted credential session. Client data travels directly
to storage. Valid explicit storage-disabled discovery can use local; denied/broken credentials/S3
cannot. A valid credential session is reusable, but not an indefinitely stale SDK read snapshot.
USER isolation includes installation. [S3 contract](../guides/s3-and-ui-network.md) and
[auth](../guides/authentication.md) define exact setup and local development exceptions.

## APIs and frontend

| Interface | Contract owner | Representation |
|---|---|---|
| CLI | `graphit-app/src/cli.rs` and domain `*_entry.rs` | Exact parsed commands/options; JSON or original text as documented |
| Root MCP | Source/Task/Graph catalogs | [67 current tools](../guides/mcp_tools_reference.md), strict input, TOON/JSON selector |
| Role/Live MCP | Role/Live server with actual credential | Filtered grant-specific catalog; metadata hints do not grant permissions |
| Native Task HTTP | Task router/catalog | Typed `selection/input`, actual native operation result |
| Browser HTTP | Verified Browser actor adapters | JSON; cookie/CSRF/current project/provider validation |
| React UI | `ui/src/api/`, stores and domain components | Views over same services, no second persistent data model |

Follow [UI](../specs/ui_dashboard.md), [Task protocol](../specs/rust_task_protocol.md),
[Hub consumers](../specs/hub_consumer_contract.md), [Source](../specs/rust_source_entrypoint.md)
and [output](../specs/output_module.md) for actual endpoint/input/output shapes. A present domain
library is not evidence that every interface exposes every operation.

## Customization and distribution

- Global/project/env/command layers use existing Config; providers/accounts use Auth commands.
- Custom parser images use CSV extension mappings and the documented Docker SCIP mounts/output.
- Native extraction profiles use Tree-sitter captures and official ANTLR C++ XPath/traversal with
  small typed capture operations. No replacement custom DSL or ContextPath interpreter exists.
- Instructions use central Markdown templates with actual Brand/tool/alias/context values; raw
  overrides and explicit Hub bundles retain precedence and validation. Static files never contain
  runtime mandatory-memory snapshots or credentials. Package canonical source + installed hash
  avoids a second retained render payload store.
- Releases install both executables plus native loader closure/runtime/notices. CPU remains
  functional/default; optional GPU adds backend/userspace libraries while the host supplies driver
  and OS libraries. Source builds need the pinned SDK revisions and actual C++ toolchain.
- [Libs/forks](../development/rust-forks.md), [packaging](../development/native-runtime-packaging.md)
  and [workspace](../development/rust-workspace.md) distinguish prepared sources from published
  immutable artifacts. Never fabricate a published SHA or native platform PASS.

## Detailed specification map

| Domain | Required technical contracts |
|---|---|
| Identity/config/filesystem | [Identity](../specs/project_identity.md), [Config](../specs/config_module.md), [storage](storage_layout.md), [shared native documents](../specs/native_shared_documents.md) |
| Code | [AST](../specs/ast_module.md), [profiles](../guides/ast_extensibility.md), [Source](../specs/rust_ast_source_handler.md), [Cypher](../specs/rust_captured_cypher.md) |
| Documentation | [Knowledge](../specs/wiki_module.md), [source streaming](../specs/rust_knowledge_source_stream.md), [semantic data](rust-knowledge-semantic-data.md) |
| Records | [Memory](../specs/memory_module.md), [Task](../specs/task_module.md), [Sessions](../specs/task_sessions.md), [Git](../specs/git_records.md) |
| Services | [AI](../specs/ai_engine.md), [model manifest](../specs/rust_model_manifest.md), [model service](../specs/rust_model_service.md), [Process](../specs/rust_process_service.md) |
| Collaboration | [Hub](../specs/hub_collaboration.md), [consumer admission](../specs/hub_consumer_contract.md), [references](../specs/record_relations.md) |
| Agents/UI/operations | [Instruction templates](rust-agent-instruction-templates.md), [bindings](../development/native-host-bindings.md), [UI](../specs/ui_dashboard.md), [daemon](../specs/daemon_module.md) |

These contracts plus the exact schema/CLI references are the reconstruction boundary. Historical
receipts explain what was run on specific sources; they do not supersede current contracts or
supply unimplemented behavior. When implementation and documentation conflict, resolve the concrete
mismatch and keep a truthful limitation rather than silently inventing a feature.
