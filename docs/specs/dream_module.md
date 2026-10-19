---
title: "Dream Module Specification"
description: "Technical contract for idle-triggered, agentic Memory consolidation."
content-type: reference
audience: developers
updated: 2026-09-23
keywords: [dream, idle, memory, consolidation, agent, mcp, lancedb]
prerequisites:
  - "docs/architecture/architecture_overview.md"
related:
  - "docs/specs/memory_module.md"
  - "docs/guides/ai_models.md"
  - "docs/guides/mcp_tools_reference.md"
---


# Dream Module Specification

Dream consolidates admitted Memory through a constrained proposal/application
flow. The model proposes domain operations; Records and Auth determine whether
those operations are valid and actually commit. Model text, an observed tool
call or a successful child exit is not proof of a Memory mutation.

## Responsibilities

[PROJECT Dream](rust_project_dream.md) composes the Source activity signal,
existing process owner, instruction renderer and Records application. The
[Dream records contract](rust_dream_records.md) defines paired admission,
Memory preconditions, fencing and recovery. A distinct Coordinator USER lane
uses its own verified subject and authorization; PROJECT access cannot grant it.

The [consolidation prompt](../architecture/rust-dream-instructions.md) is a
maintained Markdown template. Dream does not create a second task manager,
watcher, semantic report store or mandatory operational ledger.

## Idle activity and ownership

Human activity is signalled by the configured authenticated host ingress and
selected project/version. Filesystem changes are not human activity. The
foreground serializes the selected Dream execution and retains its actual
process and writer ownership through cancellation, deadline and shutdown.
Dream disabled or unavailable does not cause a background fallback to another
provider or project.

## Agent authority

The PROJECT Dream role has authorized Memory reads and fenced Memory mutations:
insert, update, delete, promote and demote, each with its concrete domain
preconditions. Task/session operations, claims, Knowledge, AST, Hub, Git,
shell/native edits and indirect prohibited handlers are outside that role.
Tool discovery and direct invocation enforce the same policy, including aliases.
USER work requires the separate USER lane and current USER admissions.

Native tool restrictions do not create an OS sandbox or replace Graphit
permissions. Existing user trust settings are not modified to make a test pass.
Controlled integration peers do not require a vendor installation, account or
paid model plan.

## Memory integrity and recovery

Current revision, target identity, expected content and real write authority
must match the operation contract. Failed archive-before-delete leaves the
live record. CAS mismatch requires a fresh read rather than blind retry.
Automatic folding preserves the native winner/orphan, history and recovery
rules; see [Memory records](rust_memory_records.md).

Confirmed partial effects remain confirmed when a later step fails. Recovery
reobserves the actual durable decision under current authority; it does not
invent rollback, erase history or count an attempted call as a committed effect.
There is no data importer or old state-file migration.

## Product consumers

The [foreground caller](rust_project_dream.md) and
[Source composition](rust_source_foreground.md) define opt-in configuration,
activity, startup and drain. Their execution status is maintained in Task;
this contract does not assert native execution on an untested platform.
