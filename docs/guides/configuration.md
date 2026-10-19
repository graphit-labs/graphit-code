---
title: "Configuration Reference"
updated: 2026-10-30
description: "Rust configuration commands, typed settings, layers, scopes, module switches and domain configuration references."
content-type: reference
audience: users
keywords:
  - configuration
  - modules
  - environment variables
  - embedding
  - rerank
  - S3
  - daemon
  - security
related:
  - "docs/guides/getting_started.md"
  - "docs/guides/user_manual.md"
  - "docs/specs/config_module.md"
  - "docs/specs/ai_engine.md"
  - "docs/guides/ai_models.md"
  - "docs/guides/daemon_operations.md"
  - "docs/guides/github-actions-artifacts.md"
references:
  - target: {type: task, id: tsk-b45e}
    relation: implements
  - target: {type: task, id: tsk-861f}
    relation: implements
  - target:
      type: task
      id: tsk-f4cd
    relation: supports
  - target:
      type: task
      id: tsk-3747
    relation: supports
  - target:
      type: session
      id: ses-c98a
    relation: relates_to
  - target:
      type: task
      id: tsk-6997
    relation: supports
  - target:
      type: task
      id: tsk-fcf0
    relation: supports
  - target:
      type: task
      id: tsk-542d
    relation: supports
  - target:
      type: task
      id: tsk-a4ca
    relation: supports
  - target:
      type: task
      id: tsk-2dfb
    relation: supports
  - target:
      type: task
      id: tsk-6e2e
    relation: supports
  - target:
      type: task
      id: tsk-8ba1
    relation: supports
  - target:
      type: task
      id: tsk-c10c
    relation: supports
  - target:
      type: task
      id: tsk-9d9d
    relation: supports
  - target:
      type: task
      id: tsk-ba35
    relation: supports
  - target:
      type: memory
      id: 01M48ZVYFZQE3W17F7T2ZMAE84
    relation: derived_from
  - target: {type: task, id: tsk-3ddb}
    relation: relates_to
  - target: {type: task, id: tsk-ad05}
    relation: relates_to
  - target: {type: task, id: tsk-e493}
    relation: relates_to
  - target: {type: task, id: tsk-7a13}
    relation: relates_to
  - target: {type: task, id: tsk-c3bf}
    relation: relates_to
  - target: {type: task, id: tsk-0257}
    relation: relates_to
  - target: {type: task, id: tsk-47fd}
    relation: relates_to
  - target: {type: task, id: tsk-d75f}
    relation: relates_to
  - target: {type: task, id: tsk-b788}
    relation: relates_to
  - target: {type: task, id: tsk-4323}
    relation: relates_to
  - target: {type: task, id: tsk-376b}
    relation: relates_to
  - target: {type: task, id: tsk-13b9}
    relation: supports
  - target: {type: task, id: tsk-61a0}
    relation: derived_from
  - target: {type: task, id: tsk-ec10}
    relation: derived_from
  - target: {type: task, id: tsk-27c1}
    relation: implemented_by
  - target: {type: task, id: tsk-5f7d}
    relation: relates_to
---

# Configuration Reference

Graphit can run entirely on one workstation, coordinate several agents against shared S3-backed
state, or serve reusable contexts to a team. The same configuration system covers those modes.
This page explains configuration CRUD, typed values, resolution and scope. Domain guides document
the settings consumed by each feature. The [key/default reference](../reference/configuration-keys.md)
lists current switches, structured settings, input bounds, capture and reload behavior.
JSON examples containing only `settings` are Global documents or sections of an
initialized project document; a complete project document also requires its stable
`id` and `name`. See [the project document](#the-project-document) before editing it.

The Rust CLI edits the maintained typed configuration stores. Authentication
providers/accounts and AST profile YAML keep their own commands and consumers.
See the [configuration contract](#rust-configuration-library) for scope and safe inspection.

## List values and structured settings

Scalar-list configuration uses comma-separated text, not a JSON array argument.
Quote the whole value in the shell when needed. The same values work in Project
and Global configuration, environment overrides and `-c` command overrides, under
the existing precedence rules. A higher-priority layer replaces the whole list.

```sh
graphit config set knowledge.extensions 'md,txt,pdf'
graphit config set ast.grammars_whitelist 'rust,typescript'
graphit config set ast.scip.parsers 'foo=registry.example/team/scip-foo:1,bar=registry.example/team/scip-bar:2'
graphit -c knowledge.extensions=md,txt config get knowledge.extensions
```

The scalar CSV keys are `ast.grammar` (`extension=grammar`),
`ast.grammars_whitelist`, `ast.grammars_blacklist`, `ast.scip.parsers`
(`extension=image`) and `knowledge.extensions`. Their consumers retain each key's
validation; empty text clears or removes that key's selection according to its
contract. CSV has no custom escaping layer: a comma separates items, so do not use
it inside one item. Empty or duplicate items are handled by the corresponding
consumer's validation, rather than silently removed globally.

`task.browser` is still one strict configuration object: its `providers` field can
be `"dev,team"` or an array of provider names. The other fields (authorization
endpoint, origin and session options) keep their own types. Empty providers and
duplicates remain invalid. `daemon.models.slots` and agent instruction rules,
`skill_aliases` and `skill_bundles` contain nested records and therefore retain
structured JSON; flattening them into CSV would lose fields and types. Edit those
settings in the ordinary configuration file or use their existing specialized
commands. Do not create a second file format or encode object lists as an ad hoc
CSV language.

`config set` and `-c KEY=VALUE` use the same key-aware parser. Secrets stay literal
strings (even when the value resembles `false`, `123` or JSON), and unrelated
strings containing commas are not inferred to be lists. Boolean/numeric settings
and structured objects retain their JSON types. This is CLI/configuration syntax;
it does not change MCP or HTTP schemas.

## Resolution and scope

Ordinary layered settings use the first **present** value in this order, including
empty strings, `false` and `0`:

1. one-command override: `graphit -c key=value ...`;
2. environment variable;
3. project config in the checkout's `.graphit/project.json`;
4. global config in `~/.graphit/config.json`;
5. a value compiled into a private distribution;
6. the code default described below.

Every ordinary key gets an environment variable mechanically: uppercase the key, replace dots
with **double underscores**, and prefix it with `GRAPHIT_`. For example,
`knowledge.docs_dir` becomes `GRAPHIT_KNOWLEDGE__DOCS_DIR` and `modules.task` becomes
`GRAPHIT_MODULES__TASK`. Bootstrap variables retain their documented names.

```sh
# Persist for the current Graphit checkout.
graphit config set knowledge.docs_dir documentation

# Persist only in this installation.
graphit config --global set custom.retry_count 3

# Override one command or use the SDK environment name.
graphit -c knowledge.docs_dir=documentation config get knowledge.docs_dir
GRAPHIT_KNOWLEDGE__DOCS_DIR=documentation graphit config get knowledge.docs_dir

# Inspect resolved values or only the stored Global document.
graphit config list
graphit config get knowledge.docs_dir
graphit config --global list
graphit config unset knowledge.docs_dir
```

Default writes require an existing Graphit checkout. Default `get/list` resolve
captured layers, including the current project when available; `--global get/list`
inspect only Global and ignore Project, environment and inline overrides. Reads
outside projects are supported and create no files. Configuration operates on the
physical checkout and rejects `--project-version`. `--timeout` controls the
positive writer-lock wait, default 30s; it creates no resource reservation.

Values use JSON types or literal text; public null is invalid. `set/unset` report
only `changed`; `get/list` include source and SDK inspection. Public values appear
as `{"kind":"public","value":...}`. Supported secrets such as `client.secret`
appear only as `{"kind":"secret","configured":true|false}` and cannot be stored
in Project. No `--secret` flag or raw-secret output is needed. Authentication,
Broker, AI and S3 credentials belong to the restricted Auth provider/account store.
`ai.cli` selects one completion executable name or path, captured when the daemon starts. `ai.agent_args` remains outside runtime configuration.

## Installation settings

These settings belong to the installation and cannot be persisted in Project:

| Key | Purpose |
| --- | --- |
| `ai.cli` | Completion CLI executable, separate from the selected agent integration; restart the daemon after changing it. |
| `unit.id` | Local installation attribution; it does not grant remote authority. |
| `client.secret` | Secret installation salt for event pseudonyms. |
| `ui.auth.cookie_encryption_key` | Accepted secret scope policy; not consumed by the current opaque-handle Rust Browser. |

For example:

```sh
graphit config --global set ai.cli copilot
graphit config --global get ai.cli
```

## AST compiler images

`ast.scip.parsers` is a string of comma-separated `extension=image` associations,
defaulting to the empty string. It uses ordinary configuration layers and wins as
a complete value: Command > Environment > Project > Global > Distribution.
A higher-layer empty string clears lower routes; lists are not merged. The normal
JSON configuration envelope stores a string, for example
`{"settings":{"ast.scip.parsers":".zig=registry.example/team/scip-zig:1"}}`.

```sh
graphit config set ast.scip.parsers '.zig=registry.example/team/scip-zig:1'
export GRAPHIT_AST__SCIP__PARSERS='zig=localhost:5000/scip-zig:dev'
graphit -c 'ast.scip.parsers=.zig=registry.example/team/scip-zig:1' daemon foreground
```

Extensions accept an optional leading dot and ASCII-case-insensitive matching;
collisions and malformed winning configuration fail. Surrounding item/assignment
whitespace is trimmed; missing `=`, empty extension/image and empty comma-separated
segments are errors. `graphit config set ast.scip.parsers ''` clears the current
Project associations. Custom routes override the
maintained compiler route for their extensions and can admit compiler-only
extensions. References are passed unchanged to Docker; `ast.scip.version` applies
only to maintained Graphit images. Docker's normal credentials handle private
registries. An image association does not install a syntax grammar; fallback uses
the Tree-sitter/ANTLR grammars linked into Graphit. See
[AST extensibility](ast_extensibility.md#custom-compiler-images-by-extension) for
the image, mounts, output, cache, failure and Source contracts.

## Module switches

Module switches use actual JSON booleans. The CLI validates recognized switches
with the same `ModuleSettings` API used by consumers before persisting them.
An invalid winning value is an error; it never falls through to a default.

| Keys | Default |
| --- | --- |
| `modules.task`, `modules.memory`, `modules.ast`, `modules.knowledge`, `modules.hub` | `true` |
| `modules.daemon`, `modules.sync`, `modules.embedding`, `modules.hooks`, `modules.agent` | `true` |
| `modules.dream`, `modules.daemon_ui` | `false` |

`modules.embedding=false` is the embedding opt-out, including Task consumers;
enabling Task or selecting a provider/model must not bypass it. It does not
disable the independently selected rerank service. Source publication and
lexical retrieval do not require embedding inference.

```sh
graphit config --global set modules.embedding false
```

Task and Memory orchestration consume this same switch; inspect the selected
build/provider and current service error rather than assuming model installation
enables work. See the [foreground runtime contract](../specs/rust_source_foreground.md).

```sh
graphit config set modules.sync false
graphit config --global set modules.agent false
GRAPHIT_MODULES__DAEMON_UI=true graphit config get modules.daemon_ui
```

`modules.daemon_ui` is Environment-only: both persisted scopes reject it. Its
variable is `GRAPHIT_MODULES__DAEMON_UI`. Environment booleans accept JSON
`true`/`false` with surrounding JSON whitespace. `TRUE`, `1`, `yes`, an empty
value and a quoted boolean string are invalid for the typed module consumer.
Inspection reports the captured environment text; typed consumers interpret it.

Module switches control orchestration. They grant no PROJECT permission and do
not prove an external CLI is installed. Disabling Agent leaves local retrieval,
graph and model gates independent. Enabling Daemon UI does not itself start a
server. Consumers capture or reload their own settings; see
[daemon operations](daemon_operations.md) and the
[module contract](../specs/config_module.md#rust-module-settings-contract).

## Authentication, models and agent selection

Providers, accounts, Broker topology, AI service selectors, S3 credentials and
renewable sessions belong to Auth, outside ordinary configuration CRUD. A Broker
provider permits only `broker` or explicit `disabled` for embedding and rerank.
It cannot select Local/Direct inference or silently fall back to it. A Local
provider can select its two services independently. Local execution requires the
exact model, device, rule and authority admitted by the service credential.
See [authentication](authentication.md), [Broker](auth-broker.md) and
[AI models](ai_models.md).

The public `agent` setting selects native artifact/host integration. `ai.cli`
selects completion independently; neither stores vendor model credentials. Kiro uses the current `kiro-v3`
binding. See [setup and CLI commands](cli_reference.md) and
[native installation contracts](../specs/hub_native_installations.md).

### Rust local model install and select

Use the model commands to install and select a verified local model:

```sh
graphit models install embeddinggemma2
graphit models select embeddinggemma2-bfcd298762cc34d0357ece5ebdd31791a3a374d8/model-256.json
```

The model consumer stores named slots in `daemon.models`. Use its dedicated
commands for the manifest, artifact integrity and runtime validation rather than
hand-building this structured value. An explicit empty slot list is an opt-out;
model installation preserves an existing embedding selection. See
[local model installation](ai_models.md#rust-local-model-install-and-select) and
[the model CLI contract](../specs/rust_cli_models.md).

### Rerank activation and request overrides

`search.rerank` defaults to `false`. Selecting a rerank provider or installing
its model does not activate inference. Set the key explicitly in the scope you
need; an explicit PROJECT value overrides Global:

```sh
graphit config --global set search.rerank true
graphit config set search.rerank false
```

An omitted request option follows the effective configuration. An explicit
`--rerank=false` (or request `search.rerank: false`) overrides even configured
`true` and skips rerank RPCs, discovery, automatic preparation/download and model
loading. It does not suppress embedding requested by a semantic/hybrid query.
Use lexical mode when embedding inference is not needed:

```sh
graphit knowledge search "parser cancellation" --mode fts --rerank=false
```

Embedding and rerank remain independent: `modules.embedding=false` does not turn
off an explicitly enabled reranker. See [rerank modes and activation](ai_models.md#rerank-modes-and-activation).

### Local model idle timeout

`models.idle_timeout_seconds` defaults to `300` seconds and applies to both local
embedding and local rerank, with activity tracked separately for each loaded
model. Configure this installation-wide policy in Global:

```sh
graphit config --global set models.idle_timeout_seconds 300
```

The interval starts when actual inference work completes; active work and native
cleanup after cancellation finish before unloading. Cache hits do not extend the
interval. The next uncached operation reloads the selected model. Unloading
weights preserves installed files, indexed vectors and model identity; it does
not terminate the model-service process. Zero and unsupported durations are
invalid. Broker and Direct services control their own residency. See
[the model service contract](../specs/rust_model_service.md).

## Knowledge indexing

Knowledge consumes its captured documentation settings, including
`knowledge.docs_dir`, `knowledge.include_readme` and `knowledge.extensions`.
Changing a setting through Config persists it; indexing and context selection
remain Knowledge operations. See [Knowledge](../specs/rust_foreground_knowledge.md) and
[the Rust Knowledge settings contract](../architecture/rust-knowledge-settings.md)
for defaults, typed validation and update behavior.

## AST indexing and language control

The local native graph uses `ast.query.buffer_pool_bytes` and
`ast.query.threads` (default `2`) for its shared query and FILE publication
handle. An absent pool setting, or `0`, uses Ladybug's own default of approximately
80% of available system memory. This is a maximum for each native database pool;
data pages use physical memory on demand. Opening a database also allocates the
SDK's pool metadata, so initialization is not allocation-free. An explicit positive
value in bytes is preserved and must be at least `4096`. The pool must accommodate
uncommitted FILE rows together with query workspace; insufficient explicit values
return an allocation error. This setting is not a limit on the daemon's total
memory or a shared budget for all projects.

For example, set an explicit 1 GiB pool, or restore native automatic sizing:

```text
graphit config set ast.query.buffer_pool_bytes 1073741824
graphit config set ast.query.buffer_pool_bytes 0
```

The setting is used when the native database handle is opened.

AST profile YAML, grammar actions, linked grammars and index state retain their
own loaders and stores. Configuration CRUD does not parse source, load a parser
or fabricate an index. Public AST settings and compiler image routes remain separate. The obsolete `ast.cluster` alias is rejected.
See [AST grammars and parser extensibility](ast_extensibility.md),
[AST configuration](../specs/ast_module.md) and
[ignore files](ignore_files.md).

## Hub, shared state and project versions

Storage and remote identity use the authenticated provider. Public configuration
cannot grant Hub access. Installed artifacts, maintained project versions and
instance attribution use their domain stores; Config operates on the physical
checkout and rejects `--project-version`. It performs no conversion of an old
configuration or data layout. See [Hub access control](../specs/hub_access_control.md)
and [project identity and versions](../specs/project_identity.md).

### Git hook commands

`githooks` maps Git events to ordered arrays of complete commands. The
normal layered Config store owns command bodies; Git stores only a portable dispatcher
in `hook.<brand>--<event>.(command|event|enabled)` (Git 2.54+). Config CRUD changes command
bodies immediately for registered events; `graphit git sync` or project-wide `sync`
reconciles added/removed events. Commands are never CSV. See [Git hooks](git-hooks.md)
for installation, shell/input/error contracts and Task/Memory integration, and the
[registry reference](../specs/git_module.md#rust-configured-hook-registry) for local ownership.

## Network listeners and browser authentication

Listener, CORS and browser-session settings are consumed by their HTTP runtimes.
CORS is separate from authentication; a valid JWT is separate from PROJECT
authorization. Access-token validation uses trusted local signature and claim
validation until expiration. UserInfo is only needed for missing profile data,
not as a required per-request validity check. Account blocks, revocation and
claim changes can take until access-token expiration to become visible.
See [authentication](authentication.md), [Broker](auth-broker.md),
[daemon operations](daemon_operations.md) and the
[CLI reference](cli_reference.md) for current entrypoints and listener options.

`ui.auth.cookie_encryption_key` is a supported secret and is installation-only.
`get/list` expose only its configured state. Persisting that key does not enable
web authentication, register an OAuth client, encrypt current opaque handles or grant browser access.
The current Browser keeps credential-bearing sessions in memory; see [authentication](authentication.md#web-ui-login).

## Bootstrap, deployment and runtime controls

`--global-dir` and the branded `GLOBAL_DIR` environment variable locate the
installation before ordinary configuration is loaded. They are not persisted
runtime keys. The CLI captures CWD, environment and installation once before
constructing its stores and snapshots. Resolution prefers an explicit
`--global-dir`, then the captured branded `GLOBAL_DIR`, then the ordinary
home-based installation directory. It does not infer an installation from the
executable's parent directory.

Published hooks and MCP configurations use the branded executable through PATH;
PROJECT hooks select `.` relative to the workspace. A host using a custom
installation must inherit the installation variable and the correct PATH.
Codex forwards the variable by name through `mcp_servers.<name>.env_vars`; the
published document contains no resolved installation path. Graphit's builtin
allowlist also names the transient Role/Live transport variables needed when
those authenticated modes launch; credential values are never stored in the
MCP document. Configure the host
launch environment rather than storing literal `$HOME` or `~` in JSON/TOML
arguments. Changing the selector does not import or merge another installation.

Only keys requested by a consumer are read from environment. An arbitrary
variable does not create a feature. Config inspection captures maintained known
keys, stored/inline keys and the explicitly requested `get` key. Names with no
consumer can be stored as public custom settings but have no implicit effect.

For image and platform inputs, use the maintained
[container guide](container.md), [getting started](getting_started.md) and
[daemon operations](daemon_operations.md). Use the SDK's double-underscore
mapping for ordinary layered keys. Bootstrap and tool-native variables retain
their documented spellings.

## Verify an effective setup

```sh
graphit config list
graphit config --global list
graphit config get modules.sync
graphit config --help
```

`get` fails if a key is absent from the captured documents. Generic inspection
does not materialize every domain default or validate every custom key's meaning.
A successful write proves persistence under the selected scope; its domain
consumer owns applying or reloading the value.

## Rust configuration library

The Main commands above use `ConfigStore`, `ConfigDocument`, `ConfigSnapshot`
and SDK inspection. Global configuration remains `<global>/config.json`, with root
`githooks` and ordinary keys inside `settings`. It never contains project identity.

### The project document

The checkout has one shared `.graphit/project.json` (or `.<brand>/project.json`):

```json
{
  "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "name": "payments-api",
  "description": "Payment service",
  "cluster": {"domain": ["payments"]},
  "ephemeral": false,
  "githooks": {"pre-commit": ["git diff --cached --check"]},
  "settings": {"knowledge.docs_dir": "docs", "modules.ast": true}
}
```

Preserve the initialized ID; never copy an example ID to create a different project.
The document has these responsibilities:

| Field | Meaning |
|---|---|
| `id` | Required canonical, nonzero stable ULID. Owns the same project across checkouts; not a Config key. |
| `name` | Required validated project name; changing it does not rekey stores or versions. |
| `description` | Optional descriptive text, preserved literally. |
| `cluster` | Optional maintained map of labels to string arrays, under existing normalization/validation rules. |
| `ephemeral` | Boolean, false by default; retains the existing persistent-project eligibility semantics. |
| `githooks` | Optional event → ordered command-array map at the root. Inherits through normal Config layers; never CSV. |
| `settings` | Ordinary typed configuration keys; empty or omitted means no ordinary Project overrides. |

Identity and metadata belong to this project, outside inherited settings. Global,
environment and `-c` values cannot replace them; `config set id`, `name`,
`description`, `cluster` or `ephemeral` is rejected. Unknown root fields, malformed
metadata, invalid commands and duplicate representations fail rather than being
silently dropped. Root `githooks` is the only representation of that setting.

Global documents use the same configuration sections without the project fields:

```json
{"githooks":{"pre-push":["cargo xtask gate"]},"settings":{"knowledge.docs_dir":"docs"}}
```

The first present captured layer wins. Stored and command values retain JSON types.
Environment inspection shows the original string; the hook consumer parses
`GRAPHIT_GITHOOKS` as a JSON object, never as CSV. A Project
`githooks: {}` suppresses all lower-layer command hooks; absence inherits. The whole
map replaces lower maps. `graphit config unset githooks` removes the Project
field and exposes Global again; `graphit config --global unset githooks` removes
only the Global field. Changing commands is immediate at the next registered hook;
adding/removing events needs `graphit git sync` or general project `sync`.

Wrong winning types report errors rather than selecting a lower layer. Secrets are
redacted throughout inspection and are rejected in the Project document. Reading
creates nothing; malformed data fails without repair. Both Project writers use
`.project.json.lock`, reread the complete document and atomically replace it:
metadata edits preserve settings/hooks, Config edits preserve identity/metadata.
Global writers keep `.config.json.lock`. Equal changes avoid rewriting the target.
Project configuration writes require an initialized identity and never generate one.

Version the unified Project document and authored rules/profile overrides. Ignore
locks, secrets, caches and generated state through the shared `.gitignore`. No
second Project config file, alias, automatic importer or old-layout discovery exists.

See [Config Module](../specs/config_module.md#rust-configuration-contract) for
key syntax, strict decoding, per-file limits, cancellation, permissions, errors
and safe internal secret access. There is one maintained format and no importer.

### Rust adapter selection and layout

The maintained Hub selector consumes a captured snapshot and membership,
returning `NativeBinding` and origin. Its explicit/ambient selection rules are
specified in [Config Module](../specs/config_module.md#rust-adapter-selection-and-layout).
Generic Config precedence remains unchanged. Selection metadata grants no
authority and does not prove native vendor execution.

### Rust installation identity and event salt

`InstallationSettings::resolve` validates captured `unit.id` and `client.secret`
without I/O. Loading Global identity creates nothing. Explicit `ensure` generates
only missing fields and publishes them in one existing Config transaction,
preserving valid fields and other settings. Invalid present fields require an
explicit correction. Generated IDs are ULIDs; salts use OS randomness and remain
secret. Local attribution is separate from remote identity and OAuth cookies.
See [the identity contract](../specs/config_module.md#rust-installation-identity-and-salt).

### Rust module settings

The twelve switches above are resolved from the same immutable snapshot by
`ModuleSettings`. Already captured views do not change after a write. Consumers
choose their own reload boundary and authorization. See
[the module contract](../specs/config_module.md#rust-module-settings-contract).

## Rust configured-hook registry status

The registry takes an explicit checkout, Brand, canonical events and trusted
commands. It changes only its owned local Git config fields and coordinates
cooperating managers with a stable common-dir lock. A partial write is reported
causally and recovered by explicit reconciliation. Effective activation also
depends on Git scope precedence and the registered program being available.
See [the configured-hook contract](../specs/git_module.md#rust-configured-hook-registry).

## Rust TCP listener reservation

`tcp_listener` retains the bound socket and hands that same handle to the server,
avoiding a probe-and-rebind race. It supports an explicit IP with an ephemeral
port or inclusive nonzero port range, cancellation and a monotonic deadline.
Only address-in-use advances to another port; other errors remain causal.
Dropping the listener closes its handle. DNS, async handoff, readiness and
shutdown belong to the runtime using it. See
[daemon operations](daemon_operations.md) for the product listener lifecycle.

## Rust captured CORS policy

The HTTP CORS layer takes explicit origins, methods and headers. Disabled,
loopback HTTP, any-origin and exact-origin policies remain distinct. Request
origin validation has concrete input bounds; middleware never replaces
session/CSRF, token validation or PROJECT access control. Each HTTP surface
selects its own policy. See [network and UI configuration](s3-and-ui-network.md).

## Rust background startup priority

The maintained startup helper attempts lower CPU priority and, on Linux, idle
I/O priority. It preserves an already lower priority and reports unavailable or
unsupported operations without inventing resource reservations. Background
consumers call it before creating work threads and retain causal diagnostics;
interactive commands are separate. Scheduling outcomes depend on the OS.
See [daemon operations](daemon_operations.md).

## Rust caller scopes and Kiro binding

Caller contracts capture project/global/version scopes and retain concrete
input validation, cancellation and transport admission. Physical files remain
shared by a checkout even when domain versions differ. Kiro uses only the
current `kiro-v3` binding. See [the Hub consumer contract](../specs/hub_consumer_contract.md).
