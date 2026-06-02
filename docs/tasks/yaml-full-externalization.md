---
title: Full YAML Externalization of AST Pipeline
status: done
created: 2026-06-01
updated: 2026-06-02
tags: [ast, yaml, externalization, refactoring]
---

# Full YAML Externalization of AST Pipeline

## Objective

Transform the AST extraction engine from a binary with hardcoded language/framework knowledge into a pure YAML-driven engine. All language-specific logic — export detection, self keywords, context types, declaration types, framework detection (decorators, heritage, imports), ecosystem detection, and entry point scoring — is now runtime-customizable via YAML files. Adding new frameworks, ecosystems, and customizing existing language behavior requires no recompilation. Adding support for entirely new languages still requires compiling the Tree-sitter grammar.

## Implementation Details

### Phase 1: Query Externalization

Extracted all Tree-sitter query patterns from hardcoded Go maps into **16 language-specific YAML files** under `internal/ast/queries/`. Created `query_loader.go` to load, cache, and resolve queries at runtime using a **4-level resolution chain**:

1. **Project** (`.graphit/ast/queries/<lang>.yaml`) — per-project overrides
2. **User Global** (`~/.graphit/ast/queries/<lang>.yaml`) — user-wide customizations
3. **Runtime** (`~/.graphit/runtime/<version>/ast/queries/`) — factory defaults extracted from the binary
4. Internally, embedded defaults (compiled via `//go:embed`) serve as a fallback if the runtime directory is unavailable

Each YAML file defines extraction patterns for functions, classes, methods, variables, imports, calls, fields, parameters, and other language-specific entities using Tree-sitter S-expression queries with named capture groups.

### Phase 2: Language Config Extension

Extended each of the 16 language YAML files to include full language configuration beyond just query patterns:

- **`exports`** — patterns and conventions for detecting exported/public symbols (e.g., capitalized names in Go, `export` keywords in TypeScript/JavaScript)
- **`self_keywords`** — language-specific `self`/`this` keywords (e.g., `self` in Python/Rust/Swift, `this` in Java/TypeScript/C#/Kotlin/Dart/PHP, `@` in Ruby)
- **`context_types`** — mapping of Tree-sitter node types to Graphit context categories (e.g., `function_declaration → function`, `class_declaration → class`, `method_declaration → method`)
- **`declaration_types`** — node types that represent entity declarations for each language

This replaced multiple hardcoded Go `switch` statements and `map[string]` variables in `treesitter_adapter.go` with dynamic lookups from YAML.

### Phase 3: Framework Externalization

Created **59 framework YAML files** under `internal/ast/frameworks/`, covering **51+ framework definitions** across all supported languages. Each framework YAML specifies:

- **`decorators`** — decorator/annotation patterns that identify framework usage (e.g., `@Controller`, `@injectable`, `@app.route`)
- **`heritage`** — base classes/interfaces that indicate framework adoption (e.g., `APIView`, `StatelessWidget`, `React.Component`)
- **`imports`** — import patterns that signal framework presence (e.g., `from fastapi import`, `import express`, `package:flutter/`)
- **`ecosystem`** — the ecosystem this framework belongs to (e.g., `web`, `mobile`, `api`)

Simplified `enrichment.go` from **975 lines to 327 lines** by removing all hardcoded decorator maps, heritage maps, and import maps. The enrichment pipeline now dynamically loads framework definitions from YAML and applies matching rules generically.

### Phase 4: Ecosystem Externalization

Created `internal/ast/ecosystems.yaml` containing **120+ ecosystem detection entries**. Each entry maps import patterns or file indicators to ecosystem classifications:

- Web frameworks (Express, Next.js, Django, Rails, ASP.NET, etc.)
- Mobile SDKs (Flutter, React Native, SwiftUI, Android SDK, etc.)
- API frameworks (FastAPI, Gin, Spring Boot, etc.)
- Database drivers and ORMs
- Testing frameworks
- CLI tools and infrastructure libraries

The ecosystem detection engine reads this file at startup and applies pattern matching against a project's import graph to classify the project's technology stack.

### Phase 5: Hardcoded Removal

Systematically deleted all remaining hardcoded fallback logic:

- **`scoreFunction` deleted** — entry point scoring was previously hardcoded in Go; now driven entirely by scoring rules in each language YAML
- **`matchDecorators` deleted** — decorator matching was a complex Go function with embedded string maps; now a generic YAML-driven matcher
- **All fallback branches removed** — the engine no longer has "if YAML not found, use hardcoded default" paths. YAML is the **only** source of truth. If a language YAML is missing, the engine simply has no patterns for that language.

### Phase 6: Atomization

Eliminated `base_scoring.yaml` (a shared scoring configuration file) by embedding all scoring rules directly into each language YAML file. Each `_lang.yaml` is now fully **self-contained** — it includes queries, language config, and scoring rules in a single atomic file. This ensures:

- No implicit dependencies between YAML files
- Each language can define entirely custom scoring without inheriting defaults
- Overriding a language at the project level gives complete control

## Files Changed

| File | Status | Description |
|---|---|---|
| `internal/ast/query_loader.go` | Modified | 4-level resolution chain, loading, caching, thread-safe lazy initialization |
| `internal/ast/queries_embed.go` | Modified | Updated `//go:embed` directives to include all YAML resources |
| `internal/ast/enrichment.go` | Modified | Reduced from 975→327 lines; all hardcoded decorator/heritage/import maps removed |
| `internal/ast/treesitter_adapter.go` | Modified | Hardcoded `switch` statements for exports, self keywords, and context types removed |
| `internal/ast/queries/*.yaml` | Created/Modified | 16 language YAML files (Go, TypeScript, JavaScript, Python, Java, Rust, C, C++, C#, Kotlin, Swift, Dart, PHP, Ruby, SQL, HCL) |
| `internal/ast/frameworks/*.yaml` | Created | 59 framework YAML files covering 51+ frameworks |
| `internal/ast/ecosystems.yaml` | Created | 120+ ecosystem detection entries |

## Key Decisions

### No Hardcoded Fallbacks
YAML is the **only** source of truth. There are zero Go-side fallback maps or default values. If a YAML file does not define a pattern, the pattern simply does not exist. This forces explicit, auditable configuration and eliminates hidden behavior.

### Atomic Language Files
Each `<lang>.yaml` is fully self-contained: queries + language config + scoring rules. No shared base files, no inheritance. This maximizes override clarity — when a user customizes a language at the project level, they get complete, predictable control.

### 4-Level Resolution Chain
The resolution chain (Project > User Global > Runtime) provides maximum flexibility:
- **Project-level** overrides let teams customize per-repository
- **User-global** overrides let individual developers customize across all projects
- **Runtime** defaults are auto-extracted from the binary on first run, providing a stable baseline

### Framework Merging Strategy
Frameworks **merge additively** across resolution levels. If a project defines additional frameworks, they are added to (not replacing) the runtime/embedded frameworks. This allows teams to add custom framework definitions without losing built-in ones. In contrast, query patterns use **precedence** — a project-level query definition fully replaces the same query from lower levels.

## Impact

- **No recompilation required** to add new frameworks, ecosystem patterns, or customize existing language behavior
- **New language support** requires compiling the Tree-sitter grammar, but all extraction rules are YAML-driven
- **Community contributions** can be YAML-only PRs for frameworks and ecosystems — no Go knowledge needed
- **Enterprise customization** is trivial: drop YAML files in the project or user directory
- **`enrichment.go`** reduced by 66% (975 → 327 lines), dramatically improving maintainability
- **`treesitter_adapter.go`** simplified by removing all language-specific `switch` blocks
- **Testing** is simplified: YAML files can be validated independently of the Go compilation pipeline

## Verification

- All 16 language parsers produce identical AST output before and after externalization
- Framework detection matches all previously hardcoded patterns
- Ecosystem classification produces identical results
- Entry point scoring produces identical scores
- Resolution chain correctly prioritizes project > user > runtime > embedded
- Incremental indexing remains unaffected
