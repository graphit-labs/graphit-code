---
title: Rust product architecture and delivery
description: Maintained boundaries, product contracts and validation commands for Graphit Rust.
content-type: guide
audience: developers
references:
  - target: {type: session, id: ses-c98a}
    relation: relates_to
  - target: {type: task, id: tsk-03ae}
    relation: implements
---

# Rust product architecture and delivery

Graphit uses Rust for its first-party services, CLI and tooling. The React and
TypeScript interface lives in `ui/`. Generated parsers and native engines keep
their upstream languages behind small typed integrations. Product contracts
live in the domain pages below; work state and execution evidence live in Task.

## Source and daemon

The [Source entrypoint](../specs/rust_source_entrypoint.md),
[foreground composition](../specs/rust_source_foreground.md),
[watchers](../specs/rust_daemon_watchers.md) and
[user services](../specs/rust_daemon_services.md) define startup, multi-project
selection, incremental work, recovery and joined shutdown. Each project/version
retains its own authority and observed state. Cancellation, process termination
and drain failures remain real failures.

The [Now contract](../specs/rust_now_presence_integration.md) covers instance
registration, selected project versions, realtime observations and CLI/MCP
consumers. A sent update is not a Broker acknowledgement. Authentication and
module permissions remain independent of discovery and presence.

## Core and product contexts

[Filesystem state](../guides/filesystem_contract.md),
[project identity](../specs/project_identity.md),
[configuration](../specs/config_module.md) and
[ignore rules](../guides/ignore_files.md) define immutable contexts, native
paths, private writes and captured selection. Identity and version identify
project data across checkouts. Each installation resolves local roots at runtime.
Reads fail on missing or corrupt state when the contract requires that state.

[Git](../specs/git_module.md) uses the system Git executable for checkout facts
and configured hooks. [Output](../specs/output_module.md) separates structured
encoding, terminal presentation, diagnostics, file sinks and tail readers.
Domain operations do not write protocol stdout directly.

## AST and storage

[AST](../specs/ast_module.md) uses generated ANTLR C++ parsers/runtime through
`cxx`, retained Tree-sitter grammars and native semantic consumers. Maintained
query profiles are under `crates/graphit-ast/profiles/queries/`. The native
storage/query boundary is specified in [Icebug](../specs/rust_icebug_storage.md).

Cargo resolves the official Lance and Ladybug crates. The build prepares Ladybug
from the crate's sources and shares that build between the binding and the C++
adapter. See [native dependencies](rust-forks.md) and
[runtime packaging](native-runtime-packaging.md).

[Knowledge](../architecture/rust-knowledge-semantic-data.md) processes documents
incrementally and releases file bodies after their consumer finishes. Search
and context composition retain their selected publication and authority. AST
source reads can fall back to disk at the exact authorized project/version
path, including unsupported extensions; no Git operation is needed for that
fallback.

## Records, agents and interface

[Task](../specs/task_module.md), [sessions](../specs/task_sessions.md),
[Memory](../specs/rust_memory_records.md) and [Dream](../specs/dream_module.md)
retain current authorization, CAS, fencing, history and honest mutation effects.
Completed and cancelled children are terminal; pending descendants still block
completion, and each parent must satisfy its own criteria.

[Instruction templates](../architecture/rust-agent-instruction-templates.md)
centralize Markdown policy, skills, roles and hook text. Callers supply the
effective brand, artifact, agent and tool names. Native bindings follow Kiro V3,
OpenCode 2 and AGY contracts. Compatibility tests use controlled peers and real
Graphit consumers without requiring a vendor binary, account or paid plan.

The interface uses the same authorized HTTP services as the other consumers;
see [foreground Task](../specs/rust_foreground_tasks.md),
[Memory](../specs/rust_foreground_memory.md) and
[Knowledge](../specs/rust_foreground_knowledge.md). Task Gantt uses explicit dates.

## Validation and release

Run the suites affected by the change and necessary consumers. Preserve valid
results for unchanged code. Keep regressions for authorization, integrity,
concurrency, cancellation, actual process retirement and concrete input/output
limits. Remove tests exclusive to mechanisms that no longer exist.

```text
cargo xtask check
cargo xtask test PACKAGE/TARGET ...
cargo xtask gate
```

The selector arguments must be actual entries in the maintained catalog.
Pre-commit checks staged whitespace, workflow syntax, native safety and
formatting. Pre-push adds strict workspace Clippy and the locked `xtask` tests.
CI runs the full integration gate. The
[hook/CI contract](rust-fast-guard.md) defines their executable commands.
A successful local check does not establish native execution on another OS or
hosted CI. Artifact generation, installation and real consumers are separate
from a library build; see [runtime packaging](native-runtime-packaging.md).

Efficiency focuses on avoiding repeated work, redundant network calls, copies
and retained corpus/model memory. Do not add global quotas, resource ledgers,
reservation frameworks, universal allocation proofs or benchmark campaigns.
Measure when a concrete problem or design decision needs it. Preserving product
functionality does not require preserving retired internal APIs or wire layouts.
