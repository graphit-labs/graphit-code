---
title: Capability and Surface Matrix
type: reference
updated: 2026-10-29
tags: [capabilities, cli, mcp, ui, modules, configuration]
references:
  - target: {type: task, id: tsk-4b26}
    relation: supports
  - target: {type: task, id: tsk-a2bb}
    relation: supports
  - target: {type: task, id: tsk-97d9}
    relation: relates_to
  - target: {type: task, id: tsk-6b5f}
    relation: relates_to
  - target: {type: task, id: tsk-9a3d}
    relation: relates_to
  - target: {type: task, id: tsk-c3c2}
    relation: relates_to
---

# Capability and surface matrix

This matrix describes the current Rust product. Exact commands/options and MCP fields are listed
in [CLI](cli_reference.md), [complete CLI options](../reference/cli-options.md) and
[MCP](mcp_tools_reference.md). Source support, an installed Linux proof and published native
Windows/macOS qualification are separate claims.

| Capability | CLI | Root MCP | Browser / HTTP | Activation and boundary |
|---|---|---|---|---|
| Project setup/inventory | `setup`, `projects` | No generic init tool | Authorized inventory/selection | Stable PROJECT ID; real checkout/version |
| Configuration | `config set/get/list/unset` | No generic config tool | Consumed by services | Captured layers and typed domain validation |
| AST full/incremental publication | Maintained daemon jobs | No generic index tool in current Root catalog | Reads progressive publications | SCIP standard, Docker specialization, syntax fallback |
| AST search/schema/query/source | `ast` commands | Yes | AST Explorer | PROJECT/exact installed context; Source fallback confined to authorized checkout |
| Knowledge compilation/history/source/search/export | Daemon and `knowledge` | Search/source/log/export | Knowledge Explorer | Authored sources; one searchable dataset with progressive vectors |
| Knowledge Ask | `knowledge ask` | `graphit_knowledge_ask` | Ask/Conversation consumers | Captured CLI binding and literal requested model |
| Memory search/index | `memory index`; search via MCP/Browser | Search/index only | Memory CRUD/history/search | Verified PROJECT or independent USER scope |
| Task/Session lifecycle | Flat `task` commands | Yes | Task/Session/Gantt | Real claims/fences, checks, hierarchy and authorization |
| Task search/query | `task search/query/schema` | Yes | Text/Semantic/Hybrid search | FTS default; partial compatible vectors; no Session text search |
| Task commit metadata | `task set-task-commit` | `graphit_task_commit_set` | Supported Task detail/write consumers | Optional full actual SHA, preserves status/checks |
| Git record snapshots/merge | `git install/remove/snapshot/restore` | No generic Git mutation in Root | Authenticated hook consumer | Git available; PROJECT Task/Memory only |
| Hub authored discovery/publication | `hub authored`, publication commands | Yes | Hub readers/mutations | Producer PROJECT/version admission and exact artifact tuple |
| Hub installation/recovery/update | `hub` / `package` | Yes | Supported FILE/Native workflows | Exact origin/version; user edits protected; S3/current grants |
| General artifact/agent sync | `sync` and component sync commands | `graphit_sync` | No equivalent arbitrary Web administration | Administrative Runtime; no silent artifact upgrade |
| Agent hooks/roles/templates | `coordinator`, `hooks`, `roles`, `skills` | Administrative sync; Role catalog separate | Native host files | Kiro V3, OpenCode 2, AGY and maintained bindings |
| Model installation/selection | `models install/select` | No download/install tool | Readable runtime state | Explicit verified GGUF artifact; no local work for remote modes |
| Local embedding | Background/query consumers | Via retrieval | Same consumers | EmbeddingGemma2 BF16; CPU default; independent device |
| Local rerank | Explicit search/config choice | AST/Knowledge search | Same search consumers | BGE V2 M3 F16; **off by default** |
| Remote inference | Provider/account commands | Via authorized retrieval | Same consumers | Broker provider accepts Broker/disabled only |
| Now/instances | `now` | `graphit_now`, `graphit_instances` | Realtime Now | Actual instance/project/version registration |
| UI and Linux tray | `ui`, `tray` | No generic desktop opener | Embedded Browser | Browser configured with verified Broker identity; tray needs desktop bus/host |
| User service | `daemon` | No arbitrary service-control tool | Authenticated local control/status | Native per-user manager; login startup explicit |
| Dream | Daemon configuration | Scoped Role tools during run | Operational state where exposed | Module plus `dream.enabled`; configured program/model; Memory-scoped work |
| Live | Owned HTTP/Browser lifecycle | History/control and bound tools | Live workspace/events | Explicit selected artifacts, USER/PROJECT grants and native CLI |

## Defaults and opt-ins

Core module switches are enabled by default except `modules.dream` and `modules.daemon_ui`.
These switches are orchestration inputs, not access grants or a replacement for required
configuration. Browser availability requires `task.browser`; setting a module alone does not
create an OAuth client/session. Dream also needs `dream.enabled`, model and program.

`modules.embedding=false` opts out of embedding work while preserving CRUD/FTS. Configuring a
model/provider does not enable rerank; `search.rerank` defaults false. CPU is default for each
local purpose. Installing accelerator libraries does not change it. Explicit unavailable GPU
fails; auto reports chosen backend and fallback. Remote services choose their own hardware.

General sync preserves exact installed versions. Hub update is explicit. New and changed data is
available as its safe FILE/record publications commit, before all embeddings finish. Versions use
native dataset branches; S3 accesses the authoritative table directly. [Configuration](configuration.md),
[indexing](indexing.md) and [storage](../architecture/storage_layout.md) detail these rules.

## What this matrix does not imply

A missing surface is not a promised alias: current Root MCP has no generic Memory CRUD tool,
Task batch/export, force-takeover, Session text search or configuration writer. UI downloads can
assemble explicit pinned pages without inventing a separate export endpoint. Role/Live catalogs
are governed by actual credentials, not `ReadOnlyHint` descriptions. Tests use documented
controlled peers and do not require vendor executables, accounts or plans.

Native Windows/macOS jobs and release publication must retain their actual execution status.
Prepared CI, cross-compilation or a Linux run does not certify an unexecuted platform.
