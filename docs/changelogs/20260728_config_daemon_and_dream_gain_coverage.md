# `config`, `daemon` and `dream` gain coverage — without a sixth mandate

**Date:** 2026-07-28
**Scope:** `internal/hub/rule.go`, `internal/knowledge/rule.go`, `internal/ast/rule.go`,
`internal/improvements/{rule,rules}.go` and new tests
**Origin:** step 2 of Graphit Task `tsk-ee3c758773b8`

---

## The architecture decision

Three options were on the table: own skill per domain, one "operations" skill covering all three,
or sections inside existing ones. **Chose the third**, and not to save effort.

### The mandate is expensive, not the skill

The mandate block is in context in **every** session. The skill body is loaded on demand.
A sixth permanent block for 11 occasionally-used tools is a bad trade — and the mandate is
precisely the scarce resource in this design.

### Each domain already had a skill that led the agent to the door and abandoned it

This is what decided. It's not "where does this tool fit", it's **what the agent is doing
when it needs it** — and all three answers pointed to an existing skill:

| domain | skill | the trigger that already existed without mechanism |
|---|---|---|
| `dream` | improvements | *"you noticed something worth fixing outside the current change"* — and the skill offered no way out |
| `daemon` | knowledge | exception table opens with *"daemon not running"* — and there was no way to check |
| `config` | hub | hub already owns `cluster_*`; configuration is the same vacancy: the framework, its registry, its projects |

### Precedent

Skills here are already grouped by **trigger**, not by tool prefix: hub owns
`cluster_*`, knowledge owns `wiki_*`. One skill per tool prefix would be the novelty,
not the opposite.

---

## `dream` → improvements: the third way out

Every review finds more than the current change should carry. Before this the finding had
two exits, both bad: **widen the diff until it's unreviewable**, or **mention in prose and
lose it**. The skill's frontmatter already promised "dream subjects" and "schedule work for later
autonomous processing" — the content never delivered.

The new section teaches the five tools with the two preconditions that decide whether a subject
is useful at all:

**The dream agent does not inherit this session.** It doesn't know which files you were in, what the
user said, or why it matters. A subject that says *"fix the duplication we discussed"*
is a subject that produces nothing. `body` is the full briefing — paths, symptom, what was already
ruled out, how to know it worked. Same pattern as task logs.

**Dream is opt-in.** Missing `modules.dream` means off. So **don't promise the user
something will be done overnight without looking at `dream_status`**: with `enabled: false` or
`daemon_running: false` the subject sits there forever and the finding is lost exactly as if you
hadn't said anything. The Reflection Summary gained a `### Deferred` section that requires answering
*"will something pick this up?"*.

And a reading trap: **`dream_reports` marks as read**. The default call returns new
*and advances the marker*, so the second call comes back empty — reports didn't
disappear. `all: true` to see them again.

## `daemon` → knowledge: the question the table asked without answering

The first row of the exception table — *"daemon not running → nothing is watching"* —
is a **question**, and the skill gave no way to answer it. `daemon_status` answers, without
`project_dir` because the daemon is global.

With field-by-field reading: `running: false` explains **every** stale-index symptom in
the section at once; `uptime_seconds` later than your edit means the watcher never saw it;
`recent_logs` is where a rebuild that failed says why.

**"Wiki is stale" and "daemon crashed" are indistinguishable from where the agent stands, and only one of
the two is a bug.** Check before reporting.

### The transient failure that masquerades as missing index

Discovered empirically in this session: with the daemon running, a graph read that lands in
the window where it holds the write lock fails with

```
ladybug open: failed to open database with status 1
```

The message **names the database**, so it reads as "no graph here". It's a lock, not absence —
the same query works seconds later. An agent that believes the message falls back to grep,
abandoning the graph precisely because it was busy building itself. Documented in both
skills, with the contrast that disambiguates: truly missing index reports
*"no AST database found at ..."*, different text.

`daemon_stop` entered with the why attached: it stops automatic reindex for **all** projects on the
machine and takes dream sessions with it — after that everything the skill asserts about
"reindexing is automatic" stops being true. Only when the user asks.

## `config` → hub: almost every "why did the framework do that" has an answer here

The section is diagnostic, not tutorial. Observed situation → key that explains:

| what you observe | key |
|---|---|
| wiki indexes unexpected files | `knowledge.docs_dir` — **default is `.`, the whole project**, not `docs/` |
| tools of a module return nothing and nothing looks broken | `modules.<name>` |
| `ast_source` has no source for an indexed file | `ast.index_source` |
| nothing happens overnight | `modules.dream`, opt-in |

And the three traps:

**Precedence.** Inline → **environment variable** → project (`graphit.lock.json`) → global
(`~/.graphit/config.json`) → compiled defaults. Practical consequence: a value can be in
effect with `config_list` showing nothing, because env var beats both files and appears in
neither. Listed config contradicting observed behavior = **suspect env var before
bug**.

**`config_get` answers in prose when the key doesn't exist** — returns the sentence `Key "x" is not
set locally.`, not an error nor empty value. Don't forward that as if it were the configuration.

**`modules.<name>` reads backward.** `"false"` disables, `"true"` enables. And missing is not
equal to `"true"`: for an opt-in module, missing means off.

Written with explicit scope: `global: true` changes **every** project on the machine — never on own initiative.

## Mandates

Each module gained new tools in inventory and concrete triggers:

- **hub** — *"a module of this framework behaved in a way you cannot explain — read
  configuration before calling it a bug"*
- **knowledge** — *"an index looks stale, or a graph read failed to open the database —
  find out whether the daemon is alive before concluding anything"*
- **ast** — *"a graph read failed to open the database: that's a lock, not a missing index;
  retry before falling back"*
- **improvements** — the out-of-scope finding trigger now says *"there is a tool for this,
  it doesn't have to be dropped or crammed in"*

## Tests

- `TestImprovementsRuleContentTeachesDreamSubjects` / `...WarnsAboutDreamPreconditions`
- `TestKnowledgeRuleContentTeachesDaemonStatus` / `...ExplainsTheTransientLock`
- `TestHubRuleContentTeachesConfiguration` / `...ExplainsTheConfigTraps`

Those that verify a warning, not just a mention, exist because mere mention doesn't prevent the error: citing
`dream_subject_add` without saying the dream agent doesn't inherit the conversation produces useless subjects.

`golangci-lint` clean.
