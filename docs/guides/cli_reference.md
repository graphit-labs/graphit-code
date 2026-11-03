---
title: "CLI Command Reference"
description: "Commands available in the current Rust executable and its productive setup journey."
content-type: reference
audience: developers
updated: 2026-10-30
references:
  - target: {type: task, id: tsk-cab0}
    relation: corrected_by
keywords:
  - CLI
  - commands
  - setup
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
The [complete option reference](../reference/cli-options.md) enumerates all 133 inspected
help paths, command-specific flags, positional inputs and declared defaults.

## Shared options

| Option | Purpose |
|---|---|
| `--global-dir <directory>` | Select the private installation. Otherwise use the branded global-directory environment setting or the current home directory. |
| `-c, --config <KEY=VALUE>` | Override one configuration key; repeat for multiple keys. The last value wins. |
| `--non-interactive` | Run without interactive prompts. |
| `--project-version <version>` | Select an existing retained PROJECT version for commands that accept PROJECT scope. |
| `--version` | Print the compiled build version. |
| `-h, --help` | Print command help. |

A working directory supplies the current PROJECT for PROJECT commands. `setup`,
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

Task checks use parallel `--check-kind acceptance|test` and `--check-text TEXT`
options, paired in occurrence order. Authored references similarly pair
`--reference-type`, `--reference-id` and `--reference-relation`. For an optional
reference field, omit that option entirely or provide one value for every
reference. Empty scope/scope-ID/context entries mean absence. An empty
`--reference-field` is a literal empty field; use repeated
`--reference-field-absent INDEX` (1-based) for missing fields in a mixed list.
Omitting the field option entirely leaves every field absent. Mismatched counts,
invalid indices and conflicting options are rejected. Use the typed MCP/HTTP contracts for large structured automation payloads.
Claims remain private; flags do not manufacture authorization or bypass ownership.

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
| `graphit setup` | Configure this installation and optionally initialize a maintained project |
| `graphit config` | `set`, `get`, `list`, `unset`; `--global` selects the Global store |
| `graphit ui [--no-open]` | Open the configured Browser, or print its URL for headless use |
| `graphit tray` | Control the installation through the Linux native tray menu |
| `graphit self-update` | Verify and install the latest public release; resume this installation's daemon only if it was running |
| `graphit daemon` | `foreground`, `start`, `stop`, `restart`, `status`, `install`, `login`, `uninstall` |
| `graphit models` | `install`, `select` |
| `graphit coordinator` | `install`, `sync`, `remove`; select `--binding` |
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
| `graphit hub` | `install`, `install-local`, `sync`, `update`, `remove`, `recover`, `forget`, `content`, `list`, `projects`, `search`, `publish`, `publish-native`, `unpublish`, `install-native` |
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
graphit --project-version main hub update --binding codex
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
observed effects, pending state or uncertainty. Processing stops at the first
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
graphit coordinator sync --binding codex # Coordinator, hooks and MCP
graphit hooks sync --binding codex   # hooks and MCP independently
graphit hub sync                     # all bindings in the current PROJECT
graphit hub sync --global            # global Hub installations
```

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

`--skill-bundle MODULE=ID@VERSION` explicitly selects an exact installed Hub Skill as raw instruction input for one module: `task`, `memory`, `ast`, `knowledge` or `hub`. Project and Global raw overrides still take priority. Local sources retain physical and registry checks. A selected remote FILE source requires its current origin admission, the producer's PROJECT read authorization and the existing verified canonical cache receipt; missing or changed cache fails without downloading a replacement. For standalone sources, `--skill-identity-provider MODULE=PROVIDER` selects an explicit verified Native identity when needed. It is rejected for Broker or local sources and never supplies storage credentials or PROJECT permissions. Native files owned by other tools are preserved, including an occupied destination with equal content. Selected Hub license and metadata are retained. A malformed selected template fails without fallback. `roles run` rejects `--skill-bundle`; publish with install or sync first.

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
values. `set/unset` return JSON `changed`, and missing `get` fails without output.
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

## Installation and project setup

```sh
graphit setup --agent kiro-v3
graphit models install embeddinggemma2
graphit setup --agent kiro-v3 --project-dir ./project --name example
cd ./project
graphit coordinator install --binding kiro-v3
graphit coordinator remove --binding kiro-v3
```

Setup records the selected native agent. With `--project-dir`, it initializes
current PROJECT identity when absent, preserves an existing identity and metadata,
and maintains that checkout in the local daemon. It rejects the installation
root as a project. `--name` is only an initial project name. Optional
`--project-id <ULID>` requires `--project-dir` and initializes an absent identity
with that canonical, nonzero ID. An existing equal ID preserves all metadata; a
conflicting ID fails before changing the identity or installation agent setting.
Without this option a new project receives a generated ID. Selecting an ID
imports no data and grants no PROJECT permission. Project setup may start and
register the native daemon manager before maintaining the checkout; installation-only
setup does not start it. Setup does not download models or authenticate the agent CLI.
Setup rejects `--project-version` before changing configuration; it maintains the
actual checkout. Use `projects maintain-checkout` for an explicit maintained
version. For read commands and Root MCP, `--project-version` selects the requested
view only: automatic startup maintains the actual checkout and does not add the
requested version to the maintained inventory.

`coordinator install` publishes the versioned product Coordinator and PROJECT
context hooks through the authenticated daemon. The release includes the stock
Markdown artifact under `graphit-runtime/agents/coordinator` beside the executable;
no Graphit source checkout is needed. `--source-dir <directory>` explicitly
selects another artifact for the existing producer to validate. The response
contains the actual installed profile; use that returned value to start the agent.
Select `--binding` explicitly. Kiro uses `kiro-v3` and a Markdown profile. Other bindings publish their documented native configuration; Antigravity publishes workspace MCP and PreInvocation without claiming customprofile MCP. Kimi uses one global configuration under the captured KIMI_CODE_HOME or ~/.kimi-code, serving native project cwd without fixed project/version arguments. The Coordinator installer
needs no engine flag. Graphit's ACP consumers launch
`kiro-cli acp --agent-engine=v3` and select the model through session configuration.

`hooks install` publishes native agent context hooks. Repository Git hooks are a
separate integration configured by `githooks`; see [Git hooks](git-hooks.md).
`graphit git install` snapshots current Task/Memory state and registers the native
dispatcher; `graphit git sync` reconciles events without another snapshot. Project
setup and project-wide `sync` use the same registry producer. Git 2.54+ is required.
Commands are arrays of whole strings, never CSV. Changing commands of an already
registered event requires no reinstall; changing the event set requires sync.

For an existing Kiro Markdown profile, use
`graphit hooks install --binding kiro-v3 --profile coordinator.md`. This command preserves the profile's
foreign content and requires the existing profile to pass validation.

## Local models

`models install embeddinggemma2` explicitly downloads the pinned EmbeddingGemma 2
BF16 model. It preserves an existing embedding selection. `models select` selects
an installed manifest and any explicit MRL recipe accepted by its help. Both
commands operate on the installation without requiring PROJECT identity. Use
`models --timeout <seconds> ...` to bound the operation. No model download is a
side effect of setup.

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

`now --project <id> --provider <provider>` follows the Broker view continuously.
Use `--version` and `--instance` on that subcommand to filter the live view.

`mcp --stdio` serves the MCP client transport. `--project-dir` selects a default
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

`hooks remove --binding <binding>` removes owned hook tuples and MCP configuration. Kiro external profiles require --profile; use `coordinator remove --binding <binding>` for the complete product profile and its inline hooks. Kimi keeps global hooks/MCP until the selected project was the last reference, preserving foreign fields. These operations do not retire project data or grant/revoke authorization.

## Remote Hub catalog retraction

`graphit hub unpublish` runs outside a local PROJECT and selects the configured Broker provider, remote producer, kind and full artifact ID explicitly:

```text
graphit hub unpublish --provider hub-service --producer 01J00000000000000000000001 --kind skill --id xpto/review --version release/one
graphit hub unpublish --provider hub-service --producer 01J00000000000000000000001 --kind skill --id xpto/review --all-versions
```

Use real identities from the authorized remote catalog. Exactly one of `--version` and `--all-versions` is required. Versions are literal; `latest` is rejected. Optional `--expected-sha256` requires `--version` and checks its current hash before any effect. The MCP equivalent is `graphit_hub_unpublish` with provider, producer, artifact_type, id and strict target `{kind:"version",version:"release/one",expected_sha256?:"64 lower-case hex"}` or `{kind:"artifact"}`. It accepts no project_dir, project_version, binding or source path. This consumer supports Broker Native; it does not silently fall back to Standalone.

The current Runtime/Local caller needs its configured HubWrite authority and the Broker producer namespace grant. This does not grant PROJECT access or require a fictional local producer checkout. Removing a release preserves the other releases; if it was latest, latest becomes unset. Last-version and whole-artifact withdrawal delete only the current catalog conditionally. Immutable payloads, project metadata, source files, caches and local installations remain intact.

Success returns `status: unpublished` and `retraction` with producer, artifact_type, id, targeted_versions, desired_latest and catalog `updated` or `removed`. Failures preserve known effects or `uncertain`; a late cancellation/deadline may wrap the operation result under `outcome`. CLI failure retains that JSON in its diagnostic and exits unsuccessfully. Inspect the original remote selection after uncertain delivery or `inspection_required`; an empty MCP error under a very small output limit also requires inspection. No receipt identity or version is truncated, and no mutation is retried automatically.

The Broker must declare conditional catalog deletion support. If it is unavailable, whole-artifact and last-version withdrawal return Unsupported with catalog `not_attempted`, without sending DELETE. Removing one version while others remain still uses conditional catalog PUT and does not need that deletion capability. See [the storage contract](../specs/remote_artifact_storage.md#broker-stsv3-storage-capabilities) for the required STSv3 fields and SDK copy strategy.

To associate an authorized, successful Git commit with a Task, obtain the full SHA with `git rev-parse HEAD`, read the Task's current revision, and call:

```sh
graphit task set-task-commit --id '<task-id>' --expected-revision 7 --commit '<full-lowercase-sha>' --reason 'Record delivered commit'
```

Add `--claim-token TOKEN --claim-revision REV` when owned; never bypass another agent's claim. `--clear-commit` clears an incorrect CLI association (`commit:null` in MCP). Completed Tasks accept the metadata update without reopening or resetting evidence. After an amend, associate the replacement SHA only with Tasks included in that commit. These commands do not authorize or initiate commit/push.
