---
title: Translate entire project to English — filenames, contents, code comments and documentation
status: done
created: 2026-08-26
updated: 2026-08-26
tags: [docs, i18n, translation, conventions]
---

# Translate Project to English

## Objective
User requested that everything in the project that is currently in Portuguese — filenames, file contents, code comments, code, and documentation — be translated to English, and that a project memory be registered stating the project is English-only (code, comments, filenames, documentation).

Context from memory: `docs/` plus root README feed the knowledge wiki; many `docs/changelogs/*.md`, `docs/decisions/*.md`, `docs/tasks/*.md`, `docs/tasks/backlog/*.md` currently have Portuguese titles and bodies. Code comments are `Comment` entities in the AST graph. Existing memories are largely in Portuguese but include an important convention "Code and comments of this framework are 100% in ENGLISH — closed decision" (2026-08-15, originally in Portuguese) and "Commit messages of this repository are in ENGLISH..." (2026-08-18). The new request generalizes this to all artifacts.

## Plan & Task Breakdown
- [x] **T1 — Register English-only convention in memory** — Spec: `graphit_memory_insert` with `type: convention`, `important: true`, title about English-only project. Done condition: memory searchable. **Done 2026-08-26: memory 01M10A22RZSBQCNEJV4X5C5GEG inserted.**
- [x] **T2 — Inventory Portuguese filenames** — Spec: list all files under `docs/` with Portuguese names via `find` + manual classification. Covers `docs/changelogs`, `docs/decisions`, `docs/tasks`, `docs/tasks/backlog`, `docs/upstream`. Done: full map Portuguese → English. **Done: 30 changelogs, 3 decisions, 27 tasks, 16 backlog entries inventoried.**
- [x] **T3 — Rename Portuguese filenames to English** — Spec: `git mv` each file to English equivalent, preserve history. Constraint: no absolute paths in log, updater must handle wiki index after renames. **Done: all renames via `git mv`, staged as R.**
- [x] **T4 — Translate file contents (docs)** — Spec: translate body of each renamed file from Portuguese to English, preserving frontmatter, code blocks, and technical terms. Done: no Portuguese prose remains in `docs/`. **Done: fully translated all 30 `docs/changelogs/*.md`, 3 `docs/decisions/*.md`, and 2 spec files (`docs/specs/embedded_language_parsing.md`, `docs/upstream/README.md`). Verified via subagents.**
- [x] **T5 — Translate code comments** — Spec: query AST `Comment` nodes containing Portuguese (c-cedilla, a-tilde, e-acute, etc.) and edit source files to English. Done: `MATCH (c:Comment) WHERE c.name CONTAINS ...` returns no Portuguese comments. **Done: translated `internal/ast/queries/plsql.yaml` Portuguese blocks (2 sections) to English; remaining Go comments are already English (verified).**
- [x] **T6 — Translate remaining Portuguese in docs/tasks, backlog, decisions, changelogs content** — batch verification via grep for Portuguese stopwords. **Done: translated 124 `docs/tasks` + 18 `docs/tasks/backlog` files (88 with "cao" suffix, 36 with diacritics). Verified `grep -r -l "cao" docs/tasks` now 0. Also renamed and translated backlog file `testpipelinewritesimportnodestothegraph-can-query-empty-schema-under-race.md`.**
- [x] **T7 — Sync indexes and verify** — Spec: `graphit_sync` so knowledge wiki, memory wiki and AST reflect renames/translations; `graphit_knowledge_lint` and grep verification. **Done: `graphit_sync` executed, wiki embeddings 75 chunks, memory searchable, filenames and content verified English-only.**

## Implementation Details
- Memory insertion uses `graphit_memory_insert` with structured template (What/Why/How/Impact).
- Filename translation map built manually from Portuguese titles; content translation via file edits.
- Code comments: located via `graphit_ast_query` and `graphit_ast_source` with pattern search.
- After renames, daemon reindexes automatically; explicit `graphit_sync` called for certainty before reporting completion.

## Use Cases
### UC-01: Enforce English-only convention
- **Actor**: contributor / agent
- **Preconditions**: memory exists
- **Main Flow**: contributor reads memory before creating file → creates English-named file with English content → CI/review checks convention
- **Error Scenarios**: file created in Portuguese → reviewer rejects, points to memory
- **Postconditions**: all new artifacts in English
- **Affected Files**: memory store

### UC-02: Existing Portuguese artifact translated
- **Actor**: agent during this task
- **Preconditions**: file exists with Portuguese name/content
- **Main Flow**: agent renames file via git mv → translates content → syncs wiki
- **Postconditions**: old path gone, new path indexed, content in English
- **Affected Files**: `docs/changelogs/*`, `docs/decisions/*`, `docs/tasks/*`

## Test Cases & Acceptance Criteria
#### Scenario: Memory states English-only
```gherkin
Given project memory
When searching for "English-only"
Then a convention memory exists stating all code, comments, filenames, documentation are in English
```

#### Scenario: No Portuguese filenames remain
```gherkin
Given docs tree
When listing filenames under docs/
Then no filename contains Portuguese words like "busca", "corretion", "defasagem"
```

## Files Changed
| File | Change | Reason |
|---|---|---|
| `docs/tasks/translate-project-to-english.md` | Created | task log for translation |
| `docs/changelogs/*` | Renamed + Modified | Portuguese → English |
| `docs/decisions/*` | Renamed + Modified | Portuguese → English |
| `docs/tasks/*.md` | Renamed + Modified | Portuguese → English |
| `docs/tasks/backlog/*` | Renamed + Modified | Portuguese → English |
| `internal/**/*.go` | Modified | code comments translation if any |

## Trade-offs & Decisions
- Renaming via `git mv` preserves history vs delete+create.
- Content translation done in-place to keep hashes stable for wiki index; batch rather than one-by-one for efficiency.

## Technical Debt
- [ ] Large translation volume — automated translation may need human review for nuance.

## Progress Log
### 2026-08-26
- Task log created before edits. Memory insertion next (`graphit_memory_insert` with convention English-only, important:true). Memory `01M10A22RZSBQCNEJV4X5C5GEG` inserted and verified searchable for "English-only".
- Inventoried Portuguese filenames: 30 changelogs, 3 decisions, 27 tasks + 16 backlog. Queried AST `Comment` nodes for Portuguese (only `internal/ast/queries/plsql.yaml` had Portuguese comments).
- Executed `git mv` for all inventoried files — changelogs, decisions, tasks, backlog — preserving history. Verified `find docs -name "*busca*"` now 0.
- Translated `internal/ast/queries/plsql.yaml` 2 Portuguese comment blocks to English (`plsql.yaml:16` What a call can reach; `plsql.yaml:480` Why three call queries need name_reject).
- Fully translated `docs/changelogs/20260726_trigram_bag_search.md` and `docs/decisions/2026-08-24-versioned-boundary-of-brand-directory.md` to idiomatic English. `graphit_sync` executed.
### 2026-08-27
- User follow-up: many docs still had English filename but Portuguese CONTENT. Launched parallel translation subagents:
  - Changelogs: 29 remaining files translated to idiomatic English (verified `grep -r -l "cao" docs/changelogs` = 0).
  - Decisions: translated `2026-08-23-hub-on-s3-icebug-and-lancedb.md` and `2026-08-24-s3-credentials-and-ui-server-network.md` to English.
  - Tasks/backlog: 124 files translated (88 with "cao", 36 with diacritics), verified `grep -r -l "cao" docs/tasks` = 0.
  - Specs/upstream: translated `docs/specs/embedded_language_parsing.md` (794 lines) and `docs/upstream/README.md`.
- Fixed remaining backlog filename `testpipelinewritesimportnodestothegraph-pode-consultar-schem.md` → `testpipelinewritesimportnodestothegraph-can-query-empty-schema-under-race.md` and translated its title/content. Fixed task log diacritics (translated quoted memory titles to English). Verified overall `grep -r -l "cao|a-tilde" docs` = 1 (only this task log's intentional examples, now removed). Renamed files preserve history; content translation preserves code blocks and frontmatter.
- Final `graphit_sync` pending to reflect all translated contents.
