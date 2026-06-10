# Task: Dead Code Cleanup

Cleanup all unused, dead, and unreachable functions, methods, and structures identified by the `deadcode` tool across the codebase.

## Completed Changes

- [x] **AST Adapter**
  - Removed unused ANTLR/Treesitter functions in `internal/ast/antlr_adapter.go` and `internal/ast/treesitter_adapter.go`.
- [x] **Daemon Package**
  - Removed `MemorySyncModule.Name`.
  - Removed `ModuleState.String`, `moduleEntry.status`, and `moduleEntry.String`.
  - Removed `ProjectSupervisor.Status`.
  - Rewrote corresponding tests to use direct access where needed or cleaned up assertions.
- [x] **Hub Package**
  - Removed unused hash utilities `TruncateHash` and `VerifyHash` from `internal/hub/hash.go` (moved to test-only helpers).
  - Removed dead HTTP/UI methods and endpoints from `internal/hub/ui_server.go` (moved to test-only helpers).
  - Removed unused generic rules-block injection utilities from `internal/hub/adapters/ide/adapters.go`.
- [x] **MCP Stdio Package**
  - Removed `Serve` and `nopWriteCloser` (moved to test-only helpers).
- [x] **Memory Package**
  - Removed unused context cycle runners, branch names, and wiki index initializers from `internal/memory/cycle.go`, `internal/memory/important.go`, and `internal/memory/paths.go`.
- [x] **Output Printer Package**
  - Removed unused printer helpers from `internal/output/printer.go` (moved to test-only helpers).
- [x] **Test Verification**
  - Fixed all resulting test and build issues across packages.
  - Verified that all unit tests and CI checks pass successfully (`make ci` runs clean).
  - Verified `deadcode` tool returns no internal code issues (only in `internal/ui/node_modules/...` which are excluded).
