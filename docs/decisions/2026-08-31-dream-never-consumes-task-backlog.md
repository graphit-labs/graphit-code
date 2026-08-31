---
title: "Dream Never Consumes the Task Backlog"
status: accepted
date: 2026-08-31
tags: [dream, backlog, knowledge, architecture]
---

# Dream Never Consumes the Task Backlog

## Context

Dream exists to improve project knowledge during idle periods. It mines conversations, consolidates
memories, evaluates agent artifacts, and records knowledge-improvement reports. The task backlog is
a documentation-backed registry of tasks that should survive the conversation that identified them.

An earlier implementation connected these separate concerns: Dream selected the oldest pending
backlog item, embedded it in the agent prompt, exposed it through status APIs, and interpreted a
`.done.md` file as completion. That made an autonomous knowledge process behave like a task worker
and leaked the coupling into the CLI, MCP, HTTP API, UI, tests, rules, and guides.

## Decision

Dream never reads, selects, executes, completes, reports, or exposes task-backlog items.

Dream inputs are project knowledge, persistent memories, conversation history, existing agent
artifacts, consolidation outcomes, and prior Dream reports. Its outputs are improved knowledge
artifacts and a Dream session report.

The backlog provides only task registration operations: add, list, and remove. It has no pending,
picked, done, or result state. Backlog HTTP routes live in a neutral handler, and backlog controls
are not part of the Dream dashboard.

## Consequences

- A backlog item remains unchanged across Dream sessions.
- Dream status and prompts never contain backlog data.
- A user or agent performs backlog work through an explicit task workflow and records that work in
  the normal task log.
- Legacy `.done.md` files are ignored so they cannot revive completion semantics.
- Tests and generated guidance assert the strict boundary, not merely that Dream is optional.

## Alternatives Rejected

- **Dream as an optional backlog consumer.** Rejected because optional consumption still conflates
  knowledge improvement with task execution and keeps the runtime contract ambiguous.
- **Keep pending/done fields for another future executor.** Rejected because no current executor owns
  that lifecycle; retaining it would preserve a misleading scheduler contract in a task registry.
