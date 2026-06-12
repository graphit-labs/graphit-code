<h1 align="center">Graphit Code</h1>

<p align="center">
  <strong>The Enterprise Collaboration Hub for AI-Guided Software Development</strong>
</p>

<p align="center">
  <a href="https://github.com/graphit-labs/graphit-code/releases/latest"><img src="https://img.shields.io/github/v/release/graphit-labs/graphit-code?style=flat-square&color=blue" alt="Release"></a>
  <a href="https://github.com/graphit-labs/graphit-code/actions"><img src="https://img.shields.io/github/actions/workflow/status/graphit-labs/graphit-code/release.yml?style=flat-square" alt="Build"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/graphit-labs/graphit-code?style=flat-square" alt="License"></a>
  <img src="https://img.shields.io/badge/platform-linux%20%7C%20macOS%20%7C%20windows-informational?style=flat-square" alt="Platform">
  <img src="https://img.shields.io/badge/dependencies-zero-success?style=flat-square" alt="Zero Dependencies">
</p>

<p align="center">
  <a href="https://graphit-labs.github.io/graphit-code">Website</a> ·
  <a href="#-installation">Installation</a> ·
  <a href="#-key-features">Features</a> ·
  <a href="#-quick-start">Quick Start</a> ·
  <a href="#-documentation">Documentation</a>
</p>

<p align="center">
  <img src="docs/site/assets/ast-explorer-3d.png" alt="Graphit Code — AST Explorer 3D" width="100%">
</p>

---

## From Solo Engineer to Enterprise Ecosystem

You've been here before. New session, same ritual — **explain everything again**:

> *"Our API uses this pattern..."*  
> *"No, we don't use that library anymore..."*  
> *"I already told you — error responses follow this format..."*  

Every session starts from zero. **You are the memory.** You repeat, re-explain, re-correct — and the agent forgets it all when the conversation ends. But this is just the surface. The deeper problem is that your AI agent has **no real understanding** of where it's working. It doesn't know your architecture. It doesn't know the sibling services. It greps and guesses code structure instead of querying it. It hallucinates APIs from other projects because it can't see them.

**Graphit Code changes the paradigm.** 

Stop being an engineer who works *with* an AI assistant. Start working inside a **real collaborative flow of a complete enterprise ecosystem**. Graphit Code transforms your local development environment into a collaborative hub where knowledge is progressive, understanding is deterministic, and your agent comprehends the entire system.

---

## The Core Pillars

**1. Deterministic Precision (AST) & Go Performance**  
No more "grep and guess". Built in Go for blazing-fast performance, Graphit Code features a full AST graph database (LadybugDB + Tree-sitter + ANTLR v4). Your agent queries code structure deterministically with Cypher. The indexing is instant and auto-incremental.

**2. Ecosystem Over Individual**  
The agent doesn't just see the current folder. It automatically discovers all managed projects within the ecosystem. It can query sibling codebases, read their shared knowledge, and generate cross-service integrations using real routes and DTOs. Zero hallucinations.

**3. Enterprise Collaboration Hub**  
Share knowledge, personal and shared memories, skills, rules, and MCP servers across your entire team. Fosters **progressive knowledge** across the entire enterprise ecosystem, ensuring that every project benefits from shared intelligence. All Hub artifact management events are tracked via a Git repo.

**4. 100% Private, Zero Cost, No API Keys**  
Everything stays in your possession. Data is 100% private and anonymous. Git provides persistence for all memories and knowledge. There is **zero additional cost** and no LLM API key required, because Graphit operates via your existing local IDE and CLI agents.

**5. Progressive, Continuous Improvement**  
Memory is persistent and automatic. The system includes an autonomous **Dream Module** that works during idle time — mining conversation history, crystallizing recurring patterns into reusable skills, evaluating and improving existing skills, and generating memories from undocumented knowledge.

---

## 🏆 The Ultimate Advantage: Private Knowledge Ecosystems

> **This is the game-changer.** Whether you're a solo developer managing multiple projects or an enterprise team scaling across dozens of engineers — Graphit Code turns your Git infrastructure into a **persistent knowledge backbone** that grows smarter over time.

For **individual developers**, it means your agent knows every project you work on, remembers past decisions, and carries context across your entire ecosystem. For **teams**, knowledge compounds: corrections, conventions, and skills propagate automatically to every member. Either way, context is never lost.

### 🔗 Private Hub Registry — Your Team's Artifact Center

During the interactive `graphit setup`, you can point the **Hub** to any Git repository — a personal repo for solo use, or a shared private repo (self-hosted GitLab, Bitbucket, GitHub Enterprise, or any SSH/HTTPS accessible repo) for teams. This repository becomes the **centralized artifact registry** for your entire ecosystem:

- **Shared Coding Rules** — Enforce company-wide standards automatically across every developer's IDE.
- **Team Skills** — Codify complex workflows (k8s debugging, internal API patterns, deployment checklists) so every agent on the team knows the procedures.
- **Knowledge Artifacts** — Publish documentation about frameworks, APIs, and integration specs that every developer's agent can discover and install.
- **Language Queries** — Share Tree-sitter and ANTLR extraction `.yaml` query files for customizing how entities are extracted from the built-in languages. Drop-in AST query customization without recompilation.
- **Framework Configs** — Distribute framework detection `.yaml` rules (decorators, heritage, imports) for internal or niche frameworks across the team.
- **MCP Servers, Commands, Agent Profiles** — Share reusable automation across the entire organization.

Every artifact is versioned via Git, published directly, and distributed securely through your existing authentication infrastructure (SSH keys, SSO, access tokens).

### 🧠 Shared Memory Repository — Collective Team Intelligence

Point the **Memory** sync to a separate Git repository and your ecosystem gains **shared persistent memory**:

- **Corrections compound across the team** — When one developer corrects their agent ("we don't use that library anymore"), the correction propagates to everyone.
- **Conventions are learned once** — Architecture decisions, API patterns, and coding standards are stored as team-wide memories that every agent follows automatically.
- **Institutional knowledge persists** — New team members' agents immediately benefit from the collective learning of the entire team. No more onboarding friction.
- **Full Git history** — Every memory change is tracked, auditable, and reversible.

### 🚀 The Result: A Private, Self-Hosted Collaboration Loop

```
┌─────────────────────────────────────────────────────────────┐
│                   Your Private Git Server                   │
│  (GitLab / Bitbucket / GitHub Enterprise / Any Git Host)    │
│                                                             │
│   ┌──────────────────┐      ┌──────────────────────────┐   │
│   │  Hub Repository   │      │  Memory Repository        │   │
│   │  (Team Artifacts) │      │  (Shared Learning)        │   │
│   │                   │      │                           │   │
│   │  • Rules          │      │  • Corrections            │   │
│   │  • Skills         │      │  • Conventions            │   │
│   │  • Knowledge      │      │  • Decisions              │   │
│   │  • MCP Servers    │      │  • Institutional Memory   │   │
│   └────────┬──────────┘      └────────────┬──────────────┘   │
│            │                              │                  │
└────────────┼──────────────────────────────┼──────────────────┘
             │        git push/pull         │
     ┌───────┴──────────────────────────────────┴───────┐
     │   Solo Developer or Entire Team                  │
     │                                              │
     │   graphit setup  →  configure your repos     │
     │   graphit sync   →  push/pull knowledge      │
     │   graphit init   →  inject rules into IDE    │
     └──────────────────────────────────────────────┘
```

**Zero cloud dependencies. Zero SaaS subscriptions. Zero data leaving your network.**  
Your knowledge stays 100% private, versioned in Git, and shared securely through your existing infrastructure.

```bash
# One-time setup — point to your own repos (personal or team)
graphit setup
# → Enter Hub Git URL:    git@gitlab.company.com:team/graphit-hub.git
# → Enter Memory Git URL: git@gitlab.company.com:team/graphit-memory.git

# That's it. Every sync propagates team knowledge automatically.
graphit sync
```

---

## Installation

**Zero dependencies required.** Supports Windows, Linux, and macOS out of the box. Fully auto-configurable.

### Option 1: Download from GitHub Artifacts (Recommended)

<details>
<summary><strong>Linux (amd64)</strong></summary>

```bash
curl -fsSL https://github.com/graphit-labs/graphit-code/releases/latest/download/graphit-linux-amd64 -o graphit
chmod +x graphit
sudo mv graphit /usr/local/bin/

# Configure & Init
graphit setup
graphit init --ide <antigravity|gemini|claude|cursor|kiro|codex|opencode>
```

</details>

<details>
<summary><strong>Windows (amd64)</strong></summary>

```powershell
Invoke-WebRequest -Uri "https://github.com/graphit-labs/graphit-code/releases/latest/download/graphit-windows-amd64.exe" -OutFile "graphit.exe"
# Move graphit.exe to a folder in your PATH

# Configure & Init
graphit setup
graphit init --ide <antigravity|gemini|claude|cursor|kiro|codex|opencode>
```

</details>

<details>
<summary><strong>macOS (Apple Silicon)</strong></summary>

```bash
curl -fsSL https://github.com/graphit-labs/graphit-code/releases/latest/download/graphit-darwin-arm64 -o graphit
chmod +x graphit
sudo mv graphit /usr/local/bin/

# Configure & Init
graphit setup
graphit init --ide <antigravity|gemini|claude|cursor|kiro|codex|opencode>
```

</details>

### Option 2: Build from Source

Requires: Go 1.23+, Node.js 22+, Make, gcc/g++

```bash
git clone https://github.com/graphit-labs/graphit-code.git
cd graphit-code
make install   # Compiles and installs to /usr/local/bin/
```

---

## Key Features

### 1. Unified Graphical UI
Manage the entire Hub and search knowledge across your own code or multiple projects simultaneously. The dashboard facilitates interactions and lets you chat about the total knowledge of your project.
```bash
graphit ui  # Opens http://localhost:8080
```
- **AST Explorer:** 3D interactive graph with Cypher queries.
- **Wiki Explorer:** FTS, semantic search, and documentation routing.
- **Hub Manager:** Decentralized registry interface.

<p align="center">
  <img src="docs/site/assets/hub-project-artifacts.png" alt="Hub Project Artifacts" width="100%">
</p>

### 2. AST Graph Explorer — Instant & Deterministic
Query the AST across the ecosystem instantly. Auto-incremental indexing ensures your agent always knows exactly where a function is defined or called. **Eliminates hallucinations** by grounding answers in exact structural truths, and drastically **reduces LLM token usage** by passing only precise nodes instead of massive files.

#### Supported Languages (23)

Graphit Code uses two parser backends: **Tree-sitter** (incremental, fast) and **ANTLR v4** (full grammar, enterprise SQL). Tree-sitter grammars are compiled natively via CGO for maximum performance. ANTLR grammars use native Go binaries. All 23 programming languages — 18 Tree-sitter languages and 5 ANTLR grammars — are built into the binary.

| Language | Parser | Extensions | Extracted Entities |
|---|---|---|---|
| **Go** | Tree-sitter | `.go` | Functions, Methods, Structs, Interfaces, Types, Constants, Variables, Fields, Parameters |
| **TypeScript** | Tree-sitter | `.ts` | Functions, Classes, Interfaces, Types, Enums, Variables, Fields, Parameters, Decorators |
| **TSX** | Tree-sitter | `.tsx` | Functions, Classes, Interfaces, Types, Enums, Variables, Fields, Parameters, Decorators |
| **JavaScript** | Tree-sitter | `.js`, `.jsx`, `.mjs` | Functions, Classes, Variables, Fields, Parameters, Exports |
| **Python** | Tree-sitter | `.py` | Functions, Classes, Variables, Parameters, Decorators |
| **Java** | Tree-sitter | `.java` | Functions, Classes, Interfaces, Enums, Variables, Fields, Parameters, Packages, Annotations |
| **Rust** | Tree-sitter | `.rs` | Functions, Structs, Enums, Traits, Types, Constants, Variables, Fields, Parameters |
| **C** | Tree-sitter | `.c`, `.h` | Functions, Structs, Enums, Types, Variables, Fields, Parameters |
| **C++** | Tree-sitter | `.cpp`, `.hpp`, `.cc`, `.cxx`, `.hxx`, `.hh` | Functions, Classes, Structs, Enums, Namespaces, Types, Fields, Parameters |
| **C#** | Tree-sitter | `.cs` | Functions, Classes, Interfaces, Enums, Structs, Properties, Namespaces, Fields, Parameters |
| **Kotlin** | Tree-sitter | `.kt`, `.kts` | Functions, Classes, Interfaces, Enums, Variables, Fields, Parameters, Packages |
| **Swift** | Tree-sitter | `.swift` | Functions, Classes, Structs, Enums, Protocols, Variables, Fields, Parameters |
| **Dart** | Tree-sitter | `.dart` | Functions, Methods, Classes, Enums, Mixins, Variables, Fields, Parameters |
| **PHP** | Tree-sitter | `.php` | Functions, Methods, Classes, Interfaces, Traits, Enums, Constants, Namespaces, Fields, Parameters |
| **Ruby** | Tree-sitter | `.rb` | Functions, Classes, Modules, Variables, Fields, Parameters |
| **SQL** | Tree-sitter | `.sql` | Functions, Tables, Views |
| **XML** | Tree-sitter | `.xml`, `.xsl`, `.xslt`, `.xsd`, `.svg`, `.wsdl`, `.plist`, `.xhtml` | Elements |
| **HTML** | Tree-sitter | `.html`, `.htm` | Elements, Script/Style blocks + ID/Class/href/src/action/name attributes, data-*/aria-* attributes, ARIA roles, label-for binding |
| **PL/SQL** | ANTLR v4 | `.sql`, `.pks`, `.pkb`, `.pls`, `.plb`, `.prc`, `.fnc`, `.trg`, `.typ`, `.bdy`, `.spc`, `.vw` | Functions, Procedures, Packages, Types, Triggers, Tables, Views, Materialized Views, Indexes, Sequences, Synonyms, DB Links, Columns, Parameters, Variables, Constants, Cursors, Exceptions, Constraints, Savepoints + DML tracking (SELECTS, INSERTS, UPDATES, DELETES) |
| **PostgreSQL** | ANTLR v4 | `.sql`, `.pgsql`, `.plpgsql`, `.pg` | Functions, Procedures, Tables, Views, Materialized Views, Schemas, Triggers, Sequences, Indexes, Extensions, Types (domain, composite, enum, range), Columns, Parameters, Constraints, Variables + DML tracking |
| **DB2** | ANTLR v4 | `.sql`, `.db2` | Functions, Stored Procedures, Tables, Views, Triggers, Indexes, Sequences, Types, Schemas, Aliases, Tablespaces, Columns, Parameters, Variables + DML tracking |
| **T-SQL** | ANTLR v4 | `.sql`, `.tsql` | Stored Procedures, Functions, Tables, Views, Triggers, Indexes, Sequences, Types, Schemas, Columns, Parameters, Variables + DML tracking |
| **COBOL 85** | ANTLR v4 | `.cob`, `.cbl`, `.cpy`, `.cobol` | Programs, Sections, Paragraphs, Data Items, File Descriptions, Condition Names + PERFORM/CALL/GO TO tracking, File I/O (READ/WRITE/OPEN/CLOSE), COPY references |

#### What the AST Maps

Every source file is parsed via **Tree-sitter** or **ANTLR v4** into a graph stored in **LadybugDB** (embedded graph database). The YAML language configuration determines which parser to use. The graph captures:

- **Nodes:** `File`, `Directory`, `Function`, `Procedure`, `Method`, `Constructor`, `Class`, `Record`, `Annotation`, `Object`, `Struct`, `Interface`, `Protocol`, `Trait`, `Mixin`, `Extension`, `Enum`, `Type`, `Module`, `Variable`, `Constant`, `Parameter`, `Field`, `Column`, `Namespace`, `Package`, `Table`, `View`, `MaterializedView`, `Export`, `Element`, `Schema`, `Trigger`, `Index`, `Sequence`, `StoredProcedure`, `Constraint`, `Synonym`, `DBLink`, `Tablespace`, `Alias`, `Cursor`, `Exception`, `Savepoint`, `Property`
- **Relationships:** `CONTAINS` (ownership), `IMPORTS` (dependencies), `CALLS` (invocations), `HAS_PARAMETER`, `HAS_FIELD`, `READS_FIELD` / `WRITES_FIELD` (data access tracing), `INHERITS`, `IMPLEMENTS`, `SELECTS` / `INSERTS` / `UPDATES` / `DELETES` / `ALTERS` / `DROPS` / `REFERENCES` (DML/DDL tracking — PL/SQL, PostgreSQL, T-SQL, DB2, COBOL 85)
- **Properties:** `name`, `path`, `line_number`, `end_line`, `cyclomatic_complexity`, `is_exported`, `entry_point_score`, `docstring`, `source`, `lang`, `cluster`

#### Fully Customizable — Pure YAML-Driven Engine

The entire AST pipeline is driven by **external YAML files** — not hardcoded. Every aspect of language understanding, framework detection, and scoring is runtime-customizable without recompilation:

- **Language Query Patterns** — 23 language YAML files define all extraction patterns (functions, classes, imports, calls, etc.) for both Tree-sitter and ANTLR grammars.
- **Framework Detection** — 51+ framework definitions across 59 YAML files. Decorators, heritage classes, and import patterns for frameworks like React, Django, Spring Boot, Flutter, Express, FastAPI, and many more.
- **Ecosystem Detection** — `ecosystems.yaml` with 120+ entries classifying projects by technology stack (web, mobile, API, database, CLI, etc.)
- **Entry Point Scoring** — Scoring rules embedded in each language YAML determine how functions are ranked as potential entry points.
- **Language Configuration** — Export strategies, import match types (`prefix`, `exact`, `contains`, `suffix`, `regex`), `self`/`this` keywords, context type mappings, relation types, and declaration types are all defined per-language in YAML.
- **Relation Types** — All entity and relation types (e.g., `CALLS`, `IMPORTS`, `INHERITS`, `IMPLEMENTS`, `READS_FIELD`, `WRITES_FIELD`) are defined in YAML. New relation types can be added without recompilation.

All configuration follows a **4-level cascading resolution chain**:

- **Per Project:** `.graphit/ast/queries/<language>.yaml` — override for one project only.
- **User Global:** `~/.graphit/ast/queries/<language>.yaml` — your customizations across all projects.
- **Runtime Defaults:** `~/.graphit/runtime/<version>/ast/queries/` — factory defaults, auto-extracted from the binary per version.
- **Embedded:** Built-in YAML defaults compiled into the binary — used only when no external file is found at any higher level.

**Extend without recompilation:** Add new frameworks, ecosystem patterns, entry point scoring rules, relation types, and customize all language behavior by simply dropping YAML files into the project or user directory. Community contributions can be YAML-only PRs — no Go knowledge required.

> **Fully Customizable AST Extraction:** While grammars are compiled natively into the binary (Tree-sitter via CGO, ANTLR via native Go), the entire extraction pipeline is driven by **external YAML files** that are fully customizable without recompilation:
>
> 1. **Customize extraction queries** — Modify which entities (functions, classes, imports, etc.) are extracted from any of the 23 built-in languages by editing the YAML query files.
> 2. **Override per project or globally** — Drop YAML files into `.graphit/ast/queries/` (project) or `~/.graphit/ast/queries/` (global) to override the runtime defaults.
> 3. **Add framework detection** — Create framework YAML files to detect custom frameworks via decorators, inheritance, and import patterns.
>
> To add support for an entirely new language grammar, you must add CGO bindings (Tree-sitter) or a native Go parser (ANTLR) to the source and recompile.

Resolution follows a cascading priority chain: **project → user global → runtime → embedded**. See the [User Manual](docs/guides/user_manual.md#customizing-ast-tree-sitter-queries) for examples, and the [AST Module Spec](docs/specs/ast_module.md#-external-query-customization) for the full technical reference.

#### Indexing: Full & Incremental

- **Full Index:** Scans every file via Tree-sitter or ANTLR v4 (depending on language YAML), extracts entities, writes the graph, builds FTS5 + trigram + vector indices.
- **Incremental Index:** Uses SHA-256 hash cache per file — only changed files are re-parsed and re-written. Concurrent worker pool with single-threaded graph writer prevents contention.
- **File Watcher:** Git-based polling (every 2s) with 500ms debounce. Detects both uncommitted edits and committed changes. Zero file descriptors used.

### 3. LLM Wiki & Knowledge Discovery
Documentation designed for agents. Replaces opaque vector embeddings with deterministic **self-discovery** and explicit **back-referencing**. The agent organically explores the wiki graph to find exact context, guaranteeing precise retrieval without hallucinated semantic matches.
- **Auto Sync:** Runs smoothly in the background to automatically keep your knowledge and code graphs perfectly aligned in real-time.

<p align="center">
  <img src="docs/site/assets/memory-explorer.png" alt="Memory Explorer" width="100%">
</p>

### 4. Continuous Collaborative Memory
Supports both **shared project memory** for the entire team and **personal memory** tailored to the individual user. The system automatically self-refines its memories over time, guaranteeing extreme assertiveness in the agent's actions by recalling conventions, corrections, and past decisions across all sessions.

### 5. Standard Skills & Router Strategy
Graphit Code ships with default explicit standard skills. To prevent LLM context exhaustion and loss of attention, it employs a **router strategy** that loads only the relevant context when needed. All rules are fully customizable globally or per project.

### 6. IDE Agnostic Auto-Configuration
Supports and synchronizes across multiple IDEs working together (Claude Code, Cursor, Gemini, Kiro, Codex, OpenCode).

```bash
# Auto-configure for your environment
graphit init --ide <antigravity|gemini|claude|cursor|kiro|codex|opencode>
```

### 7. Cluster Discovery — Cross-Project Ecosystem

Group related projects with **cluster labels** and let your agent discover, navigate, and query the entire ecosystem. Projects sharing at least one label are automatically linked as cluster members.

```bash
# Assign labels to group projects
graphit cluster domain backend
graphit cluster team payments

# View your labels
graphit cluster --list

# Discover all projects in your cluster
graphit cluster projects

# Filter by a specific label
graphit cluster projects domain
```

Your AI agent uses the same discovery via MCP (`graphit_cluster_projects`) to:
- **Find sibling projects** and read their source or documentation
- **Query code across projects** using AST with the sibling's `project_dir`
- **Read another project's wiki** to understand APIs and contracts
- **Make cross-project changes** using the discovered paths

This prevents **integration hallucinations** — the agent sees real routes, DTOs, and interfaces from the actual sibling codebases instead of guessing.

> See the [Cluster Discovery Spec](docs/specs/cluster_microservices.md) for the full technical reference.

---

## Quick Start

```bash
# 1. Setup global hub and memory repos. During this interactive setup, you can point to 
# your organization's private Git repositories (e.g., self-hosted GitLab or GitHub 
# Enterprise) to establish a secure, collaborative environment for your IT team.
graphit setup

# 2. Initialize your project (Auto-configures IDE rules, indexes AST + Wiki)
cd your-project
graphit init --ide <antigravity|gemini|claude|cursor|kiro|codex|opencode>

# 3. Keep the agent updated after you change files
graphit sync &

# 4. Open the collaborative dashboard
graphit ui
```

---

## Architecture

Graphit Code is a single self-contained binary written in Go. All data persists locally via standard Git repositories, meaning you retain 100% control over your knowledge and interactions.

- **AST Module:** LadybugDB + Tree-sitter + ANTLR v4.
- **Knowledge Module:** LLM Wiki & explicit back-referencing.
- **Hub & Memory:** Git-backed transparent artifact storage.
- **Dream Module:** Background skill generation, conversation mining, and knowledge extraction.

---

## Enterprise & Brand Customization (White-Labeling)

Graphit Code is built to be customized for private, self-hosted enterprise environments:
- **100% Private Data:** Local syntactic parsing (Tree-sitter + ANTLR v4), local embedded graph database (LadybugDB), local embedding computations (ONNX runtime), and Git-backed repositories that stay inside your private network.
- **Custom Branding (White-Labeling):** Compile the binary using Go `-ldflags` overrides to customize the binary name (`Brand`), the display name (`DisplayName`), environment variables, and default Git-backed registry repositories.
- **Private Team Ecosystems (Git-Backed):** Set up team-wide coding standards, shared agent memories, and custom developer skills without relying on cloud-hosted SaaS databases. During the interactive `graphit setup` command, developers configure private Git repositories (e.g., self-hosted GitLab, Bitbucket, or GitHub Enterprise) as backend sync stores. The harness pushes/pulls memories, rules, and skills through standard SSH/HTTPS Git authentication, establishing a secure collaborative environment for your entire IT team.
- **Keyless AI Harness:** Run agentic tasks without configuring separate LLM API keys. The framework leverages the active sessions, credentials, and quotas of developers' existing IDE and CLI agents natively.

For instructions, see the **[Private Branding & Customization Guide](docs/guides/private_brand_customization.md)**.

---

## Documentation

Visit the **[Documentation Hub](docs/README.md)** for a complete index of all guides and manuals.

### User Guides
- **[Getting Started](docs/guides/getting_started.md)**: Install, requirements, setup, and verification.
- **[CLI Command Reference](docs/guides/cli_reference.md)**: Manual detailing every command and flag.
- **[User Manual](docs/guides/user_manual.md)**: Interactive canvas, memories, and autonomous skill generation.
- **[Private Branding & Customization](docs/guides/private_brand_customization.md)**: White-labeling build parameters, private hubs, and secure networks.

### Technical Specifications
- **[System Architecture](docs/architecture/architecture_overview.md)**: Layer design and process wrapper.
- **[AST Module Spec](docs/specs/ast_module.md)**: Graph database LadybugDB, Cypher builder, and parser.
- **[Wiki Module Spec](docs/specs/wiki_module.md)**: Obsidian wiki, LPA, fuzzy match, and search engine.
- **[Memory Module Spec](docs/specs/memory_module.md)**: Scope partitioning and consolidation.
- **[Hub Collaboration Spec](docs/specs/hub_collaboration.md)**: Registry git-store and locks.
- **[Daemon Module Spec](docs/specs/daemon_module.md)**: Supervisors, schedulers, and servers.
- **[Dream Module Spec](docs/specs/dream_module.md)**: Autonomous skill generation, conversation mining, and knowledge extraction.
- **[Improvements Module Spec](docs/specs/improvements_module.md)**: Rules resolved order and Clean Code.
- **[UI Dashboard Spec](docs/specs/ui_dashboard.md)**: React force-directed canvas and uiserver handlers.
- **[AI Engine Spec](docs/specs/ai_engine.md)**: ONNX models, embedding providers, and prompt completions.
- **[Cluster Discovery Spec](docs/specs/cluster_microservices.md)**: Projects registry and cross-project delegated queries.

---

## License

[MIT](LICENSE) — Graphit Labs

<p align="center">
  <strong>Graphit Code</strong> — The Enterprise Collaboration Hub for AI-Guided Software Development<br>
  <em>Tell your agent once. It remembers forever.</em>
</p>
