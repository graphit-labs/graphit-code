---
title: Configure and run native Git hooks
description: Shared command arrays, installation, sync, Git context, shell execution and failure recovery.
content-type: guide
audience: developers
updated: 2026-10-30
---

# Configure and run native Git hooks

Graphit registers one dispatcher per owned event in Git's native `hook.<name>.event`,
`command` and `enabled` configuration. Commands live in the normal Graphit configuration;
Git stores only `graphit git hook <event> --`. Git 2.54 or newer and Graphit on `PATH`
are required. Older Git produces an explicit error; there is no `core.hooksPath` fallback.
See the [Git 2.54 release notes](https://github.com/git/git/blob/v2.54.0/Documentation/RelNotes/2.54.0.adoc)
and [native hook reference](https://git-scm.com/docs/git-hook/2.54.0).

Agent lifecycle hooks, installed through `graphit hooks install`, are separate from
Git hooks. The latter combine repository commands with Task/Memory commit notifications.

## Configure commands

Commit the project's unified `.graphit/project.json`, preserving its existing ID,
metadata and settings. For example, this repository uses:

```json
{
  "id": "01KSH1CRFFG8Z74B5ZS78WW808",
  "name": "graphit-code",
  "ephemeral": false,
  "githooks": {
    "pre-commit": [
      "git diff --cached --check",
      "actionlint",
      "cargo xtask check"
    ],
    "pre-push": [
      "cargo xtask check",
      "cargo xtask cargo clippy --workspace --all-targets --locked -- -D warnings",
      "cargo test --locked -p xtask --bin xtask"
    ]
  },
  "settings": {}
}
```

`githooks` is a root field in both Project and Global documents. It is never a
member of `settings`. Global configuration stays in `<global>/config.json` and
contains no project identity. See [the project document](configuration.md#the-project-document).

These are this repository's fast checks; another project chooses its own commands.
Run affected functional suites during development. The complete `cargo xtask gate`
remains available explicitly and runs in CI; pushing does not repeat every integration
journey. Install
required executables on `PATH`. Each event has an ordered array of complete shell commands.
Commas inside a command are literal: these arrays never use CSV. Unknown event names,
wrong value types, empty commands and NUL fail validation before registry changes.
An absent key inherits the next configuration layer. An empty object explicitly suppresses
all inherited repository commands. An empty event array configures no commands for that event.
Sync removes unused registrations, unless Task/Memory still needs them.

The ordinary [configuration precedence](configuration.md) applies:
Command > Environment > Project > Global > Distribution. The **whole `githooks` object**
replaces a lower layer; event arrays are not implicitly merged across layers. The environment
name is `GRAPHIT_GITHOOKS`, whose value is a JSON object with arrays, not CSV. For example:

```sh
GRAPHIT_GITHOOKS='{"pre-commit":["printf \"comma, preserved\\n\""]}' graphit git sync
```

Use normal Config commands; they preserve identity, metadata and unrelated settings:

```sh
graphit config --global set githooks '{"pre-commit":["git diff --cached --check"]}'
graphit config set githooks '{"pre-commit":["cargo xtask check"]}'
# Suppress all inherited command hooks for this project.
graphit config set githooks '{}'
# Remove the Project override and inherit Global again.
graphit config unset githooks
# Remove the Global policy, preserving the other Global settings.
graphit config --global unset githooks
```

Each map replaces the whole map below it; it does not patch individual inherited events.
The canonical CLI key is `githooks` and the environment name is `GRAPHIT_GITHOOKS`.
Invalid values fail explicitly. No previous key or file layout is read.

Environment and command-line overrides must also be available when Git later launches the
hook. Prefer the project file for a shared persistent policy. Configuration contains executable
commands: review changes as code. Selecting a project/version never grants filesystem access
or remote PROJECT permissions.

## Install, update and remove

From the intended checkout:

```sh
graphit git install
graphit git sync
```

`git install` binds the current Task/Memory state to HEAD when those modules are enabled,
then reconciles registrations. `git sync` only reconciles the current event set; it does not
create another record snapshot and needs no daemon/authentication for that registry operation.
Project setup installs the same registrations when Git is present. Projects without Git
continue to use their normal records and do not need Git hooks.

`graphit sync` and the MCP `graphit_sync` tool also reconcile the selected project's Git
registrations, alongside their existing agent/Hub work. Their result includes
`steps.native.git_hooks.state` (`reconciled` or `non_git`). They retain their existing authenticated
Source permissions. A global-only sync does not walk or edit every project's Git repository.
After adding/removing events in global configuration, sync each affected checkout.

Changing **commands of an already registered event** takes effect at its next execution,
without reinstalling. Adding/removing events requires `git sync` or the project-wide sync.
For inspection:

```sh
git config --local --get-regexp '^hook\.graphit--'
git hook run pre-commit
```

The namespace follows the installed brand (`graphit--pre-commit`, for example). Reconciliation
changes only that brand's registrations; other native hooks and traditional third-party hooks
remain Git's responsibility. Graphit neither sets nor clears `core.hooksPath` as part of the
product. A third party may use it independently. To remove only Graphit's registrations:

```sh
graphit git remove
```

## Shell, working directory and hook input

Commands use the POSIX shell selected by Git and reported by `git var GIT_SHELL_PATH`.
Git for Windows resolves `sh` through its execution environment (normally its bundled shell). Graphit does not parse shell
syntax or switch to PowerShell/cmd. The runtime shell path is discovered locally and never
written into shared registration. Use POSIX quoting and installed executable names. Quote
paths containing spaces, for example `cargo --manifest-path "tools/checker/Cargo.toml" check`.
Do not rely on PowerShell `$env:NAME`, cmd `%NAME%`, or machine-specific absolute paths.
See [Git's shell-path contract](https://git-scm.com/docs/git-var). A minimal Git distribution
without a usable POSIX shell cannot execute these commands: install a Git distribution
that supplies one (normally Git for Windows on Windows), rather than switching syntax silently.

Each array entry starts a separate shell with `sh -c`; shell state (`cd`, `export`, variables)
does not carry to the next entry. There is no implicit `set -e` inside a compound entry:
use `&&` or an explicit shell policy when every subcommand must succeed. Git's environment
is inherited, including `GIT_DIR`, index selection and push context. Executables resolve via
that environment's `PATH`, which may differ between a desktop Git client and a terminal.

Graphit preserves the dispatcher’s actual Git working directory. Usually this is the worktree
root; bare/server receive events use the Git directory according to Git's contract. Relative
commands resolve there, even though Graphit resolves project configuration from the project base.
A repository receiving server-side events must have an appropriate Graphit identity/configuration
at the resolved project base and Graphit available in the server's `PATH`.

Original hook arguments are passed separately to the shell as `$1`, `$2`, … and `"$@"`.
They are **not automatically appended** to every tool in a configured command. They are also
available as:

| Variable | Meaning |
|---|---|
| `GRAPHIT_HOOK_NAME` | Git event, such as `pre-push` |
| `GRAPHIT_HOOK_ARG_COUNT` | Number of original Git arguments |
| `GRAPHIT_HOOK_ARG_1`, `GRAPHIT_HOOK_ARG_2`, … | Original individual argument values |

For a checker that explicitly accepts pre-push's remote name and location, the Global
document (or these fields alongside the Project identity) is:

```json
{"githooks":{"pre-push":["./tools/check-push \"$1\" \"$2\""]},"settings":{}}
```

Never build an `eval` string from those variables. The registry's `--` separates original
arguments from Graphit options, including an argument that starts with a hyphen.

Finite stdin payloads (`pre-push`, `pre-receive`, `post-receive`, `post-rewrite`,
`reference-transaction`) are spooled once to a private anonymous temporary file, then rewound
for each command and the native record handler. Binary bytes and newlines remain intact;
there is no whole-payload memory buffer. A minimal supervised instance of the same executable
copies stdin before normal CLI/runtime initialization, allowing cancellation even when the
input pipe remains open. The file is closed/deleted when the dispatcher exits.
Other events inherit their live stdin. `proc-receive` is an interactive bidirectional protocol;
it is never buffered until EOF. It and `fsmonitor-watchman`, which owns an output protocol,
accept at most one configured command. If several operations are needed, configure a single
script that owns that protocol and coordinates them. No generic fan-out can replay an
interactive conversation correctly.

## Output, failure and record notifications

Command stdout and stderr are inherited directly and appear while the command is running.
Git may redirect hook stdout to stderr according to its own invocation contract.
Graphit's diagnostics use stderr, preserving stdout for hooks with response protocols.
On the first nonzero exit, Graphit names the event, command index and command, stops the
sequence and returns the process code when representable. On Unix, signal exits use
`128 + signal`; cancellation returns 130 after joining owned processes and descendants.
There is no RPC duration limit on a running quality command; startup and cleanup retain
concrete process-runner deadlines. Failure to start or clean up is an explicit error.

Git controls whether a hook's result can veto its operation. `pre-commit` can reject a commit;
`post-commit` cannot undo one that already exists. A failing post hook must be diagnosed and
recovered, rather than treated as a rolled-back commit.

When either Task or Memory is enabled, Graphit retains native handlers for `post-commit`,
`post-checkout`, `post-merge`, `post-rewrite`, `post-applypatch` and `reference-transaction`.
Their union with configured events uses **one** dispatcher per event. Authenticated native
record handling runs before configured commands, so a failed post-event quality command does
not prevent recording an already completed Git operation. Ignored reference-transaction phases
do not repeat native snapshots, but configured commands still receive the actual event/phase.
If native handling fails, that failure is explicit and configured commands do not run.

Use `graphit git snapshot` to retry binding current records to HEAD after a failed commit
notification, or `graphit git restore [PREVIOUS_COMMIT]` for the existing checkout-restore
recovery contract. These operations use the authenticated Source and native Lance branches;
quality-only events do not initialize a daemon, models or authentication. Disabling both
record modules removes their six automatic registrations on sync, while explicitly configured
events still work.

## Implementation and verification

The existing gix discovery distinguishes repository eligibility (including bare repositories)
from checkout-only Task/Memory facts. Core's `git_hooks` module owns version checks, local registry reconciliation and preservation
of third-party configuration. App's `git_records` retains authenticated record semantics;
`git_hook_commands` reads configuration and uses `graphit-process` supervision for command
execution and stdin copying. Setup and the CLI/MCP general sync share the same registry producer.
There are no generated hook scripts or separate script registry.

Affected tests are `graphit-app/git_hooks_executable`, the Git journey in `task_executable`,
the general-sync cases in `managed_roles` and `source_executable`, Core `git_hooks`, and Process
`host_runtime` inherited-I/O cases. Tests use temporary repositories, real native hooks and
local fixtures; pre-push uses `git hook run`, never a real push. Linux execution is recorded
with the delivery evidence. Cross-platform shell/supervision code and CI configuration alone
do not qualify native Windows/macOS execution.
