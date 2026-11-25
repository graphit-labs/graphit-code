---
title: Complete CLI option reference
description: Inspected Rust CLI command tree, positional arguments, flags, accepted values and defaults.
content-type: reference
audience: developers
updated: 2026-11-10
---

# Complete CLI option reference

This reference records the current executable's public help tree. All command
paths below were read with `--help`, without configuration or service startup.
Start with the [CLI guide](../guides/cli_reference.md) for workflows and result
formats. Options shown here are parser defaults; configuration may add consumer
defaults described in the [configuration reference](configuration-keys.md).

Commands print readable text by default, even when redirected. Use global
`--json` for structured records. Raw AST source, Ask answers and protocol streams
retain their text/protocol contract. Native-agent options prefer `--agent`,
with `--binding` accepted where previously supported. AST search prefers
`--top-k`; `--k` is retained. MCP uses stdio by default; `--stdio` is accepted. Now uses `--project-id`,
with `--project` retained as an alias.
Task checks accept repeated `--check KIND TEXT`, references accept repeated
`--reference TYPE ID RELATION`. Parallel legacy forms remain accepted and
conflict with the corresponding compact form.

Shared options are accepted before or after subcommands. The selected operation
still validates its scope: init, remove, uninstall and config reject retained project-version selection.

## Shared options

| Option | Contract |
|---|---|
| `--json` | Structured records for scripts; readable text is the default. |
| `-c, --config KEY=VALUE` | Repeatable configuration override; the last value wins. |
| `--global-dir DIRECTORY` | Select the private installation. |
| `--project-version VERSION` | Select a retained PROJECT version where supported. |
| `--non-interactive` | Disable prompts. |
| `-V, --version` | Print the compiled version; exclusive with other operations. |
| `-h, --help` | Print help without executing the operation. |

## graphit init

```text
Prepare the installation and synchronize the current project

Usage: graphit init [OPTIONS]

Options:
      --agent <AGENT>
          Agent binding. Uses the configured agent or defaults to Codex
      --project-dir <DIRECTORY>
          Project root to initialize. A dot discovers the current project [default: .]
      --project-id <ID>
          Initial project ULID. An existing project must have this ID
      --name <NAME>
          Initial project name. Existing metadata is preserved
      --coordinator-source <COORDINATOR_SOURCE>
          Use explicit Coordinator sources when running from a development build
  -h, --help
          Print help

Global options:
      --json
          Print structured JSON for scripts instead of human-readable text
  -c, --config <KEY=VALUE>
          Override a configuration value; repeat for different keys (the last value
          wins)
      --non-interactive
          Run without interactive prompts
      --project-version <PROJECT_VERSION>
          Select the retained version of the current project
      --global-dir <DIRECTORY>
          Override the installation directory (otherwise BRAND_GLOBAL_DIR or the home
          directory)
```


## graphit remove

```text
Remove the Graphit project document and integrations, or withdraw one agent

Usage: graphit remove [OPTIONS]

Options:
      --project-dir <DIRECTORY>  Project root to remove. A dot discovers the current
                                 project [default: .]
      --agent <AGENT>            Withdraw only the integrations owned for this agent
  -h, --help                     Print help

Global options:
      --json
          Print structured JSON for scripts instead of human-readable text
  -c, --config <KEY=VALUE>
          Override a configuration value; repeat for different keys (the last value
          wins)
      --non-interactive
          Run without interactive prompts
      --project-version <PROJECT_VERSION>
          Select the retained version of the current project
      --global-dir <DIRECTORY>
          Override the installation directory (otherwise BRAND_GLOBAL_DIR or the home
          directory)
```


## graphit uninstall

```text
Stop and remove this user's Graphit installation and its local data

Usage: graphit uninstall [OPTIONS]

Options:
  -h, --help  Print help

Global options:
      --json
          Print structured JSON for scripts instead of human-readable text
  -c, --config <KEY=VALUE>
          Override a configuration value; repeat for different keys (the last value
          wins)
      --non-interactive
          Run without interactive prompts
      --project-version <PROJECT_VERSION>
          Select the retained version of the current project
      --global-dir <DIRECTORY>
          Override the installation directory (otherwise BRAND_GLOBAL_DIR or the home
          directory)
```


## graphit sync

```text
Reconcile installed Hub artifacts, native skills/agents, Git/agent hooks and MCP

Usage: graphit sync [OPTIONS]

Options:
      --agent <AGENT>
          Native agent; defaults to the effective Graphit agent configuration
      --global
          Reconcile global skills/roles and Hub installations without a PROJECT
      --project-dir <DIRECTORY>
          Select a project directory; relative paths use the working directory.
      --coordinator-source <COORDINATOR_SOURCE>
          Explicit product Coordinator source for development/package verification
  -h, --help
          Print help
```


## graphit skills

```text
Reconcile maintained module skills independently

Usage: graphit skills [OPTIONS] <COMMAND>

Commands:
  sync  Synchronize module skills.
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit skills sync

```text
Synchronize module skills.

Usage: graphit skills sync [OPTIONS]

Options:
      --destination <DESTINATION>
          Physical destination. Global never captures the current directory as a PROJECT
          [default: project] [possible values: project, global]
      --source-project <DIRECTORY>
          Explicit PROJECT origin for raw overrides/installed bundles; independently
          admitted
      --agent <AGENT>
          Native agent integration to configure.
      --role <ROLE>
          [possible values: scout, tracker, scribe]
      --rule <RULE>
          A qualified raw bundle for one role: ROLE=ID@VERSION
      --skill-alias <SKILL_ALIAS>
          Effective configured skill name: MODULE=NAME
      --skill-bundle <SKILL_BUNDLE>
          Installed instruction bundle: MODULE=ID@VERSION; remote origins require
          current access
      --skill-identity-provider <SKILL_IDENTITY_PROVIDER>
          Explicit Native identity for a selected standalone bundle: MODULE=PROVIDER
      --module-skill <MODULE_SKILL>

  -h, --help
          Print help
```


## graphit config

```text
Inspect or persist the current Rust configuration

Usage: graphit config [OPTIONS] <COMMAND>

Commands:
  set    Persist JSON or literal text; CSV settings and secrets remain strings
  get    Inspect a configured value and its source, with secret redaction
  list   List configured values and their sources, with secret redaction
  unset  Remove a value from the selected Project or Global store
  help   Print this message or the help of the given subcommand(s)

Options:
      --global             Write or inspect only the installation's global configuration
      --timeout <TIMEOUT>  Maximum time to wait for a configuration writer, in seconds
                           [default: 30]
  -h, --help               Print help
```


## graphit config set

```text
Persist JSON or literal text; CSV settings and secrets remain strings

Usage: graphit config set [OPTIONS] <KEY> <VALUE>

Arguments:
  <KEY>
  <VALUE>

Options:
      --global             Write or inspect only the installation's global configuration
      --timeout <TIMEOUT>  Maximum time to wait for a configuration writer, in seconds
                           [default: 30]
  -h, --help               Print help


CSV list settings: ast.grammar, ast.grammars_whitelist, ast.grammars_blacklist,
ast.scip.parsers, knowledge.extensions. An empty value clears the selected list, subject
to that setting's validation.

Examples:
  graphit config set knowledge.extensions md,txt
  graphit config set ast.scip.parsers
  .zig=registry.example/parser:1,.exm=registry.example/parser:2

Structured settings such as githooks, task.browser, daemon.models and
agents.<agent>.instructions require JSON objects. Hook commands are complete strings,
never CSV. Project writes preserve identity and metadata in project.json; githooks is at
the document root, outside settings. Inside task.browser, providers accepts an array of
strings or a comma-separated string.
```


## graphit config get

```text
Inspect a configured value and its source, with secret redaction

Usage: graphit config get [OPTIONS] <KEY>

Arguments:
  <KEY>

Options:
      --global             Write or inspect only the installation's global configuration
      --timeout <TIMEOUT>  Maximum time to wait for a configuration writer, in seconds
                           [default: 30]
  -h, --help               Print help
```


## graphit config list

```text
List configured values and their sources, with secret redaction

Usage: graphit config list [OPTIONS]

Options:
      --global             Write or inspect only the installation's global configuration
      --timeout <TIMEOUT>  Maximum time to wait for a configuration writer, in seconds
                           [default: 30]
  -h, --help               Print help
```


## graphit config unset

```text
Remove a value from the selected Project or Global store

Usage: graphit config unset [OPTIONS] <KEY>

Arguments:
  <KEY>

Options:
      --global             Write or inspect only the installation's global configuration
      --timeout <TIMEOUT>  Maximum time to wait for a configuration writer, in seconds
                           [default: 30]
  -h, --help               Print help
```


## graphit self-update

```text
Install the latest verified release and resume this installation's active daemon

Usage: graphit self-update [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit ui

```text
Open the configured Browser served by the active daemon

Usage: graphit ui [OPTIONS]

Options:
      --no-open  Print the configured Browser URL without launching a desktop
                 application
  -h, --help     Print help
```


## graphit tray

```text
Control this installation from the Linux desktop tray

Usage: graphit tray [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit projects

```text
Discover local projects or maintain their checkout inventory

Usage: graphit projects [OPTIONS] <COMMAND>

Commands:
  list               List authorized local projects, checkouts and retained versions
  show               Locate one local project by its exact ID or unique name
  label-set          Add a label value to an active local checkout
  label-unset        Remove a label value or the entire key when no value is given
  maintain-checkout  Observe this checkout/version, keep sibling jobs and publish the
                     whole inventory
  retire-checkout    Stop this checkout's jobs, retain its versions, and publish the
                     inventory
  help               Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit projects list

```text
List authorized local projects, checkouts and retained versions

Usage: graphit projects list [OPTIONS]

Options:
      --name <NAME>        Name of the selected item.
      --project-id <ID>    Exact project identifier; selection does not grant access.
      --version <VERSION>
      --label-key <KEY>    Require this exact local label key.
      --label-value <VALUE>
                           Require this exact value of the selected key.
      --online[=<ONLINE>]  Filter local availability; use --online=false for offline
                           entries. [possible values: true, false]
      --no-paths           Omit local checkout paths from the result.
      --page-size <COUNT>  Maximum entries in this page. [default: 25]
      --cursor <CURSOR>    Continuation cursor returned by the previous page.
  -h, --help               Print help
```


## graphit projects show

```text
Locate one local project by its exact ID or unique name

Usage: graphit projects show [OPTIONS]

Options:
      --project-id <ID>  Exact project identifier; selection does not grant access.
      --name <NAME>      Name of the selected item.
      --no-paths         Omit local checkout paths from the result.
  -h, --help             Print help
```


## graphit projects label-set

```text
Usage: graphit projects label-set [OPTIONS] --project-id <ID> --key <KEY> --value <VALUE>

Options:
      --project-id <ID>
      --project-dir <DIR>  Select a checkout directory. Defaults to the current project.
      --version <VERSION> Select an exact active version when the checkout has multiple versions.
      --key <KEY>
      --value <VALUE>
  -h, --help               Print help
```

## graphit projects label-unset

```text
Usage: graphit projects label-unset [OPTIONS] --project-id <ID> --key <KEY>

Options:
      --project-id <ID>
      --project-dir <DIR>  Select a checkout directory. Defaults to the current project.
      --version <VERSION> Select an exact active version when the checkout has multiple versions.
      --key <KEY>
      --value <VALUE>
  -h, --help               Print help
```


## graphit projects maintain-checkout

```text
Observe this checkout/version, keep sibling jobs and publish the whole inventory

Usage: graphit projects maintain-checkout [OPTIONS] --project-id <ID>

Options:
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --project-dir <DIRECTORY>
          Select a checkout directory. Defaults to the current project
      --version <VERSION>
          [default: ""]
      --detached-base <DETACHED_BASE>
          [default: ""]
      --select

  -h, --help
          Print help
```


## graphit projects retire-checkout

```text
Stop this checkout's jobs, retain its versions, and publish the inventory

Usage: graphit projects retire-checkout [OPTIONS] --project-id <ID>

Options:
      --project-id <ID>          Exact project identifier; selection does not grant
                                 access.
      --project-dir <DIRECTORY>  Select a checkout directory. Defaults to the current
                                 project
  -h, --help                     Print help
```


## graphit now

```text
Follow the live Broker Now view for an explicitly selected project

Usage: graphit now [OPTIONS] --provider <PROVIDER> --project-id <ID>

Options:
      --project-id <ID>      Canonical project ID; authorization remains with the daemon
                             and Broker [alias: --project]
      --provider <PROVIDER>  Explicit configured Broker provider
      --version <VERSION>    Filter the Broker view by its literal project version
      --instance <INSTANCE>  Filter the Broker view by its instance ID
  -h, --help                 Print help
```


## graphit knowledge

```text
Search, query or export Knowledge through the active service

Usage: graphit knowledge [OPTIONS] <COMMAND>

Commands:
  list    List published documents with metadata filters and pagination
  lint    Validate the complete published corpus; exit 1 for issues, 3 for no corpus
  ask     Return the final answer from the service's captured provider binding
  search  Search an authorized PROJECT or an installed Knowledge artifact
  source  Read complete native Knowledge Markdown, or an explicit line/pattern slice
  log     Read committed document synchronization history in the selected PROJECT
  export  Freeze the current Knowledge snapshots and export an installable native
          package
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit knowledge list

```text
List published documents with metadata filters and pagination

Usage: graphit knowledge list [OPTIONS]

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --kind <KIND>
          Keep documents with this metadata kind
      --source-prefix <SOURCE_PREFIX>
          Keep documents whose source path starts with this prefix
      --stale[=<STALE>]
          Filter stale status; use --stale=false for current documents [possible values:
          true, false]
      --metadata-errors[=<METADATA_ERRORS>]
          Filter missing required metadata; use --metadata-errors=false to exclude it
          [possible values: true, false]
      --page-size <COUNT>
          Maximum documents in this page [default: 25]
      --cursor <CURSOR>
          Exact JSON continuation object from the previous page; quote it as one
          argument
  -h, --help
          Print help
```


## graphit knowledge lint

```text
Validate the complete published corpus; exit 1 for issues, 3 for no corpus

Usage: graphit knowledge lint [OPTIONS]

Options:
      --project-dir <DIRECTORY>  Exact native PROJECT locator; selection grants no
                                 authority
      --project-id <ID>          Exact project identifier; selection does not grant
                                 access.
      --max-age-days <DAYS>      Report persisted stale markers plus pages older than
                                 this explicit age
  -h, --help                     Print help
```


## graphit knowledge ask

```text
Return the final answer from the service's captured provider binding

Usage: graphit knowledge ask [OPTIONS] --query <QUERY> --model <MODEL>

Options:
      --query <QUERY>            Literal question, without trimming or shell
                                 interpretation
      --slug <SELECTED_SLUGS>    Explicit Knowledge page slug; repeat to select more
                                 pages
      --retrieval <RETRIEVAL>    Retrieve pages from the question instead of selecting
                                 explicit slugs [possible values: fts, semantic, hybrid]
      --top-k <COUNT>            Number of ranked pages to provide; requires automatic
                                 retrieval
      --metric <METRIC>          Vector metric for semantic or hybrid retrieval;
                                 defaults to cosine [possible values: cosine, l2, dot]
      --model <MODEL>            Requested model identifier passed literally to the
                                 captured completion binding
      --project-dir <DIRECTORY>  Select a retained local project rather than the current
                                 project
  -h, --help                     Print help
```


## graphit knowledge search

```text
Search an authorized PROJECT or an installed Knowledge artifact

Usage: graphit knowledge search [OPTIONS] <QUERY>

Arguments:
  <QUERY>  Query the selected Knowledge corpus

Options:
      --mode <MODE>
          [default: fts] [possible values: fts, semantic, hybrid]
      --metric <METRIC>
          [possible values: cosine, l2, dot]
      --top-k <COUNT>
          Limit ranked results; zero retains the service's unbounded-result semantics
      --page-size <COUNT>
          Maximum entries in this page.
      --preview

      --rerank[=<RERANK>]
          Override configured reranking; --rerank=false always disables it [possible
          values: true, false]
      --cursor <CURSOR>
          Continue with the exact opaque JSON cursor returned by the service
      --cursor-file <CURSOR_FILE>
          Read the unchanged cursor object from a UTF-8 JSON file
      --project-dir <DIRECTORY>
          Select a project directory; relative paths use the working directory.
      --artifact-id <ARTIFACT_ID>
          Select an installed Knowledge artifact independently of PROJECT
      --artifact-version <ARTIFACT_VERSION>

      --context-alias <CONTEXT_ALIAS>

  -h, --help
          Print help
```


## graphit knowledge source

```text
Read complete native Knowledge Markdown, or an explicit line/pattern slice

Usage: graphit knowledge source [OPTIONS] <SLUG>

Arguments:
  <SLUG>  Exact Knowledge page slug; this is not a filesystem path

Options:
      --head <HEAD>
          Return the first N lines.
      --tail <TAIL>
          Return the last N lines.
      --start <START_LINE>
          First line to include (1-based).
      --end <END_LINE>
          Last line to include (inclusive).
      --pattern <PATTERN>
          Text to match in the selected source.
      --regex
          Treat the pattern as a regular expression.
      --before <BEFORE>
          Context lines before each match.
      --after <AFTER>
          Context lines after each match.
      --line-numbers
          Include original line numbers.
      --project-dir <DIRECTORY>
          Select a project directory; relative paths use the working directory.
      --artifact-id <ARTIFACT_ID>

      --artifact-version <ARTIFACT_VERSION>

      --context-alias <CONTEXT_ALIAS>

  -h, --help
          Print help
```


## graphit knowledge log

```text
Read committed document synchronization history in the selected PROJECT

Usage: graphit knowledge log [OPTIONS]

Options:
      --limit <COUNT>            Maximum synchronization records in this page [default:
                                 10]
      --cursor <CURSOR>          Continue with the opaque cursor returned by the service
      --project-dir <DIRECTORY>  Select a project directory; relative paths use the
                                 working directory.
  -h, --help                     Print help
```


## graphit knowledge export

```text
Freeze the current Knowledge snapshots and export an installable native package

Usage: graphit knowledge export [OPTIONS] --output <OUTPUT>

Options:
      --format <FORMAT>
          Export an installable native package or a Markdown, OKF or Obsidian directory
          [default: native] [possible values: native, markdown, okf, obsidian]
      --output <OUTPUT>
          Destination package file or text directory, relative to this working directory
      --project-dir <DIRECTORY>
          Select a retained local project; otherwise use the current selection
      --artifact-id <ARTIFACT_ID>
          Select an installed Knowledge artifact independently of PROJECT
      --artifact-version <ARTIFACT_VERSION>

      --context-alias <CONTEXT_ALIAS>

  -h, --help
          Print help
```


## graphit memory

```text
Read, search, edit, or index authorized Memory records

Usage: graphit memory [OPTIONS] <COMMAND>

Commands:
  list       List Memory using supported metadata filters
  get        Read one current record
  search     Search current records and optionally deleted history
  history    Read creation and immutable revision history
  create     Create a Memory record from text, a file, or standard input
  update     Change record fields when its current revision matches
  delete     Tombstone a record and preserve its history
  mandatory  Set or remove mandatory status using native Memory update
  important  Set or remove important status using native Memory update
  index      Ensure or explicitly refresh lexical indexes and configured embeddings
  help       Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help

Global options:
      --json
          Print structured JSON for scripts instead of human-readable text
  -c, --config <KEY=VALUE>
          Override a configuration value; repeat for different keys (the last value
          wins)
      --non-interactive
          Run without interactive prompts
      --project-version <PROJECT_VERSION>
          Select the retained version of the current project
      --global-dir <DIRECTORY>
          Override the installation directory (otherwise BRAND_GLOBAL_DIR or the home
          directory)
```


## graphit memory list

```text
List Memory using supported metadata filters

Usage: graphit memory list [OPTIONS]

Options:
      --project-dir <DIRECTORY>
          Select a project directory (project scope only)
      --project-id <ID>
          Select an exact project ID (project scope only)
      --scope <SCOPE>
          Select project records or personal records for the local installation or
          signed-in account [default: project] [possible values: project, user]
      --classification <CLASSIFICATION>
          Keep records with this classification [possible values: fact, skill, tension,
          decision, convention, correction]
      --tag <TAG>
          Keep records with this exact tag
      --mandatory[=<MANDATORY>]
          Filter mandatory status; use --mandatory=false for unmarked records [possible
          values: true, false]
      --important[=<IMPORTANT>]
          Filter important status; use --important=false for unmarked records [possible
          values: true, false]
      --exclude-mandatory
          Exclude mandatory records from the results
  -h, --help
          Print help

Global options:
      --json
          Print structured JSON for scripts instead of human-readable text
  -c, --config <KEY=VALUE>
          Override a configuration value; repeat for different keys (the last value
          wins)
      --non-interactive
          Run without interactive prompts
      --project-version <PROJECT_VERSION>
          Select the retained version of the current project
      --global-dir <DIRECTORY>
          Override the installation directory (otherwise BRAND_GLOBAL_DIR or the home
          directory)
```


## graphit memory get

```text
Read one current record

Usage: graphit memory get [OPTIONS] --id <ID>

Options:
      --project-dir <DIRECTORY>  Select a project directory (project scope only)
      --project-id <ID>          Select an exact project ID (project scope only)
      --scope <SCOPE>            Select project records or personal records for the
                                 local installation or signed-in account [default:
                                 project] [possible values: project, user]
      --id <ID>                  Exact identifier returned by the corresponding list or
                                 search command.
  -h, --help                     Print help

Global options:
      --json
          Print structured JSON for scripts instead of human-readable text
  -c, --config <KEY=VALUE>
          Override a configuration value; repeat for different keys (the last value
          wins)
      --non-interactive
          Run without interactive prompts
      --project-version <PROJECT_VERSION>
          Select the retained version of the current project
      --global-dir <DIRECTORY>
          Override the installation directory (otherwise BRAND_GLOBAL_DIR or the home
          directory)
```


## graphit memory search

```text
Search current records and optionally deleted history

Usage: graphit memory search [OPTIONS] <QUERY>

Arguments:
  <QUERY>  Words to search for in Memory

Options:
      --project-dir <DIRECTORY>
          Select a project directory (project scope only)
      --project-id <ID>
          Select an exact project ID (project scope only)
      --scope <SCOPE>
          Select project records or personal records for the local installation or
          signed-in account [default: project] [possible values: project, user]
      --classification <CLASSIFICATION>
          Keep records with this classification [possible values: fact, skill, tension,
          decision, convention, correction]
      --tag <TAG>
          Keep records with this exact tag
      --mandatory[=<MANDATORY>]
          Filter mandatory status; use --mandatory=false for unmarked records [possible
          values: true, false]
      --important[=<IMPORTANT>]
          Filter important status; use --important=false for unmarked records [possible
          values: true, false]
      --exclude-mandatory
          Exclude mandatory records from the results
      --top-k <COUNT>
          Maximum number of search results [default: 20]
      --mode <MODE>
          Search mode; semantic and hybrid require configured embeddings [default: fts]
          [possible values: fts, semantic, hybrid]
      --include-deleted-history
          Also search retained history of deleted records
  -h, --help
          Print help

Global options:
      --json
          Print structured JSON for scripts instead of human-readable text
  -c, --config <KEY=VALUE>
          Override a configuration value; repeat for different keys (the last value
          wins)
      --non-interactive
          Run without interactive prompts
      --project-version <PROJECT_VERSION>
          Select the retained version of the current project
      --global-dir <DIRECTORY>
          Override the installation directory (otherwise BRAND_GLOBAL_DIR or the home
          directory)
```


## graphit memory history

```text
Read creation and immutable revision history

Usage: graphit memory history [OPTIONS] --id <ID>

Options:
      --project-dir <DIRECTORY>  Select a project directory (project scope only)
      --project-id <ID>          Select an exact project ID (project scope only)
      --scope <SCOPE>            Select project records or personal records for the
                                 local installation or signed-in account [default:
                                 project] [possible values: project, user]
      --id <ID>                  Exact identifier returned by the corresponding list or
                                 search command.
  -h, --help                     Print help

Global options:
      --json
          Print structured JSON for scripts instead of human-readable text
  -c, --config <KEY=VALUE>
          Override a configuration value; repeat for different keys (the last value
          wins)
      --non-interactive
          Run without interactive prompts
      --project-version <PROJECT_VERSION>
          Select the retained version of the current project
      --global-dir <DIRECTORY>
          Override the installation directory (otherwise BRAND_GLOBAL_DIR or the home
          directory)
```


## graphit memory create

```text
Create a Memory record from text, a file, or standard input

Usage: graphit memory create [OPTIONS] --idempotency-key <IDEMPOTENCY_KEY> --title <TITLE> <--body <BODY>|--body-file <FILE>>

Options:
      --project-dir <DIRECTORY>
          Select a project directory (project scope only)
      --project-id <ID>
          Select an exact project ID (project scope only)
      --scope <SCOPE>
          Select project records or personal records for the local installation or
          signed-in account [default: project] [possible values: project, user]
      --idempotency-key <IDEMPOTENCY_KEY>
          Stable key for retrying the same logical change
      --title <TITLE>
          Title of the record.
      --body <BODY>
          Literal Markdown body
      --body-file <FILE>
          UTF-8 file, or - to read standard input
      --classification <CLASSIFICATION>
          Classification of the Memory record. [default: fact] [possible values: fact,
          skill, tension, decision, convention, correction]
      --mandatory
          Include this record in mandatory project or user context.
      --important
          Mark this record as important.
      --tag <TAG>
          Add a tag; repeat --tag for each tag
  -h, --help
          Print help

Global options:
      --json
          Print structured JSON for scripts instead of human-readable text
  -c, --config <KEY=VALUE>
          Override a configuration value; repeat for different keys (the last value
          wins)
      --non-interactive
          Run without interactive prompts
      --project-version <PROJECT_VERSION>
          Select the retained version of the current project
      --global-dir <DIRECTORY>
          Override the installation directory (otherwise BRAND_GLOBAL_DIR or the home
          directory)
```


## graphit memory update

```text
Change record fields when its current revision matches

Usage: graphit memory update [OPTIONS] --idempotency-key <IDEMPOTENCY_KEY> --reason <REASON> --expected-revision <REVISION> --id <ID> <--title <TITLE>|--body <BODY>|--body-file <FILE>|--classification <CLASSIFICATION>|--tag <TAG>|--clear-tags>

Options:
      --project-dir <DIRECTORY>
          Select a project directory (project scope only)
      --project-id <ID>
          Select an exact project ID (project scope only)
      --scope <SCOPE>
          Select project records or personal records for the local installation or
          signed-in account [default: project] [possible values: project, user]
      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --expected-revision <REVISION>
          Current record revision; a mismatch rejects the change
      --idempotency-key <IDEMPOTENCY_KEY>
          Stable key for retrying the same logical change
      --reason <REASON>
          Explain why this record is being changed
      --title <TITLE>
          Title of the record.
      --body <BODY>
          Literal Markdown body
      --body-file <FILE>
          UTF-8 file, or - to read standard input
      --classification <CLASSIFICATION>
          Classification of the Memory record. [possible values: fact, skill, tension,
          decision, convention, correction]
      --tag <TAG>
          Replace all tags with these values; repeat --tag for each tag
      --clear-tags
          Remove all tags
  -h, --help
          Print help

Global options:
      --json
          Print structured JSON for scripts instead of human-readable text
  -c, --config <KEY=VALUE>
          Override a configuration value; repeat for different keys (the last value
          wins)
      --non-interactive
          Run without interactive prompts
      --project-version <PROJECT_VERSION>
          Select the retained version of the current project
      --global-dir <DIRECTORY>
          Override the installation directory (otherwise BRAND_GLOBAL_DIR or the home
          directory)
```


## graphit memory delete

```text
Tombstone a record and preserve its history

Usage: graphit memory delete [OPTIONS] --idempotency-key <IDEMPOTENCY_KEY> --reason <REASON> --expected-revision <REVISION> --id <ID>

Options:
      --project-dir <DIRECTORY>
          Select a project directory (project scope only)
      --project-id <ID>
          Select an exact project ID (project scope only)
      --scope <SCOPE>
          Select project records or personal records for the local installation or
          signed-in account [default: project] [possible values: project, user]
      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --expected-revision <REVISION>
          Current record revision; a mismatch rejects the change
      --idempotency-key <IDEMPOTENCY_KEY>
          Stable key for retrying the same logical change
      --reason <REASON>
          Explain why this record is being changed
  -h, --help
          Print help

Global options:
      --json
          Print structured JSON for scripts instead of human-readable text
  -c, --config <KEY=VALUE>
          Override a configuration value; repeat for different keys (the last value
          wins)
      --non-interactive
          Run without interactive prompts
      --project-version <PROJECT_VERSION>
          Select the retained version of the current project
      --global-dir <DIRECTORY>
          Override the installation directory (otherwise BRAND_GLOBAL_DIR or the home
          directory)
```


## graphit memory mandatory

```text
Set or remove mandatory status using native Memory update

Usage: graphit memory mandatory [OPTIONS] --idempotency-key <IDEMPOTENCY_KEY> --reason <REASON> --value <VALUE> --expected-revision <REVISION> --id <ID>

Options:
      --project-dir <DIRECTORY>
          Select a project directory (project scope only)
      --project-id <ID>
          Select an exact project ID (project scope only)
      --scope <SCOPE>
          Select project records or personal records for the local installation or
          signed-in account [default: project] [possible values: project, user]
      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --expected-revision <REVISION>
          Current record revision; a mismatch rejects the change
      --idempotency-key <IDEMPOTENCY_KEY>
          Stable key for retrying the same logical change
      --reason <REASON>
          Explain why this record is being changed
      --value <VALUE>
          true sets the flag; false removes it through the native attribute update
          [possible values: true, false]
  -h, --help
          Print help

Global options:
      --json
          Print structured JSON for scripts instead of human-readable text
  -c, --config <KEY=VALUE>
          Override a configuration value; repeat for different keys (the last value
          wins)
      --non-interactive
          Run without interactive prompts
      --project-version <PROJECT_VERSION>
          Select the retained version of the current project
      --global-dir <DIRECTORY>
          Override the installation directory (otherwise BRAND_GLOBAL_DIR or the home
          directory)
```


## graphit memory important

```text
Set or remove important status using native Memory update

Usage: graphit memory important [OPTIONS] --idempotency-key <IDEMPOTENCY_KEY> --reason <REASON> --value <VALUE> --expected-revision <REVISION> --id <ID>

Options:
      --project-dir <DIRECTORY>
          Select a project directory (project scope only)
      --project-id <ID>
          Select an exact project ID (project scope only)
      --scope <SCOPE>
          Select project records or personal records for the local installation or
          signed-in account [default: project] [possible values: project, user]
      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --expected-revision <REVISION>
          Current record revision; a mismatch rejects the change
      --idempotency-key <IDEMPOTENCY_KEY>
          Stable key for retrying the same logical change
      --reason <REASON>
          Explain why this record is being changed
      --value <VALUE>
          true sets the flag; false removes it through the native attribute update
          [possible values: true, false]
  -h, --help
          Print help

Global options:
      --json
          Print structured JSON for scripts instead of human-readable text
  -c, --config <KEY=VALUE>
          Override a configuration value; repeat for different keys (the last value
          wins)
      --non-interactive
          Run without interactive prompts
      --project-version <PROJECT_VERSION>
          Select the retained version of the current project
      --global-dir <DIRECTORY>
          Override the installation directory (otherwise BRAND_GLOBAL_DIR or the home
          directory)
```


## graphit memory index

```text
Ensure or explicitly refresh lexical indexes and configured embeddings

Usage: graphit memory index [OPTIONS]

Options:
      --scope <SCOPE>            Select project Memory or personal Memory for the local
                                 installation or signed-in account [default: project]
                                 [possible values: project, user]
      --refresh                  Replace the existing lexical index explicitly; never
                                 triggered by a read
      --project-dir <DIRECTORY>  Select a project directory; relative paths use the
                                 working directory.
  -h, --help                     Print help

Global options:
      --json
          Print structured JSON for scripts instead of human-readable text
  -c, --config <KEY=VALUE>
          Override a configuration value; repeat for different keys (the last value
          wins)
      --non-interactive
          Run without interactive prompts
      --project-version <PROJECT_VERSION>
          Select the retained version of the current project
      --global-dir <DIRECTORY>
          Override the installation directory (otherwise BRAND_GLOBAL_DIR or the home
          directory)
```


## graphit provider

```text
Manage authentication providers in the private installation

Usage: graphit provider [OPTIONS] <COMMAND>

Commands:
  list    List authentication providers.
  show    Inspect authentication providers.
  add     Add authentication providers.
  update  Update authentication providers.
  remove  Remove authentication providers.
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit provider list

```text
List authentication providers.

Usage: graphit provider list [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit provider show

```text
Inspect authentication providers.

Usage: graphit provider show [OPTIONS] <NAME>

Arguments:
  <NAME>

Options:
  -h, --help  Print help
```


## graphit provider add

```text
Add authentication providers.

Usage: graphit provider add [OPTIONS] --type <KIND> <NAME>

Arguments:
  <NAME>

Options:
      --type <KIND>
          [possible values: local, broker]
      --broker-endpoint <BROKER_ENDPOINT>

      --mcp-resource <MCP_RESOURCE>

      --broker-allow-anonymous

      --broker-deny-anonymous

      --clear-broker

      --allow-aws-credential-chain

      --deny-aws-credential-chain

      --embedding-mode <EMBEDDING_MODE>
          [possible values: local, direct, broker, disabled]
      --embedding-device <EMBEDDING_DEVICE>
          [possible values: auto, cpu, cuda, metal]
      --embedding-device-id <EMBEDDING_DEVICE_ID>

      --embedding-protocol <EMBEDDING_PROTOCOL>
          [possible values: openai, openai-compatible, cohere, cohere-v2, voyage,
          voyage-v1, google-gemini-1, google-gemini-2, jina, jina-v1]
      --embedding-endpoint <EMBEDDING_ENDPOINT>

      --embedding-model <EMBEDDING_MODEL>

      --embedding-model-revision <EMBEDDING_MODEL_REVISION>
          Explicit pinned model or deployment revision supplied by the provider owner
      --embedding-recipe <EMBEDDING_RECIPE>
          Explicit reproducible embedding input/output recipe identifier
      --embedding-dimensions <EMBEDDING_DIMENSIONS>

      --embedding-request-dimensions <EMBEDDING_REQUEST_DIMENSIONS>

      --embedding-clear-request-dimensions

      --embedding-batch-endpoint <EMBEDDING_BATCH_ENDPOINT>

      --embedding-clear-batch-endpoint

      --rerank-mode <RERANK_MODE>
          [possible values: local, direct, broker, disabled]
      --rerank-device <RERANK_DEVICE>
          [possible values: auto, cpu, cuda, metal]
      --rerank-device-id <RERANK_DEVICE_ID>

      --rerank-protocol <RERANK_PROTOCOL>
          [possible values: openai, openai-compatible, cohere, cohere-v2, voyage,
          voyage-v1, google-gemini-1, google-gemini-2, jina, jina-v1]
      --rerank-endpoint <RERANK_ENDPOINT>

      --rerank-model <RERANK_MODEL>

      --rerank-model-revision <RERANK_MODEL_REVISION>

      --rerank-recipe <RERANK_RECIPE>

      --rerank-dimensions <RERANK_DIMENSIONS>

      --s3-bucket <S3_BUCKET>

      --s3-region <S3_REGION>

      --s3-endpoint <S3_ENDPOINT>

      --s3-prefix <S3_PREFIX>

      --s3-credential-source <S3_CREDENTIAL_SOURCE>
          [possible values: login, aws-chain, broker]
      --clear-s3

      --expected-revision <REVISION>
          Current record revision; a mismatch rejects the change.
  -h, --help
          Print help
```


## graphit provider update

```text
Update authentication providers.

Usage: graphit provider update [OPTIONS] <NAME>

Arguments:
  <NAME>

Options:
      --type <KIND>
          [possible values: local, broker]
      --broker-endpoint <BROKER_ENDPOINT>

      --mcp-resource <MCP_RESOURCE>

      --broker-allow-anonymous

      --broker-deny-anonymous

      --clear-broker

      --allow-aws-credential-chain

      --deny-aws-credential-chain

      --embedding-mode <EMBEDDING_MODE>
          [possible values: local, direct, broker, disabled]
      --embedding-device <EMBEDDING_DEVICE>
          [possible values: auto, cpu, cuda, metal]
      --embedding-device-id <EMBEDDING_DEVICE_ID>

      --embedding-protocol <EMBEDDING_PROTOCOL>
          [possible values: openai, openai-compatible, cohere, cohere-v2, voyage,
          voyage-v1, google-gemini-1, google-gemini-2, jina, jina-v1]
      --embedding-endpoint <EMBEDDING_ENDPOINT>

      --embedding-model <EMBEDDING_MODEL>

      --embedding-model-revision <EMBEDDING_MODEL_REVISION>
          Explicit pinned model or deployment revision supplied by the provider owner
      --embedding-recipe <EMBEDDING_RECIPE>
          Explicit reproducible embedding input/output recipe identifier
      --embedding-dimensions <EMBEDDING_DIMENSIONS>

      --embedding-request-dimensions <EMBEDDING_REQUEST_DIMENSIONS>

      --embedding-clear-request-dimensions

      --embedding-batch-endpoint <EMBEDDING_BATCH_ENDPOINT>

      --embedding-clear-batch-endpoint

      --rerank-mode <RERANK_MODE>
          [possible values: local, direct, broker, disabled]
      --rerank-device <RERANK_DEVICE>
          [possible values: auto, cpu, cuda, metal]
      --rerank-device-id <RERANK_DEVICE_ID>

      --rerank-protocol <RERANK_PROTOCOL>
          [possible values: openai, openai-compatible, cohere, cohere-v2, voyage,
          voyage-v1, google-gemini-1, google-gemini-2, jina, jina-v1]
      --rerank-endpoint <RERANK_ENDPOINT>

      --rerank-model <RERANK_MODEL>

      --rerank-model-revision <RERANK_MODEL_REVISION>

      --rerank-recipe <RERANK_RECIPE>

      --rerank-dimensions <RERANK_DIMENSIONS>

      --s3-bucket <S3_BUCKET>

      --s3-region <S3_REGION>

      --s3-endpoint <S3_ENDPOINT>

      --s3-prefix <S3_PREFIX>

      --s3-credential-source <S3_CREDENTIAL_SOURCE>
          [possible values: login, aws-chain, broker]
      --clear-s3

      --expected-revision <REVISION>
          Current record revision; a mismatch rejects the change.
  -h, --help
          Print help
```


## graphit provider remove

```text
Remove authentication providers.

Usage: graphit provider remove [OPTIONS] --yes <NAME>

Arguments:
  <NAME>

Options:
      --cascade

      --yes

      --expected-revision <REVISION>
          Current record revision; a mismatch rejects the change.
  -h, --help
          Print help
```


## graphit account

```text
List, inspect or select local profiles

Usage: graphit account [OPTIONS] <COMMAND>

Commands:
  list  List stored account profiles.
  show  Inspect stored account profiles.
  use   Select stored account profiles.
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit account list

```text
List stored account profiles.

Usage: graphit account list [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit account show

```text
Inspect stored account profiles.

Usage: graphit account show [OPTIONS] [PROFILE]

Arguments:
  [PROFILE]

Options:
  -h, --help  Print help
```


## graphit account use

```text
Select stored account profiles.

Usage: graphit account use [OPTIONS] <PROFILE>

Arguments:
  <PROFILE>

Options:
      --expected-revision <REVISION>
          Current record revision; a mismatch rejects the change.
  -h, --help
          Print help
```


## graphit login

```text
Log in through a configured Local or Broker provider

Usage: graphit login [OPTIONS] --profile <PROFILE> --provider <PROVIDER>

Options:
      --device
          Authorize with a public user code instead of a loopback callback
      --profile <PROFILE>
          Stored account profile name.
      --provider <PROVIDER>
          Configured provider name.
      --username <USERNAME>

      --organization <ORGANIZATION>

      --team <TEAMS>

      --anonymous

      --s3-disabled

      --mcp-key <MCP_KEY>

      --broker-key <BROKER_KEY>

      --embedding-api-key <EMBEDDING_API_KEY>

      --rerank-api-key <RERANK_API_KEY>

      --s3-access-key <S3_ACCESS_KEY>

      --s3-secret-key <S3_SECRET_KEY>

      --s3-session-token <S3_SESSION_TOKEN>

      --s3-expires-at <S3_EXPIRES_AT>

      --aws-profile <AWS_PROFILE>

      --expected-revision <REVISION>
          Current record revision; a mismatch rejects the change.
  -h, --help
          Print help
```


## graphit logout

```text
Remove a stored profile

Usage: graphit logout [OPTIONS]

Options:
      --profile <PROFILE>
          Stored account profile name.
      --expected-revision <REVISION>
          Current record revision; a mismatch rejects the change.
  -h, --help
          Print help
```


## graphit refresh

```text
Refresh the active native Broker profile

Usage: graphit refresh [OPTIONS]

Options:
      --expected-revision <REVISION>
          Current record revision; a mismatch rejects the change.
  -h, --help
          Print help
```


## graphit package

```text
Export, inspect or import native packages

Usage: graphit package [OPTIONS] <COMMAND>

Commands:
  install  Install a native corpus in the installation's global cache
  export   Package a completed native payload directory
  inspect  Validate physical package structure and every payload body
  import   Publish a validated native tree into a missing directory
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit package install

```text
Install a native corpus in the installation's global cache

Usage: graphit package install [OPTIONS] --version <VERSION> --input <INPUT> --producer <PRODUCER> --id <ID>

Options:
      --input <INPUT>        Source package file.
      --producer <PRODUCER>  Project ID that owns the artifact.
      --id <ID>              Exact identifier returned by the corresponding list or
                             search command.
      --version <VERSION>
      --kind <KIND>
  -h, --help                 Print help
```


## graphit package export

```text
Package a completed native payload directory

Usage: graphit package export [OPTIONS] --kind <KIND> --source <SOURCE> --output <OUTPUT>

Options:
      --kind <KIND>
      --source <SOURCE>
      --output <OUTPUT>  Destination file or directory.
  -h, --help             Print help
```


## graphit package inspect

```text
Validate physical package structure and every payload body

Usage: graphit package inspect [OPTIONS] --input <INPUT>

Options:
      --input <INPUT>  Source package file.
      --kind <KIND>
  -h, --help           Print help
```


## graphit package import

```text
Publish a validated native tree into a missing directory

Usage: graphit package import [OPTIONS] --input <INPUT> --output <OUTPUT>

Options:
      --input <INPUT>    Source package file.
      --output <OUTPUT>  Destination file or directory.
      --kind <KIND>
  -h, --help             Print help
```


## graphit roles

```text
Install or synchronize three managed roles on the active daemon

Usage: graphit roles [OPTIONS] <COMMAND>

Commands:
  install  Install managed roles.
  sync     Synchronize managed roles.
  run      Manage managed roles.
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit roles install

```text
Install managed roles.

Usage: graphit roles install [OPTIONS]

Options:
      --destination <DESTINATION>
          Physical destination. Global never captures the current directory as a PROJECT
          [default: project] [possible values: project, global]
      --source-project <DIRECTORY>
          Explicit PROJECT origin for raw overrides/installed bundles; independently
          admitted
      --agent <AGENT>
          Native agent integration to configure.
      --role <ROLE>
          [possible values: scout, tracker, scribe]
      --rule <RULE>
          A qualified raw bundle for one role: ROLE=ID@VERSION
      --skill-alias <SKILL_ALIAS>
          Effective configured skill name: MODULE=NAME
      --skill-bundle <SKILL_BUNDLE>
          Installed instruction bundle: MODULE=ID@VERSION; remote origins require
          current access
      --skill-identity-provider <SKILL_IDENTITY_PROVIDER>
          Explicit Native identity for a selected standalone bundle: MODULE=PROVIDER
      --module-skill <MODULE_SKILL>

  -h, --help
          Print help
```


## graphit roles sync

```text
Synchronize managed roles.

Usage: graphit roles sync [OPTIONS]

Options:
      --destination <DESTINATION>
          Physical destination. Global never captures the current directory as a PROJECT
          [default: project] [possible values: project, global]
      --source-project <DIRECTORY>
          Explicit PROJECT origin for raw overrides/installed bundles; independently
          admitted
      --agent <AGENT>
          Native agent integration to configure.
      --role <ROLE>
          [possible values: scout, tracker, scribe]
      --rule <RULE>
          A qualified raw bundle for one role: ROLE=ID@VERSION
      --skill-alias <SKILL_ALIAS>
          Effective configured skill name: MODULE=NAME
      --skill-bundle <SKILL_BUNDLE>
          Installed instruction bundle: MODULE=ID@VERSION; remote origins require
          current access
      --skill-identity-provider <SKILL_IDENTITY_PROVIDER>
          Explicit Native identity for a selected standalone bundle: MODULE=PROVIDER
      --module-skill <MODULE_SKILL>

  -h, --help
          Print help
```


## graphit roles run

```text
Manage managed roles.

Usage: graphit roles run [OPTIONS] --prompt <PROMPT>

Options:
      --destination <DESTINATION>
          Physical destination. Global never captures the current directory as a PROJECT
          [default: project] [possible values: project, global]
      --source-project <DIRECTORY>
          Explicit PROJECT origin for raw overrides/installed bundles; independently
          admitted
      --agent <AGENT>
          Native agent integration to configure.
      --role <ROLE>
          [possible values: scout, tracker, scribe]
      --rule <RULE>
          A qualified raw bundle for one role: ROLE=ID@VERSION
      --skill-alias <SKILL_ALIAS>
          Effective configured skill name: MODULE=NAME
      --skill-bundle <SKILL_BUNDLE>
          Installed instruction bundle: MODULE=ID@VERSION; remote origins require
          current access
      --skill-identity-provider <SKILL_IDENTITY_PROVIDER>
          Explicit Native identity for a selected standalone bundle: MODULE=PROVIDER
      --module-skill <MODULE_SKILL>

      --model <MODEL>

      --prompt <PROMPT>

  -h, --help
          Print help
```


## graphit hooks

```text
Install PROJECT context hooks on the active daemon

Usage: graphit hooks [OPTIONS] <COMMAND>

Commands:
  install  Install agent context hooks. [alias: sync]
  remove   Remove owned hooks and MCP, preserving Kimi references from other projects
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit hooks install

```text
Install agent context hooks.

Usage: graphit hooks install [OPTIONS] --agent <AGENT>

Options:
      --agent <AGENT>      Native agent schema selected explicitly [alias: --binding]
      --profile <PROFILE>  Existing Coordinator Markdown filename; required only for
                           Kiro
  -h, --help               Print help
```


## graphit hooks remove

```text
Remove owned hooks and MCP, preserving Kimi references from other projects

Usage: graphit hooks remove [OPTIONS] --agent <AGENT>

Options:
      --agent <AGENT>      Native agent schema selected explicitly [alias: --binding]
      --profile <PROFILE>  Existing Coordinator Markdown filename; required only for
                           Kiro
  -h, --help               Print help
```


## graphit hub

```text
Publish captured native SDK artifacts to an authorized Hub provider

Usage: graphit hub [OPTIONS] <COMMAND>

Commands:
  authored        Manage Hub artifacts.
  install         Install Hub artifacts.
  install-local   Manage Hub artifacts.
  update          A captured installation target. An omitted selector updates its remote
                  entries
  sync            Reapply all installed artifacts from their recorded sources and
                  versions
  remove          Remove Hub artifacts.
  forget          Manage Hub artifacts.
  content         Manage Hub artifacts.
  list            List Hub artifacts.
  projects        Manage Hub artifacts.
  search          Available manifests of an explicit remote PROJECT; all filters are
                  literal
  publish         Manage Hub artifacts.
  publish-native  Manage Hub artifacts.
  unpublish       Retract remote catalog references while preserving payloads and local
                  installations
  install-native  Manage Hub artifacts.
  help            Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit hub authored

```text
Manage Hub artifacts.

Usage: graphit hub authored [OPTIONS] <COMMAND>

Commands:
  list     Enumerate authored resource identities in the selected PROJECT/version
  review   Review files in one explicitly selected authored resource tree
  content  Read a text file relative to one explicitly selected authored resource tree
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit hub authored list

```text
Enumerate authored resource identities in the selected PROJECT/version

Usage: graphit hub authored list [OPTIONS]

Options:
      --kind <KIND>              [possible values: agent, ast, command, knowledge,
                                 language, mcp, power, rule, skill, workflow]
      --id <ID>                  Exact identifier returned by the corresponding list or
                                 search command.
      --version <VERSION>
      --limit <COUNT>            [default: 100]
      --cursor <CURSOR>          Continuation cursor returned by the previous page.
      --project-dir <DIRECTORY>  Select a project directory; relative paths use the
                                 working directory.
  -h, --help                     Print help
```


## graphit hub authored review

```text
Review files in one explicitly selected authored resource tree

Usage: graphit hub authored review [OPTIONS] --kind <KIND> --version <VERSION> --id <ID>

Options:
      --kind <KIND>              [possible values: agent, ast, command, knowledge,
                                 language, mcp, power, rule, skill, workflow]
      --id <ID>                  Exact identifier returned by the corresponding list or
                                 search command.
      --version <VERSION>
      --offset <OFFSET>          [default: 0]
      --limit <COUNT>            [default: 100]
      --project-dir <DIRECTORY>  Select a project directory; relative paths use the
                                 working directory.
  -h, --help                     Print help
```


## graphit hub authored content

```text
Read a text file relative to one explicitly selected authored resource tree

Usage: graphit hub authored content [OPTIONS] --kind <KIND> --version <VERSION> --path <PATH> --id <ID>

Options:
      --kind <KIND>              [possible values: agent, ast, command, knowledge,
                                 language, mcp, power, rule, skill, workflow]
      --id <ID>                  Exact identifier returned by the corresponding list or
                                 search command.
      --version <VERSION>
      --path <PATH>
      --project-dir <DIRECTORY>  Select a project directory; relative paths use the
                                 working directory.
  -h, --help                     Print help
```


## graphit hub install

```text
Install Hub artifacts.

Usage: graphit hub install [OPTIONS] --kind <KIND> --provider <PROVIDER> --producer <PRODUCER> --id <ID>

Options:
      --transport <TRANSPORT>
          [default: broker] [possible values: broker, standalone]
      --provider <PROVIDER>
          Configured provider name.
      --profile <PROFILE>
          Explicit standalone transport profile. Omit only for unsigned standalone S3
      --identity-provider <IDENTITY_PROVIDER>
          Explicit Native identity for standalone ACLs; does not select S3 credentials
      --producer <PRODUCER>
          Project ID that owns the artifact.
      --kind <KIND>
          [possible values: skill, command, workflow, agent, rule, mcp, language, power]
      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --version <VERSION>
          [default: latest]
      --exact
          Treat a numeric version as a literal rather than a stable release prefix
      --global

      --project-dir <DIRECTORY>
          Select a project directory; relative paths use the working directory.
      --agent <AGENT>
          Native agent integration to configure. [alias: --binding]
      --alias <ALIAS>

      --entry-name <ENTRY_NAME>

      --mode <MODE>
          PROJECT installation mode. The checkout receives a copy [possible values:
          copy]
  -h, --help
          Print help
```


## graphit hub install-local

```text
Manage Hub artifacts.

Usage: graphit hub install-local [OPTIONS] --producer <PRODUCER> --kind <KIND> --version <VERSION> --source-dir <DIRECTORY> --id <ID>

Options:
      --source-dir <DIRECTORY>   Absolute artifact directory; its canonical root is
                                 captured before publication
      --producer <PRODUCER>      Provenance only; this does not grant access to the
                                 consumer PROJECT
      --kind <KIND>              [possible values: agent, skill, command, workflow,
                                 rule, mcp, language, power]
      --id <ID>                  Exact identifier returned by the corresponding list or
                                 search command.
      --version <VERSION>        Literal artifact version, independent of the selected
                                 PROJECT version
      --project-dir <DIRECTORY>  Select a checkout directory. Defaults to the current
                                 project
      --agent <AGENT>            Native agent integration to configure. [alias:
                                 --binding]
      --alias <ALIAS>
      --entry-name <ENTRY_NAME>
      --mode <MODE>              PROJECT installation mode. The checkout receives a copy
                                 [possible values: copy]
  -h, --help                     Print help
```


## graphit hub update

```text
A captured installation target. An omitted selector updates its remote entries

Usage: graphit hub update [OPTIONS]

Options:
      --global

      --project-dir <DIRECTORY>
          Select a project directory; relative paths use the working directory.
      --agent <AGENT>
          Native agent integration to configure. [alias: --binding]
      --identity-provider <IDENTITY_PROVIDER>

      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --transport <TRANSPORT>
          [possible values: broker, standalone]
      --provider <PROVIDER>
          Configured provider name.
      --profile <PROFILE>
          Stored account profile name.
      --producer <PRODUCER>
          Project ID that owns the artifact.
      --kind <KIND>
          [possible values: skill, command, workflow, agent, rule, mcp, language, power]
      --version <VERSION>

      --exact

      --alias <ALIAS>

      --entry-name <ENTRY_NAME>

      --mode <MODE>
          [possible values: copy]
  -h, --help
          Print help
```


## graphit hub sync

```text
Reapply all installed artifacts from their recorded sources and versions

Usage: graphit hub sync [OPTIONS]

Options:
      --global

      --project-dir <DIRECTORY>
          Select a project directory; relative paths use the working directory.
      --agent <AGENT>
          Native agent integration to configure. [alias: --binding]
      --identity-provider <IDENTITY_PROVIDER>

  -h, --help
          Print help
```


## graphit hub remove

```text
Remove Hub artifacts.

Usage: graphit hub remove [OPTIONS] --kind <KIND> --id <ID>

Options:
      --kind <KIND>
      --scope <SCOPE>            [default: project] [possible values: project,
                                 global-cache, global-native]
      --id <ID>                  Exact identifier returned by the corresponding list or
                                 search command.
      --version <VERSION>        Optional exact installed artifact version; never a
                                 release selector
      --project-dir <DIRECTORY>  Select a project directory; relative paths use the
                                 working directory.
      --agent <AGENT>            Native agent integration to configure. [alias:
                                 --binding]
  -h, --help                     Print help
```


## graphit hub forget

```text
Manage Hub artifacts.

Usage: graphit hub forget [OPTIONS] --kind <KIND> --id <ID>

Options:
      --kind <KIND>
      --scope <SCOPE>            [default: project] [possible values: project,
                                 global-cache, global-native]
      --id <ID>                  Exact identifier returned by the corresponding list or
                                 search command.
      --version <VERSION>        Optional exact installed artifact version; never a
                                 release selector
      --project-dir <DIRECTORY>  Select a project directory; relative paths use the
                                 working directory.
      --agent <AGENT>            Native agent integration to configure. [alias:
                                 --binding]
  -h, --help                     Print help
```


## graphit hub content

```text
Manage Hub artifacts.

Usage: graphit hub content [OPTIONS] <COMMAND>

Commands:
  installed  Manage Hub artifacts.
  remote     Manage Hub artifacts.
  help       Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit hub content installed

```text
Manage Hub artifacts.

Usage: graphit hub content installed [OPTIONS] --kind <KIND> --id <ID>

Options:
      --kind <KIND>
          [possible values: skill, command, workflow, agent, rule, mcp, language, power]
      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --version <VERSION>
          Exact installed artifact version; required when a global ID has multiple
          versions
      --identity-provider <IDENTITY_PROVIDER>
          Explicit verified Native identity for a stored standalone origin
      --global

      --project-dir <DIRECTORY>
          Select a project directory; relative paths use the working directory.
      --agent <AGENT>
          Native agent integration to configure. [alias: --binding]
      --path <PATH>
          Relative file in a directory artifact; omit for a projected single file
  -h, --help
          Print help
```


## graphit hub content remote

```text
Manage Hub artifacts.

Usage: graphit hub content remote [OPTIONS] --kind <KIND> --path <PATH> --provider <PROVIDER> --producer <PRODUCER> --id <ID>

Options:
      --transport <TRANSPORT>
          [default: broker] [possible values: broker, standalone]
      --provider <PROVIDER>
          Configured provider name.
      --profile <PROFILE>
          Explicit standalone transport profile. Omit only for unsigned standalone S3
      --identity-provider <IDENTITY_PROVIDER>
          Explicit Native identity for standalone ACLs; does not select S3 credentials
      --producer <PRODUCER>
          Project ID that owns the artifact.
      --kind <KIND>
          [possible values: skill, command, workflow, agent, rule, mcp, language, power]
      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --version <VERSION>
          [default: latest]
      --exact
          Treat a numeric version as a literal rather than a stable release prefix
      --path <PATH>

  -h, --help
          Print help
```


## graphit hub list

```text
List Hub artifacts.

Usage: graphit hub list [OPTIONS]

Options:
      --global

      --identity-provider <IDENTITY_PROVIDER>
          Explicit Native identity used only for stored standalone origins
      --project-dir <DIRECTORY>
          Select a project directory; relative paths use the working directory.
      --kind <KIND>
          [possible values: skill, command, workflow, agent, rule, mcp, language, power]
      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --version <VERSION>

      --agent <AGENT>
          Native agent integration to configure. [alias: --binding]
      --offset <OFFSET>
          [default: 0]
      --limit <COUNT>
          [default: 50]
  -h, --help
          Print help
```


## graphit hub projects

```text
Manage Hub artifacts.

Usage: graphit hub projects [OPTIONS] --provider <PROVIDER>

Options:
      --transport <TRANSPORT>
          [default: broker] [possible values: broker, standalone]
      --provider <PROVIDER>
          Configured provider name.
      --profile <PROFILE>
          Stored account profile name.
      --identity-provider <IDENTITY_PROVIDER>

      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --name-prefix <NAME_PREFIX>

      --limit <COUNT>
          [default: 50]
      --cursor <CURSOR>
          Continuation cursor returned by the previous page.
  -h, --help
          Print help
```


## graphit hub search

```text
Available manifests of an explicit remote PROJECT; all filters are literal

Usage: graphit hub search [OPTIONS] --provider <PROVIDER> --producer <PRODUCER>

Options:
      --transport <TRANSPORT>
          [default: broker] [possible values: broker, standalone]
      --provider <PROVIDER>
          Configured provider name.
      --profile <PROFILE>
          Stored account profile name.
      --identity-provider <IDENTITY_PROVIDER>

      --producer <PRODUCER>
          Project ID that owns the artifact.
      --kind <KIND>

      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --version <VERSION>

      --limit <COUNT>
          [default: 50]
      --cursor <CURSOR>
          Continuation cursor returned by the previous page.
  -h, --help
          Print help
```


## graphit hub publish

```text
Manage Hub artifacts.

Usage: graphit hub publish [OPTIONS] --kind <KIND> --version <VERSION> --provider <PROVIDER> --id <ID> --name <NAME>

Options:
      --kind <KIND>
          [possible values: skill, command, workflow, agent, rule, mcp, language, power]
      --provider <PROVIDER>
          Configured provider name.
      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --version <VERSION>

      --name <NAME>
          Name of the selected item.
      --description <DESCRIPTION>
          Description of the requested work.
      --tag <TAG>

      --author <AUTHOR>

      --dependency <KIND> <ID> <SELECTOR>
          Declare one dependency; repeat the three values for each dependency
      --clear-dependencies
          Publish an explicitly empty dependency list; omission preserves declarations
      --replace-sha256 <REPLACE_SHA256>
          Current archive SHA256 required to replace content at this concrete version
      --set-latest
          Make this concrete version the catalog's latest pointer
      --project-dir <DIRECTORY>
          Select a project directory; relative paths use the working directory.
  -h, --help
          Print help
```


## graphit hub publish-native

```text
Manage Hub artifacts.

Usage: graphit hub publish-native [OPTIONS] --kind <KIND> --version <VERSION> --provider <PROVIDER> --id <ID> --name <NAME>

Options:
      --kind <KIND>                [possible values: ast, knowledge]
      --provider <PROVIDER>        Configured provider name.
      --id <ID>                    Exact identifier returned by the corresponding list
                                   or search command.
      --version <VERSION>
      --name <NAME>                Name of the selected item.
      --description <DESCRIPTION>  Description of the requested work.
      --tag <TAG>
      --author <AUTHOR>
      --project-dir <DIRECTORY>    Select a project directory; relative paths use the
                                   working directory.
  -h, --help                       Print help
```


## graphit hub unpublish

```text
Retract remote catalog references while preserving payloads and local installations

Usage: graphit hub unpublish [OPTIONS] --kind <KIND> --provider <PROVIDER> --producer <PRODUCER> --id <ID>

Options:
      --provider <PROVIDER>
          Configured provider name.
      --producer <PRODUCER>
          Project ID that owns the artifact.
      --kind <KIND>
          [possible values: agent, ast, command, knowledge, language, mcp, power, rule,
          skill, workflow]
      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --version <VERSION>
          Literal artifact version; latest and release-prefix selection are not accepted
      --all-versions
          Retract every version observed in the current artifact catalog
      --expected-sha256 <EXPECTED_SHA256>
          Require this current version hash before changing the catalog
  -h, --help
          Print help
```


## graphit hub install-native

```text
Manage Hub artifacts.

Usage: graphit hub install-native [OPTIONS] --kind <KIND> --version <VERSION> --provider <PROVIDER> --producer <PRODUCER> --id <ID>

Options:
      --provider <PROVIDER>  Configured provider name.
      --producer <PRODUCER>  Project ID that owns the artifact.
      --kind <KIND>          [possible values: ast, knowledge]
      --id <ID>              Exact identifier returned by the corresponding list or
                             search command.
      --version <VERSION>
  -h, --help                 Print help
```


## graphit coordinator

```text
Publish the product Coordinator and PROJECT context hooks

Usage: graphit coordinator [OPTIONS] <COMMAND>

Commands:
  install  Install the Graphit Coordinator. [alias: sync]
  remove   Withdraw the owned Coordinator, its hooks and MCP selection
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit coordinator install

```text
Install the Graphit Coordinator.

Usage: graphit coordinator install [OPTIONS] --agent <AGENT>

Options:
      --agent <AGENT>           Native host schema for this publication [alias:
                                --binding]
      --source-dir <DIRECTORY>  Override the bundled versioned Graphit/Coordinator
                                artifact directory
  -h, --help                    Print help
```


## graphit coordinator remove

```text
Withdraw the owned Coordinator, its hooks and MCP selection

Usage: graphit coordinator remove [OPTIONS] --agent <AGENT>

Options:
      --agent <AGENT>  Native agent integration to configure. [alias: --binding]
  -h, --help           Print help
```


## graphit models

```text
Install or select local embedding models

Usage: graphit models [OPTIONS] <COMMAND>

Commands:
  install  Install a pinned GGUF model, preserving its existing task selection
  select   Select a captured installed embedding or rerank manifest
  help     Print this message or the help of the given subcommand(s)

Options:
      --timeout <TIMEOUT>  Maximum operation duration in seconds [default: 3600]
  -h, --help               Print help
```


## graphit models install

```text
Install a pinned GGUF model, preserving its existing task selection

Usage: graphit models install [OPTIONS] <MODEL>

Arguments:
  <MODEL>  [possible values: embeddinggemma2, bge-reranker-v2-m3]

Options:
      --file <FILE>        Install a previously prepared, digest-verified GGUF instead
                           of downloading it
      --timeout <TIMEOUT>  Maximum operation duration in seconds [default: 3600]
  -h, --help               Print help
```


## graphit models select

```text
Select a captured installed embedding or rerank manifest

Usage: graphit models select [OPTIONS] <MANIFEST_RELATIVE>

Arguments:
  <MANIFEST_RELATIVE>

Options:
      --timeout <TIMEOUT>  Maximum operation duration in seconds [default: 3600]
  -h, --help               Print help
```


## graphit task

```text
Read and mutate native Task/Session records through the active Source service

Usage: graphit task [OPTIONS] <COMMAND>

Commands:
  export                Export one Task with all pinned history and comments to JSON
  export-session        Export one Session with all pinned history and linked Task heads
  get-task              Read current Task state and explicitly requested snapshot-bound
                        history/comment pages
  catalogue             Read a cursor-pinned Task catalogue, including explicit dates
                        and dependencies
  search                Search current Task specifications and comments using textual,
                        semantic or hybrid retrieval
  query                 Read a finite projected Task or Session page from one native pin
  schema                Read the native Task query tables and permitted column types
  get-session           Read a Session and its immutable native history
  session-heads         Read native Session heads
  create-task           Create a native Task with acceptance and test checks, and
                        explicit optional calendar dates
  create-session        Create a native Session
  claim-task            Claim a Task; return the opaque participant fence only to this
                        caller
  claim-session         Claim a Session; return its opaque participant fence
  heartbeat-task        Renew a Task owner's actual lease and revision
  heartbeat-session     Renew a Session owner's actual lease and revision
  add-task-comment      Add a literal Markdown comment with the current participant
                        fence and an idempotency key
  progress-task         Record the current Task owner's progress
  checkpoint-session    Record the current Session owner's checkpoint
  release-task          Release a Task using its current participant fence
  release-session       Release a Session using its current participant fence
  revise-task           Revise Task specification; omitted fields stay, --clear-schedule
                        removes dates
  revise-session        Revise the coordinated Session specification; omitted fields
                        stay
  verify-task-check     Verify a current Task check with evidence
  supersede-task-check  Supersede a Task check, optionally creating a replacement
  update-task-links     Replace Task parent or dependency links with recorded reason
  set-task-flag         Set or clear the native Task flag with recorded reason
  set-task-commit       Associate or clear the full Git commit ID without changing Task
                        status/checks
  complete-task         Complete a Task after its actual gates pass
  cancel-task           Cancel an eligible Task with recorded reason and live fence when
                        owned
  complete-session      Complete a Session after its native Task gates pass
  cancel-session        Cancel a Session after explicitly resolving its Tasks; requires
                        revision CAS
  help                  Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit task export

```text
Export one Task with all pinned history and comments to JSON

Usage: graphit task export [OPTIONS] --output <FILE> --id <ID>

Options:
      --project-dir <DIRECTORY>  Exact native PROJECT locator; selection grants no
                                 authority
      --project-id <ID>          Exact project identifier; selection does not grant
                                 access.
      --id <ID>                  Exact identifier returned by the corresponding list or
                                 search command.
      --output <FILE>            Publish this JSON file only after every audit page has
                                 reached EOF
      --page-size <COUNT>        Records requested per audit page; every page is
                                 exported through EOF [default: 20]
  -h, --help                     Print help
```


## graphit task export-session

```text
Export one Session with all pinned history and linked Task heads

Usage: graphit task export-session [OPTIONS] --output <FILE> --id <ID>

Options:
      --project-dir <DIRECTORY>  Exact native PROJECT locator; selection grants no
                                 authority
      --project-id <ID>          Exact project identifier; selection does not grant
                                 access.
      --id <ID>                  Exact identifier returned by the corresponding list or
                                 search command.
      --output <FILE>            Publish this JSON file only after every audit page has
                                 reached EOF
      --page-size <COUNT>        Records requested per audit page; every page is
                                 exported through EOF [default: 20]
  -h, --help                     Print help
```


## graphit task get-task

```text
Read current Task state and explicitly requested snapshot-bound history/comment pages

Usage: graphit task get-task [OPTIONS] --id <ID>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Exact Task ULID
      --history-page-size <HISTORY_PAGE_SIZE>
          Request the history section with this positive page size
      --history-cursor <HISTORY_CURSOR>
          Opaque history continuation returned for this record
      --history-cursor-file <HISTORY_CURSOR_FILE>
          Read raw history cursor text from a regular bounded file
      --comments-page-size <COMMENTS_PAGE_SIZE>
          Request the comments section with this positive page size
      --comments-cursor <COMMENTS_CURSOR>

      --comments-cursor-file <COMMENTS_CURSOR_FILE>
          Read raw comments cursor text from a regular bounded file
  -h, --help
          Print help
```


## graphit task catalogue

```text
Read a cursor-pinned Task catalogue, including explicit dates and dependencies

Usage: graphit task catalogue [OPTIONS] --page-size <COUNT>

Options:
      --project-dir <DIRECTORY>    Exact native PROJECT locator; selection grants no
                                   authority
      --project-id <ID>            Exact project identifier; selection does not grant
                                   access.
      --status <STATUS>            [possible values: open, in-progress, completed,
                                   cancelled]
      --session-id <SESSION_ID>
      --flagged <FLAGGED>          Filter by the flag's exact boolean value [possible
                                   values: true, false]
      --blocked <BLOCKED>          [possible values: true, false]
      --query <QUERY>              Literal catalogue filter text
      --page-size <COUNT>          Positive number of rows in this page
      --cursor <CURSOR>            Opaque cursor returned by this exact operation (JSON
                                   object for catalogue/search, text for query)
      --cursor-file <CURSOR_FILE>  Read the opaque cursor from a regular bounded file;
                                   JSON object for catalogue/search, raw text for query
  -h, --help                       Print help
```


## graphit task search

```text
Search current Task specifications and comments using textual, semantic or hybrid
retrieval

Usage: graphit task search [OPTIONS] --text <TEXT> --page-size <COUNT> --top-k <COUNT>

Options:
      --project-dir <DIRECTORY>    Exact native PROJECT locator; selection grants no
                                   authority
      --project-id <ID>            Exact project identifier; selection does not grant
                                   access.
      --status <STATUS>            [possible values: open, in-progress, completed,
                                   cancelled]
      --session-id <SESSION_ID>
      --flagged <FLAGGED>          Filter by the flag's exact boolean value [possible
                                   values: true, false]
      --blocked <BLOCKED>          [possible values: true, false]
      --mode <MODE>                [default: fts] [possible values: fts, semantic,
                                   hybrid]
      --text <TEXT>                Literal search text
      --page-size <COUNT>          Positive number of rows in this page
      --top-k <COUNT>              Positive maximum number of results for the complete
                                   query
      --cursor <CURSOR>            Opaque cursor returned by this exact operation (JSON
                                   object for catalogue/search, text for query)
      --cursor-file <CURSOR_FILE>  Read the opaque cursor from a regular bounded file;
                                   JSON object for catalogue/search, raw text for query
  -h, --help                       Print help
```


## graphit task query

```text
Read a finite projected Task or Session page from one native pin

Usage: graphit task query [OPTIONS] --table <TABLE> --column <COLUMNS> --page-size <COUNT> --top-k <COUNT>

Options:
      --project-dir <DIRECTORY>    Exact native PROJECT locator; selection grants no
                                   authority
      --project-id <ID>            Exact project identifier; selection does not grant
                                   access.
      --table <TABLE>              [possible values: tasks, sessions]
      --id <IDS>                   Exact IDs; repeat the flag or separate values by
                                   commas
      --empty-ids                  Select an explicitly empty ID set (zero results),
                                   rather than omitting the ID filter
      --status <STATUS>            [possible values: open, in-progress, completed,
                                   cancelled]
      --session-id <SESSION_ID>    Task table only; a Session query cannot filter by
                                   session ID
      --column <COLUMNS>           Projected columns in output order; repeat or use
                                   commas [possible values: id, status, title, revision,
                                   created-at, updated-at, session-id, parent-id,
                                   task-type, priority, flagged]
      --page-size <COUNT>          Positive number of rows in this page
      --top-k <COUNT>              Positive maximum number of results for the complete
                                   query
      --cursor <CURSOR>            Opaque cursor returned by this exact operation (JSON
                                   object for catalogue/search, text for query)
      --cursor-file <CURSOR_FILE>  Read the opaque cursor from a regular bounded file;
                                   JSON object for catalogue/search, raw text for query
  -h, --help                       Print help
```


## graphit task schema

```text
Read the native Task query tables and permitted column types

Usage: graphit task schema [OPTIONS]

Options:
      --project-dir <DIRECTORY>  Exact native PROJECT locator; selection grants no
                                 authority
      --project-id <ID>          Exact project identifier; selection does not grant
                                 access.
  -h, --help                     Print help
```


## graphit task get-session

```text
Read a Session and its immutable native history

Usage: graphit task get-session [OPTIONS] --id <ID>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --history-page-size <HISTORY_PAGE_SIZE>
          Request the history section with this positive page size
      --history-cursor <HISTORY_CURSOR>
          Opaque history continuation returned for this record
      --history-cursor-file <HISTORY_CURSOR_FILE>
          Read raw history cursor text from a regular bounded file
  -h, --help
          Print help
```


## graphit task session-heads

```text
Read native Session heads

Usage: graphit task session-heads [OPTIONS]

Options:
      --project-dir <DIRECTORY>  Exact native PROJECT locator; selection grants no
                                 authority
      --project-id <ID>          Exact project identifier; selection does not grant
                                 access.
  -h, --help                     Print help
```


## graphit task create-task

```text
Create a native Task with acceptance and test checks, and explicit optional calendar
dates

Usage: graphit task create-task [OPTIONS] --idempotency-key <IDEMPOTENCY_KEY> --title <TITLE> --description <DESCRIPTION> --task-type <TASK_TYPE> --session-id <SESSION_ID> --priority <PRIORITY>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --idempotency-key <IDEMPOTENCY_KEY>
          Stable key identifying this creation or comment request
      --title <TITLE>
          Literal title; omit during revision to preserve it
      --description <DESCRIPTION>
          Literal description; omit during revision to preserve it
      --task-type <TASK_TYPE>
          Task classification
      --session-id <SESSION_ID>

      --priority <PRIORITY>
          Priority from 0 through 4
      --parent-id <PARENT_ID>
          Parent Task ULID
      --depends-on <DEPENDS_ON>
          Dependency Task ULIDs; repeat or separate by commas
      --commit <COMMIT>
          Full lowercase hexadecimal Git commit ID (40 or 64 characters)
      --start-date <START_DATE>
          Explicit calendar date in YYYY-MM-DD format
      --end-date <END_DATE>
          Explicit calendar date in YYYY-MM-DD format
      --due-date <DUE_DATE>
          Explicit calendar date in YYYY-MM-DD format
      --check <KIND> <TEXT>
          Add a check; repeat with acceptance or test: --check acceptance 'Expected
          behavior'
      --check-kind <CHECK_KINDS>
          Check categories paired by occurrence with --check-text (new checks when
          revising) [possible values: acceptance, test]
      --check-text <CHECK_TEXTS>
          Literal check text paired by occurrence with --check-kind
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task create-session

```text
Create a native Session

Usage: graphit task create-session [OPTIONS] --idempotency-key <IDEMPOTENCY_KEY> --title <TITLE> --description <DESCRIPTION> --strategy <STRATEGY>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --idempotency-key <IDEMPOTENCY_KEY>
          Stable key identifying this creation or comment request
      --title <TITLE>
          Literal title; omit during revision to preserve it
      --description <DESCRIPTION>
          Literal description; omit during revision to preserve it
      --strategy <STRATEGY>
          Literal Session strategy
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task claim-task

```text
Claim a Task; return the opaque participant fence only to this caller

Usage: graphit task claim-task [OPTIONS] --lease-ms <LEASE_MS> --id <ID>

Options:
      --project-dir <DIRECTORY>  Exact native PROJECT locator; selection grants no
                                 authority
      --project-id <ID>          Exact project identifier; selection does not grant
                                 access.
      --id <ID>                  Exact identifier returned by the corresponding list or
                                 search command.
      --lease-ms <LEASE_MS>      Positive claim lifetime in milliseconds
  -h, --help                     Print help
```


## graphit task claim-session

```text
Claim a Session; return its opaque participant fence

Usage: graphit task claim-session [OPTIONS] --lease-ms <LEASE_MS> --id <ID>

Options:
      --project-dir <DIRECTORY>  Exact native PROJECT locator; selection grants no
                                 authority
      --project-id <ID>          Exact project identifier; selection does not grant
                                 access.
      --id <ID>                  Exact identifier returned by the corresponding list or
                                 search command.
      --lease-ms <LEASE_MS>      Positive claim lifetime in milliseconds
  -h, --help                     Print help
```


## graphit task heartbeat-task

```text
Renew a Task owner's actual lease and revision

Usage: graphit task heartbeat-task [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --lease-ms <LEASE_MS> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Task or Session ULID selected by this command
      --claim-token <CLAIM_TOKEN>
          Current opaque participant fence; never an actor selector
      --expected-revision <REVISION>
          Revision returned with the current claim
      --lease-ms <LEASE_MS>
          Positive claim lifetime in milliseconds
  -h, --help
          Print help
```


## graphit task heartbeat-session

```text
Renew a Session owner's actual lease and revision

Usage: graphit task heartbeat-session [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --lease-ms <LEASE_MS> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Task or Session ULID selected by this command
      --claim-token <CLAIM_TOKEN>
          Current opaque participant fence; never an actor selector
      --expected-revision <REVISION>
          Revision returned with the current claim
      --lease-ms <LEASE_MS>
          Positive claim lifetime in milliseconds
  -h, --help
          Print help
```


## graphit task add-task-comment

```text
Add a literal Markdown comment with the current participant fence and an idempotency key

Usage: graphit task add-task-comment [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --lease-ms <LEASE_MS> --idempotency-key <IDEMPOTENCY_KEY> --kind <KIND> --body <BODY> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Task or Session ULID selected by this command
      --claim-token <CLAIM_TOKEN>
          Current opaque participant fence; never an actor selector
      --expected-revision <REVISION>
          Revision returned with the current claim
      --lease-ms <LEASE_MS>
          Positive claim lifetime in milliseconds
      --idempotency-key <IDEMPOTENCY_KEY>
          Stable key identifying this creation or comment request
      --kind <KIND>
          Comment category [possible values: note, decision, problem, lesson, knowledge]
      --body <BODY>
          Literal Markdown comment body
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task progress-task

```text
Record the current Task owner's progress

Usage: graphit task progress-task [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --lease-ms <LEASE_MS> --summary <SUMMARY> --next-step <NEXT_STEP> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Task or Session ULID selected by this command
      --claim-token <CLAIM_TOKEN>
          Current opaque participant fence; never an actor selector
      --expected-revision <REVISION>
          Revision returned with the current claim
      --lease-ms <LEASE_MS>
          Positive claim lifetime in milliseconds
      --summary <SUMMARY>
          Literal progress or release summary
      --next-step <NEXT_STEP>
          Literal next action
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task checkpoint-session

```text
Record the current Session owner's checkpoint

Usage: graphit task checkpoint-session [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --lease-ms <LEASE_MS> --summary <SUMMARY> --next-step <NEXT_STEP> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Task or Session ULID selected by this command
      --claim-token <CLAIM_TOKEN>
          Current opaque participant fence; never an actor selector
      --expected-revision <REVISION>
          Revision returned with the current claim
      --lease-ms <LEASE_MS>
          Positive claim lifetime in milliseconds
      --summary <SUMMARY>
          Literal progress or release summary
      --next-step <NEXT_STEP>
          Literal next action
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task release-task

```text
Release a Task using its current participant fence

Usage: graphit task release-task [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --summary <SUMMARY> --next-step <NEXT_STEP> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Task or Session ULID selected by this command
      --claim-token <CLAIM_TOKEN>
          Current opaque participant fence; never an actor selector
      --expected-revision <REVISION>
          Revision returned with the current claim
      --summary <SUMMARY>
          Literal progress or release summary
      --next-step <NEXT_STEP>
          Literal next action
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task release-session

```text
Release a Session using its current participant fence

Usage: graphit task release-session [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --summary <SUMMARY> --next-step <NEXT_STEP> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Task or Session ULID selected by this command
      --claim-token <CLAIM_TOKEN>
          Current opaque participant fence; never an actor selector
      --expected-revision <REVISION>
          Revision returned with the current claim
      --summary <SUMMARY>
          Literal progress or release summary
      --next-step <NEXT_STEP>
          Literal next action
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task revise-task

```text
Revise Task specification; omitted fields stay, --clear-schedule removes dates

Usage: graphit task revise-task [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --lease-ms <LEASE_MS> --reason <REASON> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Task or Session ULID selected by this command
      --claim-token <CLAIM_TOKEN>
          Current opaque participant fence; never an actor selector
      --expected-revision <REVISION>
          Revision returned with the current claim
      --lease-ms <LEASE_MS>
          Positive claim lifetime in milliseconds
      --reason <REASON>
          Recorded reason for this change
      --title <TITLE>
          Literal title; omit during revision to preserve it
      --description <DESCRIPTION>
          Literal description; omit during revision to preserve it
      --task-type <TASK_TYPE>
          Task classification
      --priority <PRIORITY>
          Priority from 0 through 4
      --start-date <START_DATE>
          Explicit calendar date in YYYY-MM-DD format
      --end-date <END_DATE>
          Explicit calendar date in YYYY-MM-DD format
      --due-date <DUE_DATE>
          Explicit calendar date in YYYY-MM-DD format
      --clear-schedule
          Remove all scheduled dates; omission preserves the current schedule
      --check <KIND> <TEXT>
          Add a check; repeat with acceptance or test: --check acceptance 'Expected
          behavior'
      --check-kind <CHECK_KINDS>
          Check categories paired by occurrence with --check-text (new checks when
          revising) [possible values: acceptance, test]
      --check-text <CHECK_TEXTS>
          Literal check text paired by occurrence with --check-kind
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task revise-session

```text
Revise the coordinated Session specification; omitted fields stay

Usage: graphit task revise-session [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --lease-ms <LEASE_MS> --reason <REASON> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Task or Session ULID selected by this command
      --claim-token <CLAIM_TOKEN>
          Current opaque participant fence; never an actor selector
      --expected-revision <REVISION>
          Revision returned with the current claim
      --lease-ms <LEASE_MS>
          Positive claim lifetime in milliseconds
      --reason <REASON>
          Recorded reason for this change
      --title <TITLE>
          Literal title; omit during revision to preserve it
      --description <DESCRIPTION>
          Literal description; omit during revision to preserve it
      --strategy <STRATEGY>
          Literal Session strategy
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task verify-task-check

```text
Verify a current Task check with evidence

Usage: graphit task verify-task-check [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --lease-ms <LEASE_MS> --check-id <CHECK_ID> --passed <PASSED> --evidence <EVIDENCE> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Task or Session ULID selected by this command
      --claim-token <CLAIM_TOKEN>
          Current opaque participant fence; never an actor selector
      --expected-revision <REVISION>
          Revision returned with the current claim
      --lease-ms <LEASE_MS>
          Positive claim lifetime in milliseconds
      --check-id <CHECK_ID>
          Current check ULID
      --passed <PASSED>
          Explicit verification result: true or false [possible values: true, false]
      --evidence <EVIDENCE>
          Evidence supporting this check result
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task supersede-task-check

```text
Supersede a Task check, optionally creating a replacement

Usage: graphit task supersede-task-check [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --lease-ms <LEASE_MS> --check-id <CHECK_ID> --reason <REASON> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Task or Session ULID selected by this command
      --claim-token <CLAIM_TOKEN>
          Current opaque participant fence; never an actor selector
      --expected-revision <REVISION>
          Revision returned with the current claim
      --lease-ms <LEASE_MS>
          Positive claim lifetime in milliseconds
      --check-id <CHECK_ID>
          Current check ULID
      --reason <REASON>
          Recorded reason for this change
      --replacement-text <REPLACEMENT_TEXT>
          Create a replacement check with this literal text
      --replacement-kind <REPLACEMENT_KIND>
          Optional replacement category; omission preserves the superseded category
          [possible values: acceptance, test]
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task update-task-links

```text
Replace Task parent or dependency links with recorded reason

Usage: graphit task update-task-links [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --lease-ms <LEASE_MS> --reason <REASON> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Task or Session ULID selected by this command
      --claim-token <CLAIM_TOKEN>
          Current opaque participant fence; never an actor selector
      --expected-revision <REVISION>
          Revision returned with the current claim
      --lease-ms <LEASE_MS>
          Positive claim lifetime in milliseconds
      --reason <REASON>
          Recorded reason for this change
      --parent-id <PARENT_ID>
          Parent Task ULID
      --clear-parent
          Remove the parent link
      --depends-on <DEPENDS_ON>
          Dependency Task ULIDs; repeat or separate by commas
      --clear-dependencies
          Replace dependencies with an empty list
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task set-task-flag

```text
Set or clear the native Task flag with recorded reason

Usage: graphit task set-task-flag [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --lease-ms <LEASE_MS> --flagged <FLAGGED> --reason <REASON> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Task or Session ULID selected by this command
      --claim-token <CLAIM_TOKEN>
          Current opaque participant fence; never an actor selector
      --expected-revision <REVISION>
          Revision returned with the current claim
      --lease-ms <LEASE_MS>
          Positive claim lifetime in milliseconds
      --flagged <FLAGGED>
          Explicit flag value: true or false [possible values: true, false]
      --reason <REASON>
          Recorded reason for this change
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task set-task-commit

```text
Associate or clear the full Git commit ID without changing Task status/checks

Usage: graphit task set-task-commit [OPTIONS] --reason <REASON> --id <ID> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --expected-revision <REVISION>
          Current record revision; a mismatch rejects the change.
      --claim-token <CLAIM_TOKEN>

      --claim-revision <CLAIM_REVISION>
          Revision of an optional claim (separate from set-task-commit's record CAS)
      --commit <COMMIT>
          Full lowercase hexadecimal Git commit ID (40 or 64 characters)
      --clear-commit
          Explicitly remove the commit association
      --reason <REASON>
          Recorded reason for this change
  -h, --help
          Print help
```


## graphit task complete-task

```text
Complete a Task after its actual gates pass

Usage: graphit task complete-task [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Task or Session ULID selected by this command
      --claim-token <CLAIM_TOKEN>
          Current opaque participant fence; never an actor selector
      --expected-revision <REVISION>
          Revision returned with the current claim
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task cancel-task

```text
Cancel an eligible Task with recorded reason and live fence when owned

Usage: graphit task cancel-task [OPTIONS] --reason <REASON> --id <ID>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --claim-token <CLAIM_TOKEN>

      --claim-revision <CLAIM_REVISION>
          Revision of an optional claim (separate from set-task-commit's record CAS)
      --reason <REASON>
          Recorded reason for this change
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task complete-session

```text
Complete a Session after its native Task gates pass

Usage: graphit task complete-session [OPTIONS] --id <ID> --claim-token <CLAIM_TOKEN> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Task or Session ULID selected by this command
      --claim-token <CLAIM_TOKEN>
          Current opaque participant fence; never an actor selector
      --expected-revision <REVISION>
          Revision returned with the current claim
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit task cancel-session

```text
Cancel a Session after explicitly resolving its Tasks; requires revision CAS

Usage: graphit task cancel-session [OPTIONS] --reason <REASON> --id <ID> --expected-revision <REVISION>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --id <ID>
          Exact identifier returned by the corresponding list or search command.
      --expected-revision <REVISION>
          Current record revision; a mismatch rejects the change.
      --claim-token <CLAIM_TOKEN>
          Live private coordinator token; omit for an open Session or a terminal retry
      --reason <REASON>
          Literal reason retained in immutable history
      --reference <TYPE> <ID> <RELATION>
          Add a reference; repeat for more records: --reference task ID supports
      --reference-type <REFERENCE_TYPES>
          Replace authored references. Pair repeated type/id/relation flags by
          occurrence [possible values: task, session, memory, knowledge]
      --reference-id <REFERENCE_IDS>

      --reference-relation <REFERENCE_RELATIONS>

      --reference-scope <REFERENCE_SCOPES>
          Omit entirely or repeat once per reference; an empty value leaves that entry's
          scope absent
      --reference-scope-id <REFERENCE_SCOPE_IDS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-context <REFERENCE_CONTEXTS>
          Omit entirely or repeat once per reference; empty means absent
      --reference-field <REFERENCE_FIELDS>
          Omit entirely or repeat once per reference; empty text is a literal empty
          field
      --reference-field-absent <REFERENCE_FIELDS_ABSENT>
          Omit fields at these 1-based reference positions; repeat or use commas. The
          corresponding field slot must be empty
      --clear-references
          Explicitly replace authored references with an empty list
  -h, --help
          Print help
```


## graphit references

```text
Read registered incoming/outgoing relationships without opening foreign targets

Usage: graphit references [OPTIONS] <COMMAND>

Commands:
  task            Query outgoing and incoming edges of a Task or Session from the Task
                  corpus
  task-backlinks  Find Task/Session sources that reference any logical target; never
                  open that target
  knowledge       Query outgoing document edges and incoming logical-target edges in
                  Knowledge
  help            Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit references task

```text
Query outgoing and incoming edges of a Task or Session from the Task corpus

Usage: graphit references task [OPTIONS]

Options:
      --project-dir <DIRECTORY>   Exact native PROJECT locator; selection grants no
                                  authority
      --project-id <ID>           Exact project identifier; selection does not grant
                                  access.
      --task <ID>                 Exact Task ID; choose either --task or --session
      --session <ID>              Exact Session ID; choose either --task or --session
      --page-size <COUNT>         Maximum references per page [default: 20]
      --outgoing-cursor <CURSOR>  Exact JSON cursor for the next outgoing page; quote it
                                  as one argument
      --incoming-cursor <CURSOR>  Exact JSON cursor for the next incoming page; quote it
                                  as one argument
  -h, --help                      Print help
```


## graphit references task-backlinks

```text
Find Task/Session sources that reference any logical target; never open that target

Usage: graphit references task-backlinks [OPTIONS] --target-kind <TARGET_KIND> --target-id <ID>

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --target-id <ID>
          Exact identifier of the logical target
      --target-kind <TARGET_KIND>
          Kind of the logical target [possible values: task, session, memory, knowledge]
      --target-project-id <ID>
          Project that owns the target; mutually exclusive with --target-user-id
      --target-user-id <ID>
          User that owns the target; mutually exclusive with --target-project-id
      --target-context <TARGET_CONTEXT>
          Installed context of the logical target, when applicable
      --page-size <COUNT>
          Maximum references per page [default: 20]
      --cursor <CURSOR>
          Exact JSON continuation object returned by the previous page
  -h, --help
          Print help
```


## graphit references knowledge

```text
Query outgoing document edges and incoming logical-target edges in Knowledge

Usage: graphit references knowledge [OPTIONS]

Options:
      --project-dir <DIRECTORY>
          Exact native PROJECT locator; selection grants no authority
      --project-id <ID>
          Exact project identifier; selection does not grant access.
      --outgoing <OUTGOING>
          Source document slug for outgoing edges
      --target-id <ID>
          Logical target for incoming edges; it does not authorize target content
      --target-kind <TARGET_KIND>
          Kind of the logical target [possible values: task, session, memory, knowledge]
      --target-project-id <ID>
          Project that owns the target; mutually exclusive with --target-user-id
      --target-user-id <ID>
          User that owns the target; mutually exclusive with --target-project-id
      --target-context <TARGET_CONTEXT>
          Installed context of the logical target, when applicable
      --page-size <COUNT>
          Maximum references per page [default: 20]
      --outgoing-cursor <CURSOR>
          Exact JSON cursor for the next outgoing page; quote it as one argument
      --incoming-cursor <CURSOR>
          Exact JSON cursor for the next incoming page; quote it as one argument
  -h, --help
          Print help
```


## graphit index

```text
Diagnose indexing or request a coalesced rescan from the existing supervisor

Usage: graphit index [OPTIONS] <COMMAND>

Commands:
  status  Show index availability, processing phases, last completion, and errors
  run     Request a project rescan; acceptance does not mean indexing is complete
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help


Examples:
  graphit index status
  graphit index run --module knowledge --wait-ms 5000

A request may be coalesced with existing project work. Ctrl+C stops waiting only;
shared indexing continues. Use status to inspect its outcome.
```


## graphit index status

```text
Show index availability, processing phases, last completion, and errors

Usage: graphit index status [OPTIONS]

Options:
      --project-dir <DIRECTORY>  Exact native PROJECT locator; selection grants no
                                 authority
      --project-id <ID>          Exact project identifier; selection does not grant
                                 access.
  -h, --help                     Print help
```


## graphit index run

```text
Request a project rescan; acceptance does not mean indexing is complete

Usage: graphit index run [OPTIONS]

Options:
      --project-dir <DIRECTORY>  Exact native PROJECT locator; selection grants no
                                 authority
      --project-id <ID>          Exact project identifier; selection does not grant
                                 access.
      --wait-ms <MILLISECONDS>   Wait up to this many milliseconds; 0 returns after
                                 acceptance. Timeout exits 3 [default: 0]
      --module <MODULE>          Request this module; the coalesced supervisor processes
                                 the whole project [possible values: ast, knowledge]
  -h, --help                     Print help


Ctrl+C stops waiting only; shared indexing continues. A timeout exits 3.
Use graphit index status to inspect the later outcome.
```


## graphit git

```text
Configure Git hooks and bind native Task/Memory state to commits and merges

Usage: graphit git [OPTIONS] <COMMAND>

Commands:
  install   Snapshot the current state, then install the native Git notifications
  sync      Reconcile configured commands and Task/Memory events without taking a new
            snapshot
  remove    Remove only this brand's native Git hook registrations
  snapshot  Bind the current record state to HEAD, or retry a failed commit hook
  restore   Retry restoring the record snapshot for the checked-out commit
  hook      Dispatch current configured commands and Task/Memory notifications for a Git
            event
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit git install

```text
Snapshot the current state, then install the native Git notifications

Usage: graphit git install [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit git sync

```text
Reconcile configured commands and Task/Memory events without taking a new snapshot

Usage: graphit git sync [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit git remove

```text
Remove only this brand's native Git hook registrations

Usage: graphit git remove [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit git snapshot

```text
Bind the current record state to HEAD, or retry a failed commit hook

Usage: graphit git snapshot [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit git restore

```text
Retry restoring the record snapshot for the checked-out commit

Usage: graphit git restore [OPTIONS] [PREVIOUS_COMMIT]

Arguments:
  [PREVIOUS_COMMIT]

Options:
  -h, --help  Print help
```


## graphit git hook

```text
Dispatch current configured commands and Task/Memory notifications for a Git event

Usage: graphit git hook [OPTIONS] <EVENT> [ARGUMENTS]...

Arguments:
  <EVENT>
  [ARGUMENTS]...

Options:
  -h, --help  Print help
```


## graphit ast

```text
Inspect source from the authenticated local service

Usage: graphit ast [OPTIONS] <COMMAND>

Commands:
  source        Read indexed source or the physical file in the retained project version
  export        Export a published PROJECT graph or an installed AST context
  imports       List exact installed Native AST contexts
  search        Search the current PROJECT or an exact installed AST context
  query         Query the current PROJECT or an explicitly selected installed graph
  schema        Show graph labels, columns and relationships
  sample        Show a bounded sample of graph nodes and edges
  neighborhood  Page over the relationships of one node
  help          Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit ast source

```text
Read indexed source or the physical file in the retained project version

Usage: graphit ast source [OPTIONS] <PATH>

Arguments:
  <PATH>

Options:
      --entity <ENTITY>

      --entity-key <ENTITY_KEY>
          Exact primary-key value, requiring entity_type and excluding entity name
      --entity-key-type <ENTITY_KEY_TYPE>
          Type of --entity-key (default: string); int64 must be explicit [possible
          values: string, int64]
      --entity-type <ENTITY_TYPE>

      --head <HEAD>
          Return the first N lines.
      --tail <TAIL>
          Return the last N lines.
      --start <START_LINE>
          First line to include (1-based).
      --end <END_LINE>
          Last line to include (inclusive).
      --pattern <PATTERN>
          Text to match in the selected source.
      --regex
          Treat the pattern as a regular expression.
      --before <BEFORE>
          Context lines before each match.
      --after <AFTER>
          Context lines after each match.
      --line-numbers
          Include original line numbers.
      --context <CONTEXT>

      --project-dir <DIRECTORY>
          Select a project directory; relative paths use the working directory.
  -h, --help
          Print help
```


## graphit ast export

```text
Export a published PROJECT graph or an installed AST context

Usage: graphit ast export [OPTIONS] --output <OUTPUT>

Options:
      --output <OUTPUT>          Destination package file, resolved from this
                                 invocation's working directory
      --project-dir <DIRECTORY>  Select a retained local project; otherwise use the
                                 current selection
      --context <CONTEXT>        Repackage an exact installed AST, independently of
                                 PROJECT selection
  -h, --help                     Print help
```


## graphit ast imports

```text
List exact installed Native AST contexts

Usage: graphit ast imports [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit ast search

```text
Search the current PROJECT or an exact installed AST context

Usage: graphit ast search [OPTIONS] <QUERY>

Arguments:
  <QUERY>

Options:
      --context <CONTEXT>
      --project-dir <DIRECTORY>  Select a project directory; relative paths use the
                                 working directory.
      --mode <MODE>              [default: semantic] [possible values: fts, hybrid,
                                 semantic, vector]
      --top-k <COUNT>            [alias: --k]
      --metric <METRIC>          [default: cosine] [possible values: l2, cosine, dot]
      --rerank [<RERANK>]        Override search.rerank for this query; omitted uses
                                 configuration (default false) [possible values: true,
                                 false]
  -h, --help                     Print help
```


## graphit ast query

```text
Query the current PROJECT or an explicitly selected installed graph

Usage: graphit ast query [OPTIONS] <QUERY>

Arguments:
  <QUERY>

Options:
      --context <CONTEXT>
      --project-dir <DIRECTORY>   Select a project directory; relative paths use the
                                  working directory.
      --param <NAME=VALUE>        Named typed Cypher value; repeat for each name (VALUE
                                  is a JSON value)
      --param-string <NAME=TEXT>  Named literal Cypher string; repeat for each name,
                                  without JSON quoting
  -h, --help                      Print help
```


## graphit ast schema

```text
Show graph labels, columns and relationships

Usage: graphit ast schema [OPTIONS]

Options:
      --context <CONTEXT>
      --project-dir <DIRECTORY>  Select a project directory; relative paths use the
                                 working directory.
  -h, --help                     Print help
```


## graphit ast sample

```text
Show a bounded sample of graph nodes and edges

Usage: graphit ast sample [OPTIONS]

Options:
      --context <CONTEXT>
      --project-dir <DIRECTORY>  Select a project directory; relative paths use the
                                 working directory.
  -h, --help                     Print help
```


## graphit ast neighborhood

```text
Page over the relationships of one node

Usage: graphit ast neighborhood [OPTIONS] --anchor-label <ANCHOR_LABEL> --anchor-key <ANCHOR_KEY>

Options:
      --context <CONTEXT>

      --project-dir <DIRECTORY>
          Select a project directory; relative paths use the working directory.
      --anchor-label <ANCHOR_LABEL>

      --anchor-key <ANCHOR_KEY>

      --key-type <KEY_TYPE>
          [default: string] [possible values: string, int64]
      --relationship <RELATIONSHIP>

      --other-label <OTHER_LABEL>

      --direction <DIRECTION>
          [possible values: outgoing, incoming]
      --limit <COUNT>
          [default: 100]
      --cursor <CURSOR>
          Exact opaque JSON cursor returned by the preceding neighborhood page
      --cursor-file <CURSOR_FILE>
          Read that unchanged cursor object from a UTF-8 JSON file
  -h, --help
          Print help
```


## graphit mcp

```text
Serve the MCP client transport over standard input/output

Usage: graphit mcp [OPTIONS]

Options:
      --role                     Forward only the transient credential of an owned
                                 managed-role run
      --live-user                Use only the process-bound Web credentials of an owned
                                 Live turn
      --project-dir <DIRECTORY>  Default PROJECT locator for the authenticated Root
                                 Source consumer
  -h, --help                     Print help
```


## graphit daemon

```text
Run or manage the authenticated local Source service

Usage: graphit daemon [OPTIONS] [COMMAND]

Commands:
  foreground  Run until SIGINT or SIGTERM, then join jobs and remove owned discovery/key
  start       Start the user's daemon for this installation
  stop        Stop the user manager before draining the daemon's retained owners
  restart     Restart the registered service with its native user manager
  status      Observe the authenticated daemon or its native manager
  install     Register the service; login startup is enabled only with --login
  login       Change startup at user login explicitly
  uninstall   Stop and remove the service and its login registration
  help        Print this message or the help of the given subcommand(s)

Options:
      --managed  Run the registered daemon from the installation's maintained inventory
  -h, --help     Print help
```


## graphit daemon foreground

```text
Run until SIGINT or SIGTERM, then join jobs and remove owned discovery/key

Usage: graphit daemon foreground [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit daemon start

```text
Start the user's daemon for this installation

Usage: graphit daemon start [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit daemon stop

```text
Stop the user manager before draining the daemon's retained owners

Usage: graphit daemon stop [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit daemon restart

```text
Restart the registered service with its native user manager

Usage: graphit daemon restart [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit daemon status

```text
Observe the authenticated daemon or its native manager

Usage: graphit daemon status [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit daemon install

```text
Register the service; login startup is enabled only with --login

Usage: graphit daemon install [OPTIONS]

Options:
      --login
  -h, --help   Print help
```


## graphit daemon login

```text
Change startup at user login explicitly

Usage: graphit daemon login [OPTIONS] <COMMAND>

Commands:
  enable   Enable the local daemon.
  disable  Disable the local daemon.
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```


## graphit daemon login enable

```text
Enable the local daemon.

Usage: graphit daemon login enable [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit daemon login disable

```text
Disable the local daemon.

Usage: graphit daemon login disable [OPTIONS]

Options:
  -h, --help  Print help
```


## graphit daemon uninstall

```text
Stop and remove the service and its login registration

Usage: graphit daemon uninstall [OPTIONS]

Options:
  -h, --help  Print help
```

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
`show` matches an exact ID or unique exact name. Unknown and unauthorized IDs
share a single not-found-or-denied result.

Each entry includes `description` and the `cluster` map of label keys to values.
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

`--scope user` opens the installation's private USER Memory without login in
local mode. An active Broker profile requires a verified Native subject. USER
admission rejects PROJECT selectors and caller-supplied subjects. PROJECT is the
default. Text is the default terminal output; add `--json` for automation.

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
