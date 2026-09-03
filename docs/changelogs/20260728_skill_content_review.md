# Skill content review: obsolete, contradictory, wrong and concessive

**Date:** 2026-07-28
**Scope:** `internal/{ast,knowledge,memory,improvements}/rule.go`, `internal/improvements/rules.go`
and new tests
**Origin:** step 3 of Graphit Task `tsk-ee3c758773b8`

---

## The worst: `memory_gc` was documented backwards, and it deletes

```go
// internal/mcpstdio/tools_memory.go
if !input.DryRun && len(report.Candidates) > 0 {
    for _, c := range report.Candidates {
        _ = svc.RemoveMemory(c.ID)
    }
}
```

`DryRun` is `bool`, default `false`. **A call without a parameter deletes.** The skill said the opposite:

```
memory_gc(project_dir: "...")                   # find stale/empty memories (dry-run)
memory_gc(project_dir: "...", dry_run: false)   # delete GC candidates
```

An agent following that would destroy memories believing it was scanning. No confirmation and
no undo.

Fixed, with its own warning and the correct order — **scan, read, only then delete**. And with the reason
not to trust the criterion: candidate is memory untouched for `stale_days` (default 30) or empty,
and thirty days without reading is weak evidence to delete a `convention` or `correction` — those are
exactly the memories that sit idle until the one session where they stop you from repeating a
mistake. Read the scan, `memory_promote` what should survive, then collect.

## Contradiction inside the same skill: the leftover sync step

The knowledge skill had both instructions at once:

```
1. Do the work
2. Write the documentation
3. Sync the wiki — call graphit_sync ...      ← here
4. Only then report the task as complete
```

and, further down, the *"Reindexing is automatic — do not call sync in the normal flow"*
section introduced in commit `9e179bc9`. An agent reading top to bottom obeys the first.

Step 3 left. What stays in its place says **there is no reindex step** and points to the
section explaining exceptions. The completion requirement is the **record**, not its index.

## The same obsolete block still existed whole in ast

`### ⚡ MANDATORY: Sync After Every File Modification`, with *"forgetting to call sync is a
framework integrity violation"*. The previous commit removed the one from knowledge and left this one.

And it's obsolete for the same reason, verified in `internal/daemon/syncmodule.go`: the watcher observes the
source tree and calls `reindexAST` with the exact paths that changed — the comment in code
says naming the paths lets reindex skip discovery entirely, ~350ms of a ~1.07s incremental in
a 35k-file repository.

Became an exceptions table with **the right tool per row**, not `sync` for everything:

| situation | what to call |
|---|---|
| daemon stopped | `ast_index` |
| code came from outside — pull, checkout, rebase, restore | `ast_index` |
| query returns stale stuff a minute after edit | `ast_index` with `path` |
| wrong grammar for the extension | `ast_index` with `grammar` |
| semantic search returns nothing | `ast_embed` |

On every row the narrow tool wins: same effect, fraction of the work.

## Wrong tool for missing vectors

Phase 2.3 said: *"if semantic results are empty, call `graphit_sync`"*. That
reindexes the AST graph, both wikis and the Hub to fix a set of vectors. It's `ast_embed`.

Added what was missing so the agent doesn't draw the wrong conclusion: in hybrid mode search falls
back to FTS when there are no vectors, so **empty semantic is not empty hybrid** — try hybrid
before concluding the code doesn't exist.

## Parameter the tool doesn't have, in the only mandatory step of the skill

The Hub-first protocol of the knowledge skill — *"before implementing ANY integration"* —
said:

```
hub_list(project_dir: "/path/to/project", type: "knowledge")
```

`hub_list` has no `project_dir`. And Workflow step 0 said *"call `hub_list` filtering by
name/type"*: `hub_list` **doesn't filter by name**, only by type. The only mandatory step of the skill
was a call that cannot do what was asked of it.

Now it's `hub_search(query: "<system name>", type: "knowledge")` — search by name, which is what
you have —, with `hub_list` as fallback when search comes back empty. Same mistake
fixed in step 6 of the ast investigation flow.

## Native tool accepted without the harness having been tried

Four places. In each the harness tool went first, with reason attached and the
exception named — rule without why is ignored the first time it gets in the way.

### "I already know the path, I'll just read the file"

Two occurrences in ast said *"use your native read tools — they're faster and
simpler"*. `ast_source` reads the indexed copy: one call gives line range, a function by name,
or a pattern with context — where direct reading gives the whole file and you pay every line in
tokens. And it's the only one of the two that works in an imported context, whose files aren't in this
checkout.

The real exception, named: the file **is not in the graph** — just created, `.astignore`
excludes it, or `ast.index_source` is `false`. `ast_source` says so when it happens; that response is the
signal to read from disk, not a reason to skip the tool.

### "I need to search inside comments"

*"Searching inside string literals or comments → grep/ripgrep on source files"* — obsolete since
commits `6ab88223` and `d5b1b66b`. Comment is a `Comment` entity with text in `name`.
Verified in this session:

```
MATCH (f:File {path: 'x.go'})-[:CONTAINS]->(e) RETURN label(e) AS type, e.name, e.line_number ORDER BY e.line_number
→ 1|Package main is a probe fixture.|Comment
  7|Quartzo|Function
  ...
```

The graph wins precisely on what grep supposedly does well: no regex to escape, result already
with file and line, and **block comment comes as a single node** instead of five matched
lines without relation. String literal inside a function body remains outside the graph — and even there,
`ast_source` with `pattern` before grep.

`Comment` entered the properties table and gained a cookbook section, with six queries: marker
anywhere, comments of a file in reading order, skeleton with interpolated comments, forgotten commented-out code, license header, and comment adjacent to a
declaration. The two structurally new ones were executed against the graph before inclusion.

### Leaving the wiki is not going to grep

The "when NOT to use the wiki" table sent to *"Normal file tools"* and *"grep/ripgrep on source
code"*. Leaving the wiki hands the agent to the **AST skill**, not to textual search — the two indexes
cover different things and grep is below both. Said explicitly, because the natural
reading of the old table was the opposite.

### Web search without Hub having been tried

The improvements skill has an entire section — *"When to Search the Internet"* — that sends to web
search on unknown error, library quirk, uncertainty about approach and dependency
choice. **With no mention of Hub**, against Hub's mandate, which is categorical.

A gate entered above the entire section: `hub_search` first for anything external; web only
after empty and **telling the user**. With the reason that matters: Hub artifact is curated,
versioned and matched to the version this project uses — which is exactly what a
search result is not.

And the internal equivalent: before web-searching about **this** project's behavior, graph and
wiki already know. `ast_search` and `knowledge_search` answer from what's here; a search engine
answers from what is usually true.

## Two claims the implementation doesn't support

**`memory_search` doesn't read raw `.md` files.** The skill said that in one line and the opposite a hundred
lines earlier, in the same table. `wiki.BM25Search` opens the compiled wiki via SQLite FTS5 and only falls
back to an in-memory BM25 index if the FTS database doesn't exist. Fixed in both — and added the
consequence the agent needs to explain the symptom: memory written seconds ago may not
appear because the wiki hasn't recompiled yet. `memory_list` reads the store and sees what the wiki hasn't yet
compiled; `memory_index` forces rebuild.

**`hub_list` doesn't show what's installed** — already fixed in step 1, cited here because it's the
same class: claim about a tool the tool doesn't support.

## Hand-written tool name

`internal/memory/rule.go` had `` `graphit_wiki_browse` `` as a literal instead of
`brand.MCPToolRef("wiki", "browse")`. In a rebranded build that renders a tool the
agent doesn't have. Also missing `wiki: "memory"` — default is the project wiki, i.e., the
memory skill was telling to browse the wrong wiki.

## Decision Validation Gate bypassed the harness

The gate says to verify whether any prior decision justifies the current implementation. Two of four
steps ignored the tools:

- *"Check `docs/decisions/` for an ADR"* → `knowledge_search`, which ranks and brings
  cross-references, so it finds the ADR mentioning your module even when the filename doesn't
  mention it.
- *"Look for comments like `// DECISION:`"* → graph query, which covers the entire codebase in one
  call instead of the file in front of the agent, and returns the full justification instead of the matched
  line.

The memories step gained sibling projects: the decision may have been made next door. The
previous-reports step gained `dream_reports` with `all: true` — an overnight session may have looked at
exactly this and concluded to leave it as is.

And step 4 of artifact coding (*"look in IDE artifact directories"*) now resolves
the path with `hub_type-path`: each IDE organizes differently, and looking in the wrong place answers "no
such artifact" when there is one.

## Tests

Each asserts the fix **and** what replaced it, because just forbidding the old text lets
removal without replacement pass:

| test | what it locks |
|---|---|
| `TestMemoryRuleContentDoesNotInvertTheGCDryRun` | destructive form doesn't come back as dry-run |
| `TestMemoryRuleContentDescribesSearchAccurately` | `memory_search` doesn't go back to "reads raw `.md`" |
| `TestMemoryRuleContentBuildsTheBrowseToolFromTheBrand` | tool name built from brand, `memory` scope |
| `TestASTRuleContentDoesNotDemandSyncAfterEveryEdit` | obsolete block doesn't come back |
| `TestASTRuleContentSendsMissingEmbeddingsToEmbed` | missing vectors go to `ast_embed`, not `sync` |
| `TestASTRuleContentTreatsCommentsAsQueryable` | comments don't go back to grep |
| `TestASTRuleContentPrefersSourceToolOverNativeRead` | and the legitimate exception stays named |
| `TestKnowledgeRuleContentWorkflowHasNoSyncStep` | internal contradiction doesn't come back |
| `TestKnowledgeRuleContentUsesHubSearchForIntegrations` | `hub_list` doesn't come back with `project_dir` or name filter |
| `TestKnowledgeRuleContentRoutesCodeQuestionsToTheGraph` | leaving wiki still leads to graph |
| `TestImprovementsRuleContentGatesWebSearchBehindTheHub` | Hub gate doesn't leave |
| `TestImprovementsRuleContentValidatesDecisionsThroughTheHarness` | decision gate doesn't go back to listing a directory |

One of my tests was fixed along the way: `TestMemoryRuleContentHasNoHardcodedToolNames` mutated
global `brand.Brand` with `t.Parallel()` and the race detector caught it — other parallel tests in the
package read the variable. Swapped for a direct assertion about the fix, which doesn't need to touch
global state.

`golangci-lint` clean, suite with `-race` green.
