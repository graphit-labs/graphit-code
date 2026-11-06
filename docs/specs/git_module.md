---
title: "Git Module Specification"
description: "System Git checkout/hooks and native record snapshots, Role gix reads, process ownership and configured-hook contracts."
content-type: reference
audience: developers
keywords:
  - git
  - cli backend
  - hooks
  - ignore
  - ssh
prerequisites:
  - "docs/architecture/architecture_overview.md"
related:
  - "docs/specs/daemon_module.md"
  - "docs/specs/mcpstdio_module.md"
references:
  - target:
      type: session
      id: ses-c98a
    relation: relates_to
  - target:
      type: task
      id: tsk-b5d6
    relation: supports
  - target:
      type: task
      id: tsk-8d62
    relation: supports
  - target:
      type: task
      id: tsk-6997
    relation: supports
  - target:
      type: task
      id: tsk-a4ca
    relation: supports
  - target:
      type: task
      id: tsk-2dfb
    relation: supports
---

# Git Module Specification

Checkout facts, configured hooks and Task/Memory snapshot integration use the system Git
executable. Role Git status/diff readers use the mature `gix` library through their own
authorized read boundary. These are deliberate consumer-specific backends, not a shared
singleton or a second homegrown Git implementation. Commands retain their captured project,
environment policy, deadline and cancellation, and report the actual operation failure.
See [record snapshots and merges](git_records.md) for Git/Lance version binding.

## Checkout and version

`crates/graphit-core/src/git_checkout.rs` reads checkout facts through Git itself,
including worktrees, submodules and detached HEAD. It performs no repository or
Graphit mutation. [Project versions](project_identity.md#project-version) use
that observation without turning a branch name into project identity or authority.

On Windows, captured roots and Git common directories keep their canonical native
paths for identity and validation. The typed path passed to Git's
`--git-dir` argument uses `dunce::simplified`; fixture worktree/submodule paths use
the same library at their Git boundary. This is a borrowed argv representation,
without recanonicalizing the root or rewriting refs, options or arbitrary strings.
The library conservatively retains verbatim paths that cannot be simplified,
including long paths, reserved names and non-Unicode names. Their acceptance still
depends on Git; this adaptation does not claim support for every Windows path.

## Command execution and ownership

Configured Git operations use explicit captured paths and suppress interactive
prompts where the consumer cannot answer them. Authenticated native callers
retain the required project grants and process custody. A failed SSH host-key
check remains an actionable error; accepting or replacing host trust is a user
operation, not an automatic retry policy.

## Rust configured-hook registry

`crates/graphit-core/src/git_hooks.rs` reconciles brand-owned local hook
registrations through Git 2.54 or newer on PATH. Missing or unsupported Git
fails before config mutation. See the official
[configured-hook](https://git-scm.com/docs/git-hook/2.54.0) contract.

### Capture, desired state and namespace

`Registry::capture(checkout: &Path, brand: Brand, options: Options,
&Cancellation)` is read-only. It canonicalizes an explicitly supplied existing
main/linked-worktree/submodule root or bare Git directory, asks Git for
show-toplevel/common Git dir (the Git dir itself for bare repositories),
and rejects nested/non-Git inputs. Native returned paths strip only the
Git line terminator, preserving Unix native bytes and requiring valid Windows
UTF-8. There is no implicit CWD fallback or private .git-file parser. Clones
retain captured paths/brand/options, without an FD or global registry cache.

`Options { timeout, max_command_bytes, max_snapshot_bytes }` is mandatory.
Timeout is nonzero/monotonic-overflow checked, commands have a positive bound,
and snapshot acceptance is at most the process cap of 64 KiB. Checked preflight
requires room for all 28 owned event registrations and their key/event/enabled
overhead at the command bound; oversized options fail before Git/lock.
Subprocess capture itself remains capped at 64 KiB per stream, with a smaller
caller snapshot cap checked on the response. Path/OS/allocator overhead is no
hard-RSS guarantee.

`reconcile(&self, desired: &[Registration { event, command }], &Cancellation)`
receives trusted borrowed UTF-8 Git commands. Empty/whitespace-only/NUL/oversized
commands and duplicate desired events fail validation. `Event::ALL` contains
28 canonical events; FromStr/as_str rejects unknown/case/traversal names. An
empty desired list removes owned registrations; the registry invents no
pre-commit selection, command, authorization or executable-skip policy.
`Report` exposes selected/changed/removed events and successful Git config-write
count. Registration/Registry/HookError Debug omit commands/raw paths/output;
there is no automatic Serialize contract.

The owned friendly name is `<brand>--<event>`.
Events contain no `--` and Brand ends in alnum, making the suffix unambiguous
even for brands with internal double hyphens. Thus `graphit--post-update` and
`graphit-post--update` remain independent, as do graphit/prepare-commit-msg and
graphit-prepare/commit-msg. No namespace conversion or old registration importer is provided.

### Local ownership and cooperative changes

Every operation uses the captured Git common dir explicitly and the shared
Git command helper: inherited GIT_* routing is removed, system/global config
and prompts are disabled, while native Git resolves repository paths/config.
A permanent `<git-common-dir>/.hooks-manager.lock` coordinates cooperating
brands/worktrees with FileLock and remaining operation budget. It must never
be unlinked/replaced during contention. Reconcile may create this sidecar even
when config already matches; it creates no directories or script replacements.
External Git tools use native config.lock per write, not this grouped lock;
hostile filesystem races/hardlink/case-alias coordination are not certified.

Only local `hook.<brand>--<canonical-event>.command/event/enabled` fields are read
with config get --local --no-includes --all --show-names --regexp --null. The
bounded response uses Git's key-LF-value-NUL protocol, borrowing values and
counting duplicates; this is not a Git config-file parser. Unknown fields/names,
other brands, scripts, includes, global and worktree config are preserved.
Set/unset use --local and Git alone writes its local config; --no-includes is a
read option, not accepted by those mutation commands.

Exact single command/event/enabled=true values are a no-op: zero config writes
and unchanged config mtime. For each changed selected event, stable event order
writes enabled=false, command, event, then enabled=true. Removal unsets present
event first, then enabled and command, preserving unknown fields. No-op proof is
the existing snapshot; after mutations a fresh local snapshot must match desired
before a report succeeds.

Git's individual config writes are atomic; reconciling multiple fields/events
is not a transaction or proof of effective global/worktree activation. Failures
abort with possible disabled/partially updated local state, without rollback or
automatic whole-operation retry. A caller's explicit next reconcile can converge.
The registry does not execute checks; Git governs trusted command execution,
arguments and ordinary user-hook ordering. `reconcile_events` limits a consumer
to its selected events and preserves the brand's other registrations. App uses the existing gix discovery to distinguish non-Git projects from bare repositories;
Git itself validates the exact root, version and configuration before mutation. The
[configured dispatcher](../guides/git-hooks.md) uses full `reconcile`
for the union of nonempty `githooks` events and enabled Task/Memory events.
Git stores only `<brand> git hook <event> --`, never the configured command bodies.
`git install`, `git sync`, setup and project-wide CLI/MCP sync share that producer.
The [Task/Memory consumer](git_records.md) sends authenticated post-operation
notifications and retains native Lance commit snapshots before configured commands run.
Quality-only events need no Source startup. `git var GIT_SHELL_PATH` selects the local
POSIX shell; original argv stays separate, stdin uses inherited handles/replay,
and the existing Process supervisor owns cancellation and descendants. No parallel
registry, generated hook scripts or `core.hooksPath` fallback is installed.

### Budgets, errors and proof boundary

Capture and each reconcile have fresh deadlines. Within reconciliation the
shared Budget covers Git version/preflight, lock waiting, writes and verification;
FileLock receives only remaining time. Cancellation is explicit. Blocking
filesystem calls/OS close/power loss are not hard deadline or durability promises.
`HookError { stage, failure, may_have_changed }` has fixed stage/failure categories
and I/O kind/OS code or Git exit/event, with no raw command/path/stdout/stderr.
may_have_changed becomes true before the first mutating Git attempt, even if it
fails or times out after the child could have committed. Preflight/read/lock-only
failures remain false. A primary operation error wins over unlock failure.
