---
title: Capability and Surface Matrix
type: reference
updated: 2026-09-03
tags: [capabilities, cli, mcp, ui, modules, configuration]
---

# Capability and Surface Matrix

This matrix answers three practical questions: what Graphit implements, where a user or agent can
reach it, and what enables or disables it. A blank surface is intentional; it should not be inferred
from a similar command on another interface.

## Product capabilities

| Capability | CLI | MCP | Observatory | Main controls and boundaries |
|---|---|---|---|---|
| Setup, init, sync, update, remove | Yes | Yes | No | `ide`, `cli`, module switches, installed artifacts |
| Configuration read/write | Yes | Yes | No | Project/global scope, `-c`, mechanically derived environment variables |
| AST index and incremental watch | Yes | Yes for index | Explorer reads results | `modules.ast`, `modules.sync`, `ast.*`, ignore files, daemon activity window |
| AST full-text, semantic, and hybrid RRF search | Yes | Yes | Explorer search | Embedding configuration; lexical search does not require an agent CLI |
| Exact Cypher graph queries and schema | Yes | Yes | Explorer visualization | Natural-language Cypher generation is CLI/UI agent-dependent; exact Cypher is not |
| Indexed source reads and exports | Yes | Yes | Source details | `ast.index_source`; source omission removes this evidence surface |
| Knowledge indexing, lint, schema, and pages | Yes | Yes | Knowledge Explorer | `modules.knowledge`, `knowledge.*`, `.wikiignore` |
| Wiki full-text, semantic, hybrid, browse, xrefs, log, source | Yes | Yes | Knowledge and Memory explorers | Wiki retrieval is deterministic and agent-independent |
| AI wiki answers and chat sessions | Yes | No | Yes | Requires `modules.agent=true` and a supported agent CLI |
| Persistent project and user memory | Yes | Yes | Memory Explorer | `modules.memory`, `unit.id`, optional S3-compatible shared storage |
| Mandatory and important memory lifecycle | Yes | Yes | Read-oriented explorer | Mandatory rows load unconditionally before contextual search |
| Deterministic Task backlog and search | Yes | Yes | No | `modules.task`; authoritative LanceDB tables; `task.prefix` |
| Claims, leases, fencing, takeover, dependencies, subtasks | Yes | Yes | No | Mutations require current ownership and deterministic lifecycle gates |
| Acceptance/test checks, flags, audited revision, completion | Yes | Yes | No | Completion fails until required checks and child work satisfy the specification |
| Hub discover, show, install, update, submit, link, unlink | Yes | Yes | Registry and project artifacts | `modules.hub`, `hub.*`, version-pinned lockfile membership |
| Hub conventional type paths | Yes | Yes | Registry details | Artifact types define adapter destinations and reusable content |
| Bounded Hub artifact content reads | No | Yes | Registry details | Agents can read selected artifact source without downloading unrelated content |
| Live Search over selected artifacts | Yes | No | Yes | Requires `modules.agent=true`; uses an ephemeral workspace under the global sessions tree |
| Agent adapter synchronization and lifecycle hooks | Via init/sync | Via init/sync | No | `ide`, installed rules/skills/agents/commands/MCP/languages, `modules.*` mandates |
| Daemon status and stop | Yes | Yes | Daemon dashboard | `modules.daemon`, `mcp.*`, `daemon.activity_window` |
| Background filesystem synchronization | Daemon | Lifecycle sync only | Status | `modules.sync`, AST/Knowledge switches, ignores, extensions, watcher limits; 1 s/5 s event batching |
| Background embeddings | Daemon and explicit commands | Explicit embed tools | Status | `modules.embedding`, embedding provider, runtime resource variables |
| Dream status and reports | Yes | Yes | Dream dashboard | `modules.dream` is opt-in; Dream does not consume Task work |
| Ecosystem cluster labels and project lookup | Yes | Yes | Ecosystem view | Cluster labels are separate from AST node `ast.cluster_map` tagging |
| Daemon-hosted UI | Daemon | No | The hosted application | `modules.daemon_ui` is opt-in; `ui.host`, `ui.allowed_origins` |
| Standalone UI | `graphit ui` | No | The application | UI is unauthenticated; loopback is the default trust boundary |

## Retrieval stages

| Stage | Implemented | Used by current product entry points | Notes |
|---|---|---|---|
| BM25 full-text retrieval | Yes | Yes | AST, Knowledge, Memory, Task, and multi-wiki surfaces use LanceDB-backed lexical ranking where applicable. |
| Semantic vector retrieval | Yes | Yes | Available after embeddings exist; small stores can scan without a trained vector index. |
| Hybrid reciprocal rank fusion | Yes | Yes | Combines lexical and semantic rankings with deterministic tie behavior. |
| Bounded source selection | Yes | Yes | Graphit searches for candidates, then reads selected code or wiki sources with slices. |
| Exact structural traversal | Yes | Yes | Cypher answers relationships after discovery; it is not a text-search substitute. |
| Cross-encoder/remote second-stage reranking | Yes, as pipeline and providers | Not yet attached by CLI, MCP, or UI search call sites | `search.rerank` and `ai.rerank.*` configure integration-ready infrastructure; current product retrieval stops after hybrid RRF. |
| Agent synthesis | Yes | CLI/UI surfaces only | Requires `modules.agent=true`; synthesis is not part of deterministic retrieval scoring. |

## Collaboration and multi-system model

| Scope | Durable identity | Shareable state | Isolation rule |
|---|---|---|---|
| Project | Lockfile ULID | Artifacts, project memory, and Task tables can use S3-compatible storage | Project config and membership remain in its lockfile. |
| User | `unit.id` | User memory can follow a trusted identity across machines | User conventions remain separate from project facts. |
| Agent session | Host session ID or `GRAPHIT_AGENT_SESSION_ID` | Task audit records attribution and current ownership | A stale fencing token cannot mutate current work. |
| Imported system | Context name or versioned Hub artifact identity | Code graphs and knowledge can be mounted or queried without repository copies | Access is explicit through lockfile membership or qualified Hub reference. |
| Live Search session | Ephemeral project ID | Selected artifacts and event history exist only for the session | It owns no persistent project graph, wiki, or project-memory scope. |

For key-level behavior, continue with the [Configuration Reference](configuration.md). For every
file and watch boundary, see [Filesystem, State, and Watchers](filesystem_contract.md). For language
and parser extension behavior, see [AST Grammars and Parser Extensibility](ast_extensibility.md). For
tool parameters, see the [MCP Tools Reference](mcp_tools_reference.md) and
[CLI Reference](cli_reference.md).
Provider/model behavior is catalogued in [AI Models, Providers, and Agent CLIs](ai_models.md), and
the complete background lifecycle is in [Daemon Operations and Monitoring](daemon_operations.md).
