---
title: "Git Module Specification"
description: "Technical specification of Git operations, the singleton CLI backend, Default()/DefaultErr() accessors, SSH error handling, block manager, hooks, and ignore patterns."
content-type: reference
audience: developers
keywords:
  - git
  - singleton
  - cli backend
  - hooks
  - ignore
  - ssh
prerequisites:
  - "docs/architecture/architecture_overview.md"
related:
  - "docs/specs/daemon_module.md"
  - "docs/specs/mcpstdio_module.md"
---

# Git Module Specification

The `internal/git` package provides a Git abstraction layer for all Graphit operations that interact with Git repositories. It implements a singleton CLI backend pattern, SSH error wrapping, content block injection/removal, git hook management, and gitignore management.

---

## ⚙️ Architecture

The module is structured around a `Git` interface with a single concrete implementation (`cliBackend`) that shells out to the system `git` binary. A package-level singleton ensures a single, lazy-initialized instance shared across the application.

```mermaid
graph TD
    Caller["Any Graphit Module"] --> Singleton["Default() / DefaultErr()"]
    Singleton -->|sync.Once| LookPath["exec.LookPath('git')"]
    LookPath -- found --> Backend["cliBackend{}"]
    LookPath -- not found --> Error["defaultInitErr"]
    Backend --> BuildCmd["buildCmd(repoDir, env, args)"]
    BuildCmd --> GitCLI["exec.Command('git', ...)"]
    GitCLI -->|error| SSHWrap["wrapSSHError()"]
    GitCLI -->|stderr| CleanStderr["CleanStderr()"]
    
    subgraph "Block Manager"
        InjectBlock["InjectBlock()"]
        RemoveBlock["RemoveBlock()"]
    end
    
    subgraph "Hooks"
        HookManager["HookManager"]
        HookManager --> Install["Install()"]
        HookManager --> Remove["Remove()"]
    end
    
    subgraph "Ignore"
        InjectGitignore["InjectGitignore()"]
        RemoveGitignore["RemoveGitignore()"]
    end
```

---

## 🧩 Key Types & Interfaces

### `Git` Interface

```go
type Git interface {
    Run(repoDir string, args ...string) error
    RunOutput(repoDir string, args ...string) (string, error)
    RunSilent(repoDir string, args ...string) string
    RunWithStdin(repoDir string, data []byte, args ...string) (string, error)
    RunWithEnv(repoDir string, env map[string]string, args ...string) error
    RunOutputWithEnv(repoDir string, env map[string]string, args ...string) (string, error)
    RunGlobal(args ...string) error
    RunGlobalOutput(args ...string) (string, error)
}
```

All methods accept a `repoDir` parameter which is translated to `git -C <repoDir>`. An empty `repoDir` runs git in the current working directory.

| Method | Returns | Description |
|---|---|---|
| `Run` | `error` | Execute git with combined stdout+stderr. Returns wrapped error on failure. |
| `RunOutput` | `(string, error)` | Execute git and return trimmed stdout. Stderr captured separately. |
| `RunSilent` | `string` | Execute git silently, swallowing errors. Returns trimmed stdout. |
| `RunWithStdin` | `(string, error)` | Execute git with piped stdin data. |
| `RunWithEnv` | `error` | Execute git with additional environment variables. |
| `RunOutputWithEnv` | `(string, error)` | Execute git with env and return output. |
| `RunGlobal` | `error` | Execute git without a repo directory (delegates to `Run("")`). |
| `RunGlobalOutput` | `(string, error)` | Execute git globally and return output. |

### `cliBackend`

```go
type cliBackend struct{}
```

The sole concrete implementation. Contains no state — all configuration is resolved per-command.

---

## 🔄 Singleton Pattern

### `Default() Git`

Returns the singleton `Git` instance. Uses `sync.Once` to:
1. Call `exec.LookPath("git")` to verify the `git` binary exists in PATH.
2. If found, create a `&cliBackend{}` and store it as `defaultInstance`.
3. If not found, store the error in `defaultInitErr` and leave `defaultInstance` as `nil`.

**Returns `nil`** if git is not available. Callers that cannot tolerate a nil instance should use `DefaultErr()`.

### `DefaultErr() (Git, error)`

Returns both the singleton instance and any initialization error. Forces `Default()` to run first (if it has not already), then returns `(defaultInstance, defaultInitErr)`.

### Thread Safety

Both functions are safe for concurrent use. `sync.Once` guarantees the initialization runs exactly once regardless of how many goroutines call it simultaneously.

---

## 🛠️ Command Execution

### `buildCmd(repoDir string, env map[string]string, args ...string) *exec.Cmd`

1. If `repoDir` is non-empty, prepends `-C repoDir` to the args.
2. Creates `exec.Command("git", fullArgs...)`.
3. Sets `GIT_SSH_COMMAND=ssh -o BatchMode=yes` unless the caller has already set `GIT_SSH_COMMAND` in their environment.
4. Merges any caller-provided env vars via `MapToEnv()`.

The `BatchMode=yes` SSH option prevents interactive prompts (password requests, host key confirmations) that would hang headless processes like the daemon.

---

## 🔐 SSH Error Handling

### `wrapSSHError(err error, stderr string) error`

Intercepts git errors where stderr contains SSH host key verification failures. Detection triggers:
- `"host key verification failed"`
- `"no matching host key"`
- `"known_hosts"`

When detected, appends an actionable remediation message:

```
the remote host is not in your known_hosts file.
  Verify the host manually:  ssh -T git@github.com
  Once verified, retry the operation.
```

### `extractHost(stderr string) string`

Scans stderr lines for tokens containing `@` (e.g., `git@github.com`) to provide a specific host in the remediation message. Falls back to a generic `git@<hostname>` placeholder.

---

## 📝 Stderr Handling

### `CleanStderr(raw string) string`

Filters and normalizes git stderr output to produce actionable error messages:

1. Strips empty lines.
2. Removes **progress lines** (lines containing transfer progress like `"Counting objects:"`, `"Compressing objects:"`, `"Receiving objects:"`, `"Resolving deltas:"`, and `"remote: ..."` prefixed progress).
3. If no meaningful lines remain, returns the last non-empty line from the raw output.
4. If more than 3 meaningful lines remain, keeps only the last 3.
5. Joins lines with `"; "`.

Returns `"(no stderr output)"` if raw is empty, or `"(git returned no useful error details)"` if only progress lines were found.

### `IsProgressLine(line string) bool`

Returns `true` if the line matches any of the recognized progress keywords:
`Counting objects:`, `Compressing objects:`, `Receiving objects:`, `Resolving deltas:`, `remote: Counting`, `remote: Compressing`, `remote: Total`.

### `MapToEnv(m map[string]string) []string`

Converts a `map[string]string` to `[]string` of `"KEY=VALUE"` entries suitable for `exec.Cmd.Env`.

---

## 📦 Block Manager

The block manager provides idempotent injection and removal of delimited content blocks within files. It is used by hooks, gitignore management, and Agent rule installation.

### Block Styles

| Style | Start | End | End Prefix | End Suffix |
|---|---|---|---|---|
| `ShellBlockStyle` | `# --- ` | ` ---` | `# --- END ` | ` ---` |
| `HTMLBlockStyle` | `<!-- ` | ` -->` | `<!-- END ` | ` -->` |

A block with marker `"FOO"` in shell style looks like:
```sh
# --- FOO ---
...content...
# --- END FOO ---
```

### `InjectBlock(filePath, content, marker, shebang string) error`

Injects a content block into a file:

1. Reads existing file content (creates file if missing).
2. If a block with the same marker already exists, **replaces it in-place** preserving surrounding newlines.
3. If no existing block, **appends** the block to the end of the file.
4. Normalizes excessive blank lines (3+ consecutive newlines → 2).
5. If the file only contained a shell shebang before injection, replaces the content entirely.
6. If `shebang` is non-empty, sets file permissions to `0755` (executable).

### `RemoveBlock(filePath, marker string, deleteIfEmpty bool) (bool, error)`

Removes a block from a file:

1. Reads the file; returns `(false, nil)` if file does not exist.
2. Strips the block using regex matching.
3. If `deleteIfEmpty` is `true` and the remaining content is only a shell shebang (or empty), **deletes the file entirely**.
4. Otherwise, writes the cleaned content back.
5. Returns `(true, nil)` if the file was modified.

### `InjectBlockStyled` / `RemoveBlockStyled`

Generic versions that accept a `BlockStyle` parameter, enabling both shell and HTML block formats.

---

## 🪝 Hooks

`HookManager` installs `pre-commit` by default and each additional event named in
the lockfile's top-level `hooks` map. Keys must be event names from Git's
[githooks manual](https://git-scm.com/docs/githooks/2.54.0); unknown names fail
lockfile loading. On Git 2.54 or later it writes one repository-local
`hook.<brand>-<event>.command` and `.event=<event>` pair per selected event
(`hook.graphit-pre-push` for `pre-push` in the default build). The command checks
that the brand executable exists before invoking `<brand> _git-hook <event>` with
Git's arguments. This follows Git's
[configured hook contract](https://git-scm.com/docs/git-hook/2.54.0).

On older Git, `Install` injects a marked shell block into each selected literal
project path `.git/hooks/<event>`. The block checks the Git version, confirms that the
configured Graphit hook is absent, and checks the executable before invoking the
same runner. Thus an old file left in place after a Git upgrade does not run Graphit
twice. The marked block is placed immediately after the shebang so an existing
third-party `exit` cannot bypass the Graphit checks. Existing shell content is
preserved. A non-shell or unsupported shell shebang causes an actionable
installation warning and leaves the file intact.

The runner reads the project's `graphit.lock.json` `hooks.<event>` array and runs
all commands in order from the project root through `sh -c` on Unix or `cmd /C`
on Windows. A project-owned executable uses a path relative to that root; an
external executable is resolved by name from the system `PATH`. No check requires
`sh` as its executable, and commands must not embed machine-dependent paths.
All commands must succeed; their stdout and stderr remain visible to Git.
Git's positional arguments are `$1`, `$2`, etc. in Unix commands;
all platforms receive `GRAPHIT_HOOK_NAME`, `GRAPHIT_HOOK_ARG_COUNT`, and
`GRAPHIT_HOOK_ARG_1` (and subsequent indices). Git-supplied finite stdin is
replayed to every command for `pre-push`, `pre-receive`, `post-receive`,
`post-rewrite`, and `reference-transaction`. Other events inherit stdin without
reading it in advance, so an open terminal does not delay `pre-commit`.
Protocol events `proc-receive` and `fsmonitor-watchman` accept only one
command; `proc-receive` keeps its interactive stdin stream. Missing brand executable
causes Graphit checks to be skipped. `init` and `sync` report installation failures
with manual integration instructions for each failed event. For an older Git
installation failure, the warning also recommends Git 2.54 or newer followed by
`graphit sync`. A configured-hook failure on Git 2.54+ retains the manual
instructions without suggesting an upgrade. Events removed from the lockfile
are cleaned on sync. `remove` and `modules.hooks=false` remove all Graphit
registrations and marked blocks without changing third-party hook content.

The older path deliberately ignores `core.hooksPath`, linked-worktree `.git` pointer
files and third-party hook managers. The configured Git 2.54+ path does not depend
on `.git/hooks`. If the active hook is not installed or manually integrated,
configured checks do not run and the project's consistency gate loses substantial
determinism. No migration of earlier development hooks is performed.

---

## 🚫 Ignore Patterns

### `InjectGitignore(targetPath, content string) error`

Injects a content block into `.gitignore` using the brand's ignore marker. Used by
`graphit init` — both the CLI command and the `graphit_init` MCP tool — to keep the
project's runtime directory out of the repository.

### `RemoveGitignore(targetPath string) (bool, error)`

Removes the Graphit block from `.gitignore`. Unlike hooks, does **not** delete the file if empty (`deleteIfEmpty: false`).

### The generated `.gitignore` block

The content is not this module's to invent: it comes from `brand.GitignoreContent()`,
and contains the complete ownership policy for generated and machine-local project data.

```gitignore
# --- GRAPHIT AUTOGENERATED IGNORER ---
**/.graphit/runtime/
**/.graphit/grammars/
# --- END GRAPHIT AUTOGENERATED IGNORER ---
```

Two entries are sufficient because the brand directory is **split by ownership** rather
than ignored wholesale — see
[Storage Layout](../architecture/storage_layout.md#inside-a-projects-brand-directory).
Generated output and state live under `brand.RuntimeSubdir()`. Project-local parser
libraries live under `grammars/` and are ignored because they are platform-specific
binaries. Query YAMLs under `ast/queries/` and source overrides under `rules/` remain
repository-owned and versionable.

The block used to be `.graphit/`, which took the project's grammar overrides and rule
overrides down with the caches. Carving those back out with negations is not
possible: gitignore cannot re-include anything beneath an excluded directory, so a
`!/.graphit/ast/queries/` line under `.graphit/` is never even consulted. Doing it
properly took six lines that re-included each level in turn. Naming the two machine-local
subdirectories reaches the intended outcome without ordering rules or negations.

**The `**/` prefix is load-bearing.** A pattern with a separator in the middle is
anchored to the directory of the `.gitignore` that declares it, so a bare
`.graphit/runtime/` would ignore this project's machine state while exposing that of
every nested checkout, sub-project and test fixture below it. The prefix restores the
any-depth matching that the old trailing-slash-only pattern had for free.

`internal/brand` owns the content because the block is a naming and ownership contract:
the brand directory, its runtime subdirectory, and its project-local grammar tree. Nothing
above `brand` has to be consulted to produce it, and callers cannot maintain divergent
lists.

`InjectGitignore` runs at `graphit init` and nowhere else, so an existing checkout
keeps the block it was given until `graphit init` runs again. The injection is
idempotent and replaces the marked block in place.

---

## 📦 Dependencies

### Internal

| Package | Usage |
|---|---|
| `internal/brand` | Brand name for block markers, binary name for hook scripts, ignore marker, display name for comments. |

### External

| Package | Usage |
|---|---|
| `os/exec` | `LookPath` for git discovery, `Command` for command execution. |
| `sync` | `sync.Once` for singleton initialization. |
| `regexp` | Block detection and replacement in the block manager. |
