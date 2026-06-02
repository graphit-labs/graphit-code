# Task: Create Documentation Guides

## Summary
Created two comprehensive guide files for the Graphit Code project:
1. `docs/guides/mcp_tools_reference.md` — Complete MCP tools API reference
2. `docs/guides/troubleshooting.md` — Troubleshooting guide for common issues

## Date
2026-06-02

## Files Created
- `docs/guides/mcp_tools_reference.md`
- `docs/guides/troubleshooting.md`

## Details

### MCP Tools Reference
- Documents **all 50+ MCP tools** across 10 modules
- Modules covered: Lifecycle, AST, Knowledge, Memory, Hub, Wiki, Dream, Daemon, Cluster, Improvements
- For each tool: name, description, full parameter table (name, type, required, description)
- Includes common parameters section and error handling notes
- Extracted directly from source code in `internal/mcpstdio/tools_*.go`

### Troubleshooting Guide
- Covers **12 categories** of common issues
- Quick diagnostics section with runnable commands
- Categories: Installation & Setup, Configuration, Daemon, AST Indexing, AI/Embedding, Memory, Knowledge, Hub, MCP Connection, Dream
- Error messages extracted from source code in `internal/daemon/`, `internal/config/`, `internal/ai/`, `internal/mcpstdio/`
- Each issue includes: symptoms (exact error text), cause, and step-by-step solutions
- Diagnostic commands for each category

## Methodology
- Read all `internal/mcpstdio/tools_*.go` files to extract tool definitions
- Read `internal/daemon/daemon.go`, `pidfile.go`, `embedserver.go`, `autostart.go` for daemon errors
- Read `internal/ai/model_manager.go`, `embedding_local.go`, `embedding_proxy.go` for AI/embedding errors
- Read `internal/config/config.go` for configuration error patterns
- Used `docs/guides/getting_started.md` as the template for frontmatter format
- All paths in documentation are relative (no absolute filesystem paths)
- All documentation in English
