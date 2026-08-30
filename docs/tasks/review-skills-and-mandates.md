# Task: Complete the Review of Skills and Mandates

Status: completed on July 28, 2026. Six stages — `f0432fef`, `70fa594c`, `39184383`, `09b73553`, plus two more — each with its own changelog in the commit message. The first three were the originally intended scope; the last three came from corrections the Engineer made during the work. The assessment below serves as a reference (commands, tool catalog, conventions); the final section covers what falls outside this task.

---

## What Was Done

### Step 1 — `f0432fef` — each skill teaches its own module's tools

`hub_search` was the biggest gap: it became the first call taught in every Hub skill, with its search semantics documented (substring match on ID/NAME/DESCRIPTION, no stemming) because that detail changes how you search. All the other missing tools were added as well. Decisions the review left open:

- `hub_link` is added — not a lifecycle tool, but the origin of the imported contexts that the rest of the skill already used without explaining where they came from.
- `memory_sync` and `memory_remove` are added together, in a section about imported contexts.
- `knowledge_remove` is added paired with `install`, with a warning: without `context`, it wipes the local wiki.
- `sync` is added outside the lifecycle registry — it wasn't on the list, and it's the narrow tool for the case where the watcher failed to observe the change.

Also: `hub_type-path` wasn't in the Hub's own mandate, despite being used by the Improvements skill; and the project ecosystem (`cluster_*`) got real instruction instead of a passing mention, at the Engineer's request.

### Step 2 — `70fa594c` — `config`, `daemon`, and `dream` without a sixth mandate

Chosen approach: sections within existing skills, not for economy's sake — the mandate is what's expensive, since it stays in context for the whole session, while the skill body is loaded on demand. Each domain already had a skill that walked the agent to the door and left it there without tools:

| Domain | Skill | Trigger that already existed without a mechanism |
|---|---|---|
| `dream` | Improvements | "You noticed something out of the ordinary in this change" — and there was no way to act on it. |
| `daemon` | ast | "The daemon isn't running" — with no way to check. |
| `config` | Hub | The Hub already owns `cluster_*`; configuration is the same slot. |

The right question isn't which domain the tool belongs to; it's **what the agent is doing when it needs it.** Precedent: skills here are grouped by trigger, not by prefix.

### Step 3 — Content Review

The worst one: the documentation had it backwards, and it's destructive — `DryRun` defaults to `false`. Calling it with no parameters removes every candidate. The skill described this as a dry run.

Also fixed by commit `9e179bc9`: a step that survived in the workflow, contradicting the section it belonged to; the obsolete sync block that still existed in full inside `ast`; `hub_list` documented with a `project_dir` parameter and an unnamed filter that don't exist, in the skill's only mandatory step; four places that accepted a native tool without the harness having been tried first; and `memory_search` described as an inline read of `.md` files when it's actually FTS5 over the compiled wiki.

### Step 4 — Exploring Another Project Starts in the Ecosystem

Directive given by the Engineer during the task: mandatory whenever the question concerns code or documentation outside this repository — call `cluster_projects` first, then the same MCP tools with the sibling's `dir` as `project_dir`, and only as a last resort, a native tool.

The missing distinction was that the ast skill treated an imported context as the only way to consult code outside the repo. A registered "sibling project" already has its own graph, wiki, and memories — importing it re-indexes something that already exists, and not knowing that meant the agent defaulted to reading file by file. This turned into a four-row table: this repository, a sibling in the ecosystem, an unregistered checkout, or a dependency with no check-in at all.

Nothing needs to be installed, linked, or imported: `project_dir` is just a parameter. `hub_link` was added as the exception — it brings an artifact into this project, but it doesn't grant access to something that's already set up elsewhere.

### Step 5 — `wiki_source`: Read a Wiki Page via MCP

Requested by the Engineer. The skills said "read the wiki page for this entity," and reading that page was exactly the task with no tool for it — the only option was reading directly from a file, which is precisely what fails when the agent is confined to its own workspace and the page belongs to another project.

`wiki_source` was added, in both MCP and CLI, with the same slicing options as `ast_source` (`head`, `tail`, `start_line`/`end_line`, `line_numbers`, `pattern` + `regex`/`before`/`after`). `path` accepts a slug, a slug with `.md`, or a relative path — the filename generated for a wiki title is case-insensitive, and the slug you have in hand rarely matches it exactly.

The slicing logic was routed through `ast.SourceService` rather than duplicated — once the text is found, everything else is pure string manipulation shared with `ast_source`. `ErrPageNotFound` distinguishes a bad slug from a rejected reference; otherwise, the list of alternatives is enough to explain the rejection.

### Step 6 — Cypher, and the Bug That Erases Live Code

The Engineer noticed that the agent used `ast_search` and almost never wrote a query. It wasn't a lack of examples — it was the phase heading itself. It became "the best way to find names, never the answer," with Phase 3 renamed to "where the question gets answered."

And here's the worst finding of the whole review: every callable appears twice in the graph. `CONTAINS` links it to the `File`; `CALLS` points to a stub keyed by name only, empty, at line `0`. The two are different nodes, so `NOT ()-[:CALLS]->(f)` is true for all of them regardless of the real declaration — `Apply` was reported as dead code despite having 13 callers, because it exists in three separate places and the agent can follow any one of them and delete code that's actually in use.

Same root cause: mixing edge types around the same node returns **no rows and no error**. Two pre-existing queries had always returned empty because of it.

---

## What's Left (Out of Scope for This Task)

The `REFERENCES` edge is never persisted. Commit `6ab88223` claims every comment loads a `REFERENCES` edge pointing to the declaration right above it. It doesn't: `MATCH (c:Comment)-[:REFERENCES]->(t)` fails with *Table REFERENCES does not exist*. Root cause: a relationship type is only registered in the graph schema once at least one instance of it has been written, and since the comment adapter never populates this edge, the table is never created and the write is silently skipped. Out of scope for this task — the `Comment` nodes and the edges reachable from them are present, only this one edge is missing. A separate task log was opened for it.

**`ast_index` via MCP writes to the wrong project's graph.** ~~`openASTDBReadWrite` does a `chdir`, builds `DefaultLadybugConfig()` with a relative `DBPath`, and returns a handle whose database only actually opens on the first query — by which point something else has already changed the working directory back. Fixed~~, along with the `DeleteRepository` stub — see `docs/tasks/corrigir-indexacao-no-projeto-errado.md`. The root cause turned out to be more extensive than this paragraph suggested: the same defect showed up in four more places, including an `os.RemoveAll` on a relative path in `ast_index(reset: true)` that deleted the AST database of the wrong project.

This project's AST index still has 16 probe nodes left over from verifying the bug above — a task, a function, and a file that don't exist here, plus a few invented entity and comment names. Nothing was destroyed, since the graph was empty and `DeleteRepository` was a stub, so the call only added data. They're still there: clear them with `ast_index(reset: true)`, or, now that it works, `reindex: true`.

**`__config__` has an empty `lang` in this project.** The skill's *Identifying project frameworks* query returns a row with `frameworks` empty — enrichment didn't detect a framework for this Go CLI, which is plausible. The query itself works; it just has nothing to answer here.

**`receiver_type` is narrower than it sounds.** The text about tracing a call back to its owning class: in this graph's sample, populated values only existed for constructors in JavaScript/TypeScript. Left unchanged — the query runs and returns data, it just covers less than the sentence promises.

---

## Where Things Live

| What | Where |
|---|---|
| Mandate template | `internal/hub/adapters/ide/mandate.go` → `ModuleMandateTrigger` |
| Mandate + skill for each module | `internal/{ast,hub,knowledge,memory,improvements}/rule.go` (Improvements also has `rules.go`) |
| Skill content | the function at the top of each `rule.go` |
| Skill frontmatter | inside `InstallSkill()`, string `"---\nname: …\ndescription: …\n---"` |
| MCP tools | `internal/mcpstdio/tools_*.go`, registered via `brand.MCPToolName("domain", "action")` |
| Tool name reference in prose | rendered with the action segment using dashes (e.g. `hub_type-path`) |

Sizes: `knowledge` 1000 lines, `ast` 660, `improvements/rules.go` 566, `memory` 365, `mandate.go` 364, `hub` 289.

## Commands (Not Obvious — CGO, Tags, and the Ladybug Library)

```bash
export LBUG=~/go/pkg/mod/github.com/\!ladybug\!d\!b/go-ladybug@v0.17.0/lib
LD_LIBRARY_PATH="$LBUG:$LD_LIBRARY_PATH" go build -tags fts5 ./...
LD_LIBRARY_PATH="$LBUG:$LD_LIBRARY_PATH" go test -race -tags fts5 -p 4 -timeout 2400s \
  $(go list ./... | grep -v "/antlr/" | grep -v "/treesitter/")
golangci-lint run --timeout=5m     # RUN BEFORE COMMITTING — CI fails on this
make ci                            # vet, lint, vulncheck, test, ui, ui-lint
```

About 26 UI warnings already existed and don't block the build.

---

## Real MCP Tool Catalog (62 Verified)

```
ast          ast_search ast_query ast_schema ast_source ast_list ast_index ast_export
             ast_embed ast_install ast_remove
hub          hub_search hub_show hub_list hub_install hub_link hub_unlink hub_update
             hub_submit hub_projects hub_uninstall hub_type-path
knowledge    knowledge_search knowledge_list knowledge_schema knowledge_lint
             knowledge_export knowledge_index knowledge_sync knowledge_install knowledge_remove
memory       memory_search memory_insert memory_update memory_list memory_important
             memory_promote memory_demote memory_delete memory_index memory_gc memory_schema
             memory_export memory_sync memory_remove
wiki         wiki_search wiki_browse wiki_xrefs wiki_log wiki_embed
cluster      cluster_get cluster_set cluster_unset cluster_projects
config       config_get config_set config_unset config_list
daemon       daemon_status daemon_stop
dream        dream_status dream_reports dream_subject_add dream_subject_list
             dream_subject_remove
improvements improvements_rules
```

The count was 62; it's actually 64 once you add `hub_type-path` (which was missing from the list), not counting the five lifecycle tools (`init`, `sync`, `update`, `remove`, `version`).

## The Original Survey — Gaps, All Closed

Tools implemented in the module but missing from the skill's content:

| Module | Missing tools |
|---|---|
| ast | `ast_list`, `ast_index`, `ast_export`, `ast_embed` |
| hub | **`hub_search`**, `hub_submit`, `hub_projects`, `hub_uninstall` |
| knowledge | `knowledge_list`, `knowledge_schema`, `knowledge_lint`, `knowledge_export` |
| memory | `memory_export`, `memory_remove`, `memory_schema` |
| improvements | `improvements_rules` |

The most serious one: the mandate instructs the agent to "check the Hub via MCP before trusting its own knowledge," but the skill never taught it how to use the search tool. The agent got the order without the means to carry it out.

Decisions still open at the time of the survey:

1. `init`/`sync`/`update`/`remove`/`version` are lifecycle-exception tools — decide case by case whether they belong in a skill, don't assume yes.
2. No domain should be left without any skill at all — before writing, decide on the architecture: dedicated skills for each domain, or one "operations" skill covering the domains that don't have one yet.
3. Content review (most of it not started yet) — read every line in full, watching for: obsolete instructions, examples that don't work, a tool documented with a wrong parameter, something the harness already automates, and places where a native tool is accepted without the harness having been tried first.

---

## Done (Don't Redo)

Commit `9e179bc9`:

- Empty sections no longer emit `ModuleMandateTrigger`, `triggers []string`, or `tools []string` — they simply don't render.
- The five mandates rewritten with concrete triggers and a full tool inventory.
- The knowledge module's `⚡ MANDATORY: Sync After Every File Modification` block **removed**.
- Tests: `TestModuleMandateTriggerCarriesTriggersAndTools`, `TestModuleMandateTriggerOmitsEmptySections`.

---

## Things Learned That Change Decisions

The mandate is the trigger, the skill is the instruction. The mandate says *when* to open the skill; the procedure itself stays in the skill. Don't move procedure into the mandate.

An abstract mandate doesn't trigger. "For any structural-analysis task, use MCP" is a policy, not a trigger: an agent that gets "I think it's called saveUser" won't classify that as structural analysis and will reach for grep instead. Write the trigger the way it actually arrives in a request.

The watcher also reindexes the AST, not just the wiki. Confirmed in `internal/daemon/syncmodule.go`: one watch, two consumers (`reindexAST` and `reindexKnowledge`), each with its own ignore file. And memories compile themselves via `MemorySyncModule` too — so `sync`, `ast_index`, `knowledge_sync`, and `memory_index` are all exception tools.

The narrower tool wins by a wide margin. When only one subsystem is off, `ast_index`, `knowledge_sync`, or `memory_index` do a fraction of the work that `sync` does. `sync` reindexes AST, updates the wiki, memory, and the Hub all at once.

**The daemon holds the write lock and reads fail with a misleading message.** A graph query that lands during the reindexing retry window fails with `ladybug open: failed to open database with status 1` — easy to misread as "there's no graph here." It's just locked; retrying works. A genuinely missing index says something different (`no AST database found at ...`). This is documented in both the ast and knowledge skills, because falling back to grep at this point is the most expensive mistake available.

The watcher makes manual synchronization unnecessary. The daemon watches the docs tree and rebuilds the wiki on its own. Any instruction that implies syncing after every edit is obsolete — `sync` is an exception tool: for a stopped daemon, a change that came from outside the harness, or a provably stale index. What stays mandatory is **writing down** the task log, not re-indexing it. Apply the same standard to the ast and memory skills.

**Naming the tool in the mandate matters.** The agent decides between MCP and native tools *before* opening the skill; until then, it only knows what the mandate itself said.

Skills are authored in Go, not Markdown. They're concatenated string slices — run `gofmt` after editing them, and watch out for escaped quotes.

### Conventions of This Repository

Code, comments, and names in **English**; commits, changelogs, and documentation in **Portuguese**.

- A changelog is required at the end of each completed stage — atomic, not one giant changelog at the end.
- Never commit automatically outside the requested flow; never remove git hooks.
- Probe names in tests must be **invented**, never copied from the real corpus.
- No mocking or stubbing in functional requirements without explicit authorization.

---

## The Invariant Left in Place of the Survey

A module-level test now asserts that every tool a module owns is reachable from its own skill — because the mandate announces the inventory, and a tool the mandate announces but the skill doesn't teach is an order given without the means to carry it out. That was exactly what happened with `hub_search`.

A new MCP tool therefore carries two obligations beyond being registered in `tools_*.go`: it must be added to the module's mandate, and it must be taught in the skill — the package's test fails if the second one is missing.

The tests check for a **warning**, not just a mention, because a mention alone doesn't prevent the mistake — like omitting the fact that the dream agent doesn't inherit the conversation, which produces useless subjects, or documenting `knowledge_remove` without saying that calling it bare causes data loss.
