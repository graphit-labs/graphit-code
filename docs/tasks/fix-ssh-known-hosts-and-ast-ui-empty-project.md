---
title: Fix SSH Known Hosts Hang and AST UI on Empty Projects
status: done
created: 2026-05-30
updated: 2026-05-30
---

# Fix SSH Known Hosts Hang and AST UI on Empty Projects

## Objective

Fix two bugs reported during first-use flows:

1. **SSH known_hosts hang**: When running `graphit setup` or `graphit init` on a new machine where GitHub's SSH host key is not in `~/.ssh/known_hosts`, git operations hang indefinitely waiting for user input that can never arrive (especially in non-interactive MCP contexts).

2. **AST UI crash on empty projects**: When running `graphit init` on an empty directory followed by `graphit ui`, the system errors with "no AST database found" even after running `graphit ast index`, because the empty directory produces no database file.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/git/cli_backend.go` | Modified | Set `GIT_SSH_COMMAND` with `StrictHostKeyChecking=accept-new` in `buildCmd` to auto-accept new SSH host keys |
| `cmd/graphit/commands/runners.go` | Modified | Fall back from read-only to writable AST backend in `runUnifiedServe` when no DB exists |

## Key Decisions

- Used `StrictHostKeyChecking=accept-new` (not `no`) — accepts only first-time hosts but rejects changed keys (MITM protection preserved)
- Only sets `GIT_SSH_COMMAND` if the user hasn't already defined it (respects existing user configuration)
- Applied the SSH fix at the lowest level (`buildCmd`) so ALL git operations benefit, not just specific commands
- For the AST UI, chose to fall back to writable backend rather than creating an empty DB during `ast index`, since writable backends auto-create the DB on first use

## Use Cases

### UC-01: First-Time Setup on New Machine with SSH Remote

- **Actor**: Developer running `graphit setup` on a new machine
- **Preconditions**: GitHub SSH host key is NOT in `~/.ssh/known_hosts`; hub repo URL is an SSH URL
- **Main Flow**:
  1. Developer runs `graphit setup`
  2. Developer enters hub repo URL (SSH)
  3. System creates `GitStore` and calls `EnsureCloned()`
  4. `EnsureCloned` calls `Sync()` which runs `git pull --rebase`
  5. Git subprocess uses `GIT_SSH_COMMAND=ssh -o StrictHostKeyChecking=accept-new`
  6. SSH auto-accepts the new host key and adds it to `known_hosts`
  7. Git operation completes normally
- **Alternative Flows**:
  - User already has `GIT_SSH_COMMAND` set in environment → system respects it, does not override
  - Hub repo URL is HTTPS → SSH option is irrelevant, no impact
- **Error Scenarios**:
  - SSH host key has CHANGED since last connection → SSH rejects (MITM protection), git fails, error surfaces to user
- **Postconditions**: Hub repository cloned successfully; GitHub host key added to `known_hosts`
- **Affected Files**: `internal/git/cli_backend.go`

### UC-02: Starting UI on Empty Project

- **Actor**: Developer who just initialized graphit on an empty project
- **Preconditions**: `graphit init` has been run; project directory has no source files; no AST database exists
- **Main Flow**:
  1. Developer runs `graphit ui`
  2. `runUnifiedServe` calls `newASTBackendReadOnly()` — fails (no DB file)
  3. Falls back to `newASTBackend()` — creates writable backend (auto-creates DB)
  4. UI server starts normally
  5. AST panels show empty results (no data)
- **Alternative Flows**:
  - Project has been indexed → read-only backend succeeds on first try (no fallback needed)
- **Error Scenarios**:
  - Both read-only AND writable backend fail → error returned to user (something fundamentally broken)
- **Postconditions**: UI server running; user can browse the UI; AST panels are empty but functional
- **Affected Files**: `cmd/graphit/commands/runners.go`

## Test Cases & Acceptance Criteria

### Feature: SSH Known Hosts Auto-Accept
Ref: UC-01

#### Scenario: First connection to new SSH host
```gherkin
Given a machine where "github.com" is NOT in ~/.ssh/known_hosts
  And the hub repository URL is "git@github.com:org/repo.git"
  And GIT_SSH_COMMAND is not set in the environment
When the user runs "graphit setup"
Then the git operations complete without hanging
  And github.com is added to ~/.ssh/known_hosts
```

#### Scenario: User has custom GIT_SSH_COMMAND
```gherkin
Given GIT_SSH_COMMAND is set to "ssh -o StrictHostKeyChecking=no -i /path/to/key"
When any git operation is executed
Then the system uses the user's GIT_SSH_COMMAND unchanged
  And does not override with StrictHostKeyChecking=accept-new
```

#### Scenario: SSH host key has changed (MITM protection)
```gherkin
Given github.com IS in ~/.ssh/known_hosts with key "KEY_A"
  And github.com now presents key "KEY_B"
When a git operation contacts github.com
Then SSH rejects the connection
  And the error is surfaced to the user
```

### Feature: AST UI on Empty Project
Ref: UC-02

#### Scenario: UI starts on project with no AST database
```gherkin
Given a project directory with no source files
  And "graphit init" has been run
  And no AST database file exists
When the user runs "graphit ui"
Then the UI server starts successfully
  And AST panels show empty results
```

#### Scenario: UI starts on project with existing AST database
```gherkin
Given a project has been indexed with "graphit ast index"
  And an AST database file exists
When the user runs "graphit ui"
Then the UI server opens the database in read-only mode
  And AST panels show indexed data
```

## Notes

- The `StrictHostKeyChecking=accept-new` option requires OpenSSH 7.6+ (released 2017). This is available on all modern Linux distributions and macOS.
- The SSH fix is applied at the `buildCmd` level, meaning it affects ALL git operations including hub sync, memory sync, events push, etc.
