---
title: "Troubleshooting"
description: "Diagnose and resolve common issues with Graphit Code — daemon, AST indexing, AI/embedding, memory, hub, MCP, and configuration problems."
content-type: guide
audience: developers
updated: 2026-10-29
keywords:
  - troubleshooting
  - errors
  - debug
  - daemon
  - fix
  - common issues
prerequisites:
  - "docs/guides/getting_started.md"
related:
  - "docs/guides/mcp_tools_reference.md"
  - "docs/guides/cli_reference.md"
  - "docs/guides/user_manual.md"
  - "docs/guides/agent_hook_activation.md"
  - "docs/guides/daemon_operations.md"
  - "docs/guides/ai_models.md"
---

# Troubleshooting

Use the actual error category, selected project/version, provider and operation to diagnose a
failure. A timeout or failed response can follow a committed write; inspect current state before
retrying. Do not repair access by changing databases, deleting identity, disabling authorization
or silently switching from S3 to local. [Contracts](../architecture/system_contracts.md) explain
which component owns recovery.

## Start with a small observation

```sh
graphit --version
graphit daemon status
graphit ui --no-open
graphit config list
graphit provider list
graphit account list
```

These serve different questions: build identity, real service/manager status, actual Browser URL,
captured settings, provider topology and active account. Config/provider/account output redacts
supported secrets; do not paste private tokens, native session IDs or cookie values into reports.
Use `--help` and the [complete option reference](../reference/cli-options.md) when a command differs
from an old example. Current commands are Rust; there is no `ast index --reset` recovery alias.

## Installation and native loading

| Symptom | Diagnose and recover |
|---|---|
| `graphit` not found | Put the reported complete install directory on the host/agent PATH; do not copy an obsolete executable into another search location |
| Missing native `.so`/`.dylib`/DLL | Reinstall/preserve the complete matching release: both executables, runtime assets and notices; copying only `graphit` loses its library closure |
| Source build cannot resolve a fork | Follow [workspace](../development/rust-workspace.md) and [forks](../development/rust-forks.md); unpublished immutable sources are a publication blocker, not permission to substitute upstream |
| Missing `broker-now-wire` | Keep the matching Broker checkout in the documented sibling directory; use the paired revision |
| Version/help works but inference fails | Help does not load a model or prove driver/backend/device availability; inspect selected manifest and execution |

The client needs host OS libraries and optional GPU drivers. Packaged redistributable libraries
are not an extra manual package-manager installation. ONNX Runtime is not a local product backend.
For source SDK builders, use the pinned origin/license/build instructions rather than a new FFI.

## Daemon and tray

`daemon status` observes the authenticated Source or native per-user manager. Service registration
and startup at login are distinct: use `daemon install --help` / `daemon login --help` for the
explicit selection. A manual foreground daemon and a manager-started daemon must not race over
one installation owner. Stop/restart through the maintained command so jobs, native owners and
descendants are cancelled/joined and discovery is removed by its real owner.

The Linux tray requires a working desktop session bus and StatusNotifier/AppIndicator host. A
headless service needs neither. A locked GNOME session or disabled extension can hide the icon.
`Open interface` uses the same authenticated control as `graphit ui`; if no Browser is configured,
fix `task.browser` rather than expecting an unrelated web server. `ui --no-open` isolates URL
retrieval from the system opener. If it returns a URL but the opener fails, open that URL manually
and check the OS desktop opener. A stale previously bound port is not the current Browser origin.

A slow CLI/MCP/UI operation may be a background job, native storage query, authentication or response
issue. Capture actual command/error and timings before assuming the UI is rendering slowly. An
old loaded stdio proxy can retain its old catalog until that client reconnects; do not kill all
active connections unnecessarily.

## Browser login or an expired challenge

An invalid sign-in form/challenge often means the request expired or belonged to a replaced daemon.
Start from a fresh `graphit ui` login, not a saved `/oauth/login?id=...` URL. Complete the same
flow in one browser session; state/nonce/PKCE and callback origin are checked. Use the Broker's
actual account/MFA method; Graphit cannot infer a user's credential or treat any recovery string
as a valid challenge response.

Missing Browser configuration is headless state. Configure Global `task.browser` with trusted
Broker provider names and PROJECT authorization endpoint. Production uses valid HTTPS/public
origin; local HTTP requires explicit loopback exceptions. The current cookie is an opaque handle for server-owned in-memory state, not encrypted
access/refresh tokens. Restarting loses that session regardless of a configured cookie key. [Authentication](authentication.md)
explains callback, expiry, refresh and logout.

A valid JWT is not PROJECT permission. A 401 token/session error and a 403/denied project operation
are different. Code validates trusted JWTs locally until expiration, without mandatory UserInfo
per request. Account/claim/revocation changes may remain unseen until access expiry; do not invent
another local TTL, blacklist or per-call introspection as a workaround.

## Wrong settings or changes not taking effect

Ordinary precedence is Command > captured Environment > Project > Global > Distribution > absent
consumer default. False, zero and empty values do not fall through. Generic custom settings are
not implicit features; an environment name is applied only where captured. `config --global list`
shows stored Global values rather than effective project/env/command values.

Use the consumer's reload boundary. Global model-slot controls are observed and refresh model
selection; no blanket restart is needed for every selection. `ai.cli` is captured at Ask
attachment and requires reattachment/restart. Listener/Browser startup topology has its own
capture. Invalid winning typed data fails explicitly. See [key reference](../reference/configuration-keys.md).

## AST or Knowledge results missing

1. Confirm the authorized PROJECT/version or exact installed artifact. The same display name can
   identify different projects; a Hub selection has no invented local checkout.
2. Choose FTS/Source/schema to separate textual/structural availability from semantic coverage.
   Completed FILE/page publications are usable before the entire source or embedding pass finishes.
3. Check source/document root, extensions, ignore files, effective grammar/profile and custom image
   mapping. Unsupported compiler/syntax cannot produce facts just because a file exists.
4. Read the specific FILE/parse/size/integrity diagnostic. Fix authored input or the explicit
   configuration. A rejected oversized file is not permission to raise every project's limit.
5. A known file's Source can fall back to the selected authorized disk checkout even without a
   supported grammar; wrong/missing extents are not permission to guess another Source for rerank.

Unknown YAML profile fields are rejected for that file. The loader reports/skips an invalid
profile and captures layers on load; there is no two-second mtime cache guarantee to wait out.
Read query diagnostics as failures, not no-match evidence. Inspect the actual schema before Cypher.
[Indexing](indexing.md), [profiles](ast_extensibility.md) and [AST](../specs/ast_module.md) explain recovery.

## Docker compiler image cannot run

Maintained images use the Graphit SCIP GHCR families with a compatibility alias (`v1` default).
The alias is not their release version. `GRAPHIT_AST_SCIP_PULL_POLICY=always` is default;
`missing` pulls only if absent, and `never` cannot fetch an image that is not installed.
A configured custom image reference is passed unchanged and does not receive `ast.scip.version`.

Check Docker on PATH, daemon permission, registry login, exact image/tag/digest, platform support,
mount contract and the emitted `index.scip`. A compiler's classified unavailable/timeout outcome
can fall back only after its process/container owners are joined and Original/authority still
permit it. Authorization, integrity, parent cancellation/deadline and incomplete cleanup remain
terminal. No silent repair or infinite compiler retry loop is implied.

## Embedding or rerank is unavailable

FTS needs no model. A requested semantic/hybrid service failure is explicit, not a silent lexical
fallback. Confirm the active provider/account, service mode, discovery and vector-space identity.
Broker providers permit only Broker/disabled services; configuring local weights or a spare API
key cannot bypass an unavailable Broker advertisement. Remote modes need no local model process.

For Local inference, install/select the verified task manifest. EmbeddingGemma2 uses BF16 GGUF;
BGE V2 M3 rerank uses F16 GGUF. `graphit models install bge-reranker-v2-m3` downloads
the pinned public artifact and verifies its size and SHA-256. A prepared matching
`--file` remains available for offline installation; another model or precision
is rejected rather than silently substituted.
An explicit empty `daemon.models.slots` remains an opt-out until explicit selection.

`search.rerank` is false by default. Configuring a provider/model is not activation. Set an explicit
query false to isolate first-stage retrieval; disabled rerank must not discover/load/infer. An
invalid score/document association is an error rather than silent reordering of different results.
[AI guide](ai_models.md) contains exact provider/model commands.

## GPU selected but CPU or RAM is still visible

CPU is default independently for embedding/rerank. `auto` can legitimately choose CPU and must
report the selected device/fallback reason. Explicit GPU must fail if backend/driver/device is
unavailable. Check the complete package's accelerator files and host driver; container GPU access
also requires host runtime integration. Remote services choose their own hardware.

GPU offload does not eliminate CPU mappings, tokenizer, context, staging or metadata RAM. RSS is
not private exclusive memory. Compare private/PSS/mapped/shared memory and actual device telemetry
before calling retention a leak. Native model weights load only on effective use and unload after
300 seconds idle by default; active work/native cleanup completes first. The service process can
remain alive with unloaded weights. Cache hits do not extend model residency.

## High daemon/MCP memory or thread count

Distinguish per-process private memory, PSS, shared mappings and historical peak. Library/native
pages shared across processes must not be summed as each one's exclusive consumption. Thread count
alone does not identify the owning subsystem; compiler builds, model inference and query handles
have different owners.

`ast.query.buffer_pool_bytes=0` uses Ladybug's native maximum (about 80% system memory), not a
reservation of all pages, but opening a pool has metadata cost. Queries should reuse the existing
in-memory owner rather than create a large pool per retained project. The maximum is not a total
daemon budget. A too-small arbitrary pool can make a legitimate graph query fail; configure an
explicit value only for a concrete machine/query need and preserve clear allocation errors.
Do not add global quotas, purge every request or delete active stores as a memory workaround.

MCP's catalog/transport should retain only reusable data, release large call buffers normally and
preserve useful SDK reuse. An old RSS number alone proves neither a leak nor a fixed target.
Use an isolated client for diagnostics and preserve existing active connections.

## S3 credentials or data coherence

Read the actual provider/discovery selection. Valid storage-disabled discovery can select local;
authentication, denied scope/module, failed STS or unavailable S3 cannot silently create a local
replacement. PROJECT versions are native branches within the authorized table. USER Memory also
binds verified issuer/subject and installation. The Broker supplies credentials; data travels
straight between SDK and S3.

A valid cached STS session does not justify an indefinitely stale read snapshot. New operations
observe committed heads; an existing captured read/cursor can remain pinned. After a topology
change, do not reuse another URI's revision or infer the old local table is authoritative.
[Storage](../architecture/storage_layout.md) and [S3](s3-and-ui-network.md) explain renewal/snapshots.

## Task completion, claims and audit

Cancelled/completed descendants are terminal. Nonterminal work and the parent's own active checks
still govern completion; cancellation is not delivered functionality. Session/Now totals exclude
cancelled work but preserve its history. Do not delete or mark cancelled records completed to
manufacture a delivery count.

A stale revision/fence or expired lease requires a real current read and supported coordination.
Task and Session tokens are distinct and private. Current get does not load all history/comments;
request explicit pinned pages and continue to EOF. Invalid cursor/filter changes require a new
query, not concatenating unrelated generations. There is no generic batch/export/force-takeover
CLI alias. See [Session guide](task-sessions.md).

## Git snapshot or merge failure

Git post-event failures cannot undo an already created commit/checkout. Read the actual error,
resolve authorization, unsnapshotted edits or divergent record-family conflicts and retry the
supported `graphit git snapshot`/`restore` operation. Never move immutable tags, choose an arbitrary
merge base or fabricate historical state for pre-install commits. USER Memory is not included.
[Git records](../specs/git_records.md) defines rewrites, partial effects and recovery.

## Sync, modified agent files and disk growth

Use `graphit sync` for all installed exact artifact versions and native agent files, or its component
commands for a smaller scope. Hub update explicitly selects new remote releases. Foreign/user edits
produce conflicts under origin/version/hash ownership; restore the intended source/customization,
not a duplicate hidden payload cache. Then apply the host's documented trust/reload/new-session step.

Native maintenance compacts fragments/indexes and reclaims only unreferenced snapshots allowed by
branches/Git tags/active readers. Do not remove active Lance fragment files or Icebug shards by glob.
Build `target`/`.build` directories are development outputs, not installed authoritative corpora;
cleaning them can require recompilation and does not invalidate earlier evidence for unchanged code.
[Storage layout](../architecture/storage_layout.md) describes active/derived state and ownership.
