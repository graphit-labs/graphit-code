# Task: Create Module Specifications

**Date**: 2026-06-02
**Status**: ✅ Complete

## Summary

Created 4 comprehensive module specification documents for internal packages:

1. `docs/specs/mcpstdio_module.md` — MCP stdio server module
2. `docs/specs/config_module.md` — Configuration system module
3. `docs/specs/git_module.md` — Git abstraction layer module
4. `docs/specs/output_module.md` — Output/view layer module

## Files Created

- `docs/specs/mcpstdio_module.md` — Covers architecture, safeTool wrapper, context resolution, all 10 tool categories (62 tools), error handling, and dependencies.
- `docs/specs/config_module.md` — Covers layered resolution order, ConfigMap type, CRUD operations, IDE/CLI resolution, module enable/disable system, global config JSON, and compiled defaults.
- `docs/specs/git_module.md` — Covers singleton pattern, Git interface, CLI backend, SSH error wrapping, block manager (inject/remove), hooks (post-commit, pre-push, post-merge), ignore patterns, and stderr filtering.
- `docs/specs/output_module.md` — Covers Printer abstraction, semantic output methods, Task spinner, muting for MCP mode, TTY detection, color system, and Fatal/Interrupted handlers.

## Approach

Each spec was written by thoroughly reading all source files in the corresponding `internal/` package. All specs follow the established pattern from `docs/specs/daemon_module.md`:
- YAML frontmatter with title, description, content-type, audience, keywords, prerequisites, and related docs
- Mermaid architecture diagrams
- Detailed type/interface tables
- Error handling documentation
- Internal and external dependency listings

## Source Files Read

### mcpstdio (12 files)
- `internal/mcpstdio/server.go`
- `internal/mcpstdio/context.go`
- `internal/mcpstdio/tools_lifecycle.go`
- `internal/mcpstdio/tools_ast.go`
- `internal/mcpstdio/tools_knowledge.go`
- `internal/mcpstdio/tools_memory.go`
- `internal/mcpstdio/tools_hub.go`
- `internal/mcpstdio/tools_wiki.go`
- `internal/mcpstdio/tools_dream.go`
- `internal/mcpstdio/tools_daemon.go`
- `internal/mcpstdio/tools_cluster.go`
- `internal/mcpstdio/tools_improvements.go`

### config (1 file)
- `internal/config/config.go`

### git (6 files)
- `internal/git/git.go`
- `internal/git/cli_backend.go`
- `internal/git/block_manager.go`
- `internal/git/hooks.go`
- `internal/git/ignore.go`
- `internal/git/stderr.go`

### output (1 file)
- `internal/output/printer.go`
