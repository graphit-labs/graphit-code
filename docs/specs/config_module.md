---
title: "Config Module Specification"
description: "Rust configuration CLI, typed documents, resolution, scope, safe inspection and atomic writes."
content-type: reference
audience: developers
keywords:
  - config
  - json
  - defaults
  - environment variables
  - settings
prerequisites:
  - "docs/architecture/architecture_overview.md"
related:
  - "docs/specs/daemon_module.md"
  - "docs/specs/mcpstdio_module.md"
references:
  - target: {type: task, id: 01M666RGGWZ1EFRFR1Z4N4Y8PN}
    relation: implements
  - target: {type: task, id: tsk-b45e}
    relation: implements
  - target: {type: task, id: tsk-4504}
    relation: implements
  - target:
      type: task
      id: tsk-f4cd
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
  - target: {type: task, id: tsk-3ddb}
    relation: relates_to
  - target: {type: task, id: tsk-e493}
    relation: relates_to
  - target: {type: task, id: tsk-ad05}
    relation: supports
---

# Config Module Specification

## CLI configuration

Main exposes four explicit commands. Default mutation targets the current
Graphit checkout; `--global` targets only the installation. No old command aliases
or configuration import is provided.

```sh
graphit config set knowledge.docs_dir documentation
graphit config get knowledge.docs_dir
graphit config list
graphit config unset knowledge.docs_dir
graphit config set ast.scip.parsers '.zig=registry.example/team/scip-zig:1'
```

`set KEY VALUE` uses JSON booleans, numbers, arrays, objects and quoted strings;
other text is a literal string. Empty strings, whitespace, `false` and `0` remain
present values. Public null is rejected. Supported secret keys always receive
strings. `set` and `unset` print only `{"changed":true|false}`. Module values are
checked with the existing `ModuleSettings` before persistence.

`get` emits the SDK inspection entry with `key`, `source` and a public value or
secret configured state. A missing key fails with empty stdout. `list` emits
ordered inspection entries. Default reads capture Command, requested Environment,
current Project when present, Global and Distribution layers; `--global get/list`
inspect only the stored Global document, without Project or transient overrides.
Environment inspection retains captured text; typed domain getters interpret it.
The CLI captures the maintained known keys, stored/inline keys and the requested
`get` key. An unrelated environment variable does not become a setting.

Reads work without a project and create no configuration, lock, identity, profile
or daemon. Default writes require an existing Graphit checkout; Global writes
work outside projects. `--project-version` is rejected because configuration
belongs to the physical checkout. `--global-dir` selects the installation as in
other commands. `--timeout` is the positive writer-lock wait in seconds, default
30; cancellation and SDK errors remain causal. Clap usage errors exit2, operational
errors exit1 and interruption exits130.

Providers, account profiles and their secrets remain in `provider`/`account` and
the Auth store. AST profiles remain YAML consumed by the existing loader. See
[Configuration Reference](../guides/configuration.md) and
[CLI Reference](../guides/cli_reference.md).

## Rust configuration contract

`crates/graphit-core/src/config/{key,value,document,snapshot,store}.rs` implements
`ConfigDocument`, `ConfigSnapshot`, `ConfigStore` and safe inspection. The Main
CLI consumes these APIs in `graphit-app/src/config_entry.rs` under `tsk-b45e`.
Configuration is separate from Auth providers/accounts, AST profile YAML and
agent installation. It does not create authority or a consumer for a custom key.

### Keys, values and captured layers

`ConfigKey::new` trims and ASCII-lowercases keys, limits them to 128 bytes and
16 dot-separated components, and requires alphanumeric edges with alphanumeric
or single underscores inside each component. Double underscores are rejected;
the encoded environment suffix is also capped at 128 bytes. Public custom keys
are accepted, but this does not create a consumer for an unknown setting.
Authentication namespaces `auth`, `providers`, `accounts`, provider/account S3
keys, `mcp.api_key`, and embedding/rerank provider/model/base-url/API-key keys
are excluded. Bootstrap `global_dir`, `model_cache`, `project_version`,
`ai.agent_args` (including descendants) and `ast.cluster` are outside runtime configuration. See `config/key.rs` for the exact registry.

`ConfigDocument` owns an immutable, cheaply cloned ordered map bound to one
`Brand` and `ConfigSource`. Public values are typed `serde_json::Value`; a
setting-level null is invalid, while empty strings, `false`, `0`, arrays and
objects are values. Strings retain whitespace unless a domain consumer later
normalizes it. Secret settings require strings.

`ConfigSnapshot` captures explicit documents in this priority order:
`Command` → `Environment` → `Project` → `Global` → `Distribution`. The first
**present key** wins, including an empty value. Duplicate sources or documents
from another brand are invalid. Getters never reread files, environment or CWD;
old snapshots remain unchanged after writes. An incompatible winning type is
an error, without falling through to another layer. Typed `get<T>` interprets
JSON text only for strings sourced from `Environment`; every other source
preserves its JSON type. `layer(source)` provides candidate access used by the
implemented Hub [adapter selector](#rust-adapter-selection-and-layout), whose
specialized precedence does not change Core's generic getter. Provider and model
selection retain their own domain contracts.

`capture_environment` receives the brand, a requested-key list and an OS-string
iterator. It derives uppercase names with dots replaced by **double underscores**:
`knowledge.docs_dir` → `GRAPHIT_KNOWLEDGE__DOCS_DIR`; `a.b` and `a_b` are distinct.
Unrequested variables, including bootstrap variables, are ignored. Relevant
non-UTF-8 values, duplicate relevant names and limits fail without echoing values.
Empty environment text is an explicit override. Matching values feed the bounded
document constructor incrementally, rather than accumulating an unbounded map.

Environment names are bijective by key **within one brand**, not globally by
(brand, key): brand `acme-code` / key `modules.ast` and brand `acme` / key
`code_modules.ast` both derive `ACME_CODE_MODULES__AST`. Callers choose a brand
and key list explicitly. This namespace is not an authorization boundary.

### Scope and inspection

The shared policy marks `client.secret` and `ui.auth.cookie_encryption_key` as
secrets. They, `unit.id`, `ai.cli`, `models.embedding.id` and `models.rerank.id` reject
`Project` storage. Syntax fallback uses package-linked grammars; configuration
does not authorize loading project-provided native shared libraries.
`modules.daemon_ui` accepts only `Environment`. The registry and policy apply
across constructors, persisted reads and mutations. Consumers retain their
own semantic rules, including global model selection. `ai.cli` is a completion
executable name or path captured once at daemon startup. A configured path is
retained; an invalid selection or missing executable never selects another CLI.
Without an explicit value, the selected agent integration determines one CLI.

Secrets use pinned `secrecy` 0.10.3 `SecretString`, with explicit
`expose_secret` access for internal consumers. Logical documents have no public
`Serialize` implementation. Their `Debug` output and inspection cannot reveal
secret payloads. Inspection entries contain `key`, `source` and `value`:
public values use `{ "kind": "public", "value": ... }`, while secrets expose
only `{ "kind": "secret", "configured": true|false }`, based on nonempty text.
There is no separate scope field; `Global`/`Project` source identifies persisted
scope. Document inspection lists its stored keys; snapshot inspection lists
resolved winners. Both are ordered, and mutation results report only `changed`.

### Explicit document input

`ConfigDocument::decode(&Brand, ConfigSource, &[u8]) -> io::Result<ConfigDocument>`
exposes the strict decoder for an explicitly selected file. The caller supplies
bytes and the source: Global uses optional root `githooks` plus `settings`;
Project requires the unified `project.json` with root identity/metadata. It preserves
canonical keys, scope restrictions, secret redaction, duplicate/unknown-field
rejection and the 256 KiB input limit. Errors remain sanitized `InvalidData`.
It creates no files, imports no other layout and grants no authority. Reading
an explicitly selected path under the original filesystem capture/request
remains the caller's responsibility; `encode` stays private.

Main Config uses `ConfigStore::load` and its existing decoder. It adds no
alternate configuration-file format or independent parser. SDK decoder checks
remain separate from the affected Main executable tests.

### Persisted format, bounds and errors

Global persistence uses root configuration sections:

```json
{"githooks":{"pre-commit":["git diff --cached --check"]},"settings":{"knowledge.docs_dir":"docs","modules.ast":true,"custom.retry_count":0}}
```

Project persistence adds required `id`/`name` and maintained metadata at the root;
see [Project document](../guides/configuration.md#the-project-document). There is
one Project file. `githooks` cannot appear inside `settings` and no earlier key
is accepted. Its complete command strings preserve commas, Unicode and escapes.
The identity/metadata portion keeps its 64 KiB validation within the complete
256 KiB Project document.

The envelope rejects unknown fields and duplicate envelope/settings entries.
Persisted keys must already be canonical and satisfy source policy. Nested
public JSON follows Serde JSON behavior; this is not rejection of every nested
object duplicate. Limits are 256 KiB encoded UTF-8, 1,024 settings and, per value,
64 structural levels / 8,192 nodes. Constructors and mutation batches enforce
structural and aggregate encoded-size budgets incrementally before retaining
all large inputs. Consumers still own semantic validation of domain values.

Invalid logical input, key, policy or mutation returns `InvalidInput`; malformed
persisted JSON/key/policy returns sanitized `InvalidData`. Incompatible typed
getters return `InvalidData`; ordinary getters for a secret require explicit
secret access and return `InvalidInput`. Other I/O errors propagate;
cancellation is `Interrupted`, lock waiting timeout is `TimedOut`, and changes
to read-only targets return `PermissionDenied`. Reads do not repair bad files.

### Stores and atomic mutation

`ConfigStore::global(&InstallationPaths)` targets `<global>/config.json`;
`ConfigStore::project(&ProjectPaths)` targets `<root>/.<brand>/project.json`.
Project root metadata and identity remain project-owned, not configuration layers.
Root `githooks` is projected into the normal typed `githooks` key; it is rejected
inside `settings`. Identity keys cannot enter Global/Environment/Command layers.
Construction and loading create no directories, lock, identity or config file;
absence yields an empty document. Observed project brand directories and config
sources are revalidated: symlinks and special sources are rejected.

An owned non-Clone writer holds the stable target-adjacent lock: `.config.json.lock`
for Global and `.project.json.lock` for Project, shared with ProjectIdentityWriter.
Both Project producers open the lock relative to a retained brand-directory
handle without following links, using the same private profile from its first
creation (Unix 0600; protected explicit owner on Windows). The empty lock keeps
its physical identity across both kinds of writer; an incompatible existing
profile fails without repair or replacement. Global retains its separate lock
acquisition and directory policy.
`apply(&mut self, changes)` rereads under that lock, preserving other writers'
keys and, on Project writes, the identity and metadata; metadata writers preserve
configuration in the other direction. Convenience `set`/`unset` validate input before acquiring it. Handles
pointing to the same target coordinate on the same kernel
lock. Equal changes return `false` without target rewriting; unsetting an absent
key does not create a config file, though acquiring a mutation writer can create
its directories and lock. Neither normal release nor cleanup may replace or
unlink the lock while holders or waiters exist.

The shared configuration reader uses `Zeroizing<Vec<u8>>` from allocation,
including error exits; private encoding also uses a zeroizing buffer. Secret
ownership and raw buffers are wiped on drop, without a guarantee covering every
allocator/Serde copy or memory locking. Atomic writes use a same-parent temporary,
permissions before payload, file sync and cancellation before replacement.
Global directories are created with Unix 0700 and new global files with 0600;
existing global permissions become `original & 0600`, never adding permissions
(0460 becomes 0400), and read-only changes remain denied. Project public-config
files preserve existing permissions; new files use 0666 with umask or Windows
creation defaults. The shared filesystem limits are in
[Filesystem State](../guides/filesystem_contract.md#project-owned-files).

The shared Gitignore excludes `.graphit/.project.json.lock`; Project's unified
`project.json` remains versionable. Global's lock stays in the private installation.
Config and metadata edits publish one complete document, preserving its ID.
A Project write requires an initialized identity; it never creates/rekeys one.
CLI mutation uses this same writer and does not start a daemon or create an Auth
profile or installation identity. Root `githooks` uses whole-map precedence and
ordered command arrays; null, CSV strings, invalid events and the prior key are
rejected. See [Project document](../guides/configuration.md#the-project-document).

## Rust adapter selection and layout

The maintained Hub adapter selector consumes captured `ConfigSnapshot` and
membership. It returns a canonical `NativeBinding` and origin. Kiro uses the
current V3 engine and Markdown agents/hooks; V2 and preview bindings are absent.
Generic configuration precedence remains Command → Environment → Project →
Global → Distribution. Adapter membership selection retains its explicit and
ambient rules rather than changing the Core getter.

Selection and layout metadata grant no ownership or authorization and do not
prove installed vendor execution. See
[native installation identity](hub_native_installations.md) for the maintained
APIs, targets, registry, errors and recovery boundaries.

## Rust installation identity and salt

`crates/graphit-core/src/installation_identity.rs` implements the semantic
consumer for global `unit.id` and `client.secret` under `tsk-fcf0`. Configuration
inspection does not invoke identity generation; product callers explicitly use
the maintained `load` or `ensure` operations.

`InstallationSettings::resolve(&ConfigSnapshot)` is pure: it validates captured
winning values, represents missing/partial state and returns owned settings
without I/O, generation or persistence. Valid command/environment overrides
win without being copied to disk. `InstallationIdentityStore::new(&InstallationPaths)`
binds only `ConfigStore::global`; `load()` reads persisted global state with
optional ID/salt, creating no config, directory or lock. Neither read path treats
an invalid present value as absence or repairs it.

`InstallationId::new` trims peripheral whitespace and accepts 1–128 ASCII bytes,
alphanumeric edges and alphanumeric/`.`/`_`/`-` inside, preserving case. Custom
IDs need not be ULIDs. Generated IDs are canonical ULIDs with a 48-bit millisecond
timestamp and 80 bits of OS entropy. A clock before the epoch or beyond the
48-bit range fails rather than truncating. This identifier is local attribution
and diagnostics, not trusted Hub identity or user-memory authority; it is not
a validated path component.

The salt is `SecretString`: 32–4,096 UTF-8 bytes, not entirely whitespace.
Valid bytes are preserved without trimming; custom values have no promised
entropy. Generated salts use 32 OS-random bytes encoded as base64url without
padding (43 ASCII characters, 256 bits). Salt supports future event pseudonyms;
it is not a cookie/OAuth/access credential. Empty values, wrong types and invalid
IDs/salts require explicit config correction rather than automatic replacement.
Semantic errors from persisted Global values are sanitized `InvalidData`;
logical Command/Environment/Distribution errors are `InvalidInput`.

`ensure(timeout, &Cancellation)` acquires the existing global config writer,
rereads and validates both fields, generates only missing fields, then publishes
all required insertions in **one** `apply`. Other settings and valid existing
fields are preserved. A complete pair returns without apply, RNG or target
rewrite, including on a read-only target. It still acquires the lock and reads;
this is different from effect-free `load`/pure `resolve`. Partial read-only or
invalid persisted state fails. A complete owned `InstallationIdentity` is
returned only after publication succeeds.

Pinned `getrandom = "=0.4.3"` provides fallible OS entropy with no predictable
fallback. Raw entropy buffers are zeroizing from allocation, including partial
RNG failures. RNG/clock/budget/cancellation/permission/I/O failures do not publish
a partial pair or return generated values that were not persisted. RNG failure
is a sanitized `Other`; timeout/cancellation retain their distinct config-store
errors. OS entropy and filesystem syscalls are not interruptible. The existing
stable `.config.json.lock`, atomic writer, permissions and cleanup limits apply;
no separate identity lock is introduced.

`InstallationSettings` and complete `InstallationIdentity` have private fields,
explicit ID/secret getters, secret-redacted Debug and no public Serialize.
Shared config inspection exposes only secret configured state. This does not
promise memory locking or wiping every allocator/Serde copy. Event hash/HMAC,
domain separation, rotation/shipping and fail-closed publication belong to Hub
integration; this helper provides no hash stub. Cookie and OAuth consumers retain their separate authentication contracts.


## Rust module settings contract

`ModuleSettings::resolve` computes the twelve existing gates from one captured
snapshot. Task, Memory, AST, Knowledge, Hub, Daemon, Sync, Embedding, Hooks and Agent
are enabled by default; Dream and DaemonUi are opt-in. Invalid winning values
fail rather than becoming defaults. The CLI checks recognized module values with
this API before a mutation; `modules.daemon_ui` remains Environment-only.

These switches control orchestration, independently of authorization and native
executable availability. Changing a setting does not grant PROJECT access or
implicitly start an agent. Each consumer owns when it captures or reloads its
view; see [daemon operations](../guides/daemon_operations.md). Existing AST profile
reload remains separate from configuration CRUD.

## The one setting that is not a config key

The global directory is a bootstrap input, supplied by `--global-dir` or the
branded `GLOBAL_DIR` environment variable before configuration is loaded. It
cannot be changed by persisting `global_dir`. The SDK binds each store to that
captured installation or project; it does not rediscover CWD or environment.

### The project's grammars: `ast.queries_dir`

The AST loader consumes the configured queries directory under its existing
project admission. Configuration CRUD persists the setting; it does not load a
parser or infer a profile. See [AST customization](ast_module.md).

### Turning grammars off: `ast.grammars_blacklist` and `ast.grammars_whitelist`

Grammar selection belongs to the maintained AST selector and selects linked
syntax grammars independently of compiler images. `ast.scip.parsers` is an
ordinary layered string of comma-separated `extension=image` items. The winning
complete string replaces lower layers; an explicit empty string clears them.
The consumer trims surrounding item/assignment whitespace and rejects missing
`=`, empty extension/image, empty comma-separated segments, colliding normalized
extensions and invalid image arguments. `ConfigSnapshot.get<String>` consumes the
same value from `GRAPHIT_AST__SCIP__PARSERS`; no second environment decoder,
JSON-object reader or deep merge is used. See [custom compiler images](../guides/ast_extensibility.md#custom-compiler-images-by-extension).
See [AST](ast_module.md) and [ignore files](../guides/ignore_files.md).
