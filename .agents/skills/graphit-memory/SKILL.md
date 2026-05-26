---
name: graphit-memory
description: Persistent memory across sessions. MANDATORY: Read memory indexes at the START of every conversation before responding. Use when the user corrects you, teaches you something, or when you make design decisions. Also read memory when stuck or when implementing significant changes to check for prior constraints.
---

# Memory Management Rule

> This rule is auto-managed by Graphit Code: AI Harness for Collaborative and Progressive Knowledge. Do not edit this block manually.

## 🚨 SESSION START PROTOCOL — Execute BEFORE Any Response

**These steps are MANDATORY. Execute them BEFORE responding to the user's first message.**
**Skipping them means you WILL repeat mistakes the user already corrected.**

1. Read `.graphit/memory/project/index.md`
2. Read `.graphit/memory/user/index.md`
3. If either wiki has memories, scan titles for anything relevant to the user's request
4. If relevant memories found, read the entity page(s) and follow their guidance
5. When you act based on a memory, tell the user: "Following memory: '<title>'"
6. Only then proceed with the user's request

> If a wiki `index.md` does not exist (new project), skip that scope and proceed.


## 📖 When to Read Memory (Beyond Session Start)

**Memory reading is NOT limited to session start.** You MUST also consult memory
in these situations — proactively, without being asked:

| Situation | What to search for | Why |
|---|---|---|
| **Before implementing** any significant change | Conventions, decisions, constraints about affected modules | Avoid violating established patterns or repeating rejected approaches |
| **When stuck** or your approach isn't working (2+ failed attempts) | Skills, workarounds, corrections related to the problem | The solution may already exist — you may be repeating a known mistake |
| **Non-obvious error** or unexpected behavior | Facts, skills, debugging steps about the affected area | Someone (you or another agent) may have debugged this before |
| **Trying to understand** "why is it done this way?" | Decisions, tensions, facts about the module/pattern | The rationale is captured in memory — don't guess |
| **Multiple attempts failing** (3+ tries on same problem) | Corrections, skills, workarounds | STOP and read before trying again — you are likely repeating a known error |
| **Before proposing** architecture or technical approach | Decisions, tensions, conventions that constrain options | Avoid contradicting prior decisions the user already made |
| **User seems frustrated** or repeats an instruction | Corrections about your behavior | You may be ignoring a correction already memorized |

**How to search:** `graphit memory search "<relevant keywords>"`

## 🔒 This Framework IS Your Memory — No Other Exists

- **ALWAYS** use `graphit memory insert` to persist facts across sessions.
- **NEVER** use IDE-native memory, model memory, or any built-in "remember" feature.
- **ALWAYS** read the memory wiki to recall — never rely on what you "think you know".
- This applies to ALL persistent facts: conventions, corrections, decisions, preferences.

## 🎯 Trigger → Action Table

**When you observe a trigger, execute the corresponding action immediately.**
**Do NOT ask for permission. Do NOT skip. Confirm with: "Done, I've memorized: <title>"**

### 💾 Save Triggers — Memorize Immediately

**Every memory MUST follow the structured content template below (What/Why/How/Impact).**
This ensures memories are complete and actionable across sessions.

#### 📝 Memory Content Template

When creating a memory, always include these four fields in `--content`:

```
What: <what was done or what happened>
Why: <why it was done — the motivation, root cause, or user intent>
How: <how it was resolved — the approach, steps taken, or implementation>
Impact: <how it impacted the system — side effects, files changed, behavior changes>
```

#### Trigger Table

| You observe... | Action | Command |
|---|---|---|
| User says "always/never/prefer/avoid/must" about code | Store as convention | `graphit memory insert "<rule>" --type convention --important` |
| User corrects your behavior or approach | Store the correction | `graphit memory insert "<correction>" --type correction --important --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| User **guides, orients, or gives direction** on how to proceed | Store the guidance | `graphit memory insert "<guidance>" --type convention --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| User **intervenes** mid-task to change course or redirect | Store the intervention as correction | `graphit memory insert "<intervention>" --type correction --important --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| User gives a **tip, hint, or suggestion** on how to do something | Store as skill | `graphit memory insert "<tip>" --type skill --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| User **explains how something works** or why it's done a certain way | Store as fact | `graphit memory insert "<knowledge>" --type fact --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| User **shows you a workflow** or operational procedure (e.g., "run make install first") | Store as skill | `graphit memory insert "<procedure>" --type skill --important --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| User **repeats an instruction** they already gave (frustration signal) | Store as correction | `graphit memory insert "<what you missed>" --type correction --important --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You **complete a task** (new feature, refactor, or significant change) | Record what was done | `graphit memory insert "<task summary>" --type fact --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You **modify an existing feature** (behavior change, extension, or rework) | Record the modification | `graphit memory insert "<feature> modified: <summary>" --type fact --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You **fix a bug** | Record the bug fix | `graphit memory insert "Bug fix: <summary>" --type skill --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You make an architectural/design choice | Record the decision | `graphit memory insert "<decision>" --type decision --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You chose X over Y with explicit trade-offs | Capture the tension | `graphit memory insert "<choice>" --type tension --content "Chose: X\nOver: Y\nBecause: ...\nAccepting: ...\nImpact: ..."` |
| You **discover something unexpected** during investigation | Store the discovery | `graphit memory insert "<discovery>" --type fact --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You find a **workaround or creative solution** to a problem | Store the workaround | `graphit memory insert "<workaround>" --type skill --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You make a **non-obvious inference** that proves correct | Store the inference | `graphit memory insert "<inference>" --type skill --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You **solve a complicated multi-step problem** | Store the full solution | `graphit memory insert "<problem → solution>" --type skill --important --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You debug a non-obvious issue successfully | Save as a skill | `graphit memory insert "<solution>" --type skill --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| User reveals a non-obvious project fact | Store the fact | `graphit memory insert "<fact>" --type fact --content "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| New instruction contradicts existing memory | Replace the memory | `graphit memory delete <old-id>` then `graphit memory insert "<new>"` |
| Memory is >30 days old and still relevant | Refresh it | `graphit memory update <id> --content "<refreshed>"` |

### 📖 Read Triggers — Consult Memory Before Acting

| You observe... | Action | What to search for |
|---|---|---|
| You're about to **implement a significant change** | Read memory BEFORE coding | Constraints, conventions, decisions about affected modules |
| You're **stuck or your approach isn't working** (2+ failed attempts) | Read memory for past solutions | Skills, workarounds, debugging steps related to the problem |
| You encounter a **non-obvious error or behavior** | Read memory for known issues | The problem may have been solved before — check skills and facts |
| You need to understand **project context or "why"** | Read memory for institutional knowledge | Decisions, tensions, and facts about the area you're working on |
| **Multiple attempts have failed** (3+ tries on same problem) | STOP and read memory | You may be repeating a known mistake — check corrections and skills |
| You're about to **propose architecture or an approach** | Read memory for prior decisions | Decisions, tensions, conventions that constrain options |
| Build/test fails unexpectedly | Check memory for known issues | Read wiki for past debugging skills |

## 📁 Memory Types

Every memory has a `type` that determines how it is stored and surfaced:

| Type | When to use | Typical importance |
|---|---|---|
| `convention` | Coding standards, style patterns, project rules | ✅ important |
| `correction` | User corrected your behavior — never repeat the mistake | ✅ important |
| `decision` | Architectural or design decisions with rationale | depends |
| `tension` | Trade-off choices: chose X over Y because Z, accepting W | depends |
| `fact` | Non-obvious project facts, environment details | rarely |
| `skill` | Debugged workflows, reusable solution patterns | rarely |

Default type when `--type` is omitted: `fact`.

## 📖 How to Retrieve Memories

Read the wiki files directly — you have full file access:

| Scope | Path |
|---|---|
| **project** | `.graphit/memory/project/index.md` |
| **user** | `.graphit/memory/user/index.md` |

**Retrieval steps:**
1. Read `index.md` — scan the catalog (grouped by type: conventions, corrections, decisions...)
2. Read the entity page for relevant memories
3. Check `## Backlinks` for related memories
4. **Never** grep raw memory files — the wiki is pre-compiled and faster

## 📋 CLI Quick Reference

```bash
# Insert (default: project scope, type: fact)
graphit memory insert "<title>" --content "<body>" --type <type>

# Insert important convention
graphit memory insert "<title>" --content "<body>" --type convention --important

# Insert correction
graphit memory insert "<title>" --content "<body>" --type correction --important

# Insert with tags
graphit memory insert "<title>" --content "<body>" --tags "auth,security"

# Insert user-scoped memory
graphit memory insert "<title>" --user

# Update existing memory
graphit memory update <id> --content "<new body>"

# Delete
graphit memory delete <id>

# Search (lightweight, no AI)
graphit memory search "<term>"

# Promote/demote importance
graphit memory promote <id>
graphit memory demote <id>

# List all
graphit memory list
graphit memory important

# Maintenance (run periodically or when memory feels cluttered)
graphit memory consolidate          # AI-driven: find duplicates, contradictions
graphit memory consolidate --apply   # auto-apply safe suggestions
graphit memory gc                    # find stale/empty memories (dry-run)
graphit memory gc --dry-run=false    # delete GC candidates
```

## 🔄 Contradiction Protocol

When the user's new instruction contradicts an existing memory:

1. Read the wiki index to find the contradicted memory
2. Delete it: `graphit memory delete <id>`
3. Create the replacement: `graphit memory insert "<new>" ...`
4. Confirm: "Updated memory: removed '<old title>' and saved '<new title>'"

## 📣 Transparency Rules

- **Always** tell the user when you act based on a memory: "Following memory: '<title>'"
- **Never** ask permission to create memories — just save and confirm.
- **Always** confirm memory operations: "Memorized: '<title>'" or "Removed memory: '<title>'"

## ⚡ Reindex After Writes

After any write (`insert`, `delete`, `update`, `promote`, `demote`), the auto-cycle
runs automatically. If it fails, trigger manually:

```bash
graphit memory index &
```

Run fire-and-forget — do NOT wait.

## ⛔ Critical Rules (Never Violate)

1. **Read memory at session start.** Your context lives there. Skipping = repeating mistakes.
2. **Never leave a correction un-memorized.** If the user corrects you, save it immediately.
3. **Never edit .md memory files directly.** Always use `graphit memory` commands.
4. **Capture trade-offs, not just facts.** "We chose X over Y because Z" > "We use X".
5. **Handle contradictions.** Remove old + create new. Don't leave conflicting memories.
6. **Promote critical memories.** Conventions, corrections, and constraints should be `--important`.
