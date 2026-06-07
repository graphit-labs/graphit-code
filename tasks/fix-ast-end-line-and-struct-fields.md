# AST YAML Audit — All Languages + PL/SQL Enhancement

## Status: ✅ Complete

### Languages Audited (Dead Code Cleanup)
- [x] **Go** — Reescrito. Bugs: endLine, HAS_FIELD, imports.
- [x] **SQL** — Limpo
- [x] **JavaScript** — Reescrito
- [x] **TypeScript** — Reescrito
- [x] **TSX** — Reescrito
- [x] **Python** — Subagent (~40 dead entries)
- [x] **Ruby** — Subagent (~40 dead entries)
- [x] **PHP** — Subagent (~40 dead entries)
- [x] **Java** — Subagent (~55% dead code)
- [x] **Kotlin** — Subagent (~75% dead code + bugs)
- [x] **Dart** — Subagent (~80% dead code + critical bug)
- [x] **C** — Subagent
- [x] **C++** — Subagent (self_keywords bug)
- [x] **C#** — Subagent
- [x] **Rust** — Subagent (self_keywords bug)
- [x] **Swift** — Subagent (copy-paste bug)
- [x] **XML** — Já correto

### PL/SQL Enhancement (ANTLR)
- [x] Reescrito de 375 → 687 linhas
- [x] Adicionadas entidades: TYPE BODY, SUBTYPE, RECORD/TABLE/VARRAY types, REF CURSOR, tablespace, directory
- [x] Adicionadas relações: trigger→table, FK references, procedure_call, multi-table insert
- [x] Adicionados todos os DROP (function, procedure, package, trigger, sequence, type, synonym, dblink)
- [x] Adicionados todos os ALTER (index, view, sequence, trigger)
- [x] Adicionados REVOKE, cursor refs (OPEN/FETCH/CLOSE), RAISE, exception handlers
- [x] Mudado columns para Field label, parameters para Parameter label
- [x] Adicionado entry_points para PL/SQL

### Engine Fixes (Go code)
- [x] `treesitter_adapter.go` — EndLine fix
- [x] `cache_convert.go` — Pre-populate nameToUID
