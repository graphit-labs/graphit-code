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
No more "grep and guess". Built in Go for blazing-fast performance, Graphit Code features a full AST graph database (LadybugDB + Tree-sitter). Your agent queries code structure deterministically with Cypher. The indexing is instant and auto-incremental.

**2. Ecosystem Over Individual**  
The agent doesn't just see the current folder. It automatically discovers all managed projects within the ecosystem. It can query sibling codebases, read their shared knowledge, and generate cross-service integrations using real routes and DTOs. Zero hallucinations.

**3. Enterprise Collaboration Hub**  
Share knowledge, personal and shared memories, skills, rules, and MCP servers across your entire team. Fosters **progressive knowledge** across the entire enterprise ecosystem, ensuring that every project benefits from shared intelligence. All Hub artifact management events are tracked via a Git repo.

**4. 100% Private, Zero Cost, No API Keys**  
Everything stays in your possession. Data is 100% private and anonymous. Git provides persistence for all memories and knowledge. There is **zero additional cost** and no LLM API key required, because Graphit operates via your existing local IDE and CLI agents.

**5. Progressive, Continuous Improvement**  
Memory is persistent and automatic. The system includes an **Improvements Module** capable of autonomous work during idle time. It constantly refines, audits, and generates knowledge to push your ecosystem forward.

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

---

## Quick Start

```bash
# 1. Setup global hub and memory repos
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

- **AST Module:** LadybugDB + Tree-sitter.
- **Knowledge Module:** LLM Wiki & explicit back-referencing.
- **Hub & Memory:** Git-backed transparent artifact storage.
- **Improvements Module:** Background agent orchestration for autonomous code improvement.

---

## Documentation

Visit the **[Documentation Hub](docs/README.md)** for a complete index of all guides and manuals.

### User Guides
- **[Getting Started](docs/guides/getting_started.md)**: Install, requirements, setup, and verification.
- **[CLI Command Reference](docs/guides/cli_reference.md)**: Manual detailing every command and flag.
- **[User Manual](docs/guides/user_manual.md)**: Interactive canvas, memories, and dreaming.

### Technical Specifications
- **[System Architecture](docs/architecture/architecture_overview.md)**: Layer design and process wrapper.
- **[AST Module Spec](docs/specs/ast_module.md)**: Graph database LadybugDB, Cypher builder, and parser.
- **[Wiki Module Spec](docs/specs/wiki_module.md)**: Obsidian wiki, LPA, fuzzy match, and search engine.
- **[Memory Module Spec](docs/specs/memory_module.md)**: Scope partitioning and consolidation.
- **[Hub Collaboration Spec](docs/specs/hub_collaboration.md)**: Registry git-store and locks.
- **[Daemon Module Spec](docs/specs/daemon_module.md)**: Supervisors, schedulers, and servers.
- **[Dream Module Spec](docs/specs/dream_module.md)**: Background worker, idle timeout monitoring, and worktree isolation.
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
