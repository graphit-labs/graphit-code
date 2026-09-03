---
title: "Dream Never Consumes the Task Backlog"
status: amended
date: 2026-08-31
tags: [dream, backlog, knowledge, architecture]
---

# Dream Never Consumes the Task Backlog

## Context

Dream exists to improve project knowledge during idle periods. It mines conversations, consolidates
memories, evaluates agent artifacts, and records knowledge-improvement reports. Project work now
lives in Graphit Task's authoritative LanceDB tables; open, unclaimed tasks are backlog.

An earlier implementation connected these separate concerns: Dream selected the oldest pending
backlog item, embedded it in the agent prompt, exposed it through status APIs, and interpreted a
`.done.md` file as completion. That made an autonomous knowledge process behave like a task worker
and leaked the coupling into the CLI, MCP, HTTP API, UI, tests, rules, and guides.

## Decision

Dream never reads, selects, executes, completes, reports, or exposes task-backlog items.

Dream inputs are project knowledge, persistent memories, conversation history, existing agent
artifacts, consolidation outcomes, and prior Dream reports. Its outputs are improved knowledge
artifacts and a Dream session report.

Task owns create/search, dependency and subtask relationships, atomic claims, progress, checks,
comments, flags, release/takeover, completion, and audit state. Task controls are not part of the
Dream dashboard or Dream execution cycle.

## Consequences

- Task state remains unchanged across Dream sessions.
- Dream status and prompts never contain backlog data.
- A user or agent performs work through Graphit Task rather than Markdown or a host-native task
  mechanism.
- Legacy Markdown backlog/task files have no runtime semantics.
- Tests and generated guidance assert the strict boundary, not merely that Dream is optional.

## Alternatives Rejected

- **Dream as an optional backlog consumer.** Rejected because optional consumption still conflates
  knowledge improvement with task execution and keeps the runtime contract ambiguous.
- **Keep a registration-only backlog.** Superseded because multi-agent takeover requires an explicit,
  deterministic lifecycle with ownership and validation state.
