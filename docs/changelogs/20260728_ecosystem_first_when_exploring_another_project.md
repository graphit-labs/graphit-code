# Exploring another project: ecosystem first, and siblings are explored via MCP

**Date:** 2026-07-28
**Scope:** `internal/{hub,ast,knowledge,memory}/rule.go` and new tests
**Origin:** Engineer's request during skill review

---

## The rule

When the question is about code, documentation or behavior **not in this
repository**, the order is mandatory:

1. **Resolve in the ecosystem — `cluster_projects` — before anything else.** Before
   asking the user where the project is, before guessing a path, before `ls` in a
   parent directory, and before answering from what such a service normally does.
2. **If it's in the ecosystem, explore just like this project** — same MCP tools, its `dir` as
   `project_dir`.
3. **Only if it's not there** does the question change shape: checkout the user points to becomes an
   imported context (`ast_install`); dependency you don't have becomes Hub search.
4. **Native tool on the sibling's tree is last** — after graph and wiki, not instead
   of them.

## The distinction the skills didn't make

The ast skill spoke of **imported context** as if it were the only way to query outside
code. It's not, and the missing line is the cheapest of the four:

| code is in | how to query |
|---|---|
| this repository | default, without `context` |
| **sibling project in ecosystem** | **its graph: pass `dir` as `project_dir`.** No import, no context |
| checkout on the machine that is not a registered project | `ast_install` once, then `context` |
| dependency without checkout | `hub_search` with `type: "ast"` → `hub_install`, then `context` |

A registered project **already has an indexed graph, compiled wiki and its own memories**. Importing it
as context reindexes a graph that already exists. And not knowing that is what sends the agent to read
file by file: nothing had said there was a graph to query.

## Nothing needs to be installed, linked or imported

`project_dir` is a **parameter**. Pointing any tool at another project is passing a different
value — that's it, and that's all. Verified in code: `wiki_search`, `wiki_browse`, `wiki_log`,
`knowledge_search`, `memory_search`, `ast_*` all take `project_dir`, and
`resolveWikiDBDir(projectDir, scope)` resolves that project's wiki.

`hub_link` entered as an explicit anti-pattern, because it's the natural confusion: link exists to
develop an artifact locally, brings **one** artifact into **this** project, and grants no
access that passing `project_dir` doesn't already give. It's verbose for something it doesn't even do.

## The set the sibling deserves

```
# 1. resolve — never guess the path
cluster_projects(project_dir: "/path/to/project")

# 2. what the code does
ast_search(project_dir: "<sibling dir>", query: "token validation")
ast_query(project_dir: "<sibling dir>", query: "MATCH (f) WHERE ... RETURN ...")

# 3. read — by entity or line range, never the whole file
ast_source(project_dir: "<sibling dir>", path: "<from query>", entity: "<from query>")

# 4. what it's for, and what changed there
knowledge_search(project_dir: "<sibling dir>", query: "authentication")
wiki_search(project_dir: "<sibling dir>", query: "...", wikis: ["project", "memory"])
wiki_browse(project_dir: "<sibling dir>")
wiki_log(project_dir: "<sibling dir>")

# 5. why it's like that
memory_search(project_dir: "<sibling dir>", query: "token")
```

`wiki_log` is the one worth remembering: it lists what that project's wiki added, updated and deleted
per sync, so *"what changed there recently"* is one call instead of a hand-rebuilt diff.

`memory_search` on the sibling is what no code reading replaces: **code shows what, memories are the only record of why.** A decision made on purpose by someone who worked there
isn't reconstructed by reading its result.

## Why this order and not the obvious one

Each reason attached to the rule, because a rule without a why is ignored the first time it gets in the way:

- **A registered sibling already has a graph and wiki.** Nothing to import, nothing to index, no artifact to
  install — you have there the same tools you have here, the moment you know the path. Skipping
  step 1 is how that gets missed.
- **Grep on another project's tree is the worst available option.** Unknown layout, no ranking,
  each match paid in tokens, and no access to relations — who calls, what imports, who implements
  — which is why you were looking in the first place.
- **Guessing the path fails silently.** Wrong `project_dir` doesn't error: it answers with confidence
  about another codebase, or comes back empty and reads exactly as *"that code doesn't exist"*.
- **The user's word rarely matches the directory name.** Match against `name` and
  `description` from the output, not against the basename.

## Mandates

The decision between MCP and native happens **before** opening any skill, so order also entered
the triggers:

- **hub** — *"the user names another project, service or repository — resolve in the ecosystem
  FIRST, then explore with AST and wiki MCP tools using its `project_dir`; never guess
  the path, never read nor grep its files"*
- **ast** — the other-repository trigger became explicit, and the unconditional clause now
  says graph-first **also applies to other projects**, otherwise the agent reads it as
  applying only to the current repository
- **knowledge** — *"the documentation you need is from another project — resolve in the ecosystem and
  search ITS wiki, never walk or grep its docs tree"*
- **memory** — *"the question is about another project in the ecosystem: its memories hold why
  it is the way it is"*

`cluster_projects` entered the tool inventory of ast and knowledge mandates — the agent
needs to know the resolution tool exists at decision time, and until then it only knows what
the mandate said.

## Tests

| test | what it locks |
|---|---|
| `TestHubRuleContentMandatesTheCrossProjectProtocol` | the mandatory order and six calls on the sibling, including `project_dir` being just a parameter |
| `TestHubRuleContentForbidsNativeExplorationOfSiblings` | three anti-patterns: reimporting, `ls`/grep to orient, and `hub_link` to "gain access" |
| `TestMandateTriggerCarriesTheEcosystemFirstOrder` | order doesn't leave the mandate |
| `TestASTRuleContentDistinguishesSiblingsFromImportedContexts` | four rows of the table, and forbidding import of a registered sibling |
| `TestASTRuleContentWarnsThatAWrongProjectDirFailsSilently` | silent-failure warning |
| `TestMandateTriggerExtendsGraphFirstToOtherProjects` | graph-first still applies to another project |
| `TestKnowledgeRuleContentCoversSiblingWikis` | sibling wiki with `wiki_log` and `ast_source` included |
| `TestKnowledgeRuleContentPutsTheLookupBeforeTheReading` | order: search before reading, wiki before files |
| `TestMemoryRuleContentCoversSiblingMemories` | sibling memories are readable |

One of my step-1 tests broke and was fixed: it asserted the literal title `"Imported Contexts"`,
which became a lower-level section with different casing. Now compares case-insensitively — same lesson
as `TestHubRuleContent` in step 1: **assert content, not wording.**

`golangci-lint` clean, full suite with `-race` green.
