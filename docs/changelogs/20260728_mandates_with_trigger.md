# Mandates gain concrete trigger and tool inventory; sync stops being mandatory

**Date:** 2026-07-28
**Scope:** `internal/hub/adapters/ide/mandate.go`, `internal/{ast,hub,knowledge,memory,improvements}/rule.go`
**Origin:** Engineer's request to improve mandates and review skill content

---

## The abstract mandate problem

Each mandate said, in essence, "for any *structural analysis* task, use MCP".
That's policy, not a trigger. An agent asked to "find who calls saveUser" **doesn't
necessarily classify that as structural analysis** — and goes to grep. The mandate was
correct and still didn't fire.

`ModuleMandateTrigger` now takes two lists:

- **`triggers`** — concrete situations where the skill must be opened, written as the
  request arrives, not in module vocabulary. "You are about to run grep to
  locate code" triggers; "structural analysis" doesn't. Closes with *"if you're not sure whether
  one of these applies, it applies"*, because doubt is where the trigger fails.
- **`tools`** — MCP tools the module owns, named in the mandate itself. The agent
  needs to know the tool exists **before** opening the skill: that's when it
  decides between MCP and native, and until then the skill hasn't been read.

Empty sections aren't rendered, so a tool-less module doesn't get a stray header.

## The five triggers

Written from what makes each module get ignored in practice:

- **AST** — triggers on "about to run grep/glob/find", on requests naming a symbol, on
  relationship questions (who calls, what breaks), and on *"you are about to answer
  about code you haven't read, from memory of similar codebases"*.
- **Hub** — triggers on any external library or API **including those you think you
  know well**, and before reaching for web search.
- **Knowledge** — triggers on "why is this like that", on feature/architecture requests instead
  of symbol, and on *"you are about to assert how this project works by inference,
  not by having read it here"*.
- **Memory** — triggers at session start **before the first response**, when the user
  corrects or expresses a preference, when the second attempt fails like the first, and when
  you'd write to native IDE memory.
- **Improvements** — triggers at the end of a task, because reflection **is part of
  finishing**, not optional extra.

## Sync stops being mandatory: the watcher already does it

Knowledge's mandate required: *"After ANY code change you MUST update the task log and run
sync via MCP — a task without docs + sync is NOT complete"*. And the skill dedicated a
`### ⚡ MANDATORY: Sync After Every File Modification` block, with *"forgetting to call sync is
a framework integrity violation"*.

**This is obsolete.** The daemon watches the docs tree and rebuilds the wiki on its own. The
instruction told the agent to duplicate work that already happens and, on a large tree, wait for
a rebuild that wasn't needed.

It's not a gap in the skill — it's the wrong order in the mandate, which is the opposite direction from what I had
assumed when mapping tools.

Block became a table of **when the watcher couldn't have seen**: daemon stopped, change
coming from outside the machine (pull, checkout, restore), or search returning stale stuff minutes
after edit — then `sync`, and its error is the signal.

**What remains mandatory is the record.** Change without its task log is incomplete; that
obligation is about writing documentation, not reindexing it.

## Role split, now explicit

- **Mandate = trigger.** When to open the skill. Short.
- **Skill = teaching.** How to do it. Can cover `sync` as an exception tool without the
  mandate requiring it.

## What was NOT done

Tool coverage survey is ready and verified, but gaps remain open — skills don't cite part of their own modules' tools:

| module | module tools missing from skill content |
|---|---|
| ast | `ast_list`, `ast_index`, `ast_export`, `ast_embed` |
| hub | `hub_search`, `hub_submit`, `hub_projects`, `hub_uninstall` |
| knowledge | `knowledge_list`, `knowledge_schema`, `knowledge_lint`, `knowledge_export` |
| memory | `memory_export`, `memory_remove`, `memory_schema` |
| improvements | `improvements_rules` |

`hub_search` is the most severe: mandate says "check Hub via MCP" and skill never teaches the
search tool.

And three domains **have no skill at all**: `config` (4 tools), `daemon` (2), `dream` (5).
Mandates now name each module's tools, which reduces damage, but doesn't replace
content.

Full suite with `-race` clean, `golangci-lint` clean.
