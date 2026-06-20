# Task: Expand mem_rule — "user explains" trigger (rule.go)

## Date
2026-06-20

## Objective
Make it explicit in the `mem_rule` (inside `AGENTS.md`) that whenever a user explains **anything** to the agent — not just procedures or tips — the agent must create a memory immediately.

## Problem
The previous save trigger was:
> `User explains a procedure or gives a tip → store as skill`

This was too narrow. It only covered explicit "how to do X" explanations. The user pointed out that any explanation — how something works, why a decision was made, architecture context, domain knowledge, constraints — should also trigger memory creation.

## Changes

### `AGENTS.md`
- **Save trigger** (line ~142): Expanded from "User explains a procedure or gives a tip" to:
  > **User explains ANYTHING** — a procedure, tip, how something works, a decision rationale, architecture detail, domain knowledge, a constraint, or any context about the project or system → **memorize immediately as a skill, decision, or context entry**

- **Key Rules** (line ~167): Added a new explicit rule:
  > **If the user explains something — anything — memorize it.** This includes: how a system works, why a decision was made, what a component does, domain rules, architectural constraints, or any knowledge the user shares. Do NOT assume you will remember it. Create the memory immediately.

## Impact
Agents using this mandate will now recognize any user explanation as a memory trigger, preventing knowledge loss across sessions.
