# Task: Security Fix — File Permissions & Input Validation

**Date:** 2026-05-31
**Status:** Complete
**Type:** Security Hardening

## Summary

Fixed 7 security vulnerabilities across the codebase related to overly permissive file permissions, path traversal attacks, and weak randomness.

## Changes

### Task 1: AST config file permissions (M-1)
- **File:** `internal/ast/config.go`
- **Change:** `os.WriteFile(configFile, data, 0o644)` → `0o600`
- **Rationale:** The YAML config may contain the OpenAI API key (`openai_key` field). Restricting to owner-only prevents other system users from reading sensitive credentials.

### Task 2: Daemon log file permissions (L-5a)
- **File:** `internal/daemon/daemon.go`
- **Change:** `os.OpenFile(..., 0o644)` → `0o600`
- **Rationale:** Daemon logs may contain project paths, error details, and operational information. Owner-only access prevents information disclosure.

### Task 3: Daemon PID file permissions (L-5b)
- **File:** `internal/daemon/pidfile.go`
- **Change:** `os.WriteFile(pf.path, ..., 0o644)` → `0o600`
- **Rationale:** PID file contains process ID and start time. Restricting access prevents other users from discovering or manipulating daemon state.

### Task 4: Embed server port file permissions (M-3 partial)
- **File:** `internal/daemon/embedserver.go`
- **Change:** `os.WriteFile(s.portFile, ..., 0o644)` → `0o600`
- **Rationale:** The port file reveals the ephemeral port of the embedding server (bound to localhost). Owner-only access prevents unauthorized service discovery.

### Task 5: Upload filename path traversal (M-11)
- **File:** `internal/hub/ui_server.go`
- **Change:** Sanitized `header.Filename` using `filepath.Base()` before joining with temp directory path. Added fallback for edge cases (`.`, `..`).
- **Rationale:** A malicious multipart upload could set the filename to include directory traversal sequences (e.g., `../../etc/passwd`) to write files outside the intended temp directory.

### Task 6: handlePages dir parameter path traversal (M-12)
- **File:** `internal/uiserver/wiki_handler.go`
- **Change:** Added `filepath.Abs` + `filepath.Clean` validation and directory existence check for the `dir` query parameter in `handlePages`.
- **Rationale:** The `dir` parameter was passed directly to filesystem operations without validation, allowing arbitrary directory listing. Now mirrors the protection pattern used in the adjacent `handlePage` handler.

### Task 7: Dream ID generation with weak randomness (L-7)
- **File:** `internal/dream/dream.go`
- **Change:** Replaced `math/rand` (seeded with `time.Now().UnixNano()`) with `crypto/rand` for the random suffix in `generateDreamID()`.
- **Rationale:** `math/rand` seeded with the current timestamp is predictable. While dream IDs are not security-critical tokens, using `crypto/rand` is best practice and eliminates the class of predictability entirely.

## Impact

- No behavioral changes to end users
- All file permissions are tightened from world-readable (`0o644`) to owner-only (`0o600`)
- Upload and wiki page listing endpoints are hardened against path traversal
- Dream IDs are generated with cryptographic randomness
