---
title: Fix MCP sync AST cache directory
status: completed
created: 2026-09-02
updated: 2026-09-02
tags: [ast, mcp, sync, storage, regression]
---

# Fix MCP sync AST cache directory

## Objective

Stop `graphit_sync` through MCP from writing `manifest.json` and `shards/` directly under
`~/.graphit/ast/project`. The cache must live in the project-ID-scoped store returned by
`astConfigForProject(projectDir, "").StoreDir`.

## Diagnosis and reasoning

`LadybugConfig.StoreDir` used to be represented by `DBPath`, a path to the database file. The MCP
sync path correctly used `filepath.Dir(DBPath)` as the parse-cache directory. Commit `6fa096d0`
replaced `DBPath` with `StoreDir`, which already denotes the complete store directory, but retained
the `filepath.Dir` call. That strips the project ID and causes all MCP syncs to share and pollute the
namespace parent.

The daemon path is unaffected: `SyncModule.reindexAST` passes `cfg.StoreDir` directly. The CLI path
is also unaffected. The defect is isolated to the MCP lifecycle sync handler.

## Approach

1. Extract or use a directly testable helper for the MCP sync pipeline options.
2. Pass `StoreDir` unchanged as `PipelineOptions.CacheDir`.
3. Add a regression test that creates a project identity and proves the cache is scoped below that
   identity rather than its namespace parent.
4. Run focused MCP tests and broader package verification.
5. Rebuild/update the development runtime used by Graphit and remove the exact orphaned artifacts.

## Progress

- Added `syncASTPipelineOptions`, which passes `StoreDir` unchanged as the cache directory.
- Added a regression test with an explicit project ID; it rejects the namespace parent as the
  cache path.
- Focused regression verification passed:
  `go test ./internal/mcpstdio -run 'TestSyncASTPipelineOptionsKeepsTheProjectIDInTheCachePath|TestASTConfigForProjectResolvesTheNamedProjectsStore' -count=1`.
- Full `internal/mcpstdio` tests and `go vet ./internal/mcpstdio` passed.
- Rebuilt `cmd/mcp` and atomically replaced the development runtime binary at
  `~/.graphit/runtime/dev/graphit-mcp`; its SHA-256 changed from `4d1dc6f1...` to `96131ded...`.
- Ran `graphit_sync` after the code change so the project indexes and knowledge are current. The
  current MCP process still holds the pre-fix executable inode, so orphan cleanup remains last.
- The pre-fix process reproduced the bug at `23:43:11`, proving that the MCP sync path was the
  writer. No process held either orphan target open at cleanup time.
- Moved the exact root-level `manifest.json` and `shards/` (1,774 files, about 9.1 MB) to the
  desktop trash. Both project-ID-scoped stores were left untouched.

## Result

MCP sync now keeps its parse cache inside the project-ID-scoped AST store. The regression is
covered, the development MCP runtime has been replaced atomically, and the invalid root-level
cache has been removed recoverably.

## Acceptance criteria

- MCP `graphit_sync` uses `~/.graphit/ast/project/<project-id>` for shards and the manifest.
- A regression test fails if the project-ID segment is stripped.
- Focused tests and relevant package tests pass.
- The active development runtime contains the fix.
- The root-level orphan `manifest.json` and `shards/` are removed without touching ID-scoped stores.

## Affected files

- `internal/mcpstdio/tools_lifecycle.go`
- `internal/mcpstdio/tools_lifecycle_test.go` or a focused new test file
- `docs/tasks/fix-mcp-sync-cache-dir.md`

## Trade-offs and debt

The fix intentionally does not add compatibility, migration, or fallback behavior for the invalid
root-level cache. It corrects the writer and removes the generated artifacts once the runtime is
updated.
