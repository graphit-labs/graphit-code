---
title: Rust Gemini completion stream
type: specification
updated: 2026-10-16
tags: [ai, rust, gemini, streaming, process]
references:
  - target: {type: task, id: tsk-a9c4}
    relation: supports
  - target: {type: task, id: tsk-658c}
    relation: corrected_by
  - target: {type: task, id: tsk-bcf2}
    relation: uses
  - target: {type: session, id: ses-c98a}
    relation: relates_to
  - target: {type: task, id: tsk-247e}
    relation: implemented_by
  - target: {type: task, id: tsk-08c0}
    relation: derived_from
  - target: {type: task, id: tsk-e6ff}
    relation: implemented_by
  - target: {type: task, id: tsk-a6e8}
    relation: uses
  - target: {type: task, id: tsk-8d0c}
    relation: uses
---

# Retired Gemini CLI integration

The Gemini CLI agent, its JSONL drivers, Ask/Conversation consumers and exclusive tests have been removed from the current Rust product. Earlier receipts remain historical; they do not qualify Antigravity.

Use the supported [Antigravity agent](rust_antigravity_completion.md) through `agy`. Google Gemini API providers, models and embedding protocols remain supported and independent of CLI selection. No alias, data importer or session migration is provided.

## Incoming Conversation continuation

The former Gemini CLI continuation is retired with the agent.
