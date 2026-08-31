---
name: graphit-hub
description: 'Centralized registry of knowledge, AST, rules, skills, commands, agents, MCPs, powers, and languages. Use when: working with external libraries, APIs, or frameworks; needing documentation or code examples for a dependency; looking for reusable rules, skills, commands, or MCP servers; setting up a new project or adding dependencies; AST query returns no results for an external library. Check the hub BEFORE implementing integrations with unfamiliar systems. Also use to install/update artifacts, discover reusable components, and find sibling projects in the ecosystem.'
---

# Hub Discovery Rule

## Objective

The Hub is a centralized registry of shareable artifacts that enrich your development
environment with pre-built knowledge bases, code analysis contexts, reusable rules,
skills, commands, agents, MCP integrations, and power bundles.

You MUST leverage the Hub BEFORE assuming or hallucinating how any framework,
library, or domain concept works.

## 🔒 MANDATORY: The Hub REPLACES Guessing and Your Built-in Knowledge

> **For any external library, framework, API, or domain concept, the Hub MCP
> tools take ABSOLUTE PRECEDENCE over your own model knowledge and your
> built-in web search.** You MUST check the Hub FIRST. Use these tools via MCP
> ONLY — NEVER via the CLI, and NEVER guess an external API from memory.

### Why this replaces your tools

| Your tool | Hub MCP equivalent | Why the Hub wins |
|---|---|---|
| Recalling an external API from model memory | Call `graphit_hub_search` → `graphit_hub_show` → `graphit_hub_install` | Model knowledge is stale and hallucination-prone; Hub artifacts are curated and versioned |
| Built-in web search for library docs | Install the `knowledge` artifact, then `graphit_knowledge_search` | Hub knowledge is pre-indexed, project-local, and authoritative |
| Guessing a framework's structure | Install its `ast` artifact, then query via `graphit_ast_query` with `context` | The graph is exact; guessing is not |
| Reinventing a rule/skill/command | Call `graphit_hub_search` for a reusable artifact | Battle-tested artifacts beat ad-hoc reinvention |

### 🔒 When you MUST use the Hub (MANDATORY — no exceptions)

| Scenario | What to do | What NOT to do |
|---|---|---|
| **Working with an unfamiliar library/framework/API** | Call `graphit_hub_search` with the library name → `graphit_hub_show` → `graphit_hub_install` | ❌ Don't guess the API from model memory |
| **Needing docs/examples for a dependency** | Install its `knowledge` artifact and search it | ❌ Don't rely on built-in web search first |
| **graphit_ast_query returns nothing for an external lib** | Call `graphit_hub_search` with `type: "ast"` and install what it returns | ❌ Don't assume the code does not exist |
| **Looking for a reusable rule/skill/command/agent** | Call `graphit_hub_search` before writing your own | ❌ Don't reinvent an existing artifact |

### When you should NOT use the Hub

| Scenario | Use instead |
|---|---|
| Understanding THIS project's own code | AST MCP tools (`graphit_ast_query`) |
| Understanding THIS project's own docs | Knowledge wiki (`graphit_knowledge_search`) |
| Editing source or running builds/tests | File edit tools / terminal |

### 🔄 Fallback to Model Knowledge / Web Search — ONLY When the Hub Has Nothing

Your model knowledge and built-in web search are permitted for an external
library/API ONLY when ALL of these conditions are true:

1. You **already searched the Hub** via `graphit_hub_search` — with the library name, and then with a broader term, because the match is a plain substring on id/name/description and `fastapi` will not find an artifact registered as `python-web-frameworks`
2. The Hub **has no matching artifact** (knowledge or ast) for the library/framework/API
3. You **state explicitly** to the user: "The Hub has no artifact for X, falling back to general knowledge/web search"

**If even ONE of these conditions is not met, you MUST NOT fall back.**

### ❌ Anti-patterns (violations of this protocol)

| Anti-pattern | Why it is a violation |
|---|---|
| Guessing an external API from model memory | Hallucination risk; the Hub is the source of truth |
| Using the CLI (`graphit hub ...`) instead of MCP tools | Agent-facing work MUST go through MCP tools, never the CLI |
| Web-searching a library before checking the Hub | Skips curated, versioned artifacts |
| Reimplementing a rule/skill that already exists in the Hub | Wastes effort and diverges from shared conventions |
| Concluding "the Hub has nothing" after one `graphit_hub_search` call with an exact package name | The match is a substring, not a semantic search — one miss is not an answer |

## Artifact Types

The Hub provides these artifact types — each serves a different purpose:

| Type | What it provides | After installation |
|---|---|---|
| `knowledge` | Pre-indexed documentation wiki for a framework/library | Search via `graphit_knowledge_search` or `graphit_wiki_search` |
| `ast` | Pre-indexed code graph of a framework's source code | Query via `graphit_ast_query` tool (passing absolute `project_dir` and setting `context` parameter to the artifact ID) |
| `rule` | Coding conventions, style guides, governance rules | Auto-injected into IDE rules file |
| `skill` | Detailed methodology for specific tasks (e.g. testing, migration) | Available as an on-demand skill |
| `command` | Reusable CLI workflows/commands | Available in IDE's commands directory |
| `agent` | Pre-configured agent personas with specific expertise | Available in IDE's agents directory |
| `mcp` | MCP server configurations for external tool integrations | Auto-configured in IDE's MCP settings |
| `power` | Curated bundle combining multiple artifacts as a cohesive package | Installs all bundled artifacts at once |

## How to use the Hub

If you encounter a framework, module, or domain concept you are not fully certain
about, DO NOT guess its API or structure. Check if it is available in the Hub.

### 1. Search — the first call, every time

**`graphit_hub_search` is how you check the Hub.** You almost always arrive with a name in
hand ("the task uses Stripe"), and searching by that name is one call:
```
graphit_hub_search(query: "stripe")
graphit_hub_search(query: "stripe", type: "knowledge")
```

`query` is required; `type` narrows to one artifact type. There is no `project_dir`
parameter — the registry is global, not per-project. Passing one is an error.

How the match works, because it changes how you search: the term is lower-cased and
matched as a **substring** of the artifact id, name, and description — id/name hits rank
above description hits. It is not semantic and it does not stem. So:

- Search the **name people would register**, not only the package name: `stripe`, then `payments`.
- One empty result is not an answer. Widen the term (`fastapi` → `fastapi`, then `python`, then `web`) before concluding the Hub has nothing.
- Nothing at all after two or three terms? Fall back to `graphit_hub_list` for the whole catalogue of that type — it is small enough to read.

### 2. Catalogue
To read every available artifact, or every artifact of one type, call `graphit_hub_list`:
```
graphit_hub_list(type: "<knowledge|ast|rule|skill|command|agent|mcp|power>")
```

Use this when you do **not** have a term to search for — "what knowledge artifacts exist"
— or as the fallback when `graphit_hub_search` came back empty. Like search, it takes no
`project_dir`, and it lists what the registry offers, not what this project installed.

### 3. Inspection
To see the details, tags, and description of a specific artifact, call the `graphit_hub_show` tool:
```
graphit_hub_show(id: "<artifact-id>")
```

### 4. Installation
To download and install the artifact into the current project, call the `graphit_hub_install` tool (passing absolute `project_dir`):
```
graphit_hub_install(project_dir: "/path/to/project", id: "<artifact-id>", ide: "<ide>", alias: "<alias>")
```

`id` accepts an `@version` suffix (`stripe-knowledge@2.1.0`) to pin a version. Without it
you get the latest.

### 5. Uninstall
To remove an artifact you installed — wrong artifact, or a dependency that is gone — call `graphit_hub_uninstall`:
```
graphit_hub_uninstall(project_dir: "/path/to/project", id: "<artifact-id>", type: "<type>")
```

Do not delete the artifact's files by hand: the lockfile would still claim it is installed,
and the next `graphit_hub_update` would try to update something that is no longer there.

### 6. Updates
To keep all installed artifacts up to date, call the `graphit_hub_update` tool (passing absolute `project_dir`):
```
graphit_hub_update(project_dir: "/path/to/project")
```

Pass `id` to update exactly one artifact instead of all of them.

### 7. Link & Unlink (Local Development)
To link or unlink local development artifacts into the current project, call `graphit_hub_link` or `graphit_hub_unlink` (passing absolute `project_dir`):
```
graphit_hub_link(project_dir: "/path/to/project", name: "<name>", source_path: "/path/to/source", type: "<type>")
graphit_hub_unlink(project_dir: "/path/to/project", name: "<name>", type: "<type>")
```

Link points at a directory on this machine via symlink, so edits at the source are live.
Use it while authoring an artifact; publish it with `graphit_hub_submit` once it is worth sharing.

**This is not how you reach another project.** Link brings one artifact into *this* project,
for authoring it. It grants no access you do not already have: to read or query a sibling
project you pass its `dir` as `project_dir` — see the ecosystem protocol below.

### 8. Where a new artifact goes — ask, do not guess

Before creating a skill, rule, command, or agent, call `graphit_hub_type-path` to get the
absolute path for the current IDE:
```
graphit_hub_type-path(project_dir: "/path/to/project", type: "skill", name: "error-handling-patterns")
```

Each IDE puts artifacts in a different directory, and some types are a folder while others
are a single file. This tool answers both from the project's configured IDE — inventing the
path means the IDE never loads what you wrote.

### 9. Publishing what you built

To share a local artifact through the Hub, call `graphit_hub_submit`:
```
graphit_hub_submit(project_dir: "/path/to/project", id: "<artifact-id>", local_path: "<dir with the artifact>", type: "<type>", version: "1.0.0")
```

`local_path` is a directory, not a file, and is resolved against `project_dir` when
relative. `version` defaults to `1.0.0` and `type` to `rule` — always pass both explicitly,
because a second publish under the same version is what overwrites someone else's install.
Publishing pushes to a shared repository: do it when the user asked for it, not on your own
initiative.

### 10. Who else is on the Hub

To list the projects registered in the Hub registry, call `graphit_hub_projects`:
```
graphit_hub_projects()
```

This is the **remote** registry view — projects that publish to this Hub. For the sibling
projects checked out on this machine, use `graphit_cluster_projects` instead (below): it is
the one that returns local absolute paths you can read, query, and edit.

## Using Installed Artifacts

Once installed, artifacts enhance your capabilities automatically:

- **Knowledge**: Search the wiki via `graphit_knowledge_search` or `graphit_wiki_search` to understand
  a framework's API, architecture, and patterns — never guess.
- **AST**: Query the code graph of the installed context using the `graphit_ast_query` tool (passing absolute `project_dir` and setting `context` parameter to the installed artifact ID).
- **Rules**: Automatically injected — follow the conventions they define.
- **Skills**: Read the skill when the task matches its domain. Skills appear
  in the IDE's skills directory.
- **Commands**: Execute pre-built workflows from the IDE's commands directory.
- **Agents**: Delegate specialized tasks to agent personas with domain expertise.
- **MCPs**: External tool integrations are auto-configured — use them as available tools.
- **Powers**: All bundled artifacts are installed — use each by its individual type.

## Installed Artifacts

**`graphit_hub_list` does not answer this.** It lists what the registry offers; it has no
`project_dir` and no notion of this project. What this project installed is recorded in
`graphit.lock.json` at the project root — read it when you need the answer, and use
`graphit_hub_show` on an id from there to see what the artifact is.

## 🌐 The Project Ecosystem — where this project sits, and what else is here

The Hub is one half of the picture: artifacts to install. The other half is the **ecosystem**
— every project registered on this machine, grouped by labels the user controls. It answers
two questions the code cannot:

1. **What is this project, to the user?** Its labels say what domain, team, stack, or tier
   they filed it under. That is intent, and it is not inferable from the source tree.
2. **What else is related?** Which checkouts are siblings, and where they are on disk — so
   "the auth service" stops being a name and becomes a path you can query.

**Use it whenever you want the context of the current project or of the ecosystem around it**
— not only when told to. It is four cheap calls and it is the only place this information exists.

### Reading the current project's labels

```
graphit_cluster_get(project_dir: "/path/to/project")                 # every label on this project
graphit_cluster_get(project_dir: "/path/to/project", key: "domain")   # the values under one key
```

Do this **before assuming what a project is for**. A repository named `core` labelled
`domain=billing, tier=critical` is a different conversation from one labelled `domain=demo`.
With no `key`, you get the whole map; with a `key`, only its values.

### Grouping and ungrouping

```
graphit_cluster_set(project_dir: "/path/to/project", key: "domain", value: "billing")
graphit_cluster_unset(project_dir: "/path/to/project", key: "domain")
```

A label is a key with **one or more values** — a project can be `domain=billing` and
`domain=invoicing` at once, which is how one repository belongs to two groups. `graphit_cluster_unset`
removes the whole key, values included; there is no unset-one-value.

These labels are the user's organisation of their own work. **Read them freely; change them
only when asked.** Relabelling silently rearranges which projects the ecosystem considers
related, and the user is the one who decided that.

### Finding the related projects

```
graphit_cluster_projects(project_dir: "/path/to/project")
graphit_cluster_projects(project_dir: "/path/to/project", label: "domain")   # only same-domain siblings
```

How the grouping resolves, because it decides what you get back:

- Two projects are siblings when they share **at least one identical key *and* value**. Same
  key with different values (`domain=billing` vs `domain=search`) is not a match.
- Optional `label` narrows to one key: siblings that share a value **under that key**,
  ignoring every other label they have in common.
- Projects with **no labels at all** form their own default group — so on a machine where
  nobody has labelled anything, this returns everything.
- **The current project is included in the result.** Do not read the first entry as "another
  project"; compare `dir` against your own `project_dir`.
- The same project registered at two paths (worktrees, a second clone) appears twice, the
  second keyed with a `#2` suffix. Two entries can therefore be the same project.

Each entry carries:

| Field | Description |
|---|---|
| `dir` | Absolute path to the project root directory |
| `name` | Human-readable project name |
| `description` | Project description |
| `cluster` | Cluster labels (key→values map) |
| `registeredAt` | When the project was registered |

A project only appears here once it has been registered — which `graphit_init` does.
An empty result on a machine you know has sibling checkouts means they were never initialised,
not that they do not exist.

### What the paths let you do

`dir` is a real absolute path, so every tool in this framework accepts it as `project_dir`.
A sibling is not a black box — it is a project you can interrogate exactly like this one:

- **Query its code** — the sibling has its own graph:
  ```
  graphit_ast_query(project_dir: "/path/to/other-project", query: "MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'handler' RETURN f.name, f.path")
  ```
- **Read its documentation** — `graphit_knowledge_search` or `graphit_wiki_search` with the sibling's `project_dir`, instead of grepping its docs tree
- **Read its memories** — `graphit_memory_search` with the sibling's `project_dir`: decisions and corrections recorded over there, which is often exactly why it behaves the way it does
- **Change it** — if the user asks for a cross-project edit, the path is where you edit

### 🔒 MANDATORY: the ecosystem comes first, and a sibling is explored with MCP

**Whenever a question is about code, documentation, or behaviour that is not in this
repository, this order is obligatory:**

1. **Resolve it in the ecosystem — `graphit_cluster_projects` — before anything else.** Before
   asking the user where the project is, before guessing a path, before `ls` on a parent
   directory, and before answering from what such a service usually does.
2. **If it is in the ecosystem, explore it exactly as you explore this project.** Nothing to
   install, nothing to link, nothing to import: **`project_dir` is a parameter, so pointing a
   tool at another project is just passing a different value.** Its `dir` from step 1:
   ```
   # what the code does
   graphit_ast_search(project_dir: "<sibling dir>", query: "token validation")
   graphit_ast_query(project_dir: "<sibling dir>", query: "MATCH (f) WHERE (label(f) = 'Function' OR label(f) = 'Method') AND toLower(f.name) CONTAINS 'validate' RETURN f.name, f.path, f.line_number, label(f) AS type")

   # then read it — by entity or line range, never the whole file
   graphit_ast_source(project_dir: "<sibling dir>", path: "<path from the query>", entity: "<name from the query>")

   # what it is for, and what changed there lately
   graphit_knowledge_search(project_dir: "<sibling dir>", query: "authentication")
   graphit_wiki_search(project_dir: "<sibling dir>", query: "how tokens are issued", wikis: ["project", "memory"])
   graphit_wiki_browse(project_dir: "<sibling dir>")
   graphit_wiki_log(project_dir: "<sibling dir>")

   # why it is the way it is
   graphit_memory_search(project_dir: "<sibling dir>", query: "token")
   ```

   `graphit_wiki_log` earns its call when the sibling's behaviour surprises
   you: it lists what its wiki added, updated and deleted per sync, which is the cheapest way
   to see what moved over there recently.
3. **Only if it is not in the ecosystem** does the question change shape: a checkout the user
   points you at becomes an imported context (`graphit_ast_install`), and a
   dependency you do not have becomes a Hub lookup (`graphit_hub_search` with `type: "ast"`
   or `"knowledge"`).
4. **Native tools on a sibling's tree are last** — after the graph and the wiki, not instead
   of them, and only for what they structurally cannot hold.

#### Why this order and not the obvious one

- **A registered sibling already has its own graph and its own compiled wiki.** Nothing to
  import, nothing to index, no artifact to install — you have the same tools over there that
  you have here, the moment you know the path. Skipping step 1 is how that gets missed.
- **The sibling's memories say why it is the way it is.** Decisions, corrections and
  trade-offs recorded by whoever worked there. No amount of reading its source recovers them.
- **Grepping another project's tree is the worst option available.** Unfamiliar layout, no
  ranking, every match paid for in tokens, and no access to the relationships — callers,
  imports, implementors — that are the reason you were looking in the first place.
- **Guessing the path fails silently.** A wrong `project_dir` does not error; it answers
  confidently about a different codebase, or returns nothing and reads as "the code is not
  there".

#### ❌ Anti-patterns

| Anti-pattern | Why it is a violation |
|---|---|
| Answering about a sibling service from model knowledge of how such services work | It is on this machine, indexed, and queryable |
| Asking the user for the path before calling `graphit_cluster_projects` | The tool exists so the question is unnecessary |
| `ls`, `find`, or grep on a sibling's directory to orient yourself | `graphit_ast_search` and `graphit_wiki_browse` with its `project_dir` orient you better and cheaper |
| Reading a sibling's files one by one | `graphit_ast_source` slices by entity or line range; the graph tells you which file first |
| Importing a registered sibling as an AST context | Redundant — it has its own graph; pass its `project_dir` |
| Reaching for `graphit_hub_link` to "get access to" a sibling | Wrong tool. Link exists to develop an artifact locally, and it symlinks one artifact into **this** project. It grants no access you did not already have: `project_dir` is a parameter |
| Concluding "not in the ecosystem" without looking at `name` and `description` in the output | The user's word for a project rarely matches its directory name |

### Two worked examples

**The user names a project you have never seen.** "Why does the auth service reject our
tokens?" — call `graphit_cluster_projects`, match `name`/`description` against "auth", take its
`dir`, then `graphit_ast_query` with `project_dir` set to that path and
`query: "MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'validate' RETURN f.name, f.path, f.line_number"`.
Guessing the path, or answering from how auth services usually work, is the failure this
tool exists to prevent.

**You are about to design something.** Call `graphit_cluster_get` first. If this project is
labelled into a domain with siblings, the convention you are about to invent may already
exist next door — `graphit_cluster_projects`, then search their wikis and memories before you
decide.

## ⚙️ Configuration — read it before diagnosing framework behaviour

Almost every "why did the framework do that" has its answer in configuration, and guessing
the answer wastes the user's time on a setting they already chose. Four tools, all cheap:

```
graphit_config_list(project_dir: "/path/to/project")     # everything set for this project
graphit_config_list(global: true)                          # everything set for this machine
graphit_config_get(project_dir: "/path/to/project", key: "knowledge.docs_dir")
graphit_config_set(project_dir: "/path/to/project", key: "modules.dream", value: "true")
graphit_config_unset(project_dir: "/path/to/project", key: "modules.dream")
```

### When to read it — concrete situations, not "when configuring"

| What you observe | The key that explains it |
|---|---|
| The wiki is empty, or it is missing documentation you know exists | `knowledge.docs_dir` — **defaults to `docs`**, so documentation kept anywhere else is not indexed until this says where |
| A page came from the root `README.md` even though it is outside the docs tree | `knowledge.include_readme` — on by default; `false` indexes the docs tree alone |
| The code graph has no `File` node for a document under the docs tree | `ast.index_docs` — off by default, because the docs tree belongs to the wiki; `true` puts it in both |
| A module's tools return nothing and nothing looks broken | `modules.<name>` — the module may be switched off |
| `graphit_ast_source` has no source for an indexed file | `ast.index_source` — with `false`, the graph stores structure but not text |
| A file is parsed by the wrong grammar | `ast.grammar` overrides, per extension |
| A grammar YAML the project committed has no effect | `ast.queries_dir` — the project's grammar directory, **`.graphit/ast/queries` by default**, which IS tracked by git; check the key has not been pointed elsewhere, since a configured directory replaces the default rather than adding to it |
| Nothing ever happens overnight | `modules.dream` — dream is **opt-in**, off unless explicitly `true` |
| The task backlog is not where you expected it | `backlog.dir` — **defaults to `docs/tasks/backlog`**, inside the docs tree so items are versioned; it follows `knowledge.docs_dir` |
| Hub operations fail before reaching the network | `hub.bucket` |
| An artifact installed into the wrong IDE's directory | `ide` |

### The precedence, because it is where the confusion actually is

A key can be set in several places. From strongest to weakest:

1. Inline (a parameter on the call itself)
2. **Environment variable** — `GRAPHIT_KNOWLEDGE_DOCS_DIR` for `knowledge.docs_dir`: upper-cased, dots to underscores
3. Project — `graphit.lock.json`, which is what `graphit_config_list` shows without `global`
4. Global — `~/.graphit/config.json`, which is what it shows **with** `global: true`
5. Compiled-in defaults

So a value can be in force while `graphit_config_list` shows nothing: an env var outranks both
files and appears in neither. When the listed config contradicts observed behaviour, an
environment variable is the first suspect, not a bug.

### Two traps

**`graphit_config_get` answers in prose when a key is unset** — it returns the sentence "Key
\"x\" is not set locally." rather than an error or an empty value. Do not pass that string on
as if it were the setting.

**`modules.<name>` is inverted from how it reads.** `"false"` disables the module; `"true"`
enables it. And absent is not the same as `"true"`: for opt-in modules — `dream` — absent
means off.

### Writing configuration

`graphit_config_set` and `graphit_config_unset` change how the framework behaves for this
project, or with `global: true` for **every** project on the machine. Set what the user asked
for; do not silently "fix" a setting you disagree with, and never touch global scope on your
own initiative — the blast radius is every repository they own.

Project scope needs the project initialised (`graphit_init`); without a
lockfile these tools fail rather than falling back to global.

## ⚠️ Rule

Rely entirely on the official artifacts from the Hub rather than generic internet knowledge.
When in doubt: call `graphit_hub_search` → `graphit_hub_show` → `graphit_hub_install`.
