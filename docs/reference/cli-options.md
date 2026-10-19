---
title: Complete CLI option reference
description: Inspected Rust CLI command tree, positional arguments, flags, accepted values and defaults.
content-type: reference
audience: developers
updated: 2026-10-30
---

# Complete CLI option reference

The command tree was checked against the Rust CLI on 2026-10-29; affected Git
entrypoints are updated with the native hook dispatcher. Start with the [CLI guide](../guides/cli_reference.md) for workflows.
Each section records usage, positional arguments, command-specific options and accepted values.
Defaults below are Clap defaults; absent configuration can have an additional consumer default
described in the [configuration reference](configuration-keys.md).

Shared flags are syntactically inherited; the selected operation still enforces its scope.
In particular, setup/config reject unsupported project-version selection. `<PLACEHOLDER>` values
in workflow examples mean actual returned IDs, not literal input.

## Shared options

| Option | Contract |
|---|---|
| `-c, --config <KEY=VALUE>` | Repeatable command-layer override; the last value for a key wins. |
| `--global-dir <DIRECTORY>` | Bootstrap installation selector, before normal configuration. |
| `--project-version <VERSION>` | Select a retained domain version where supported. |
| `--non-interactive` | Disable prompts; authorization and input validation still apply. |
| `--version` | Root-only build version; command-specific artifact-version options take a value. |
| `-h, --help` | Display this installed command without executing its operation. |

## graphit setup

Configure this installation and optionally initialize a maintained project

```text
graphit setup [OPTIONS] --agent <AGENT>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--agent <AGENT>` | Native agent binding; current Kiro uses kiro-v3 |
| `--project-dir <PROJECT_DIR>` | Initialize and maintain this existing project directory |
| `--project-id <PROJECT_ID>` | Initial immutable project ULID; an existing identity must have the same ID |
| `--name <NAME>` | Initial project name; an existing identity and its metadata are preserved |

## graphit sync

Reconcile installed Hub artifacts, native skills/agents, Git/agent hooks and MCP

```text
graphit sync [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--agent <AGENT>` | Native agent; defaults to the effective Graphit agent configuration |
| `--global` | Reconcile global skills/roles and Hub installations without a PROJECT |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--coordinator-source <COORDINATOR_SOURCE>` | Explicit product Coordinator source for development/package verification |

## graphit skills

Reconcile maintained module skills independently

```text
graphit skills [OPTIONS] <COMMAND>
```

Subcommands:

- [`sync`](#graphit-skills-sync)

## graphit config

Inspect or persist the current Rust configuration

```text
graphit config [OPTIONS] <COMMAND>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--global` | Write or inspect only the installation's global configuration |
| `--timeout <TIMEOUT>` | Maximum time to wait for a configuration writer, in seconds [default: 30] |

Subcommands:

- [`set`](#graphit-config-set) — Persist JSON or literal text; supported secrets always remain strings
- [`get`](#graphit-config-get) — Inspect a configured value and its source, with secret redaction
- [`list`](#graphit-config-list) — List configured values and their sources, with secret redaction
- [`unset`](#graphit-config-unset) — Remove a value from the selected Project or Global store

## graphit self-update

Install the latest verified release and resume this installation's active daemon

```text
graphit self-update [OPTIONS]
```

## graphit ui

Open the configured Browser served by the active daemon

```text
graphit ui [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--no-open` | Print the configured Browser URL without launching a desktop application |

## graphit tray

Control this installation from the Linux desktop tray

```text
graphit tray [OPTIONS]
```

## graphit projects

Update the running daemon's maintained PROJECT inventory

```text
graphit projects [OPTIONS] <COMMAND>
```

Subcommands:

- [`maintain-checkout`](#graphit-projects-maintain-checkout) — Observe this checkout/version, keep sibling jobs and publish the whole inventory
- [`retire-checkout`](#graphit-projects-retire-checkout) — Stop this checkout's jobs, retain its versions, and publish the inventory

## graphit now

Follow the live Broker Now view for an explicitly selected project

```text
graphit now [OPTIONS] --project <PROJECT> --provider <PROVIDER>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--project <PROJECT>` | Canonical project ID; authorization remains with the daemon and Broker |
| `--provider <PROVIDER>` | Explicit configured Broker provider |
| `--version <VERSION>` | Filter the Broker view by its literal project version |
| `--instance <INSTANCE>` | Filter the Broker view by its instance ID |

## graphit knowledge

Search, query or export Knowledge through the active service

```text
graphit knowledge [OPTIONS] <COMMAND>
```

Subcommands:

- [`ask`](#graphit-knowledge-ask) — Return the final answer from the service's captured provider binding
- [`search`](#graphit-knowledge-search) — Search an authorized PROJECT or an installed Knowledge artifact
- [`source`](#graphit-knowledge-source) — Read complete native Knowledge Markdown, or an explicit line/pattern slice
- [`log`](#graphit-knowledge-log) — Read committed document synchronization history in the selected PROJECT
- [`export`](#graphit-knowledge-export) — Freeze the current Knowledge snapshots and export an installable native package

## graphit memory

Maintain the authoritative indexes of an authorized Memory corpus

```text
graphit memory [OPTIONS] <COMMAND>
```

Subcommands:

- [`index`](#graphit-memory-index) — Ensure or explicitly refresh lexical indexes and configured embeddings

## graphit provider

Manage authentication providers in the private installation

```text
graphit provider [OPTIONS] <COMMAND>
```

Subcommands:

- [`list`](#graphit-provider-list)
- [`show`](#graphit-provider-show)
- [`add`](#graphit-provider-add)
- [`update`](#graphit-provider-update)
- [`remove`](#graphit-provider-remove)

## graphit account

List, inspect or select local profiles

```text
graphit account [OPTIONS] <COMMAND>
```

Subcommands:

- [`list`](#graphit-account-list)
- [`show`](#graphit-account-show)
- [`use`](#graphit-account-use)

## graphit login

Log in through a configured Local or Broker provider

```text
graphit login [OPTIONS] --profile <PROFILE> --provider <PROVIDER>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--device` | Authorize with a public user code instead of a loopback callback |
| `--profile <PROFILE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--provider <PROVIDER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--username <USERNAME>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--organization <ORGANIZATION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--team <TEAMS>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--anonymous` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--s3-disabled` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--mcp-key <MCP_KEY>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--broker-key <BROKER_KEY>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-api-key <EMBEDDING_API_KEY>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--rerank-api-key <RERANK_API_KEY>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--s3-access-key <S3_ACCESS_KEY>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--s3-secret-key <S3_SECRET_KEY>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--s3-session-token <S3_SESSION_TOKEN>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--s3-expires-at <S3_EXPIRES_AT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--aws-profile <AWS_PROFILE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--expected-revision <EXPECTED_REVISION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit logout

Remove a stored profile

```text
graphit logout [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--profile <PROFILE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--expected-revision <EXPECTED_REVISION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit refresh

Refresh the active native Broker profile

```text
graphit refresh [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--expected-revision <EXPECTED_REVISION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit package

Export, inspect or import native packages

```text
graphit package [OPTIONS] <COMMAND>
```

Subcommands:

- [`install`](#graphit-package-install) — Install a native corpus in the installation's global cache
- [`export`](#graphit-package-export) — Package a completed native payload directory
- [`inspect`](#graphit-package-inspect) — Validate physical package structure and every payload body
- [`import`](#graphit-package-import) — Publish a validated native tree into a missing directory

## graphit roles

Install or synchronize three managed roles on the active daemon

```text
graphit roles [OPTIONS] <COMMAND>
```

Subcommands:

- [`install`](#graphit-roles-install)
- [`sync`](#graphit-roles-sync)
- [`run`](#graphit-roles-run)

## graphit hooks

Install PROJECT context hooks on the active daemon

```text
graphit hooks [OPTIONS] <COMMAND>
```

Subcommands:

- [`install`](#graphit-hooks-install) — [alias: sync]
- [`remove`](#graphit-hooks-remove) — Remove owned hooks and MCP, preserving Kimi references from other projects

## graphit hub

Publish captured native SDK artifacts to an authorized Hub provider

```text
graphit hub [OPTIONS] <COMMAND>
```

Subcommands:

- [`authored`](#graphit-hub-authored)
- [`install`](#graphit-hub-install)
- [`install-local`](#graphit-hub-install-local)
- [`update`](#graphit-hub-update) — A captured installation target. An omitted selector updates its remote entries
- [`sync`](#graphit-hub-sync) — Reapply all installed artifacts from their recorded sources and versions
- [`remove`](#graphit-hub-remove)
- [`recover`](#graphit-hub-recover)
- [`forget`](#graphit-hub-forget)
- [`content`](#graphit-hub-content)
- [`list`](#graphit-hub-list)
- [`projects`](#graphit-hub-projects)
- [`search`](#graphit-hub-search) — Available manifests of an explicit remote PROJECT; all filters are literal
- [`publish`](#graphit-hub-publish)
- [`publish-native`](#graphit-hub-publish-native)
- [`unpublish`](#graphit-hub-unpublish) — Retract remote catalog references while preserving payloads and local installations
- [`install-native`](#graphit-hub-install-native)

## graphit coordinator

Publish the product Coordinator and PROJECT context hooks

```text
graphit coordinator [OPTIONS] <COMMAND>
```

Subcommands:

- [`install`](#graphit-coordinator-install) — [alias: sync]
- [`remove`](#graphit-coordinator-remove) — Withdraw the owned Coordinator, its hooks and MCP selection

## graphit models

Install or select local embedding models

```text
graphit models [OPTIONS] <COMMAND>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--timeout <TIMEOUT>` | Maximum operation duration in seconds [default: 3600] |

Subcommands:

- [`install`](#graphit-models-install) — Install a pinned GGUF model, preserving its existing task selection
- [`select`](#graphit-models-select) — Select a captured installed embedding or rerank manifest

## graphit task

Read and mutate native Task/Session records through the active Source service

```text
graphit task [OPTIONS] <COMMAND>
```

Subcommands:

- [`get-task`](#graphit-task-get-task) — Read current Task state and explicitly requested snapshot-bound history/comment pages
- [`catalogue`](#graphit-task-catalogue) — Read a cursor-pinned Task catalogue, including explicit dates and dependencies
- [`search`](#graphit-task-search) — Search current Task specifications and comments using textual, semantic or hybrid retrieval
- [`query`](#graphit-task-query) — Read a finite projected Task or Session page from one native pin
- [`schema`](#graphit-task-schema) — Read the native Task query tables and permitted column types
- [`get-session`](#graphit-task-get-session) — Read a Session and its immutable native history
- [`session-heads`](#graphit-task-session-heads) — Read native Session heads
- [`create-task`](#graphit-task-create-task) — Create a native Task with acceptance and test checks, and explicit optional calendar dates
- [`create-session`](#graphit-task-create-session) — Create a native Session
- [`claim-task`](#graphit-task-claim-task) — Claim a Task; return the opaque participant fence only to this caller
- [`claim-session`](#graphit-task-claim-session) — Claim a Session; return its opaque participant fence
- [`heartbeat-task`](#graphit-task-heartbeat-task) — Renew a Task owner's actual lease and revision
- [`heartbeat-session`](#graphit-task-heartbeat-session) — Renew a Session owner's actual lease and revision
- [`add-task-comment`](#graphit-task-add-task-comment) — Add a literal Markdown comment with the current participant fence and an idempotency key
- [`progress-task`](#graphit-task-progress-task) — Record the current Task owner's progress
- [`checkpoint-session`](#graphit-task-checkpoint-session) — Record the current Session owner's checkpoint
- [`release-task`](#graphit-task-release-task) — Release a Task using its current participant fence
- [`release-session`](#graphit-task-release-session) — Release a Session using its current participant fence
- [`revise-task`](#graphit-task-revise-task) — Revise Task specification; omitted fields stay, schedule null clears
- [`revise-session`](#graphit-task-revise-session) — Revise the coordinated Session specification; omitted fields stay
- [`verify-task-check`](#graphit-task-verify-task-check) — Verify a current Task check with evidence
- [`supersede-task-check`](#graphit-task-supersede-task-check) — Supersede a Task check, optionally creating a replacement
- [`update-task-links`](#graphit-task-update-task-links) — Replace Task parent or dependency links with recorded reason
- [`set-task-flag`](#graphit-task-set-task-flag) — Set or clear the native Task flag with recorded reason
- [`set-task-commit`](#graphit-task-set-task-commit) — Associate or clear the full Git commit ID without changing Task status/checks
- [`complete-task`](#graphit-task-complete-task) — Complete a Task after its actual gates pass
- [`cancel-task`](#graphit-task-cancel-task) — Cancel an eligible Task with recorded reason and live fence when owned
- [`complete-session`](#graphit-task-complete-session) — Complete a Session after its native Task gates pass

## graphit git

Configure Git hooks and bind native Task/Memory state to commits and merges

```text
graphit git [OPTIONS] <COMMAND>
```

Subcommands:

- [`install`](#graphit-git-install) — Snapshot the current state, then install the native Git notifications
- [`sync`](#graphit-git-sync) — Reconcile configured commands and Task/Memory events without taking a new snapshot
- [`remove`](#graphit-git-remove) — Remove only this brand's native Git hook registrations
- [`snapshot`](#graphit-git-snapshot) — Bind the current record state to HEAD, or retry a failed commit hook
- [`restore`](#graphit-git-restore) — Retry restoring the record snapshot for the checked-out commit
- [`hook`](#graphit-git-hook) — Dispatch current configured commands and Task/Memory notifications for a Git event

## graphit ast

Inspect source from the authenticated local service

```text
graphit ast [OPTIONS] <COMMAND>
```

Subcommands:

- [`source`](#graphit-ast-source) — Read indexed source or the physical file in the retained project version
- [`export`](#graphit-ast-export) — Export a published PROJECT graph or an installed AST context
- [`imports`](#graphit-ast-imports) — List exact installed Native AST contexts
- [`search`](#graphit-ast-search) — Search the current PROJECT or an exact installed AST context
- [`query`](#graphit-ast-query) — Query the current PROJECT or an explicitly selected installed graph
- [`schema`](#graphit-ast-schema) — Show graph labels, columns and relationships
- [`sample`](#graphit-ast-sample) — Show a bounded sample of graph nodes and edges
- [`neighborhood`](#graphit-ast-neighborhood) — Page over the relationships of one node

## graphit mcp

Serve the MCP client transport over standard input/output

```text
graphit mcp [OPTIONS] --stdio
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--stdio` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--role` | Forward only the transient credential of an owned managed-role run |
| `--live-user` | Use only the process-bound Web credentials of an owned Live turn |
| `--project-dir <PROJECT_DIR>` | Default PROJECT locator for the authenticated Root Source consumer |

## graphit daemon

Run or manage the authenticated local Source service

```text
graphit daemon [OPTIONS] [COMMAND]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--managed` | Run the registered daemon from the installation's maintained inventory |

Subcommands:

- [`foreground`](#graphit-daemon-foreground) — Run until SIGINT or SIGTERM, then join jobs and remove owned discovery/key
- [`start`](#graphit-daemon-start) — Start the user's daemon for this installation
- [`stop`](#graphit-daemon-stop) — Stop the user manager before draining the daemon's retained owners
- [`restart`](#graphit-daemon-restart) — Restart the registered service with its native user manager
- [`status`](#graphit-daemon-status) — Observe the authenticated daemon or its native manager
- [`install`](#graphit-daemon-install) — Register the service; login startup is enabled only with --login
- [`login`](#graphit-daemon-login) — Change startup at user login explicitly
- [`uninstall`](#graphit-daemon-uninstall) — Stop and remove the service and its login registration

## graphit skills sync

```text
graphit skills sync [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--destination <DESTINATION>` | Physical destination. Global never captures the current directory as a PROJECT [default: project] [possible values: project, global] |
| `--source-project <SOURCE_PROJECT>` | Explicit PROJECT origin for raw overrides/installed bundles; independently admitted |
| `--agent <AGENT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--role <ROLE>` | [possible values: scout, tracker, scribe] |
| `--rule <RULE>` | A qualified raw bundle for one role: ROLE=ID@VERSION |
| `--skill-alias <SKILL_ALIAS>` | Effective configured skill name: MODULE=NAME |
| `--skill-bundle <SKILL_BUNDLE>` | Installed instruction bundle: MODULE=ID@VERSION; remote origins require current access |
| `--skill-identity-provider <SKILL_IDENTITY_PROVIDER>` | Explicit Native identity for a selected standalone bundle: MODULE=PROVIDER |
| `--module-skill <MODULE_SKILL>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit config set

Persist JSON or literal text; CSV settings and secrets remain strings

```text
graphit config set [OPTIONS] <KEY> <VALUE>
```

| Argument | Contract |
|---|---|
| `<KEY>` | Configuration key, subject to its Project/Global scope policy. |
| `<VALUE>` | Value for this setting; CSV for scalar lists, JSON for typed structures, literal text for secrets. |

| Option | Contract |
|---|---|
| `--global` | Write or inspect only the installation's global configuration |
| `--timeout <TIMEOUT>` | Maximum time to wait for a configuration writer, in seconds [default: 30] |

`githooks` is a JSON object, never a CSV value. Project mutations preserve the
identity and root metadata in `.graphit/project.json`; `githooks` is written at
its root and other configuration keys inside `settings`. `--global` retains the
installation's `config.json`, without project identity.

## graphit config get

Inspect a configured value and its source, with secret redaction

```text
graphit config get [OPTIONS] <KEY>
```

Positional arguments:

```text
<KEY>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--global` | Write or inspect only the installation's global configuration |
| `--timeout <TIMEOUT>` | Maximum time to wait for a configuration writer, in seconds [default: 30] |

## graphit config list

List configured values and their sources, with secret redaction

```text
graphit config list [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--global` | Write or inspect only the installation's global configuration |
| `--timeout <TIMEOUT>` | Maximum time to wait for a configuration writer, in seconds [default: 30] |

## graphit config unset

Remove a value from the selected Project or Global store

```text
graphit config unset [OPTIONS] <KEY>
```

Positional arguments:

```text
<KEY>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--global` | Write or inspect only the installation's global configuration |
| `--timeout <TIMEOUT>` | Maximum time to wait for a configuration writer, in seconds [default: 30] |

## graphit projects maintain-checkout

Observe this checkout/version, keep sibling jobs and publish the whole inventory

```text
graphit projects maintain-checkout [OPTIONS] --project-id <PROJECT_ID> --project-dir <PROJECT_DIR>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--project-id <PROJECT_ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | [default: ""] |
| `--detached-base <DETACHED_BASE>` | [default: ""] |
| `--select` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit projects retire-checkout

Stop this checkout's jobs, retain its versions, and publish the inventory

```text
graphit projects retire-checkout [OPTIONS] --project-id <PROJECT_ID> --project-dir <PROJECT_DIR>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--project-id <PROJECT_ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit knowledge ask

Return the final answer from the service's captured provider binding

```text
graphit knowledge ask [OPTIONS] --query <QUERY> --model <MODEL>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--query <QUERY>` | Literal question, without trimming or shell interpretation |
| `--slug <SELECTED_SLUGS>` | Explicit Knowledge page slug; repeat to select more pages |
| `--retrieval <RETRIEVAL>` | Retrieve pages from the question instead of selecting explicit slugs [possible values: fts, semantic, hybrid] |
| `--top-k <TOP_K>` | Number of ranked pages to provide; requires automatic retrieval |
| `--metric <METRIC>` | Vector metric for semantic or hybrid retrieval; defaults to cosine [possible values: cosine, l2, dot] |
| `--model <MODEL>` | Requested model identifier passed literally to the captured completion binding |
| `--project-dir <PROJECT_DIR>` | Select a retained local project rather than the current project |

## graphit knowledge search

Search an authorized PROJECT or an installed Knowledge artifact

```text
graphit knowledge search [OPTIONS] <QUERY>
```

| Argument | Contract |
|---|---|
| `<QUERY>` | Query the selected Knowledge corpus |

| Option | Contract |
|---|---|
| `--mode <MODE>` | [default: fts] [possible values: fts, semantic, hybrid] |
| `--metric <METRIC>` | [possible values: cosine, l2, dot] |
| `--top-k <TOP_K>` | Limit ranked results; zero retains the service's unbounded-result semantics |
| `--page-size <PAGE_SIZE>` |  |
| `--preview` |  |
| `--rerank[=<RERANK>]` | Override configured reranking; --rerank=false always disables it [possible values: true, false] |
| `--cursor <CURSOR>` | Continue with the exact opaque JSON cursor returned by the service |
| `--cursor-file <CURSOR_FILE>` | Read the unchanged cursor object from a UTF-8 JSON file |
| `--project-dir <PROJECT_DIR>` |  |
| `--artifact-id <ARTIFACT_ID>` | Select an installed Knowledge artifact independently of PROJECT |
| `--artifact-version <ARTIFACT_VERSION>` |  |
| `--context-alias <CONTEXT_ALIAS>` |  |

## graphit knowledge source

Read complete native Knowledge Markdown, or an explicit line/pattern slice

```text
graphit knowledge source [OPTIONS] <SLUG>
```

Positional arguments:

```text
<SLUG>  Exact Knowledge page slug; this is not a filesystem path
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--head <HEAD>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--tail <TAIL>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--start <START_LINE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--end <END_LINE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--pattern <PATTERN>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--regex` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--before <BEFORE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--after <AFTER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--line-numbers` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--artifact-id <ARTIFACT_ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--artifact-version <ARTIFACT_VERSION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--context-alias <CONTEXT_ALIAS>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit knowledge log

Read committed document synchronization history in the selected PROJECT

```text
graphit knowledge log [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--limit <LIMIT>` | Maximum synchronization records in this page [default: 10] |
| `--cursor <CURSOR>` | Continue with the opaque cursor returned by the service |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit knowledge export

Freeze the current Knowledge snapshots and export an installable native package

```text
graphit knowledge export [OPTIONS] --output <OUTPUT>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--format <FORMAT>` | Export an installable native package or a Markdown, OKF or Obsidian directory [default: native] [possible values: native, markdown, okf, obsidian] |
| `--output <OUTPUT>` | Destination package file or text directory, relative to this working directory |
| `--project-dir <PROJECT_DIR>` | Select a retained local project; otherwise use the current selection |
| `--artifact-id <ARTIFACT_ID>` | Select an installed Knowledge artifact independently of PROJECT |
| `--artifact-version <ARTIFACT_VERSION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--context-alias <CONTEXT_ALIAS>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit memory index

Ensure or explicitly refresh lexical indexes and configured embeddings

```text
graphit memory index [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--scope <SCOPE>` | Select the authorized PROJECT or the caller's own verified USER corpus [default: project] [possible values: project, user] |
| `--refresh` | Replace the existing lexical index explicitly; never triggered by a read |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit provider list

```text
graphit provider list [OPTIONS]
```

## graphit provider show

```text
graphit provider show [OPTIONS] <NAME>
```

Positional arguments:

```text
<NAME>
```

## graphit provider add

```text
graphit provider add [OPTIONS] --type <KIND> <NAME>
```

Positional arguments:

```text
<NAME>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--type <KIND>` | [possible values: local, broker] |
| `--broker-endpoint <BROKER_ENDPOINT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--mcp-resource <MCP_RESOURCE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--broker-allow-anonymous` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--broker-deny-anonymous` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--clear-broker` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--allow-aws-credential-chain` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--deny-aws-credential-chain` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-mode <EMBEDDING_MODE>` | [possible values: local, direct, broker, disabled] |
| `--embedding-device <EMBEDDING_DEVICE>` | [possible values: auto, cpu, cuda, metal] |
| `--embedding-device-id <EMBEDDING_DEVICE_ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-protocol <EMBEDDING_PROTOCOL>` | [possible values: openai, openai-compatible, cohere, cohere-v2, voyage, voyage-v1, google-gemini-1, google-gemini-2, jina, jina-v1] |
| `--embedding-endpoint <EMBEDDING_ENDPOINT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-model <EMBEDDING_MODEL>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-model-revision <EMBEDDING_MODEL_REVISION>` | Explicit pinned model or deployment revision supplied by the provider owner |
| `--embedding-recipe <EMBEDDING_RECIPE>` | Explicit reproducible embedding input/output recipe identifier |
| `--embedding-dimensions <EMBEDDING_DIMENSIONS>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-request-dimensions <EMBEDDING_REQUEST_DIMENSIONS>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-clear-request-dimensions` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-batch-endpoint <EMBEDDING_BATCH_ENDPOINT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-clear-batch-endpoint` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--rerank-mode <RERANK_MODE>` | [possible values: local, direct, broker, disabled] |
| `--rerank-device <RERANK_DEVICE>` | [possible values: auto, cpu, cuda, metal] |
| `--rerank-device-id <RERANK_DEVICE_ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--rerank-protocol <RERANK_PROTOCOL>` | [possible values: openai, openai-compatible, cohere, cohere-v2, voyage, voyage-v1, google-gemini-1, google-gemini-2, jina, jina-v1] |
| `--rerank-endpoint <RERANK_ENDPOINT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--rerank-model <RERANK_MODEL>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--rerank-model-revision <RERANK_MODEL_REVISION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--rerank-recipe <RERANK_RECIPE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--rerank-dimensions <RERANK_DIMENSIONS>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--s3-bucket <S3_BUCKET>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--s3-region <S3_REGION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--s3-endpoint <S3_ENDPOINT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--s3-prefix <S3_PREFIX>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--s3-credential-source <S3_CREDENTIAL_SOURCE>` | [possible values: login, aws-chain, broker] |
| `--clear-s3` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--expected-revision <EXPECTED_REVISION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit provider update

```text
graphit provider update [OPTIONS] <NAME>
```

Positional arguments:

```text
<NAME>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--type <KIND>` | [possible values: local, broker] |
| `--broker-endpoint <BROKER_ENDPOINT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--mcp-resource <MCP_RESOURCE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--broker-allow-anonymous` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--broker-deny-anonymous` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--clear-broker` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--allow-aws-credential-chain` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--deny-aws-credential-chain` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-mode <EMBEDDING_MODE>` | [possible values: local, direct, broker, disabled] |
| `--embedding-device <EMBEDDING_DEVICE>` | [possible values: auto, cpu, cuda, metal] |
| `--embedding-device-id <EMBEDDING_DEVICE_ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-protocol <EMBEDDING_PROTOCOL>` | [possible values: openai, openai-compatible, cohere, cohere-v2, voyage, voyage-v1, google-gemini-1, google-gemini-2, jina, jina-v1] |
| `--embedding-endpoint <EMBEDDING_ENDPOINT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-model <EMBEDDING_MODEL>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-model-revision <EMBEDDING_MODEL_REVISION>` | Explicit pinned model or deployment revision supplied by the provider owner |
| `--embedding-recipe <EMBEDDING_RECIPE>` | Explicit reproducible embedding input/output recipe identifier |
| `--embedding-dimensions <EMBEDDING_DIMENSIONS>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-request-dimensions <EMBEDDING_REQUEST_DIMENSIONS>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-clear-request-dimensions` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-batch-endpoint <EMBEDDING_BATCH_ENDPOINT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--embedding-clear-batch-endpoint` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--rerank-mode <RERANK_MODE>` | [possible values: local, direct, broker, disabled] |
| `--rerank-device <RERANK_DEVICE>` | [possible values: auto, cpu, cuda, metal] |
| `--rerank-device-id <RERANK_DEVICE_ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--rerank-protocol <RERANK_PROTOCOL>` | [possible values: openai, openai-compatible, cohere, cohere-v2, voyage, voyage-v1, google-gemini-1, google-gemini-2, jina, jina-v1] |
| `--rerank-endpoint <RERANK_ENDPOINT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--rerank-model <RERANK_MODEL>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--rerank-model-revision <RERANK_MODEL_REVISION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--rerank-recipe <RERANK_RECIPE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--rerank-dimensions <RERANK_DIMENSIONS>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--s3-bucket <S3_BUCKET>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--s3-region <S3_REGION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--s3-endpoint <S3_ENDPOINT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--s3-prefix <S3_PREFIX>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--s3-credential-source <S3_CREDENTIAL_SOURCE>` | [possible values: login, aws-chain, broker] |
| `--clear-s3` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--expected-revision <EXPECTED_REVISION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit provider remove

```text
graphit provider remove [OPTIONS] --yes <NAME>
```

Positional arguments:

```text
<NAME>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--cascade` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--yes` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--expected-revision <EXPECTED_REVISION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit account list

```text
graphit account list [OPTIONS]
```

## graphit account show

```text
graphit account show [OPTIONS] [PROFILE]
```

Positional arguments:

```text
[PROFILE]
```

## graphit account use

```text
graphit account use [OPTIONS] <PROFILE>
```

Positional arguments:

```text
<PROFILE>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--expected-revision <EXPECTED_REVISION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit package install

Install a native corpus in the installation's global cache

```text
graphit package install [OPTIONS] --input <INPUT> --producer <PRODUCER> --id <ID> --version <VERSION>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--input <INPUT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--producer <PRODUCER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--kind <KIND>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit package export

Package a completed native payload directory

```text
graphit package export [OPTIONS] --kind <KIND> --source <SOURCE> --output <OUTPUT>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--kind <KIND>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--source <SOURCE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--output <OUTPUT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit package inspect

Validate physical package structure and every payload body

```text
graphit package inspect [OPTIONS] --input <INPUT>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--input <INPUT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--kind <KIND>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit package import

Publish a validated native tree into a missing directory

```text
graphit package import [OPTIONS] --input <INPUT> --output <OUTPUT>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--input <INPUT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--output <OUTPUT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--kind <KIND>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit roles install

```text
graphit roles install [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--destination <DESTINATION>` | Physical destination. Global never captures the current directory as a PROJECT [default: project] [possible values: project, global] |
| `--source-project <SOURCE_PROJECT>` | Explicit PROJECT origin for raw overrides/installed bundles; independently admitted |
| `--agent <AGENT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--role <ROLE>` | [possible values: scout, tracker, scribe] |
| `--rule <RULE>` | A qualified raw bundle for one role: ROLE=ID@VERSION |
| `--skill-alias <SKILL_ALIAS>` | Effective configured skill name: MODULE=NAME |
| `--skill-bundle <SKILL_BUNDLE>` | Installed instruction bundle: MODULE=ID@VERSION; remote origins require current access |
| `--skill-identity-provider <SKILL_IDENTITY_PROVIDER>` | Explicit Native identity for a selected standalone bundle: MODULE=PROVIDER |
| `--module-skill <MODULE_SKILL>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit roles sync

```text
graphit roles sync [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--destination <DESTINATION>` | Physical destination. Global never captures the current directory as a PROJECT [default: project] [possible values: project, global] |
| `--source-project <SOURCE_PROJECT>` | Explicit PROJECT origin for raw overrides/installed bundles; independently admitted |
| `--agent <AGENT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--role <ROLE>` | [possible values: scout, tracker, scribe] |
| `--rule <RULE>` | A qualified raw bundle for one role: ROLE=ID@VERSION |
| `--skill-alias <SKILL_ALIAS>` | Effective configured skill name: MODULE=NAME |
| `--skill-bundle <SKILL_BUNDLE>` | Installed instruction bundle: MODULE=ID@VERSION; remote origins require current access |
| `--skill-identity-provider <SKILL_IDENTITY_PROVIDER>` | Explicit Native identity for a selected standalone bundle: MODULE=PROVIDER |
| `--module-skill <MODULE_SKILL>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit roles run

```text
graphit roles run [OPTIONS] --prompt <PROMPT>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--destination <DESTINATION>` | Physical destination. Global never captures the current directory as a PROJECT [default: project] [possible values: project, global] |
| `--source-project <SOURCE_PROJECT>` | Explicit PROJECT origin for raw overrides/installed bundles; independently admitted |
| `--agent <AGENT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--role <ROLE>` | [possible values: scout, tracker, scribe] |
| `--rule <RULE>` | A qualified raw bundle for one role: ROLE=ID@VERSION |
| `--skill-alias <SKILL_ALIAS>` | Effective configured skill name: MODULE=NAME |
| `--skill-bundle <SKILL_BUNDLE>` | Installed instruction bundle: MODULE=ID@VERSION; remote origins require current access |
| `--skill-identity-provider <SKILL_IDENTITY_PROVIDER>` | Explicit Native identity for a selected standalone bundle: MODULE=PROVIDER |
| `--module-skill <MODULE_SKILL>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--model <MODEL>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--prompt <PROMPT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit hooks install

```text
graphit hooks install [OPTIONS] --binding <BINDING>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--binding <BINDING>` | Native agent schema selected explicitly |
| `--profile <PROFILE>` | Existing Coordinator Markdown filename; required only for Kiro |

## graphit hooks remove

Remove owned hooks and MCP, preserving Kimi references from other projects

```text
graphit hooks remove [OPTIONS] --binding <BINDING>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--binding <BINDING>` | Native agent schema selected explicitly |
| `--profile <PROFILE>` | Existing Coordinator Markdown filename; required only for Kiro |

## graphit hub authored

```text
graphit hub authored [OPTIONS] <COMMAND>
```

Subcommands:

- [`list`](#graphit-hub-authored-list) — Enumerate authored resource identities in the selected PROJECT/version
- [`review`](#graphit-hub-authored-review) — Review files in one explicitly selected authored resource tree
- [`content`](#graphit-hub-authored-content) — Read a text file relative to one explicitly selected authored resource tree

## graphit hub install

```text
graphit hub install [OPTIONS] --provider <PROVIDER> --producer <PRODUCER> --kind <KIND> --id <ID>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--transport <TRANSPORT>` | [default: broker] [possible values: broker, standalone] |
| `--provider <PROVIDER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--profile <PROFILE>` | Explicit standalone transport profile. Omit only for unsigned standalone S3 |
| `--identity-provider <IDENTITY_PROVIDER>` | Explicit Native identity for standalone ACLs; does not select S3 credentials |
| `--producer <PRODUCER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--kind <KIND>` | [possible values: skill, command, workflow, agent, rule, mcp, language, power] |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | [default: latest] |
| `--exact` | Treat a numeric version as a literal rather than a stable release prefix |
| `--global` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--binding <BINDING>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--alias <ALIAS>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--entry-name <ENTRY_NAME>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--mode <MODE>` | PROJECT mode. Rendered agents, skills, commands and workflows require Copy. Original sources may use Link. Omit on update to retain its physical mode [possible values: copy, link] |

## graphit hub install-local

```text
graphit hub install-local [OPTIONS] --source-dir <SOURCE_DIR> --producer <PRODUCER> --kind <KIND> --id <ID> --version <VERSION> --project-dir <PROJECT_DIR>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--source-dir <SOURCE_DIR>` | Absolute artifact directory; its canonical root is captured before publication |
| `--producer <PRODUCER>` | Provenance only; this does not grant access to the consumer PROJECT |
| `--kind <KIND>` | [possible values: agent, skill, command, workflow, rule, mcp, language, power] |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | Literal artifact version, independent of the selected PROJECT version |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--binding <BINDING>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--alias <ALIAS>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--entry-name <ENTRY_NAME>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--mode <MODE>` | Omit for Copy on a new installation or to retain the current physical mode [possible values: copy, link] |

## graphit hub update

A captured installation target. An omitted selector updates its remote entries

```text
graphit hub update [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--global` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--binding <BINDING>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--identity-provider <IDENTITY_PROVIDER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--transport <TRANSPORT>` | [possible values: broker, standalone] |
| `--provider <PROVIDER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--profile <PROFILE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--producer <PRODUCER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--kind <KIND>` | [possible values: skill, command, workflow, agent, rule, mcp, language, power] |
| `--version <VERSION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--exact` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--alias <ALIAS>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--entry-name <ENTRY_NAME>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--mode <MODE>` | [possible values: copy, link] |

## graphit hub sync

Reapply all installed artifacts from their recorded sources and versions

```text
graphit hub sync [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--global` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--binding <BINDING>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--identity-provider <IDENTITY_PROVIDER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit hub remove

```text
graphit hub remove [OPTIONS] --kind <KIND> --id <ID>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--kind <KIND>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--scope <SCOPE>` | [default: project] [possible values: project, global-cache, global-native] |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | Optional exact installed artifact version; never a release selector |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--binding <BINDING>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit hub recover

```text
graphit hub recover [OPTIONS] --kind <KIND> --id <ID>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--kind <KIND>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--scope <SCOPE>` | [default: project] [possible values: project, global-cache, global-native] |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | Optional exact installed artifact version; never a release selector |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--binding <BINDING>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit hub forget

```text
graphit hub forget [OPTIONS] --kind <KIND> --id <ID>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--kind <KIND>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--scope <SCOPE>` | [default: project] [possible values: project, global-cache, global-native] |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | Optional exact installed artifact version; never a release selector |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--binding <BINDING>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit hub content

```text
graphit hub content [OPTIONS] <COMMAND>
```

Subcommands:

- [`installed`](#graphit-hub-content-installed)
- [`remote`](#graphit-hub-content-remote)

## graphit hub list

```text
graphit hub list [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--global` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--identity-provider <IDENTITY_PROVIDER>` | Explicit Native identity used only for stored standalone origins |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--kind <KIND>` | [possible values: skill, command, workflow, agent, rule, mcp, language, power] |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--binding <BINDING>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--offset <OFFSET>` | [default: 0] |
| `--limit <LIMIT>` | [default: 50] |

## graphit hub projects

```text
graphit hub projects [OPTIONS] --provider <PROVIDER>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--transport <TRANSPORT>` | [default: broker] [possible values: broker, standalone] |
| `--provider <PROVIDER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--profile <PROFILE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--identity-provider <IDENTITY_PROVIDER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--name-prefix <NAME_PREFIX>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--limit <LIMIT>` | [default: 50] |
| `--cursor <CURSOR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit hub search

Available manifests of an explicit remote PROJECT; all filters are literal

```text
graphit hub search [OPTIONS] --provider <PROVIDER> --producer <PRODUCER>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--transport <TRANSPORT>` | [default: broker] [possible values: broker, standalone] |
| `--provider <PROVIDER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--profile <PROFILE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--identity-provider <IDENTITY_PROVIDER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--producer <PRODUCER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--kind <KIND>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--limit <LIMIT>` | [default: 50] |
| `--cursor <CURSOR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit hub publish



```text
graphit hub publish [OPTIONS] --kind <KIND> --provider <PROVIDER> --id <ID> --version <VERSION> --name <NAME>
```

| Option | Contract |
|---|---|
| `--kind <KIND>` | [possible values: skill, command, workflow, agent, rule, mcp, language, power] |
| `--provider <PROVIDER>` | Configured Hub provider used to authorize the publication. |
| `--id <ID>` | Full Hub artifact ID. |
| `--version <VERSION>` |  |
| `--name <NAME>` | Display name of the published artifact. |
| `--description <DESCRIPTION>` | Optional authored description. |
| `--tag <TAG>` | Artifact tag; repeat the option for multiple tags. |
| `--author <AUTHOR>` | Optional metadata attribution; does not select the authenticated actor. |
| `--dependency <KIND> <ID> <SELECTOR>` | Declare one dependency; repeat the three values for each dependency |
| `--clear-dependencies` | Publish an explicitly empty dependency list; omission preserves declarations |
| `--replace-sha256 <REPLACE_SHA256>` | Current archive SHA256 required to replace content at this concrete version |
| `--set-latest` | Make this concrete version the catalog's latest pointer |
| `--project-dir <PROJECT_DIR>` |  |

## graphit hub publish-native

```text
graphit hub publish-native [OPTIONS] --kind <KIND> --provider <PROVIDER> --id <ID> --version <VERSION> --name <NAME>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--kind <KIND>` | [possible values: ast, knowledge] |
| `--provider <PROVIDER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--name <NAME>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--description <DESCRIPTION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--tag <TAG>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--author <AUTHOR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit hub unpublish

Retract remote catalog references while preserving payloads and local installations

```text
graphit hub unpublish [OPTIONS] --provider <PROVIDER> --producer <PRODUCER> --kind <KIND> --id <ID>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--provider <PROVIDER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--producer <PRODUCER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--kind <KIND>` | [possible values: agent, ast, command, knowledge, language, mcp, power, rule, skill, workflow] |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | Literal artifact version; latest and release-prefix selection are not accepted |
| `--all-versions` | Retract every version observed in the current artifact catalog |
| `--expected-sha256 <EXPECTED_SHA256>` | Require this current version hash before changing the catalog |

## graphit hub install-native

```text
graphit hub install-native [OPTIONS] --provider <PROVIDER> --producer <PRODUCER> --kind <KIND> --id <ID> --version <VERSION>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--provider <PROVIDER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--producer <PRODUCER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--kind <KIND>` | [possible values: ast, knowledge] |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit coordinator install

```text
graphit coordinator install [OPTIONS] --binding <BINDING>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--binding <BINDING>` | Native host schema for this publication |
| `--source-dir <SOURCE_DIR>` | Override the bundled versioned Graphit/Coordinator artifact directory |

## graphit coordinator remove

Withdraw the owned Coordinator, its hooks and MCP selection

```text
graphit coordinator remove [OPTIONS] --binding <BINDING>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--binding <BINDING>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit models install

Install a pinned GGUF model, preserving its existing task selection

```text
graphit models install [OPTIONS] <MODEL>
```

Positional arguments:

```text
<MODEL>  [possible values: embeddinggemma2, bge-reranker-v2-m3]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--file <FILE>` | Install a previously prepared, digest-verified GGUF instead of downloading it |
| `--timeout <TIMEOUT>` | Maximum operation duration in seconds [default: 3600] |

## graphit models select

Select a captured installed embedding or rerank manifest

```text
graphit models select [OPTIONS] <MANIFEST_RELATIVE>
```

Positional arguments:

```text
<MANIFEST_RELATIVE>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--timeout <TIMEOUT>` | Maximum operation duration in seconds [default: 3600] |

## graphit task get-task

Read current Task state and explicitly requested snapshot-bound history/comment pages

```text
graphit task get-task [OPTIONS] --id <ID>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Exact Task ULID |
| `--history-page-size <HISTORY_PAGE_SIZE>` | Request the history section with this positive page size |
| `--history-cursor <HISTORY_CURSOR>` | Opaque history continuation returned for this record |
| `--history-cursor-file <HISTORY_CURSOR_FILE>` | Read raw history cursor text from a regular bounded file |
| `--comments-page-size <COMMENTS_PAGE_SIZE>` | Request the comments section with this positive page size |
| `--comments-cursor <COMMENTS_CURSOR>` | Opaque continuation from the requested comments section. |
| `--comments-cursor-file <COMMENTS_CURSOR_FILE>` | Read raw comments cursor text from a regular bounded file |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task catalogue

Read a cursor-pinned Task catalogue, including explicit dates and dependencies

```text
graphit task catalogue [OPTIONS] --page-size <PAGE_SIZE>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--status <STATUS>` | [possible values: open, in-progress, completed, cancelled] |
| `--session-id <SESSION_ID>` | Session ULID for Task creation or the selected catalogue/query filter. |
| `--flagged <FLAGGED>` | Filter by the flag's exact boolean value [possible values: true, false] |
| `--blocked <BLOCKED>` | [possible values: true, false] |
| `--query <QUERY>` | Literal catalogue filter text |
| `--page-size <PAGE_SIZE>` | Positive number of rows in this page |
| `--cursor <CURSOR>` | Opaque cursor returned by this exact operation (JSON object for catalogue/search, text for query) |
| `--cursor-file <CURSOR_FILE>` | Read the opaque cursor from a regular bounded file; JSON object for catalogue/search, raw text for query |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task search

Search current Task specifications and comments using textual, semantic or hybrid retrieval

```text
graphit task search [OPTIONS] --text <TEXT> --page-size <PAGE_SIZE> --top-k <TOP_K>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--status <STATUS>` | [possible values: open, in-progress, completed, cancelled] |
| `--session-id <SESSION_ID>` | Session ULID for Task creation or the selected catalogue/query filter. |
| `--flagged <FLAGGED>` | Filter by the flag's exact boolean value [possible values: true, false] |
| `--blocked <BLOCKED>` | [possible values: true, false] |
| `--mode <MODE>` | [default: fts] [possible values: fts, semantic, hybrid] |
| `--text <TEXT>` | Literal search text |
| `--page-size <PAGE_SIZE>` | Positive number of rows in this page |
| `--top-k <TOP_K>` | Positive maximum number of results for the complete query |
| `--cursor <CURSOR>` | Opaque cursor returned by this exact operation (JSON object for catalogue/search, text for query) |
| `--cursor-file <CURSOR_FILE>` | Read the opaque cursor from a regular bounded file; JSON object for catalogue/search, raw text for query |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task query

Read a finite projected Task or Session page from one native pin

```text
graphit task query [OPTIONS] --table <TABLE> --column <COLUMNS> --page-size <PAGE_SIZE> --top-k <TOP_K>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--table <TABLE>` | [possible values: tasks, sessions] |
| `--id <IDS>` | Exact IDs; repeat the flag or separate values by commas |
| `--empty-ids` | Select an explicitly empty ID set (zero results), rather than omitting the ID filter |
| `--status <STATUS>` | [possible values: open, in-progress, completed, cancelled] |
| `--session-id <SESSION_ID>` | Task table only; a Session query cannot filter by session ID |
| `--column <COLUMNS>` | Projected columns in output order; repeat or use commas [possible values: id, status, title, revision, created-at, updated-at, session-id, parent-id, task-type, priority, flagged] |
| `--page-size <PAGE_SIZE>` | Positive number of rows in this page |
| `--top-k <TOP_K>` | Positive maximum number of results for the complete query |
| `--cursor <CURSOR>` | Opaque cursor returned by this exact operation (JSON object for catalogue/search, text for query) |
| `--cursor-file <CURSOR_FILE>` | Read the opaque cursor from a regular bounded file; JSON object for catalogue/search, raw text for query |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task schema

Read the native Task query tables and permitted column types

```text
graphit task schema [OPTIONS]
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task get-session

Read a Session and its immutable native history

```text
graphit task get-session [OPTIONS] --id <ID>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Exact Task/Session ULID for this operation. |
| `--history-page-size <HISTORY_PAGE_SIZE>` | Request the history section with this positive page size |
| `--history-cursor <HISTORY_CURSOR>` | Opaque history continuation returned for this record |
| `--history-cursor-file <HISTORY_CURSOR_FILE>` | Read raw history cursor text from a regular bounded file |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task session-heads

Read native Session heads

```text
graphit task session-heads [OPTIONS]
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task create-task

Create a native Task with acceptance and test checks, and explicit optional calendar dates

```text
graphit task create-task [OPTIONS] --idempotency-key <IDEMPOTENCY_KEY> --title <TITLE> --description <DESCRIPTION> --task-type <TASK_TYPE> --session-id <SESSION_ID> --priority <PRIORITY>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--idempotency-key <IDEMPOTENCY_KEY>` | Stable key identifying this creation or comment request |
| `--title <TITLE>` | Literal title; omit during revision to preserve it |
| `--description <DESCRIPTION>` | Literal description; omit during revision to preserve it |
| `--task-type <TASK_TYPE>` | Task classification |
| `--session-id <SESSION_ID>` | Session ULID for Task creation or the selected catalogue/query filter. |
| `--priority <PRIORITY>` | Priority from 0 through 4 |
| `--parent-id <PARENT_ID>` | Parent Task ULID |
| `--depends-on <DEPENDS_ON>` | Dependency Task ULIDs; repeat or separate by commas |
| `--commit <COMMIT>` | Full lowercase hexadecimal Git commit ID (40 or 64 characters) |
| `--start-date <START_DATE>` | Explicit calendar date in YYYY-MM-DD format |
| `--end-date <END_DATE>` | Explicit calendar date in YYYY-MM-DD format |
| `--due-date <DUE_DATE>` | Explicit calendar date in YYYY-MM-DD format |
| `--check-kind <CHECK_KINDS>` | Check categories paired by occurrence with --check-text (new checks when revising) [possible values: acceptance, test] |
| `--check-text <CHECK_TEXTS>` | Literal check text paired by occurrence with --check-kind |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task create-session

Create a native Session

```text
graphit task create-session [OPTIONS] --idempotency-key <IDEMPOTENCY_KEY> --title <TITLE> --description <DESCRIPTION> --strategy <STRATEGY>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--idempotency-key <IDEMPOTENCY_KEY>` | Stable key identifying this creation or comment request |
| `--title <TITLE>` | Literal title; omit during revision to preserve it |
| `--description <DESCRIPTION>` | Literal description; omit during revision to preserve it |
| `--strategy <STRATEGY>` | Literal Session strategy |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task claim-task

Claim a Task; return the opaque participant fence only to this caller

```text
graphit task claim-task [OPTIONS] --id <ID> --lease-ms <LEASE_MS>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Exact Task/Session ULID for this operation. |
| `--lease-ms <LEASE_MS>` | Positive claim lifetime in milliseconds |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task claim-session

Claim a Session; return its opaque participant fence

```text
graphit task claim-session [OPTIONS] --id <ID> --lease-ms <LEASE_MS>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Exact Task/Session ULID for this operation. |
| `--lease-ms <LEASE_MS>` | Positive claim lifetime in milliseconds |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task heartbeat-task

Renew a Task owner's actual lease and revision

```text
graphit task heartbeat-task [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <EXPECTED_REVISION> --lease-ms <LEASE_MS>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Task or Session ULID selected by this command |
| `--claim-token <CLAIM_TOKEN>` | Current opaque participant fence; never an actor selector |
| `--expected-revision <EXPECTED_REVISION>` | Revision returned with the current claim |
| `--lease-ms <LEASE_MS>` | Positive claim lifetime in milliseconds |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task heartbeat-session

Renew a Session owner's actual lease and revision

```text
graphit task heartbeat-session [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <EXPECTED_REVISION> --lease-ms <LEASE_MS>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Task or Session ULID selected by this command |
| `--claim-token <CLAIM_TOKEN>` | Current opaque participant fence; never an actor selector |
| `--expected-revision <EXPECTED_REVISION>` | Revision returned with the current claim |
| `--lease-ms <LEASE_MS>` | Positive claim lifetime in milliseconds |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task add-task-comment

Add a literal Markdown comment with the current participant fence and an idempotency key

```text
graphit task add-task-comment [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <EXPECTED_REVISION> --lease-ms <LEASE_MS> --idempotency-key <IDEMPOTENCY_KEY> --kind <KIND> --body <BODY>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Task or Session ULID selected by this command |
| `--claim-token <CLAIM_TOKEN>` | Current opaque participant fence; never an actor selector |
| `--expected-revision <EXPECTED_REVISION>` | Revision returned with the current claim |
| `--lease-ms <LEASE_MS>` | Positive claim lifetime in milliseconds |
| `--idempotency-key <IDEMPOTENCY_KEY>` | Stable key identifying this creation or comment request |
| `--kind <KIND>` | Comment category [possible values: note, decision, problem, lesson, knowledge] |
| `--body <BODY>` | Literal Markdown comment body |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task progress-task

Record the current Task owner's progress

```text
graphit task progress-task [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <EXPECTED_REVISION> --lease-ms <LEASE_MS> --summary <SUMMARY> --next-step <NEXT_STEP>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Task or Session ULID selected by this command |
| `--claim-token <CLAIM_TOKEN>` | Current opaque participant fence; never an actor selector |
| `--expected-revision <EXPECTED_REVISION>` | Revision returned with the current claim |
| `--lease-ms <LEASE_MS>` | Positive claim lifetime in milliseconds |
| `--summary <SUMMARY>` | Literal progress or release summary |
| `--next-step <NEXT_STEP>` | Literal next action |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task checkpoint-session

Record the current Session owner's checkpoint

```text
graphit task checkpoint-session [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <EXPECTED_REVISION> --lease-ms <LEASE_MS> --summary <SUMMARY> --next-step <NEXT_STEP>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Task or Session ULID selected by this command |
| `--claim-token <CLAIM_TOKEN>` | Current opaque participant fence; never an actor selector |
| `--expected-revision <EXPECTED_REVISION>` | Revision returned with the current claim |
| `--lease-ms <LEASE_MS>` | Positive claim lifetime in milliseconds |
| `--summary <SUMMARY>` | Literal progress or release summary |
| `--next-step <NEXT_STEP>` | Literal next action |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task release-task

Release a Task using its current participant fence

```text
graphit task release-task [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <EXPECTED_REVISION> --summary <SUMMARY> --next-step <NEXT_STEP>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Task or Session ULID selected by this command |
| `--claim-token <CLAIM_TOKEN>` | Current opaque participant fence; never an actor selector |
| `--expected-revision <EXPECTED_REVISION>` | Revision returned with the current claim |
| `--summary <SUMMARY>` | Literal progress or release summary |
| `--next-step <NEXT_STEP>` | Literal next action |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task release-session

Release a Session using its current participant fence

```text
graphit task release-session [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <EXPECTED_REVISION> --summary <SUMMARY> --next-step <NEXT_STEP>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Task or Session ULID selected by this command |
| `--claim-token <CLAIM_TOKEN>` | Current opaque participant fence; never an actor selector |
| `--expected-revision <EXPECTED_REVISION>` | Revision returned with the current claim |
| `--summary <SUMMARY>` | Literal progress or release summary |
| `--next-step <NEXT_STEP>` | Literal next action |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task revise-task

Revise Task specification; omitted fields stay, --clear-schedule removes dates

```text
graphit task revise-task [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <EXPECTED_REVISION> --lease-ms <LEASE_MS> --reason <REASON>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Task or Session ULID selected by this command |
| `--claim-token <CLAIM_TOKEN>` | Current opaque participant fence; never an actor selector |
| `--expected-revision <EXPECTED_REVISION>` | Revision returned with the current claim |
| `--lease-ms <LEASE_MS>` | Positive claim lifetime in milliseconds |
| `--reason <REASON>` | Recorded reason for this change |
| `--title <TITLE>` | Literal title; omit during revision to preserve it |
| `--description <DESCRIPTION>` | Literal description; omit during revision to preserve it |
| `--task-type <TASK_TYPE>` | Task classification |
| `--priority <PRIORITY>` | Priority from 0 through 4 |
| `--start-date <START_DATE>` | Explicit calendar date in YYYY-MM-DD format |
| `--end-date <END_DATE>` | Explicit calendar date in YYYY-MM-DD format |
| `--due-date <DUE_DATE>` | Explicit calendar date in YYYY-MM-DD format |
| `--clear-schedule` | Remove all scheduled dates; omission preserves the current schedule |
| `--check-kind <CHECK_KINDS>` | Check categories paired by occurrence with --check-text (new checks when revising) [possible values: acceptance, test] |
| `--check-text <CHECK_TEXTS>` | Literal check text paired by occurrence with --check-kind |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task revise-session

Revise the coordinated Session specification; omitted fields stay

```text
graphit task revise-session [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <EXPECTED_REVISION> --lease-ms <LEASE_MS> --reason <REASON>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Task or Session ULID selected by this command |
| `--claim-token <CLAIM_TOKEN>` | Current opaque participant fence; never an actor selector |
| `--expected-revision <EXPECTED_REVISION>` | Revision returned with the current claim |
| `--lease-ms <LEASE_MS>` | Positive claim lifetime in milliseconds |
| `--reason <REASON>` | Recorded reason for this change |
| `--title <TITLE>` | Literal title; omit during revision to preserve it |
| `--description <DESCRIPTION>` | Literal description; omit during revision to preserve it |
| `--strategy <STRATEGY>` | Literal Session strategy |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task verify-task-check

Verify a current Task check with evidence

```text
graphit task verify-task-check [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <EXPECTED_REVISION> --lease-ms <LEASE_MS> --check-id <CHECK_ID> --passed <PASSED> --evidence <EVIDENCE>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Task or Session ULID selected by this command |
| `--claim-token <CLAIM_TOKEN>` | Current opaque participant fence; never an actor selector |
| `--expected-revision <EXPECTED_REVISION>` | Revision returned with the current claim |
| `--lease-ms <LEASE_MS>` | Positive claim lifetime in milliseconds |
| `--check-id <CHECK_ID>` | Current check ULID |
| `--passed <PASSED>` | Explicit verification result: true or false [possible values: true, false] |
| `--evidence <EVIDENCE>` | Evidence supporting this check result |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task supersede-task-check

Supersede a Task check, optionally creating a replacement

```text
graphit task supersede-task-check [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <EXPECTED_REVISION> --lease-ms <LEASE_MS> --check-id <CHECK_ID> --reason <REASON>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Task or Session ULID selected by this command |
| `--claim-token <CLAIM_TOKEN>` | Current opaque participant fence; never an actor selector |
| `--expected-revision <EXPECTED_REVISION>` | Revision returned with the current claim |
| `--lease-ms <LEASE_MS>` | Positive claim lifetime in milliseconds |
| `--check-id <CHECK_ID>` | Current check ULID |
| `--reason <REASON>` | Recorded reason for this change |
| `--replacement-text <REPLACEMENT_TEXT>` | Create a replacement check with this literal text |
| `--replacement-kind <REPLACEMENT_KIND>` | Optional replacement category; omission preserves the superseded category [possible values: acceptance, test] |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task update-task-links

Replace Task parent or dependency links with recorded reason

```text
graphit task update-task-links [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <EXPECTED_REVISION> --lease-ms <LEASE_MS> --reason <REASON>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Task or Session ULID selected by this command |
| `--claim-token <CLAIM_TOKEN>` | Current opaque participant fence; never an actor selector |
| `--expected-revision <EXPECTED_REVISION>` | Revision returned with the current claim |
| `--lease-ms <LEASE_MS>` | Positive claim lifetime in milliseconds |
| `--reason <REASON>` | Recorded reason for this change |
| `--parent-id <PARENT_ID>` | Parent Task ULID |
| `--clear-parent` | Remove the parent link |
| `--depends-on <DEPENDS_ON>` | Dependency Task ULIDs; repeat or separate by commas |
| `--clear-dependencies` | Replace dependencies with an empty list |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task set-task-flag

Set or clear the native Task flag with recorded reason

```text
graphit task set-task-flag [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <EXPECTED_REVISION> --lease-ms <LEASE_MS> --flagged <FLAGGED> --reason <REASON>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Task or Session ULID selected by this command |
| `--claim-token <CLAIM_TOKEN>` | Current opaque participant fence; never an actor selector |
| `--expected-revision <EXPECTED_REVISION>` | Revision returned with the current claim |
| `--lease-ms <LEASE_MS>` | Positive claim lifetime in milliseconds |
| `--flagged <FLAGGED>` | Explicit flag value: true or false [possible values: true, false] |
| `--reason <REASON>` | Recorded reason for this change |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task set-task-commit

Associate or clear the full Git commit ID without changing Task status/checks

```text
graphit task set-task-commit [OPTIONS] --id <ID> --expected-revision <EXPECTED_REVISION> --reason <REASON>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Exact Task/Session ULID for this operation. |
| `--expected-revision <EXPECTED_REVISION>` |  |
| `--claim-token <CLAIM_TOKEN>` |  |
| `--claim-revision <CLAIM_REVISION>` | Revision of an optional claim (separate from set-task-commit's record CAS) |
| `--commit <COMMIT>` | Full lowercase hexadecimal Git commit ID (40 or 64 characters) |
| `--clear-commit` | Explicitly remove the commit association |
| `--reason <REASON>` | Recorded reason for this change |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task complete-task

Complete a Task after its actual gates pass

```text
graphit task complete-task [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <EXPECTED_REVISION>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Task or Session ULID selected by this command |
| `--claim-token <CLAIM_TOKEN>` | Current opaque participant fence; never an actor selector |
| `--expected-revision <EXPECTED_REVISION>` | Revision returned with the current claim |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task cancel-task

Cancel an eligible Task with recorded reason and live fence when owned

```text
graphit task cancel-task [OPTIONS] --id <ID> --reason <REASON>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Exact Task/Session ULID for this operation. |
| `--claim-token <CLAIM_TOKEN>` |  |
| `--claim-revision <CLAIM_REVISION>` | Revision of an optional claim (separate from set-task-commit's record CAS) |
| `--reason <REASON>` | Recorded reason for this change |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit task complete-session

Complete a Session after its native Task gates pass

```text
graphit task complete-session [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <EXPECTED_REVISION>
```

| Option | Contract |
|---|---|
| `--project-dir <PROJECT_DIR>` | Exact native PROJECT locator; selection grants no authority |
| `--project-id <PROJECT_ID>` | Retained PROJECT ULID; selection grants no authorization. |
| `--id <ID>` | Task or Session ULID selected by this command |
| `--claim-token <CLAIM_TOKEN>` | Current opaque participant fence; never an actor selector |
| `--expected-revision <EXPECTED_REVISION>` | Revision returned with the current claim |
| `--reference-type <REFERENCE_TYPES>` | Replace authored references. Pair repeated type/id/relation flags by occurrence [possible values: task, session, memory, knowledge] |
| `--reference-id <REFERENCE_IDS>` |  |
| `--reference-relation <REFERENCE_RELATIONS>` |  |
| `--reference-scope <REFERENCE_SCOPES>` | Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent |
| `--reference-scope-id <REFERENCE_SCOPE_IDS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-context <REFERENCE_CONTEXTS>` | Omit entirely or repeat once per reference; empty means absent |
| `--reference-field <REFERENCE_FIELDS>` | Omit entirely or repeat once per reference; empty text is a literal empty field |
| `--reference-field-absent <REFERENCE_FIELDS_ABSENT>` | Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty |
| `--clear-references` | Explicitly replace authored references with an empty list |

Fields are individual flags; list options repeat. See [Task workflows](../guides/task-sessions.md) and [CLI values and exceptions](../guides/cli_reference.md#arguments-and-lists). MCP and HTTP retain their typed JSON envelopes.

## graphit git install

Snapshot the current state, then install the native Git notifications

```text
graphit git install [OPTIONS]
```

## graphit git sync

Reconcile configured commands and Task/Memory events without taking a new snapshot

```text
graphit git sync [OPTIONS]
```

Uses `githooks` and the enabled record modules. Git 2.54+ is required.
See [Git hooks](../guides/git-hooks.md) for installation and command configuration.

## graphit git remove

Remove only this brand's native Git hook registrations

```text
graphit git remove [OPTIONS]
```

## graphit git snapshot

Bind the current record state to HEAD, or retry a failed commit hook

```text
graphit git snapshot [OPTIONS]
```

## graphit git restore

Retry restoring the record snapshot for the checked-out commit

```text
graphit git restore [OPTIONS] [PREVIOUS_COMMIT]
```

Positional arguments:

```text
[PREVIOUS_COMMIT]
```

## graphit git hook

Dispatch current configured commands and Task/Memory notifications for a Git event

```text
graphit git hook [OPTIONS] <EVENT> [ARGUMENTS]...
```

Git's registry calls `graphit git hook <event> --` and appends the original hook arguments.
The separator protects arguments beginning with `-`; callers invoking the dispatcher manually
must also use it when forwarding such arguments. Commands read the current configuration;
see [context, shell and input](../guides/git-hooks.md#shell-working-directory-and-hook-input).

## graphit ast source

Read indexed source or the physical file in the retained project version

```text
graphit ast source [OPTIONS] <PATH>
```

Positional arguments:

```text
<PATH>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--entity <ENTITY>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--entity-type <ENTITY_TYPE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--head <HEAD>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--tail <TAIL>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--start <START_LINE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--end <END_LINE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--pattern <PATTERN>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--regex` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--before <BEFORE>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--after <AFTER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--line-numbers` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--context <CONTEXT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit ast export

Export a published PROJECT graph or an installed AST context

```text
graphit ast export [OPTIONS] --output <OUTPUT>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--output <OUTPUT>` | Destination package file, resolved from this invocation's working directory |
| `--project-dir <PROJECT_DIR>` | Select a retained local project; otherwise use the current selection |
| `--context <CONTEXT>` | Repackage an exact installed AST, independently of PROJECT selection |

## graphit ast imports

List exact installed Native AST contexts

```text
graphit ast imports [OPTIONS]
```

## graphit ast search

Search the current PROJECT or an exact installed AST context

```text
graphit ast search [OPTIONS] <QUERY>
```

Positional arguments:

```text
<QUERY>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--context <CONTEXT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--mode <MODE>` | [default: semantic] [possible values: fts, hybrid, semantic, vector] |
| `--k <K>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--metric <METRIC>` | [default: cosine] [possible values: l2, cosine, dot] |
| `--rerank [<RERANK>]` | Override search.rerank for this query; omitted uses configuration (default false) [possible values: true, false] |

## graphit ast query

Query the current PROJECT or an explicitly selected installed graph

```text
graphit ast query [OPTIONS] <QUERY>
```

| Argument | Contract |
|---|---|
| `<QUERY>` | Read-only Cypher query for the selected graph. |

| Option | Contract |
|---|---|
| `--context <CONTEXT>` | Exact installed AST artifact id@version; excludes PROJECT selectors. |
| `--project-dir <PROJECT_DIR>` |  |
| `--param <NAME=VALUE>` | Named typed Cypher value; repeat for each name (VALUE is a JSON value) |
| `--param-string <NAME=TEXT>` | Named literal Cypher string; repeat for each name, without JSON quoting |

## graphit ast schema

Show graph labels, columns and relationships

```text
graphit ast schema [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--context <CONTEXT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit ast sample

Show a bounded sample of graph nodes and edges

```text
graphit ast sample [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--context <CONTEXT>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit ast neighborhood

Page over the relationships of one node

```text
graphit ast neighborhood [OPTIONS] --anchor-label <ANCHOR_LABEL> --anchor-key <ANCHOR_KEY> --relationship <RELATIONSHIP> --other-label <OTHER_LABEL>
```

| Option | Contract |
|---|---|
| `--context <CONTEXT>` | Exact installed AST artifact id@version; excludes PROJECT selectors. |
| `--project-dir <PROJECT_DIR>` |  |
| `--anchor-label <ANCHOR_LABEL>` | Starting node label returned by ast schema. |
| `--anchor-key <ANCHOR_KEY>` | Primary-key value of the starting node. |
| `--key-type <KEY_TYPE>` | [default: string] [possible values: string, int64] |
| `--relationship <RELATIONSHIP>` | Relationship kind returned by ast schema. |
| `--other-label <OTHER_LABEL>` | Label of the opposite endpoint returned by ast schema. |
| `--direction <DIRECTION>` | [default: outgoing] [possible values: outgoing, incoming] |
| `--limit <LIMIT>` | [default: 100] |
| `--cursor <CURSOR>` | Exact opaque JSON cursor returned by the preceding neighborhood page |
| `--cursor-file <CURSOR_FILE>` | Read that unchanged cursor object from a UTF-8 JSON file |

## graphit daemon foreground

Run until SIGINT or SIGTERM, then join jobs and remove owned discovery/key

```text
graphit daemon foreground [OPTIONS]
```

## graphit daemon start

Start the user's daemon for this installation

```text
graphit daemon start [OPTIONS]
```

## graphit daemon stop

Stop the user manager before draining the daemon's retained owners

```text
graphit daemon stop [OPTIONS]
```

## graphit daemon restart

Restart the registered service with its native user manager

```text
graphit daemon restart [OPTIONS]
```

## graphit daemon status

Observe the authenticated daemon or its native manager

```text
graphit daemon status [OPTIONS]
```

## graphit daemon install

Register the service; login startup is enabled only with --login

```text
graphit daemon install [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--login` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit daemon login

Change startup at user login explicitly

```text
graphit daemon login [OPTIONS] <COMMAND>
```

Subcommands:

- [`enable`](#graphit-daemon-login-enable)
- [`disable`](#graphit-daemon-login-disable)

## graphit daemon uninstall

Stop and remove the service and its login registration

```text
graphit daemon uninstall [OPTIONS]
```

## graphit hub authored list

Enumerate authored resource identities in the selected PROJECT/version

```text
graphit hub authored list [OPTIONS]
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--kind <KIND>` | [possible values: agent, ast, command, knowledge, language, mcp, power, rule, skill, workflow] |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--limit <LIMIT>` | [default: 100] |
| `--cursor <CURSOR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit hub authored review

Review files in one explicitly selected authored resource tree

```text
graphit hub authored review [OPTIONS] --kind <KIND> --id <ID> --version <VERSION>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--kind <KIND>` | [possible values: agent, ast, command, knowledge, language, mcp, power, rule, skill, workflow] |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--offset <OFFSET>` | [default: 0] |
| `--limit <LIMIT>` | [default: 100] |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit hub authored content

Read a text file relative to one explicitly selected authored resource tree

```text
graphit hub authored content [OPTIONS] --kind <KIND> --id <ID> --version <VERSION> --path <PATH>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--kind <KIND>` | [possible values: agent, ast, command, knowledge, language, mcp, power, rule, skill, workflow] |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--path <PATH>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit hub content installed

```text
graphit hub content installed [OPTIONS] --kind <KIND> --id <ID>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--kind <KIND>` | [possible values: skill, command, workflow, agent, rule, mcp, language, power] |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | Exact installed artifact version; required when a global ID has multiple versions |
| `--identity-provider <IDENTITY_PROVIDER>` | Explicit verified Native identity for a stored standalone origin |
| `--global` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--project-dir <PROJECT_DIR>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--binding <BINDING>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--path <PATH>` | Relative file in a directory artifact; omit for a projected single file |

## graphit hub content remote

```text
graphit hub content remote [OPTIONS] --provider <PROVIDER> --producer <PRODUCER> --kind <KIND> --id <ID> --path <PATH>
```

| Option | Meaning, accepted values and defaults |
|---|---|
| `--transport <TRANSPORT>` | [default: broker] [possible values: broker, standalone] |
| `--provider <PROVIDER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--profile <PROFILE>` | Explicit standalone transport profile. Omit only for unsigned standalone S3 |
| `--identity-provider <IDENTITY_PROVIDER>` | Explicit Native identity for standalone ACLs; does not select S3 credentials |
| `--producer <PRODUCER>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--kind <KIND>` | [possible values: skill, command, workflow, agent, rule, mcp, language, power] |
| `--id <ID>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |
| `--version <VERSION>` | [default: latest] |
| `--exact` | Treat a numeric version as a literal rather than a stable release prefix |
| `--path <PATH>` | Named argument shown in the usage above; see the domain guide for its authorization and state contract. |

## graphit daemon login enable

```text
graphit daemon login enable [OPTIONS]
```

## graphit daemon login disable

```text
graphit daemon login disable [OPTIONS]
```
