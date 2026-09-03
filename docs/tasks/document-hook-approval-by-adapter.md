---
title: "Document hook approval by adapter"
content-type: task
status: completed
updated: 2026-09-02
tags: [documentation, adapters, hooks, trust]
---

# Document hook approval by adapter

## Objective

Make the user-facing hook activation guide answer explicitly whether approval is required for every supported agent, when that approval or activation must be repeated, and the exact action and verification path for each adapter.

## Reasoning and approach

The current guide contains the facts inside prose, but it makes users compare seven sections to discover that approval, workspace trust, enablement, and reload are different controls. Add a summary matrix first, then give every adapter the same four fields: approval, when to repeat it, action, and verification. Preserve vendor-specific limitations instead of implying a uniform hook security model.

Primary vendor documentation was checked for Codex, Claude Code, Cursor, Gemini CLI, Kiro, Google Antigravity, and OpenCode. Project knowledge confirms that Graphit cannot self-approve project hooks and that MCP visibility remains independent from hook execution.

## Task breakdown

- [x] Confirm the current Graphit hook activation guidance and architecture decision.
- [x] Verify current trust, enablement, reload, and cloud behavior in primary vendor documentation.
- [x] Add the approval matrix and normalize the per-adapter instructions.
- [x] Validate Markdown and synchronize project knowledge.

## Acceptance criteria

- The guide opens with an unambiguous answer that approval is not required in every agent.
- Every supported adapter states whether approval is required, when it is required again, how to perform it, and how to verify the result.
- Codex content-hash trust is distinguished from Claude Code and Gemini workspace trust.
- Adapters without a documented approval prompt are clearly separated from adapters that require trust.
- Cursor Cloud's missing `sessionStart` is described as a capability limit, not an approval problem.
- Documentation validation passes and the knowledge index is synchronized.

## Affected files

- `docs/guides/agent_hook_activation.md` — add the comparison matrix and consistent per-adapter procedures.
- `docs/tasks/document-hook-approval-by-adapter.md` — record scope, evidence, progress, and validation.

## Tradeoffs and debt

- Host behavior can change independently of Graphit. The guide links primary vendor documentation and must be reviewed when an adapter changes its hook lifecycle or trust model.
- “No separate approval documented” is intentionally narrower than claiming a host can never add an operating-system, enterprise-policy, or future product prompt.

## System knowledge

- Hook trust, workspace trust, hook enablement, process reload, and MCP tool visibility are independent runtime gates.
- Only Codex among the currently supported adapters documents trust tied to the exact non-managed hook definition; changing that definition invalidates the stored trust.
- Claude Code interactive sessions gate settings-file hooks on workspace trust. Gemini CLI does so only when its Trusted Folders feature is enabled.
- Cursor Local, Kiro, Google Antigravity, and OpenCode do not currently document a separate per-hook approval step for the project integrations Graphit generates.

## Progress

- Reopened after user correction: Codex hook trust must be described explicitly as a Codex CLI operation, and every adapter must retain any concrete action the user needs to perform.
- Updated the Codex matrix entry and detailed procedure to require launching `codex` in a terminal, using `/hooks`, approving both Graphit hooks, and starting a new task.
- Made Cursor's enable/restart path and Antigravity's `enabled` correction explicit; the existing Claude Code, Gemini CLI, Kiro, OpenCode, and Cursor Cloud sections already contained their required user actions.
- Updated the durable adapter-ownership memory so future documentation keeps Codex hook approval tied explicitly to the Codex CLI `/hooks` flow.
- Added a single comparison matrix covering all seven adapters plus the distinct Cursor Cloud case.
- Normalized every adapter section to state approval, repeat conditions, exact user action, and verification separately.
- Added the Claude Code non-interactive trust nuance, Gemini's default-disabled Trusted Folders nuance, Kiro's enable/disable controls, and Antigravity's `enabled: false` behavior from current primary documentation.
- `git diff --check` passed for the documentation changes.
- `graphit_knowledge_sync` completed and the refreshed wiki exposes this task. `graphit_knowledge_lint` also completed; it reported the repository's existing orphan, broken-link, and stale-page backlog, including the already-existing unresolved `agent_hook_activation` wiki link from Getting Started, with no edit-specific parse failure.
