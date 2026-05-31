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
├── rules/                    # System rules templates
│   ├── golang_conventions.md
│   └── react_styling.md
├── skills/                   # Custom agent skill sets
│   ├── k8s-debugger/
│   │   ├── SKILL.md
│   │   └── scripts/
│   └── pg-optimizer/
└── commands/                 # Executable agent shortcuts
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

## 📐 Hub-Based Rule Overrides

The `main` branch of the Hub Git repository also serves as a **team-wide rule distribution** mechanism. When global rule files (e.g., `ast.md`, `improvements.md`, `memory.md`) are committed to the `main` branch of the Hub Git repository, they act as implicit overrides for all team members — distributed via git pull and applied automatically across all modules without requiring explicit installation.

This is part of the **multi-layer rule override system**. The `main` branch of the Hub Git repository sits at the third priority level, below project-level and global CLI overrides, but above the compiled-in defaults. For the complete specification of the override hierarchy, placeholder substitution, and CLI commands, see [docs/specs/rule_override.md](docs/specs/rule_override.md).

