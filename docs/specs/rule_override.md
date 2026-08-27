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
│  3. Hub Bucket Override                             │
│     rules/<module>.md  in the Hub bucket            │
│     rules/<module>_skill.md  in the Hub bucket      │
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

### 3. Hub Bucket Override

- **Source**: The `rules/` prefix of the shared S3-backed Hub configured by `graphit setup`.
- **Location**: Rule and skill objects use keys such as `rules/ast.md` and `rules/memory_skill.md`.
- **Scope**: Applies to all projects for all team members who share the same Hub bucket/prefix. This is the **team-wide** override mechanism.
- **Use case**: An organization wants to enforce standard coding conventions, security policies, or analysis rules across all developers and all projects — without requiring each developer to manually configure global rules.
- **How it works**: The S3-backed Hub stores registry data and shared rule objects under distinct prefixes. If a matching rule or skill object exists under `rules/`, resolution uses it for that module.
- **How to set**: Publish the rule/skill objects to the Hub `rules/` prefix. Team members receive them on the next `graphit sync` or `graphit update`.

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
| Hub bucket | `rules/<module>.md` in the Hub bucket | `rules/<module>_skill.md` in the Hub bucket |

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
2. The next resolution will fall through to the Hub bucket override (if it exists) or the compiled-in default.
3. If a project-level override exists, it remains unaffected.

---

## Hub-Based Rule Distribution

The `rules/` prefix of the Hub bucket serves as the **team-wide rule distribution** mechanism. This is distinct from Hub `rule` type artifacts (which are installable packages). Prefix rule files are **implicit overrides** — they apply automatically without explicit installation.

### How it works

1. An admin publishes rule objects (e.g., `rules/improvements.md`, `rules/ast.md`) to the Hub bucket.
2. Each developer refreshes Hub data on `graphit sync` or `graphit update`.
3. During rule resolution, if no project-level or global CLI override exists, the system checks the Hub `rules/` prefix for a matching file.
4. If found, that file is used (with placeholder substitution).
5. If not found, the compiled-in default is used.

### Precedence in practice

| Scenario | Result |
|---|---|
| Project has `.graphit/rules/ast.md` | Project file wins |
| No project file, user has `~/.graphit/rules/ast.md` | User global file wins |
| No project or user file, Hub has `rules/ast.md` | Hub file wins |
| No override at any level | Compiled-in default is used |
| Project file contains `{{_GRAPHIT_DEFAULT_RULE_CONTENT_}}` | Project file with default content embedded |
| Hub file contains `{{_GRAPHIT_DEFAULT_RULE_CONTENT_}}` | Hub file with default content embedded |

### Relationship to Hub `rule` type artifacts

Hub `rule` type artifacts are a **different mechanism**:
- **Hub `rules/` prefix files** are implicit overrides. They customize the behavior of Graphit Code's own modules (ast, knowledge, memory, hub, improvements) and are distributed to all team members through Hub synchronization.
- **Hub `rule` type artifacts** are versioned installable packages stored under artifact prefixes. They inject additional rule blocks into the IDE rules file and are installed explicitly via `graphit hub install`.

Both can coexist. The prefix rule files control how the framework's modules behave, while `rule` artifacts add supplementary content.

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

### Skill Frontmatter

Every managed `SKILL.md` opens with a YAML frontmatter block carrying exactly two fields, built
by `ide.SkillFrontmatter` (`internal/hub/adapters/ide/adapters.go`) and prepended to the
resolved skill content:

| Field | Contract |
|---|---|
| `name` | must equal the skill's directory name: lowercase letters, digits and single separating hyphens, at most `ide.MaxSkillNameLength` (64) characters |
| `description` | non-empty, at most `ide.MaxSkillDescriptionLength` (1024) characters, counted in runes |

The block is produced by **`yaml.Marshal`, never by string concatenation**, and this is a
correctness requirement rather than a style preference. Module descriptions are prose written
for a model — "Use when: …", "MANDATORY: …" — and a plain YAML scalar may not contain `": "`: a
strict parser reads the colon as a nested mapping and rejects the entire block. A skill whose
frontmatter does not parse is not degraded, it is **invisible**: the IDE discovers no metadata,
never offers the skill, and logs nothing. Kiro's loader is strict and skipped all five skills
this way while Claude Code's lenient one loaded the very same files.

Both fields are validated before marshalling, and a violation is returned as an error that
fails the sync, because every value outside these limits is dropped by the IDE in the same
silent way. One consequence is worth naming: a white-label build whose `brand.Brand` is not
already a valid name fragment (`MyCorp` → `MyCorp-ast`) now fails loudly at skill installation
instead of shipping skills no strict IDE will load.

`internal/hub/adapters/ide/frontmatter_test.go` locks the serialization down with values a
hand-written quoter gets wrong (`: `, quotes, backslashes, leading `%`/`#`/`-`/`?`/`&`/`*`,
scalars that resolve to bool/null/number/date, leading and trailing whitespace, tabs, newlines,
non-ASCII), asserting the value round-trips byte-identical.
`cmd/graphit/commands/managed_skills_frontmatter_test.go` then installs all five skills for
every supported IDE and reads them back the way an IDE would, and additionally asserts that the
descriptions still contain `": "` — otherwise valid frontmatter would only prove the content had
become bland, not that quoting works.

Agents also write skill frontmatter by hand, so the same contract is stated where they are
instructed to: the dream module's skill-crystallization prompt (`internal/dream/prompt.go`) and
Step 3b of the improvements methodology (`internal/improvements/rules.go`).

---

## Cross-Platform Compatibility

The rule override system uses `os.UserHomeDir()` to resolve the home directory, making it compatible with:

- **Linux/macOS**: `~/.graphit/rules/`
- **Windows**: `%USERPROFILE%\.graphit\rules\`

All path operations use `filepath.Join()` for cross-platform separator handling.
