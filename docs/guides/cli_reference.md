---
title: "CLI Command Reference"
description: "Commands available in the current Rust executable and its installation and project lifecycle."
content-type: reference
audience: developers
updated: 2026-11-10
references:
  - target: {type: task, id: tsk-cab0}
    relation: corrected_by
keywords:
  - CLI
  - commands
  - init
prerequisites:
  - "docs/guides/getting_started.md"
related:
  - "docs/guides/configuration.md"
  - "docs/development/product-architecture.md"
---

# CLI Reference

This page describes the current Linux Rust CLI; native qualification for other
platforms remains separate. Use `graphit --help` and
`graphit <command> --help` for the exact options accepted by your installed build.
The [getting started guide](getting_started.md) covers the complete local journey.
The [complete option reference](../reference/cli-options.md) enumerates public
help paths, command-specific flags, positional inputs and declared defaults.

## Shared options

| Option | Purpose |
|---|---|
| `--global-dir <directory>` | Select the private installation. Otherwise use the branded global-directory environment setting or the current home directory. |
| `-c, --config <KEY=VALUE>` | Override one configuration key; repeat for multiple keys. The last value wins. |
| `--non-interactive` | Run without interactive prompts. |
| `--project-version <version>` | Select an existing retained PROJECT version for commands that accept PROJECT scope. |
| `-V, --version` | Print the compiled build version. |
| `--json` | Print structured JSON for scripts instead of human-readable text; accepted before or after subcommands. |
| `-h, --help` | Print command help. |

### Project directory

When a command offers `--project-dir` and operates on a project, you can omit it
inside that project. Graphit discovers the checkout from the current directory.
Use the option to work in a different project.

| Commands | When you can omit `--project-dir` |
|---|---|
| `init`, `remove`, `sync` | In the current project. Use `sync --global` for global maintenance. |
| `projects maintain-checkout`, `projects retire-checkout` | In the target checkout. `--project-id` still identifies the project. Use an explicit absolute path to retire a checkout whose directory is gone. |
| `ast`, `knowledge`, project `memory`, `task`, `references`, `index` | For the current project. USER Memory and imported Knowledge use their own selectors. |
| Project `hub` commands, including `install-local` and `publish` | For the current project. `install-local` still needs the artifact's `--source-dir`. Hub global targets use their own scope. |
| Root `mcp` | When launching inside the intended checkout. Outside a checkout, the Root session has no directory binding. |

Commands print readable text by default, including when stdout is redirected.
Sentence-case labels and two-space indentation express hierarchy. The interface
uses no decorative icons or required color. Empty lists and unchanged operations
have explicit textual results. Errors preserve known effects and uncertainty.

Use `--json` when a script needs native structured records:

```sh
graphit --json provider list
graphit task session-heads --json
```

Raw AST source and Ask answers remain copyable text. Knowledge pages and Hub
file content print their text by default; `--json` exposes the native record.
MCP, model-service transports and internal hooks retain their own protocols.

`--agent` is the preferred native-agent selector across public commands;
`--binding` remains a compatibility alias where previously accepted. AST search
uses `--top-k`, with `--k` retained as an alias. `--page-size` and `--limit`
keep their distinct operation-specific pagination meanings. Now uses
`--project-id` for its explicit project ID; `--project` remains an alias.

A working directory supplies the current PROJECT for PROJECT commands. `init`,
provider/account/login operations, `models` and `self-update` can run outside a project.
A valid login does not grant access to every PROJECT: the daemon and Broker still
apply the requested scope and authorization.

Commands accepting `--project-dir` resolve relative paths against the working
directory and discover the selected project's root before loading its configuration
and version. For example, `--project-dir .`, `--project-dir src` and
`--project-dir ../sibling` can select a project without a machine-specific path.
`--project-version` applies to that selected project. The CLI sends its resolved
native locator to the daemon; project IDs, artifact contexts and authorization
retain their own selection rules.

## Arguments and lists

Operation fields are individual CLI options. The CLI validates them and builds the
native MCP request internally; do not pass a complete operation as JSON.

```sh
graphit task search --text parser --mode fts --page-size 10 --top-k 10
graphit task query --table tasks --column id --column status --page-size 10 --top-k 10
graphit ast query 'MATCH (f:AstFileObservation) WHERE f.path = $path RETURN f.path LIMIT 1' \
  --param-string path=src/main.rs
graphit hub publish --kind skill --provider dev --id vendor/example --version 1 \
  --name Example --dependency skill vendor/common latest \
  --dependency knowledge vendor/manual release-1
```

Repeat list options such as `--column`, `--id`, `--tag` or `--dependency`; each
command's help specifies their arity and accepted values. Task columns and IDs
also accept comma-separated values. A value containing spaces stays one shell
argument when quoted. For a literal starting with a dash, use the equals form
(for example, `--body='- first item'`) to avoid interpreting it as another option.
Text fields keep commas literally; Graphit does not split
every string indiscriminately. In Task query, `--empty-ids` selects no IDs, while omitting both `--id` and `--empty-ids` leaves the ID filter absent. Omission preserves optional state; explicit
`--clear-*` options clear fields or collections on commands that support them.
Conflicting set/clear options and mismatched repeated fields are errors.

Add Task checks with repeated `--check KIND TEXT`, where KIND is `acceptance`
or `test`. Add references with repeated `--reference TYPE ID RELATION`:

```sh
graphit task create-task ... \
  --check acceptance 'The expected behavior is available' \
  --check test 'The regression check passes' \
  --reference knowledge PAGE_ID implements
```

The previous parallel check and reference flags remain accepted for compatibility.
Advanced reference metadata uses `--reference-type`, `--reference-id` and
`--reference-relation`, paired in occurrence order, with optional scope, scope-ID,
context and field flags. Do not mix compact and parallel forms.

For an optional reference field, omit it entirely or provide one value for every
reference. Empty scope/scope-ID/context entries mean absence. An empty
`--reference-field` is literal empty text; use repeated
`--reference-field-absent INDEX` (1-based) for missing fields in a mixed list.
Mismatched counts, invalid indices and conflicts are rejected. Use typed MCP/HTTP
contracts for large structured automation payloads. Claims remain private; flags
do not manufacture authorization or bypass ownership.

### Deliberate structured-value exceptions

| Value | Why it remains structured | CLI alternative |
|---|---|---|
| Service-issued cursors | They bind a snapshot, query and continuation position; their private fields are not independent user options | Pass the exact returned `--cursor` or use `--cursor-file PATH` where supported. AST neighborhood, Knowledge search and Task catalogue/search use an object JSON file; Task detail/history/query tokens use raw text files. Knowledge log and Hub cursor options take their returned opaque text tokens directly |
| A Cypher parameter's value | A parameter can be a number, boolean, null, list or nested object | Repeat `--param NAME=JSON_VALUE` for typed values, or `--param-string NAME=TEXT` for literal strings without JSON quoting. Duplicate names are rejected. CLI names must be trimmed and contain no control characters or `=`; for other names, use the MCP/HTTP `parameters` map. A whole parameter-map CLI argument is not accepted |
| Structured configuration (Browser, model slots, agent instruction rules/bundles) | These contain typed nested records, rather than a list of scalar items | Use the normal configuration file, or `config set KEY VALUE` for that individual setting. Use `models install/select` for model slots and the repeated publication options on `roles`/`coordinator install/sync` for instruction choices. See [configuration values](configuration.md#list-values-and-structured-settings) |
| Native packages | `package --input PATH` names a ZIP file, not a JSON operation | Keep the file argument; package metadata and other operation fields already have their own flags |

Parameter values retain their JSON types; the selected graph engine still validates
which Cypher types and shapes it supports. CLI JSON parsing does not expand that
engine contract.

Cursor files are UTF-8 regular files, bounded by the existing request input limit.
They grant no authority and remain subject to native query/version validation.
These exceptions do not change JSON-RPC, MCP arguments or HTTP JSON contracts.
Shell quoting remains the shell's responsibility; Graphit uses Serde for JSON
values rather than another codec.

## Commands

| Command | Subcommands or operation |
|---|---|
| `graphit init` | Prepare the installation and synchronize the current project |
| `graphit remove` | Remove the current project integration or withdraw one agent |
| `graphit uninstall` | Remove the user installation and its private local data |
| `graphit config` | `set`, `get`, `list`, `unset`; `--global` selects the Global store |
| `graphit ui [--no-open]` | Open the configured Browser, or print its URL for headless use |
| `graphit tray` | Control the installation through the Linux native tray menu |
| `graphit self-update` | Verify and install the latest public release; resume this installation's daemon only if it was running |
| `graphit daemon` | `foreground`, `start`, `stop`, `restart`, `status`, `install`, `login`, `uninstall` |
| `graphit models` | `install`, `select` |
| `graphit coordinator` | `install`, `sync`, `remove`; select `--agent` |
| `graphit mcp` | Serve the MCP client transport over standard input/output |
| `graphit projects` | `maintain-checkout`, `retire-checkout` |
| `graphit now` | Follow the live Broker Now view for an explicitly selected project |
| `graphit git` | `install`, `remove`, `snapshot`, `restore`; PROJECT Task/Memory Git notifications |
| `graphit ast` | `source`, `export`, `imports`, `search`, `query`, `schema`, `sample`, `neighborhood` |
| `graphit knowledge` | `ask`, `search`, `source`, `log`, `export` |
| `graphit memory` | `index` |
| `graphit task` | `get-task`, `catalogue`, `search`, `query`, `schema`, `get-session`, `session-heads`, `create-task`, `create-session`, `claim-task`, `claim-session`, `heartbeat-task`, `heartbeat-session`, `add-task-comment`, `progress-task`, `checkpoint-session`, `release-task`, `release-session`, `revise-task`, `revise-session`, `set-task-commit`, `verify-task-check`, `supersede-task-check`, `update-task-links`, `set-task-flag`, `complete-task`, `cancel-task`, `complete-session` |
| `graphit provider` | `list`, `show`, `add`, `update`, `remove` |
| `graphit account` | `list`, `show`, `use` |
| `graphit sync` | Reconcile Hub global/PROJECT installations at their exact versions, native agents/skills, Coordinator, hooks and MCP |
| `graphit skills` | `sync` |
| `graphit roles` | `install`, `sync`, `run` |
| `graphit hooks` | `install`, `sync`, `remove` |
| `graphit hub` | `install`, `install-local`, `sync`, `update`, `remove`, `forget`, `content`, `list`, `projects`, `search`, `publish`, `publish-native`, `unpublish`, `install-native` |
| `graphit package` | `install`, `export`, `inspect`, `import` |
| `graphit login` | Log in through a configured Local or Broker provider |
| `graphit refresh` | Refresh the active native Broker profile |
| `graphit logout` | Remove a stored profile |

## Hub physical PROJECT publication

The Hub CLI source enables request construction, including authored Resources
list/review/content, for Linux, Windows and macOS. It uses the same native path
selection, typed MCP requests and authenticated Source transport, retaining Original
cancellation/deadline and joined connection cleanup. Native platform compilation
and host execution remain separate qualifications; existing Linux receipts retain
their recorded scope.

`graphit hub install` and `graphit hub update` copy FILE artifacts into the
authorized PROJECT checkout. Omit `--mode` or pass `--mode copy`. Select the
provider, producer, artifact, version and native binding through the FILE
options. The selected origin must pass authorization and verified-cache
checks. Project copies can move with the checkout independently of the
installation cache. Updates preserve omitted names and use the same owned
Core/Hub transaction.

`--global` selects GlobalCache and rejects any supplied mode, including `copy`,
before cache or publication effects. This option belongs to CLI and MCP FILE
publication; the Web API retains its current request shape. See the
[Hub caller contract](../specs/hub_consumer_contract.md#scope-and-selection) for
selection and lifecycle details.

`graphit hub install-local` selects an absolute canonical local source, explicit producer and literal artifact version, plus an authorized consumer PROJECT. It supports FILE artifacts (agent, skill, command, workflow, rule, MCP, language and power) through the existing renderer and writer. Repeat it after source edits to reapply; omitted alias and entry name preserve the current selection. The checkout receives a copy. SourceRef LocalLink records source provenance, and removal preserves the source. No provider/profile/global fields or vendor login are used. See [local Hub installation](../development/local-hub-installation.md) for the strict CLI/MCP contract and qualification status.

## Update installed Hub artifacts

Omit `--id` to update the remote installations of the captured target:

```text
graphit --project-version main hub update --agent codex
graphit hub update --global
```

The PROJECT form uses the selected project and retained data version. The Global
form covers installed remote FILE cache entries and Native AST/Knowledge entries.
Each entry supplies its own stored origin and producer; Resources and LocalLink
entries are left unchanged. PROJECT aliases, entry names and physical modes are
preserved. Native updates use the declared remote latest release and replace the
selected metadata installation atomically, preserving other versions and targets.
A missing declared latest release fails instead of guessing a version.

The response contains `operation: update_all` and ordered `items`. Each item names
its artifact, producer, previous version and target, with status `ok`, `error` or
`not_attempted`. Completed items retain their producer outcome; failed items retain
observed applied effects or uncertainty. Processing stops at the first
failure or cancellation; there is no rollback or automatic replay. A failed
operation returns a failing CLI exit status even when earlier items completed.

For MCP, call `graphit_hub_update` without `selection`, with `global` and the
existing captured PROJECT/version/binding fields. An explicit `selection` keeps
the single FILE update contract. For CLI single updates, retain `--id`,
`--provider`, `--producer` and `--kind`; optional version, alias, entry and mode
continue to select that one operation. Collective updates reject these overrides
because identities and origins come from installed records.

The Web consumer uses the existing `POST /api/hub/files/update` route. Omit
`selection` and send either a PROJECT `scope` with its ID, retained version and
`binding`, or `scope: {kind: global_cache}`. It resolves the PROJECT from its
verified Web actor and preserves its own mutation admission; host paths and
Runtime credentials are not accepted. This Web write scope covers PROJECT and
GlobalCache FILE entries. GlobalNative updates use the CLI/MCP Native admission.
HTTP failures use the existing `{error: {code}, outcome}` envelope. Per-item
results and observed effects remain inside `outcome.items`, including items not
attempted after the first failure.

## Authored PROJECT Resources

Use `hub authored` to discover and read the maintained resource sources of the
current authorized PROJECT. `--project-dir .` can select that project explicitly;
the existing global `--project-version` selects its retained data version.

```text
graphit --project-version main hub authored list --project-dir . --kind skill --limit 20
graphit --project-version main hub authored review --project-dir . --kind skill --id xpto/review --version release/one --limit 20
graphit --project-version main hub authored content --project-dir . --kind skill --id xpto/review --version release/one --path SKILL.md
```

Copy the exact kind/ID/artifact-version tuple from the list result into review,
content or `hub publish`; the example tuple is illustrative. List accepts optional
literal `--kind`, `--id` and `--version` filters, plus `--limit` (default 100)
and `--cursor`. Review requires the tuple and accepts `--offset` (default zero)
and `--limit` (default 100). Content additionally requires a text file's relative
`--path`, obtained from review. An absent Resources namespace lists no entries.
Names/descriptor metadata are null where no authored descriptor exists.

These reads use the selected PROJECT Hub/read admission and return no host paths.
They require no provider or storage credential. Reading does not authorize
publication. The MCP tools `graphit_hub_authored_list`,
`graphit_hub_authored_review` and `graphit_hub_authored_content` expose the same
producer through strict native project selection and operation-specific `query`.
See the [Native authored contract](../specs/hub_consumer_contract.md#native-authored-read-requests)
for fields, pagination and current qualification.

## Managed roles and module skills

`graphit sync` is the general administrative reconciliation command. It and the
Runtime MCP tool `graphit_sync` invoke the same producer: reapply global and
PROJECT Hub installations, then publish managed roles/module skills and the
Coordinator with its hooks and MCP configuration. Repetition uses the existing
ownership/hash checks; foreign or user-modified files produce explicit conflicts.
The result reports each step and preserves already committed effects on failure;
it does not promise atomic rollback across independent publications.

```sh
graphit sync                         # current PROJECT and configured agent
graphit sync --agent codex           # explicit native agent
graphit sync --project-dir ./project # another maintained checkout
graphit sync --global                # global roles/skills and Hub cache/contexts
graphit skills sync                  # module skills only
graphit roles sync                   # roles and their module skills
graphit coordinator sync --agent codex # Coordinator, hooks and MCP
graphit hooks sync --agent codex   # hooks and MCP independently
graphit hub sync                     # all bindings in the current PROJECT
graphit hub sync --global            # global Hub installations
```

With an active Broker profile, the general sync and the specific `skills sync`,
`roles sync`, `coordinator sync` and `hooks sync` commands fetch that Broker's
active instruction overrides. The same step runs through `graphit_sync` in MCP.
A specific sync still reconciles only its own destinations. See the
[template paths and precedence](../architecture/rust-agent-instruction-templates.md#broker-instruction-overrides).

Sync retains each Hub artifact's **exact recorded version**, origin, alias,
entry selection and physical mode. It reapplies authored local sources and uses
current authorization for remote sources; it never silently substitutes a local
or upstream artifact after a remote failure. Use `graphit hub update` explicitly
to choose newer remote releases. The product Coordinator is reconciled by its
native publisher rather than the generic Hub template renderer. An existing
native AST/Knowledge installation is refreshed through its native SDK producer;
sync does not rebuild its corpus or change the artifact version. Locally imported Native corpora are admitted again through the existing SDK reader and reported as verified/unchanged; they are not reimported.

Explicit role rules, module skill aliases/bundles and module-tool names are saved
as canonical publication configuration under
`agents.<binding>.instructions` (binding hyphens become underscores), in the
PROJECT or global ConfigStore matching the destination. Omitted options reuse
those choices; explicit options override them. This stores only logical choices,
not rendered payloads, credentials or checkout paths. Remove a saved choice with
the normal configuration command when returning to defaults. Local raw instruction
overrides retain their existing precedence.

`graphit_sync` accepts `project_dir`, `project_version`, `agent`, and `global`
(default false). It requires the authenticated administrative Runtime; browser
and delegated Role identities cannot use it to publish files or grant access.
Like the CLI, it propagates cancellation/deadlines and reports partial effects.
The agent Stop/SessionEnd hooks request background index rescans; they do not
upgrade Hub artifacts or replace an explicit publication sync.

`graphit roles install` and `graphit roles sync` publish the managed role profiles together with the five module skills and their reference files in the current project. They use current project configuration, including `knowledge.docs_dir`. Use `--agent` for the native binding and `--skill-alias MODULE=NAME` to choose a maintained skill's native name.

`--destination global` selects captured native agents/skills directories instead of the current PROJECT. Compiled defaults and Global raw instructions need no PROJECT. Use `--source-project PATH` only to select a retained, authorized PROJECT origin for raw overrides or its installed Hub bundle; it does not freeze that PROJECT into Global execution context. The Global consumer is integrated and awaiting functional qualification; see [native Global instructions](../development/native-global-instructions.md). `roles run` remains PROJECT-only and rejects a Global destination before transport.

Role selection still has to be representable by the native binding. For example, `graphit roles install --agent kiro-v3` publishes all three roles, whereas `graphit roles install --agent codex --role scribe` selects a profile Codex can represent. Codex also receives Scout and Tracker as instructions, preserving `features.shell_tool` without inventing `sandbox_mode`. The official [subagent configuration contract](https://learn.chatgpt.com/docs/agent-configuration/subagents#approvals-and-sandbox-controls) confirms that custom agents inherit the parent sandbox. Read-only instructions describe the Role; Graphit MCP authorization remains separate.

`--skill-bundle MODULE=ID@VERSION` explicitly selects an exact installed Hub Skill as raw instruction input for one module: `task`, `memory`, `ast`, `knowledge` or `hub`. Project and Global raw overrides still take priority. Local sources retain current registry selection and content checks. A selected remote FILE source requires its current origin admission, the producer's PROJECT read authorization and the existing verified cache content and origin; missing or changed cache content fails without downloading a replacement. For standalone sources, `--skill-identity-provider MODULE=PROVIDER` selects an explicit verified Native identity when needed. It is rejected for Broker or local sources and never supplies storage credentials or PROJECT permissions. Existing conflicting files are preserved. A project default skill with identical generated content can be reused in place, without an ownership receipt. Explicit alias and bundle destinations remain subject to the ordinary ownership check. Selected Hub license and metadata are retained. A malformed selected template fails without fallback. `roles run` rejects `--skill-bundle`; publish with install or sync first.

`roles run --agent <binding> --role <role> --prompt <text> [--model <id>]`
uses the selected host's maintained adapter and published Role. Omit model to
use that host's supported configured default. The host's Graphit MCP
configuration is required; Run passes private Role credentials only in the
child environment and never falls back to Runtime authority. The redundant
`--engine` selector has been removed. See [managed Role context](../specs/managed_role_context.md#run-owned-native-transports)
for selection, permissions and lifecycle. Multihost functional validation is
pending in tsk-0257; previous install/skill proofs remain scoped.

## Runtime configuration

```sh
graphit config set modules.sync false
graphit config get modules.sync
graphit config list
graphit config unset modules.sync
graphit config set ast.scip.parsers '.zig=registry.example/team/scip-zig:1'
graphit config get ast.scip.parsers
```

Default mutation targets the current Graphit checkout. Reads resolve captured
layers and redact supported secrets; `--global get/list` inspect only stored Global
values. `set/unset` describe whether configuration was saved, already current,
removed or already absent. With `--json`, they return the native `changed` Boolean.
A missing `get` fails without a value on stdout.
No daemon or Auth profile is created. Providers/accounts and AST profile YAML use
their existing consumers. See [Configuration Reference](configuration.md).

`ast.scip.parsers` is a string of comma-separated `extension=image` associations.
Optional leading dots and ASCII case normalize to one extension; duplicate
normalized routes fail. Surrounding item/assignment whitespace is trimmed;
missing `=`, empty extension/image and empty comma-separated segments are errors.
The winning configuration layer replaces the whole list; an empty string clears
inherited routes. Environment uses the same plain string setting
`GRAPHIT_AST__SCIP__PARSERS='.zig=registry.example/team/scip-zig:1'`.
No separate parser-install command or native shared-library opt-in is required.
Custom images override maintained routes and never receive `ast.scip.version` as
an appended tag. See [custom compiler images](ast_extensibility.md#custom-compiler-images-by-extension)
for creation, mounts, protobuf output, credentials, cache and failure semantics.

## Installation and project lifecycle

```sh
graphit init
graphit init --agent kiro-v3 --project-dir ./project --name example
graphit remove --agent kiro-v3
graphit remove
graphit uninstall
```

Init is the entry point for a fresh installation, new project or existing
checkout. It chooses the effective agent, using Codex when no selection exists.
It configures the installation, starts or attaches to the daemon, maintains the
checkout, registers Git hooks and runs global and project synchronization.
The initial `--name` and `--project-id` options apply when creating the identity.
An existing identity and metadata are preserved. The JSON result retains
completed stages when a later stage fails.

Remove withdraws owned native integrations. A specific `--agent` leaves the
project identity, Git hooks and other agents in place. Full project removal
also retires the checkout and removes the Graphit project document. Use
`--project-dir` to target another checkout. Authored files and retained
records are preserved.

Uninstall stops and unregisters the user's service. It withdraws managed
integrations from known checkouts while keeping their project identity for
future initialization. It removes recorded private installation data. When the
last registered installation is removed, it also removes files verified by the
release receipt. Existing directories without an ownership receipt stay in place,
including directories in older installations. The result identifies those
directories, unavailable checkouts, and shared or externally managed binaries.
Windows schedules package deletion after exit.

Init and Remove operate on the actual checkout. Use `projects maintain-checkout`
for an explicit maintained version. Use `--coordinator-source <directory>`
with Init for a development binary without bundled Coordinator assets.

`coordinator install` publishes the versioned product Coordinator and PROJECT
context hooks through the authenticated daemon. The release includes the stock
Markdown artifact under `graphit-runtime/agents/coordinator` beside the executable;
no Graphit source checkout is needed. `--source-dir <directory>` explicitly
selects another artifact for the existing producer to validate. The response
contains the actual installed profile; use that returned value to start the agent.
Select `--agent` explicitly. Kiro uses `kiro-v3` and a Markdown profile. Other bindings publish their documented native configuration; Antigravity publishes workspace MCP and PreInvocation without claiming customprofile MCP. Kimi uses one global configuration under the captured KIMI_CODE_HOME or ~/.kimi-code, serving native project cwd without fixed project/version arguments. The Coordinator installer
needs no engine flag. Graphit's ACP consumers launch
`kiro-cli acp --agent-engine=v3` and select the model through session configuration.

`hooks install` publishes native agent context hooks. Repository Git hooks are a
separate integration configured by `githooks`; see [Git hooks](git-hooks.md).
`graphit git install` snapshots current Task/Memory state and registers the native
dispatcher; `graphit git sync` reconciles events without another snapshot. Project
initialization and project-wide `sync` use the same registry producer. Git 2.54+ is required.
Commands are arrays of whole strings, never CSV. Changing commands of an already
registered event requires no reinstall; changing the event set requires sync.

For an existing Kiro Markdown profile, use
`graphit hooks install --agent kiro-v3 --profile coordinator.md`. This command preserves the profile's
foreign content and requires the existing profile to pass validation.

## Local models

`models install embeddinggemma2` explicitly downloads the pinned EmbeddingGemma 2
BF16 model. It preserves an existing embedding selection. `models select` selects
an installed manifest and any explicit MRL recipe accepted by its help. Both
commands operate on the installation without requiring PROJECT identity. Use
`models --timeout <seconds> ...` to bound the operation. No model download is a
side effect of initialization.

## Daemon and maintained projects

```sh
graphit daemon start
graphit daemon status
graphit daemon stop
graphit daemon restart
graphit daemon install --login
```

Service registration and startup at login use the platform's native manager.
Login startup is explicit. In a foreground session use `graphit daemon`; managed
container startup uses `graphit daemon --managed`. Stop cancels and joins owned
work before removing owned discovery and runtime credentials. Container options
and health checks are documented in [the container guide](container.md).

A clean stop of `--managed` or `--detached` exits with code 0 after Source
cleanup completes. Cancellation of interactive `daemon foreground` retains
code 130. Managed native-grammar replacement uses code 75; detached startup
keeps its existing relaunch path. Startup and cleanup failures retain their
errors and are not converted into successful stops.

`projects maintain-checkout` and `projects retire-checkout` update the running
daemon's current inventory. They preserve sibling jobs and retained project
versions. Consult each subcommand's help for its required checkout locator.

## Source, records and live views

`ast source` reads indexed source or falls back to the physical file in the
retained project version, including unsupported extensions. Its help lists line
ranges and slicing options. Installed AST contexts remain explicit for graph
queries, search and exports.

`task` exposes native Task and Session read, mutation, history and catalogue
operations, including explicit calendar dates. Owner mutations require their
current opaque claim fence; the service applies authorization and integrity checks.
`knowledge` and `memory` operate through the active authorized Source service.

`now --project-id <id> --provider <provider>` follows the Broker view continuously.
Use `--version` and `--instance` on that subcommand to filter the live view.
The terminal uses a full-screen text display; redirected output prints complete
readable snapshots. `--json` selects JSONL snapshots, including on a terminal.

`mcp` serves the MCP client transport over stdio; the old `--stdio` spelling is
still accepted. `--project-dir` selects a default
PROJECT locator. `--role` and `--live-user` are restricted transports for credentials
of owned runs and owned Live turns; they do not create permissions.

## Authentication and artifacts

`provider` manages private provider configuration; `account` inspects or selects
stored profiles. `login`, `refresh` and `logout` use those configured providers.
`package` handles native package exports, inspection and imports; `hub` publishes
captured native SDK artifacts to an authorized Hub provider. Consult command help
for explicit source, provider and scope selection.

## Command availability and release validation

The compiled command catalogue above is the available public interface.
`graphit self-update` uses the verified package preparer and installed updater;
development builds return without changing files or starting a daemon. The
[installed updater contract](../specs/rust_self_update.md) describes restart,
recovery and platform coverage. See
[native packaging](../development/native-runtime-packaging.md) for release layout
and installation checks.

`hooks remove --agent <binding>` removes owned hook tuples and MCP configuration. Kiro external profiles require --profile; use `coordinator remove --agent <binding>` for the complete product profile and its inline hooks. Kimi keeps global hooks/MCP until the selected project was the last reference, preserving foreign fields. These operations do not retire project data or grant/revoke authorization.

## Remote Hub catalog retraction

`graphit hub unpublish` runs outside a local PROJECT and selects the configured Broker provider, remote producer, kind and full artifact ID explicitly:

```text
graphit hub unpublish --provider hub-service --producer 01J00000000000000000000001 --kind skill --id xpto/review --version release/one
graphit hub unpublish --provider hub-service --producer 01J00000000000000000000001 --kind skill --id xpto/review --all-versions
```

Use real identities from the authorized remote catalog. Exactly one of `--version` and `--all-versions` is required. Versions are literal; `latest` is rejected. Optional `--expected-sha256` requires `--version` and checks its current hash before any effect. The MCP equivalent is `graphit_hub_unpublish` with provider, producer, artifact_type, id and strict target `{kind:"version",version:"release/one",expected_sha256?:"64 lower-case hex"}` or `{kind:"artifact"}`. It accepts no project_dir, project_version, binding or source path. This consumer supports Broker Native; it does not silently fall back to Standalone.

The current Runtime/Local caller needs its configured HubWrite authority and the Broker producer namespace grant. This does not grant PROJECT access or require a fictional local producer checkout. Removing a release preserves the other releases; if it was latest, latest becomes unset. Last-version and whole-artifact withdrawal delete only the current catalog conditionally. Immutable payloads, project metadata, source files, caches and local installations remain intact.

Success returns `status: unpublished` and `retraction` with producer, artifact_type, id, targeted_versions, desired_latest and catalog `updated` or `removed`. Failures preserve known effects or `uncertain`; a late cancellation/deadline may wrap the operation result under `outcome`. CLI failure preserves those effects in a readable diagnostic by default;
`--json` retains structured diagnostic details. It exits unsuccessfully. Inspect the original remote selection after uncertain delivery or `inspection_required`; an empty MCP error under a very small output limit also requires inspection. No receipt identity or version is truncated, and no mutation is retried automatically.

The Broker must declare conditional catalog deletion support. If it is unavailable, whole-artifact and last-version withdrawal return Unsupported with catalog `not_attempted`, without sending DELETE. Removing one version while others remain still uses conditional catalog PUT and does not need that deletion capability. See [the storage contract](../specs/remote_artifact_storage.md#broker-stsv3-storage-capabilities) for the required STSv3 fields and SDK copy strategy.

To associate an authorized, successful Git commit with a Task, obtain the full SHA with `git rev-parse HEAD`, read the Task's current revision, and call:

```sh
graphit task set-task-commit --id '<task-id>' --expected-revision 7 --commit '<full-lowercase-sha>' --reason 'Record delivered commit'
```

Add `--claim-token TOKEN --claim-revision REV` when owned; never bypass another agent's claim. `--clear-commit` clears an incorrect CLI association (`commit:null` in MCP). Completed Tasks accept the metadata update without reopening or resetting evidence. After an amend, associate the replacement SHA only with Tasks included in that commit. These commands do not authorize or initiate commit/push.

## Installer and developer commands

`install.sh --help` and `install.ps1 -Help` list destination, release and local
archive options without installing or downloading. Both report completion and
suggest `graphit --help`. PowerShell retains its native parameter spelling.

`cargo xtask --help` lists the development commands, including safety checks.
`cargo xtask native --help` explains native preparation without starting a build.
The installed `graphit-model-server --help` describes its installation argument;
it normally runs under the daemon and keeps its authenticated stdio protocol.


## Explicit Session cancellation

`graphit_task_session_cancel` (`graphit task cancel-session`) requires
`CoordinateTask` authority, `id`, `expected_revision`, and a canonical nonempty
`reason`. An open Session omits `claim`; an in-progress Session supplies its real
live coordinator `claim: {token, expected_revision}` matching the record CAS.
Every linked Task must already be completed or cancelled in the same writer pin.
Resolve pending Tasks individually; cancellation never cascades.

The transition publishes `cancelled` and an immutable `Cancelled { reason }`
event. It clears owner, lease and private fence, preserving claim epoch,
specification, creation and progress. Authored references are preserved unless
explicitly patched. It never marks the demand completed or reopens a terminal
Session. A retry at the cancelled record's current revision with the same reason
and no claim/references is a no-op without a new event. Stale revisions, another
reason and completed Sessions fail. Delegated Role catalogues remain restricted;
this is a Root coordinator and human CLI operation.

```sh
graphit task cancel-session --id SESSION_ID --expected-revision 4 --reason "Demand withdrawn"
# For an in-progress Session, also supply --claim-token with its current fence.
# Add --json for structured output; the default terminal result is text.
```


## Local project discovery

`graphit projects list` and `graphit projects show --project-id ID` (or
`--name NAME`) use the live authenticated service's `graphit_projects_list/get`.
They read the maintained local inventory, independently of the remote Hub.
`list` filters by `--name` (substring), `--project-id`, `--version` and
`--online[=true|false]`. Use `--label-key KEY` to require a label and add
`--label-value VALUE` to require an exact value. `--page-size` defaults to 25. Continue using the exact
`--cursor` returned, with unchanged filters. If the authorized observation
changes, the cursor fails as expired instead of silently changing pages.
`show` matches an exact ID or unique exact name and can filter by version.
Unknown and unauthorized IDs share a single not-found-or-denied result.
On `list`, `show`, `label-set`, `label-unset` and `maintain-checkout`,
`--project-version` and `--version` select the same version. If both are supplied,
they must agree. `retire-checkout` operates on the entire checkout and rejects
`--project-version`.

Each entry includes `description` and the `cluster` map of label keys to values.
The text output preserves label-key spelling for reuse with `--label-key`.
It distinguishes historical `registered`, retained `versions`, and
checkouts with `registered_version`, actual observed `exists` (null when
filesystem observation fails), daemon-local `online` membership and `current`
selection. Online does not describe Broker presence; use `graphit_instances`
for that independent journey. `--no-paths` omits local paths. Returned
`checkouts[].dir` is selection data and never grants module/content access.
Only native callers admitted for that PROJECT/version see it; installation
Configure administrators can inspect historical entries including retired
projects. Discovery never starts a project or adds access.

```sh
graphit projects list --name graphit --page-size 10
graphit projects show --name graphit-code
graphit projects list --online=false --no-paths --json
graphit projects list --label-key service --label-value api
```

Use `graphit projects label-set --project-id ID --key service --value api` to
add a value. `graphit projects label-unset --project-id ID --key service --value api`
removes that value. Omit `--value` from `label-unset` to remove the entire key.
These commands target the active checkout in the current project by default.
Use `--project-dir DIR` to select a different checkout and `--version VERSION`
when that checkout has multiple active versions. The supplied ID must match.

Root MCP tools `graphit_projects_labels_set` and `graphit_projects_labels_unset`
take `project_id`, exact `project_dir`, `key`, optional `project_version` and
`value` (required for set). For MCP discovery use `label_key` and `label_value`.
Keys and values are case-sensitive after trimming surrounding whitespace.
Labels require current administrative access to the selected checkout. A repeat
with the same intent is safe. If the command reports `inventory_refresh_failed`,
the checkout document and inventory may temporarily differ. The CLI exits with
code 1 and prints the receipt to stdout. With `--json`, inspect
`document_changed` in the receipt. After resolving the inventory write failure,
repeat the same operation to refresh the inventory.


## Memory from the terminal

The human CLI uses the existing Native record authority through the daemon;
Root MCP writes and delegated Role permissions are unchanged. Commands are
`graphit memory list`, `get`, `search TEXT`, `history`, `create`, `update`,
`delete`, `mandatory` and `important`. Reads accept `--project-dir` or
`--project-id` and the global `--project-version`; no selection grants access.
`list` and `search` filter classification, tag, mandatory, important and
exclude-mandatory. Search defaults to FTS, with explicit semantic/hybrid modes.

Creation requires `--idempotency-key`, `--title` and `--body TEXT` or
`--body-file PATH`. `--body-file -` reads UTF-8 standard input under the existing
input byte limit. Updates use the same body inputs; omitted fields stay intact.
Mutations require an idempotency key; update/delete/flag changes require
`--id`, `--expected-revision` and `--reason`. `delete` writes the existing
tombstone and retains audit history. `mandatory --value true|false` and
`important --value true|false` use the same native attribute updates as Role
mark/unmark and promote/demote. A stale CAS or conflicting idempotency key fails
without automatic replay. Partial/uncertain publication receipts retain their
actual effect; never infer rollback from a delivery failure.

`--scope user` selects personal Memory independently of the checkout. Local use
works without signing in and shares a private corpus across projects in the same
installation. With an active Broker account, the scope uses its verified Native
identity and its own USER admission. PROJECT is the default. USER rejects
PROJECT selectors. Text is the default terminal output. Use `--json` for
automation.

```sh
graphit memory list --classification decision
graphit memory create --idempotency-key decision-1 --title "Validation policy" --body-file policy.md
graphit memory mandatory --id MEMORY_ID --expected-revision 1 --idempotency-key mandatory-1 --reason "Standing policy" --value true
graphit memory history --id MEMORY_ID --json
graphit memory list --scope user
```


## Complete terminal audit export

`graphit task export --id TASK_ID --output task.json` exports the native current
record, creation, last event and generation, then follows the original pinned
history and comments start cursors through EOF. `export-session` does the same
for Session history and includes linked Task catalogue heads through EOF.
`--page-size` defaults to 20 and changes only transport page size. Authored
references, parent/dependency/session links remain in their native records.
The linked Task catalogue has its own SDK generation; no global transaction
between separate snapshots or corpora is promised. Catalogue responses now
include their actual optional generation (`null` for no corpus).

The JSON artifact uses the existing native audit record/page fields. It is
written to a temporary file and atomically published at `--output` only after
every requested section has arrived, every cursor has reached EOF and the
connection has closed successfully. An existing destination stays intact on
read/permission/cursor-expiry/cancellation/serialization failure. Successful
publication replaces it with a complete file. The terminal confirmation is
text by default (`--json` for automation); the artifact itself is always JSON.
An incomplete or repeated cursor, changed generation or missing requested
section is an error, never a truncated successful export.

```sh
graphit task export --id TASK_ID --output audit.json --page-size 10
graphit task export-session --id SESSION_ID --output demand.json
```


## Published Knowledge catalogue and lint

`graphit knowledge list` (`graphit_knowledge_list`) reads document metadata
without a textual search. Filters are exact `--kind`, `--source-prefix` over
the retained origin locator, `--stale[=true|false]` for persisted markers and
`--metadata-errors[=true|false]` for missing required effective kind metadata.
Use `--page-size` (default 25) and the unchanged JSON `--cursor` with the same
filters. Pages expose the actual generation; a changed generation or filter
expires the cursor explicitly. `state: no_corpus` differs from a published
page containing zero documents. Selection remains an admitted PROJECT/version.

`graphit knowledge lint` (`graphit_knowledge_lint`) validates the complete
published snapshot through the existing metadata and reference validators.
It reports missing metadata, broken references, orphan/short pages and stale
markers, plus age only with an explicit positive `--max-age-days`. It does not
inspect current authored files or repair/index documents. Each finding gives
a slug, retained source locator, field, severity and action. The published
metadata producer has no authored line provenance: `line` remains null and
`basis: published_metadata`; field locations are not invented source lines.
No partial report is returned if snapshot reading, limits or lint fail.

Lint exit codes: 0 for a complete published report with no issues (including
published-empty), 1 for findings or operational failure, 2 for CLI usage errors,
3 for an absent corpus. Default output is human-readable; use `--json` for CI.
Root exposes these read operations; Memory/Task Role mutation restrictions are
unchanged. Role Knowledge list continues its existing authorized head listing.

```sh
graphit knowledge list --kind guide --page-size 20
graphit knowledge list --stale --json
graphit knowledge lint --max-age-days 90 --json
```


## Incoming and outgoing references

`graphit references task --task TASK_ID` (or `--session SESSION_ID`) queries
Task/Session edges in the selected native Task corpus. `graphit references
knowledge --outgoing SLUG` queries document edges; add `--target-kind`,
`--target-id` and exactly one `--target-project-id`/`--target-user-id` to query
incoming references to a logical target. `--target-context` preserves an
imported identity. The Root MCP read tools are `graphit_task_references` and
`graphit_knowledge_references`, using the same producer contracts as Role tools.

Read permission applies to the origin corpus. Target IDs, scopes, contexts and
recorded relationships are data; they never open a foreign corpus or grant
content access. Outgoing and incoming pages have independent native opaque
JSON cursors (`--outgoing-cursor`, `--incoming-cursor`), actual generation
bindings and `--page-size` (default 20). Continue each direction until its own
EOF. An expired/invalid cursor is a failure, not an empty page.

```sh
graphit references task --task TASK_ID --json
graphit references knowledge --outgoing GUIDE_SLUG
graphit references knowledge --target-kind task --target-id TASK_ID --target-project-id PROJECT_ID
```


## Explicit index operations

`graphit index status` (`graphit_index_status`) reads the authorized selection's
current producer observations. `graphit index run` (`graphit_index_run`) requires
Configure authority and requests the existing supervisor's coalesced full-project
rescan. It neither creates an indexer/scheduler nor clears corpora/restarts the
daemon. The current administrative handle supports project scope; separate
module selection follows that module’s diagnostics while execution remains project-wide; path requests are unsupported. Automatic indexing and hook
rescans remain unchanged.

Status separates AST graph/source, lexical maintenance and embeddings, and
Knowledge documents/lexical from embeddings. It distinguishes no corpus,
unobserved, pending, running, completed and failed where observed; embeddings
can be disabled, models unavailable or have no new publication. Observations
are runtime facts, not persistent historic telemetry. Completion timestamps
are recorded only at actual observed producer completion; absent fields stay
absent. Independent phase failures and last useful errors remain visible.

Run returns `accepted` and a request sequence; this is not processing completion.
`--wait-ms N` explicitly follows the correlated supervisor pass for a bounded
period. `processed` means that pass returned; its phase errors can still report
a failed outcome. Timeout exits 3 and preserves the accepted receipt. Ctrl-C
cancels the waiter, not the shared producer or an accepted hint; a stopped
supervisor may discard an accepted request. No global corpus transaction or
percentage progress is promised. Default output is text; `--json` is explicit.

```sh
graphit index status
graphit index run
graphit index run --wait-ms 10000 --json
```

Task-origin backlinks also support any logical target (Task, Session, Memory or Knowledge), including foreign project/USER identity and imported context. `graphit references task-backlinks --target-kind knowledge --target-id guide --target-project-id PROJECT --target-context package@1 --page-size 20` calls `graphit_task_references` with `input.target`, `page_size` and an optional native `cursor`. Focus mode retains separate incoming/outgoing cursors. Both modes read the admitted source Task corpus without opening the target.

`graphit index run --module ast` (or `knowledge`; MCP `input.module`) identifies the module whose diagnostics are followed. The existing supervisor coalesces this into a full-project pass, explicitly returned as `scheduled_scope: project` with `requested_module`; other enabled modules may also process. Omission requests the project. Narrow path execution is not supported by this supervisor and is not advertised.

If a Memory write commits but its JSON receipt cannot be encoded within the response limit, the Native producer retains its effect in dedicated response headers. The terminal returns that minimal receipt with `receipt_source: response_headers` and `response_complete: false`, then exits with an operational error for incomplete delivery. The reported native state remains `completed`, `confirmed`, `uncertain` or `not_applied`; it is not rewritten as rollback or full response success. Read the current record before retrying. No unrelated headers, credentials or record body are printed by this fallback.
