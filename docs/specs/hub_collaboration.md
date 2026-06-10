---
title: "Hub Collaboration Specification"
description: "Technical specification of the Collaboration Hub module, registry managers, git store backends, and lockfiles."
content-type: reference
audience: developers
keywords:
  - hub
  - registry
  - git store
  - lockfile
  - artifacts
prerequisites:
  - "docs/architecture/architecture_overview.md"
related:
  - "docs/specs/memory_module.md"
  - "docs/specs/daemon_module.md"
---

# Hub Collaboration Specification

The Hub module provides a decentralized registry for sharing developer environment rules, custom skills, background commands, and AI agent prompt configurations across team members.

---

## 🗃️ Registry Management & Git Store

The Hub registry is backed by a standard, private or public **Git Repository** (defaulting to the URL configured during `graphit setup`).
There are no dedicated database servers; adding, modifying, or removing a registry artifact is recorded as a standard Git transaction.

### Git Store Layout

A typical Hub repository uses the following structure:

```
hub-repository/
├── registry.json             # Registry index manifest
├── languages/                # Language extraction query definitions (.yaml)
├── frameworks/               # Framework detection definitions (.yaml)
├── rules/                    # System rules templates
│   ├── golang_conventions.md
│   └── react_styling.md
├── skills/                   # Custom agent skill sets
│   ├── k8s-debugger/
│   │   ├── SKILL.md
│   │   └── scripts/
│   └── pg-optimizer/
├── agents/                   # Agent profile configurations
├── commands/                 # Executable agent shortcuts
├── knowledge/                # LLM Wiki documentation artifacts
├── ast/                      # Code graph artifacts
├── mcp-servers/              # IDE bridge MCP server configs
└── powers/                   # Bundled multi-artifact packages
```

### Artifact Operations

`internal/hub/git_store.go` manages transactions:
- **`Sync()`**: Pulls down the latest updates from the central repository and reconciles local catalogs.
- **`Submit()`**: Stages local rules, commits them to the temporary workspace, and pushes modifications to the remote registry.
- **`Install()`**: Links rules and skills from the registry into a project workspace, auto-updating local rules configs.

---

## 🔒 Project Lockfile: `graphit.lock.json`

Every project managed by Graphit Code includes a `graphit.lock.json` file in the repository root.
This lockfile tracks configuration overrides and locks artifact versions:

```json
{
  "project": {
    "id": "01JM6B7T3B...",
    "name": "graphit-code",
    "description": "Enterprise AI Harness"
  },
  "ides": ["cursor", "claude"],
  "config": {
    "ide": "cursor",
    "docs_dir": "docs",
    "modules": {
      "ast": { "disabled": false }
    }
  },
  "artifacts": {
    "language": {
      "elixir": {
        "version": "1.0.0",
        "origin": "hub"
      }
    },
    "framework": {
      "phoenix": {
        "version": "1.0.0",
        "origin": "hub"
      }
    },
    "rules": {
      "golang_conventions": {
        "version": "1.4.2",
        "installed_at": "2026-05-29T20:12:00Z"
      }
    }
  }
}
```

### Reconcile Loop

On `graphit sync`, the engine reads the lockfile and executes a reconciliation loop:
1. **Verification**: Compares installed artifacts against version locked entries.
2. **Re-injection**: If rule blocks have been deleted from files like `.cursorrules`, the registry re-injects them inside the sentinel blocks.
3. **IDE Sync**: Applies rulesets across all listed IDE targets (`ides` array).
4. **Global Lock Registration**: Registers the project ULID and directory path under the global daemon registry, enabling cluster microservices discovery.

---

## 📦 Language and Framework Artifacts

In addition to rules, skills, and commands, the Hub supports two artifact types dedicated to the AST module's language and framework detection pipeline.

### Language Artifacts

A **language** artifact packages extraction query YAML files (`.yaml`) that customize how entities are extracted from the built-in languages. These can override default extraction patterns, export strategies, context types, and other language configuration. Tree-sitter and ANTLR grammars are compiled natively into the binary and cannot be installed at runtime.

Content structure:

```
languages/
└── go-custom/
    └── go.yaml                   # Custom extraction queries, export strategy, context types
```

When installed, the query YAML is placed into `<project>/.graphit/ast/queries/`. The engine discovers it on the next `graphit sync` without recompilation.

### Framework Artifacts

A **framework** artifact packages a framework detection YAML file defining decorator, heritage, and import detection rules for a framework not included in the built-in defaults.

Content structure:

```
frameworks/
└── phoenix/
    └── phoenix.yaml              # Decorator, heritage, and import detection rules
```

When installed, the YAML file is placed into `<project>/.graphit/ast/frameworks/`. Detection rules merge with built-in defaults on the next `graphit sync`.

---

## 📐 Hub-Based Rule Overrides

The `main` branch of the Hub Git repository also serves as a **team-wide rule distribution** mechanism. When global rule files (e.g., `ast.md`, `improvements.md`, `memory.md`) are committed to the `main` branch of the Hub Git repository, they act as implicit overrides for all team members — distributed via git pull and applied automatically across all modules without requiring explicit installation.

This is part of the **multi-layer rule override system**. The `main` branch of the Hub Git repository sits at the third priority level, below project-level and global CLI overrides, but above the compiled-in defaults. For the complete specification of the override hierarchy, placeholder substitution, and CLI commands, see [docs/specs/rule_override.md](docs/specs/rule_override.md).

