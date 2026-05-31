---
title: "Rule Override Specification"
description: "Technical specification of the multi-layer rule and skill override system, covering project-level, global CLI, and Hub-based overrides with placeholder substitution."
content-type: reference
audience: developers
keywords:
  - rules
  - override
  - customization
  - skills
  - brand
  - hub
  - global rules
prerequisites:
  - "docs/specs/hub_collaboration.md"
  - "docs/architecture/architecture_overview.md"
related:
  - "docs/specs/improvements_module.md"
  - "docs/guides/user_manual.md"
  - "docs/guides/private_brand_customization.md"
---

# Rule Override Specification

Graphit Code uses a **multi-layer rule override system** that allows users to customize the behavior of every module's rules and skills. Overrides follow a strict precedence hierarchy: the most specific source wins, with each layer falling through to the next until a match is found or the compiled-in default is used.

This system applies uniformly to **all modules**: `ast`, `knowledge`, `memory`, `hub`, and `improvements`.

---

## Override Resolution Hierarchy

When a module's rule or skill content is resolved, the system checks the following sources **in order of precedence** (highest to lowest). The first source that provides a file wins:

```
┌─────────────────────────────────────────────────────┐
│  1. Project-Level Override       (highest priority) │
│     .graphit/rules/<module>.md                      │
│     .graphit/rules/<module>_skill.md                │
├─────────────────────────────────────────────────────┤
│  2. Global CLI Override                             │
│     ~/.graphit/rules/<module>.md                    │
│     ~/.graphit/rules/<module>_skill.md              │
├─────────────────────────────────────────────────────┤
│  3. Hub Main Branch Override                        │
│     rules/<module>.md  on the Hub Git repo          │
│     rules/<module>_skill.md  on the Hub Git repo    │
├─────────────────────────────────────────────────────┤
│  4. Compiled-In Default          (lowest priority)  │
│     Hardcoded in Go source code per module          │
└─────────────────────────────────────────────────────┘
```

### 1. Project-Level Override

- **Path**: `<project-root>/.graphit/rules/<module>.md`
- **Scope**: Applies only to the specific project where the file exists.
- **Use case**: A team wants custom rules for a particular repository (e.g., stricter security audit rules for a payments service).
- **How to set**: Manually create the file in the project's `.graphit/rules/` directory.

### 2. Global CLI Override

- **Path**: `~/.graphit/rules/<module>.md`
- **Scope**: Applies to all projects on the current machine that do not have their own project-level override.
- **Use case**: A developer wants to enforce personal conventions across all their projects.
- **How to set**: Use the CLI command:
  ```bash
  graphit <module> rule my-custom-rule.md
  ```
- **How to remove**: Restore the default with:
  ```bash
  graphit <module> rule --unset
  ```

### 3. Hub Main Branch Override

- **Source**: The `main` branch of the Hub Git repository (the shared, team-wide registry configured via `graphit setup` or `hub.repo`).
- **Location**: Rule and skill files are placed inside the `rules/` directory on the `main` branch of the Hub repository (e.g., `rules/ast.md`, `rules/memory_skill.md`).
- **Scope**: Applies to all projects for all team members who share the same Hub repository. This is the **team-wide** override mechanism.
- **Use case**: An organization wants to enforce standard coding conventions, security policies, or analysis rules across all developers and all projects — without requiring each developer to manually configure global rules.
- **How it works**: The Hub is backed by a Git repository that all team members clone and sync. The `main` branch of this repository holds the registry metadata, artifact index, and shared configuration. If a rule or skill file (following the same naming convention used by the CLI global overrides) exists inside the `rules/` directory on the `main` branch, it is picked up during resolution. This applies across **all modules** — any module whose rule or skill file is present will use that override.
- **How to set**: Commit rule and/or skill files to `rules/` on the `main` branch of the Hub Git repository and push. All team members will receive the override automatically on the next `graphit sync` or `graphit update`, when the Hub repository is pulled.

### 4. Compiled-In Default

- **Source**: Hardcoded in Go source code (e.g., `internal/ast/rule.go`, `internal/hub/rule.go`, `internal/memory/rule.go`, etc.).
- **Scope**: Universal fallback when no override exists at any level.
- **Content**: Each module defines its own `*RuleContent()` function that generates the default rule text.

---

## Rule Files — Naming Convention

| Override Type | Rule File | Skill File |
|---|---|---|
| Project-level | `.graphit/rules/<module>.md` | `.graphit/rules/<module>_skill.md` |
| Global CLI | `~/.graphit/rules/<module>.md` | `~/.graphit/rules/<module>_skill.md` |
| Hub main branch | `rules/<module>.md` on the Hub repo | `rules/<module>_skill.md` on the Hub repo |

Where `<module>` is one of: `ast`, `knowledge`, `memory`, `hub`, `improvements`.

---

## Placeholder Substitution

Override files support a **placeholder mechanism** that allows custom rules to embed the compiled-in default content at any point. This enables users to **wrap** the default rules with additional instructions rather than completely replacing them.

### Rule Placeholder

```
{{_GRAPHIT_DEFAULT_RULE_CONTENT_}}
```

When present in an override file, this placeholder is replaced with the module's compiled-in default rule content.

### Skill Placeholder

```
{{_GRAPHIT_DEFAULT_SKILL_CONTENT_}}
```

When present in an override file, this placeholder is replaced with the module's compiled-in default skill content.

### Example: Wrapping Default Rules

A custom rule file at `~/.graphit/rules/improvements.md`:

```markdown
# Custom Engineering Rules

## Company-Specific Requirements

- All public APIs must have OpenAPI specs before implementation.
- Database migrations must be reversible.
- All HTTP handlers must include rate limiting.

## Standard Analysis

{{_GRAPHIT_DEFAULT_RULE_CONTENT_}}

## Additional Security Requirements

- All endpoints must enforce mTLS in production.
- PII fields must use column-level encryption.
```

The placeholder `{{_GRAPHIT_DEFAULT_RULE_CONTENT_}}` is replaced at resolution time with the full compiled-in default content, so the custom file effectively **prepends** and **appends** to the standard rules.

### Branding

For white-label distributions, the placeholder adapts to the configured brand name:

```
{{_<BRAND_UPPER>_DEFAULT_RULE_CONTENT_}}
{{_<BRAND_UPPER>_DEFAULT_SKILL_CONTENT_}}
```

For example, with `Brand = "acme"`, the placeholders become `{{_ACME_DEFAULT_RULE_CONTENT_}}` and `{{_ACME_DEFAULT_SKILL_CONTENT_}}`.

---

## Resolution Functions

The resolution logic is implemented in `internal/brand/brand.go`:

### `ResolveModuleRule(module, defaultContent string) string`

Resolves the final rule content for a given module:

1. Check `<cwd>/.graphit/rules/<module>.md` → if found, read and substitute placeholders.
2. Check `~/.graphit/rules/<module>.md` → if found, read and substitute placeholders.
3. Check `rules/<module>.md` on the `main` branch of the Hub Git repository → if found, read and substitute placeholders.
4. Return `defaultContent` (the compiled-in default).

### `ResolveModuleSkill(module, defaultContent string) string`

Resolves the final skill content for a given module:

1. Check `<cwd>/.graphit/rules/<module>_skill.md` → if found, read and substitute placeholders.
2. Check `~/.graphit/rules/<module>_skill.md` → if found, read and substitute placeholders.
3. Check `rules/<module>_skill.md` on the `main` branch of the Hub Git repository → if found, read and substitute placeholders.
4. Return `defaultContent` (the compiled-in default).

---

## CLI Commands

Each module exposes a `rule` subcommand for managing global CLI overrides:

```bash
# Show the resolved rule (respecting all override layers)
graphit <module> rule

# Show only the compiled-in default (ignoring all overrides)
graphit <module> rule --default

# Set a custom global CLI override
graphit <module> rule my-custom-rule.md

# Remove the global CLI override (restores default or Hub fallback)
graphit <module> rule --unset
```

Where `<module>` is one of: `ast`, `knowledge`, `memory`, `hub`, `improvements`.

### What happens on `--unset`

When `--unset` is used:
1. The file at `~/.graphit/rules/<module>.md` is deleted.
2. The next resolution will fall through to the Hub main branch override (if it exists) or the compiled-in default.
3. If a project-level override exists, it remains unaffected.

---

## Hub-Based Rule Distribution

The `main` branch of the Hub Git repository serves as the **team-wide rule distribution** mechanism. This is distinct from Hub `rule` type artifacts (which are installable packages). The main branch rule files are **implicit overrides** — they apply automatically to all team members without explicit installation.

### How it works

1. An admin commits rule files (e.g., `improvements.md`, `ast.md`) to the `main` branch of the Hub Git repository and pushes.
2. Each developer's local Hub clone is synced on `graphit sync` or `graphit update`, pulling the latest `main` branch.
3. During rule resolution, if no project-level or global CLI override exists, the system checks the `main` branch for a matching rule file.
4. If found, that file is used (with placeholder substitution).
5. If not found, the compiled-in default is used.

### Precedence in practice

| Scenario | Result |
|---|---|
| Project has `.graphit/rules/ast.md` | Project file wins |
| No project file, user has `~/.graphit/rules/ast.md` | User global file wins |
| No project or user file, Hub main has `ast.md` | Hub file wins |
| No override at any level | Compiled-in default is used |
| Project file contains `{{_GRAPHIT_DEFAULT_RULE_CONTENT_}}` | Project file with default content embedded |
| Hub file contains `{{_GRAPHIT_DEFAULT_RULE_CONTENT_}}` | Hub file with default content embedded |

### Relationship to Hub `rule` type artifacts

Hub `rule` type artifacts are a **different mechanism**:
- **Hub `main` branch rule files** are implicit overrides committed directly to the `main` branch of the Hub Git repository. They customize the behavior of Graphit Code's own modules (ast, knowledge, memory, hub, improvements) and are distributed to all team members via git pull.
- **Hub `rule` type artifacts** are installable packages stored on dedicated artifact branches in the Hub Git repository. They inject additional rule blocks into the IDE rules file (e.g., coding conventions for a specific framework) and are installed explicitly via `graphit hub install`.

Both can coexist. The `main` branch rule files control how the framework's modules behave, while `rule` artifacts add supplementary content.

---

## Module Rule Architecture

Each module follows the same pattern:

```
internal/<module>/rule.go
├── *RuleContent()          → generates the compiled-in default rule text
├── *RouterContent()        → generates the compact router/summary for AGENTS.md
├── InstallRule()           → installs the resolved rule into the IDE config
│   └── calls brand.ResolveModuleRule(module, defaultContent)
├── InstallSkill()          → installs the resolved skill into the IDE skills dir
│   └── calls brand.ResolveModuleSkill(module, defaultContent)
├── RemoveRule()            → removes the rule block from the IDE config
└── RemoveSkill()           → removes the skill from the IDE skills dir
```

The `InstallRule()` function in each module calls `brand.ResolveModuleRule()` to get the final content, then injects it into the IDE configuration file (e.g., `AGENTS.md`, `.cursorrules`) as a managed sentinel block.

---

## Skill Override

The skill override system follows the **exact same hierarchy** as rules, but uses different file names:

| Module | Rule File | Skill File |
|---|---|---|
| `ast` | `ast.md` | `ast_skill.md` |
| `knowledge` | `knowledge.md` | `knowledge_skill.md` |
| `memory` | `memory.md` | `memory_skill.md` |
| `hub` | `hub.md` | `hub_skill.md` |
| `improvements` | `improvements.md` | `improvements_skill.md` |

Skills are the detailed instruction files that agents read on-demand (stored in the IDE's skills directory). Rules are the compact summaries injected into the global rules file (e.g., `AGENTS.md`).

---

## Cross-Platform Compatibility

The rule override system uses `os.UserHomeDir()` to resolve the home directory, making it compatible with:

- **Linux/macOS**: `~/.graphit/rules/`
- **Windows**: `%USERPROFILE%\.graphit\rules\`

All path operations use `filepath.Join()` for cross-platform separator handling.
