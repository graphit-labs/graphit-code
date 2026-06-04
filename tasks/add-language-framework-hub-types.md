# Task: Add Language and Framework Hub Artifact Types to Documentation

**Date:** 2026-06-04
**Status:** Complete

## Summary

Added documentation for two new Hub artifact types — `language` (Tree-sitter grammars) and `framework` (framework detection YAML) — across all relevant documentation files.

## Changes

### 1. `docs/specs/hub_collaboration.md`
- **Git Store Layout**: Expanded to show all 10 artifact directories (was only showing rules/, skills/, commands/). Added `languages/`, `frameworks/`, `agents/`, `knowledge/`, `ast/`, `mcp-servers/`, and `powers/`.
- **Lockfile Example**: Added `language` and `framework` entries to the `artifacts` section with `version` and `origin` fields.
- **New Section**: Added "📦 Language and Framework Artifacts" section describing content structure, installation paths, and behavior for both types.

### 2. `docs/guides/user_manual.md`
- **Hub Manager Section**: Added "Hub Artifact Types" subsection under the Hub Manager (section 3) documenting:
  - What language artifacts contain (`.wasm` grammar + `.yaml` queries)
  - What framework artifacts contain (`.yaml` detection rules)
  - Installation commands (`graphit hub install elixir-lang@1.0`, etc.)
  - Installation paths (`<project>/.graphit/ast/grammars/`, `<project>/.graphit/ast/queries/`, `<project>/.graphit/ast/frameworks/`)

### 3. `README.md`
- **Private Hub Registry section**: Added two new bullet points for "Language Grammars" and "Framework Configs" to the artifact type list, matching the existing bold-title-em-dash-description style.

### 4. `docs/site/index.html`
- **Hub Grid**: Added two new `hub-card` entries:
  - `language` (color: `#84cc16` lime-500) — "Tree-sitter Grammars"
  - `framework` (color: `#0ea5e9` sky-500) — "Framework Configs"
- Grid goes from 8 to 10 items (3 rows in the existing 4-column layout).

## Design Decisions

- Used relative paths (`<project>/.graphit/...`) in documentation, never absolute filesystem paths.
- Hub grid CSS did not need changes — the existing `repeat(4, 1fr)` grid accommodates 10 items naturally across 3 rows.
- Color choices for new cards follow existing conventions: distinct hues not already used by other cards.
