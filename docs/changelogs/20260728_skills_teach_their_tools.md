# Every skill now teaches every tool in its own module

**Date:** 2026-07-28
**Scope:** `internal/{ast,hub,knowledge,memory,improvements}/rule.go` and new tests
**Origin:** step 1 of `docs/tasks/review-skills-and-mandates.md`

---

## `hub_search`: the order existed, the means didn't

Hub's mandate says, without exception: *"before trusting your own knowledge or web search
about ANY external framework/library/API, you MUST first check the Hub via MCP
tools"*.

Hub's skill **never mentioned `graphit_hub_search`**. The agent received the order to search and the
only taught path was `hub_list` → `hub_show` → `hub_install` — i.e., list the entire catalog
and scan with eyes. Anyone arriving with a name in hand ("task uses Stripe") had to
discover alone that a tool exists that takes that name.

`hub_search` became the **first call** everywhere in the skill: in both precedence tables, in
fallback conditions, in the usage section and in the final rule. `hub_list` remains, in the role that is
really its — reading the catalog when you *don't* have a term, or when search came back empty.

And we documented how matching works, because that changes how you search: term is compared
as **substring** of id, name and description, with id/name ranking above description. It's not
semantic and doesn't stem. Hence two new rules with reason attached:

- Search for the name **someone would register**, not just the package name.
- **An empty result is not an answer.** `fastapi` finds no artifact registered as
  `python-web-frameworks`. Widen the term before concluding Hub has nothing — became
  an explicit anti-pattern.

## Wrong parameter, and a claim the tool doesn't support

`hub_list` and `hub_search` **have no `project_dir`** — the registry is global. Three skills told to
pass one:

- hub: `hub_list(project_dir: ...)` in the "Installed Artifacts" section
- knowledge: `hub_list(project_dir: "...", type: "knowledge")` in the Hub-first protocol
- ast: same, in step 6 of the investigation flow

Worse than the parameter: Hub's skill said *"to see installed artifacts, call `hub_list`"*.
`hub_list` returns `reg.ListEntries()` — what the **registry offers**, not what this project
installed. No tool exists for that; who knows is `graphit.lock.json` at the root. Section
now says exactly that.

A test now forbids recurrence: no skill may contain
`graphit_hub_list(project_dir` nor `graphit_hub_search(project_dir`.

## The project ecosystem gains real teaching

Hub is half the picture: artifacts to install. The other half is the **ecosystem** — all
registered projects on the machine, grouped by labels the user controls. The skill treated
`cluster_set`, `cluster_get` and `cluster_unset` as a passing mention in a single sentence, and the
`label` of `cluster_projects` wasn't documented.

Now the section answers the two questions code doesn't answer:

1. **What is this project, to the user?** Labels say in which domain, team, stack or tier
   it was filed. That's intent, and not inferred from the source tree.
2. **What else is related?** Which checkouts are siblings, and where they are on disk — that's how
   "the auth service" stops being a name and becomes a queryable path.

With exact semantics, which decides what comes back:

- Siblings share **at least one identical key *and* value**. Same key with different value
  doesn't match.
- One label is key → **multiple values**: one repository can be `domain=billing` and
  `domain=invoicing` at the same time. `cluster_unset` removes the entire key.
- **Current project comes in the result.** Previous skill said "all sibling projects",
  leading the agent to read the first entry as another project.
- Same project registered at two paths appears twice, the second with suffix `#2`.
- With no labels anywhere, the default group is everything.
- A project only appears after being registered — empty may mean "siblings never ran
  `init`", not "none exist".

And what paths allow: `dir` is a real absolute path, so **every** tool in this framework
accepts it as `project_dir` — including `memory_search` on the sibling project, which is often
exactly why it behaves the way it does.

Read labels freely; **change only when asked** — relabeling silently rearranges what
the ecosystem considers related, and that decision is the user's.

## The other missing tools

| module | what entered | why it matters |
|---|---|---|
| hub | `hub_uninstall`, `hub_submit`, `hub_projects`, `hub_type-path` | `hub_type-path` was used by the improvements skill and wasn't taught nor cited in Hub's own mandate |
| ast | `ast_list`, `ast_install`, `ast_remove`, `ast_index`, `ast_embed`, `ast_export` | skill told *not* to pass `context` on the own project without ever saying where a context comes from |
| knowledge | `knowledge_list`, `knowledge_lint`, `knowledge_schema`, `knowledge_export`, `knowledge_sync` | `knowledge_sync` wasn't even in the survey; it's the narrow tool for when the watcher couldn't have seen the change |
| memory | `memory_export`, `memory_schema`, `memory_remove`, `memory_sync` | `remove`/`sync` require `context` and act on an imported context — names suggest the opposite |
| improvements | `improvements_rules` | skill's methodology is a **default** that a project or Hub artifact may override |

Three decisions the survey left open, and the reason:

- **`ast_install`/`ast_remove` enter.** They're not just lifecycle: they're the origin of contexts the
  rest of the skill uses. With warning — `ast_remove` without `context` executes
  `MATCH (n) DETACH DELETE n` on the current project's graph.
- **`memory_sync` enters**, alongside `memory_remove`, in an imported-contexts subsection. The
  memory skill mentioned no context at all.
- **`knowledge_remove` enters as a pair**, with same warning: without `context`, wipes the local wiki.

`improvements_rules` gained its own section with reason: reviewing against the default when an
override exists is reporting as a defect precisely the choices the project made on purpose.

## Prefer the narrow tool over the big one

Two places told to call `graphit_sync` — which reindexes AST, wiki, memory and Hub — to
fix a problem of a single subsystem:

- `ast_search` in semantic mode without vectors → now `ast_embed`
- wiki provably stale → now `knowledge_sync`

Same effect, a fraction of the work.

## Tests

One invariant per module, with reason in comment: **every tool the module owns must be
reachable from its own skill**, otherwise the mandate advertises capability the agent
cannot use.

- `TestHubRuleContentTeachesEveryHubTool` (11 hub tools + 4 cluster)
- `TestHubRuleContentDoesNotInventAProjectDirOnRegistryTools`
- `TestMandateTriggerNamesTheSearchAndEcosystemTools`
- `TestASTRuleContentTeachesEveryASTTool`, `TestASTRuleContentExplainsImportedContexts`
- `TestKnowledgeRuleContentTeachesEveryTool`, `TestKnowledgeRuleContentPrefersTheNarrowSyncTool`
- `TestMemoryRuleContentTeachesEveryMemoryTool`, `TestMemoryRuleContentDistinguishesContextToolsFromDelete`
- `TestImprovementsRuleContentTeachesTheRulesTool`

`TestHubRuleContent` stopped requiring a literal title — which broke when renaming the
ecosystem section. Now verifies content, not wording.

`golangci-lint` clean.
