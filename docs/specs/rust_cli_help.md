---
title: Rust CLI help
description: Public help, compiled version and native package commands through one Clap parser.
type: specification
updated: 2026-11-10
references:
  - target: {type: task, id: tsk-6db1}
    relation: implements
  - target: {type: task, id: tsk-75a5}
    relation: uses
  - target: {type: task, id: tsk-d4e8}
    relation: uses
---

# Rust CLI help

Run `graphit --help`, `graphit -h` or `graphit help` to see the current executable's
commands. Help uses the compiled Brand and the maintained Graphit description.
It prints to stdout and exits 0 without configuration, credentials or project
setup. An empty invocation prints Clap's missing-command help on stderr and
exits 2. Runtime metadata variables do not change the compiled name or version.

The current parser exposes public help, [compiled version](rust_cli_version.md)
and [native packages](../development/native-packages.md), plus the Linux
[model commands](rust_cli_models.md). `graphit package --help`
lists export, inspect and import. Commands appear here when their Rust entrypoints
are connected; library qualification alone does not register an executable command.

Clap 4.6.7 parses native arguments once in
[cli.rs](../../crates/graphit-app/src/cli.rs). Package arguments use its nested
`Args` and `Subcommand` producers; package execution receives parsed arguments,
the actual configuration and Cancellation. It has no second parser. Configuration
and signal handling start only for a valid operation. Model arguments are also
passed directly to their typed producer. Help uses one recursively applied Clap presentation with an 88-column width,
consistent Usage/Commands/Options sections and no ANSI color. Native help
precedence and the help subcommand remain available.

`--version` is exclusive with an operation and preserves the exact compiled
version line. Invalid or conflicting arguments exit 2 with Clap diagnostics.
Version metadata and output errors exit 1; partial writes are not retried.
Failure to print a help or error response also exits 1. The manual argument parser,
its obsolete parsing unit and fixed-size help rendering plan were retired.

## Terminal presentation

Finite public commands print readable text by default, even through a pipe.
Sentence-case labels and two-space indentation express hierarchy. Global
`--json` selects structured output for scripts, before or after subcommands.
Native service DTOs and protocols retain their contracts. Raw AST source and
Ask answers remain literal text. Knowledge pages and Hub content print their
text by default and expose native records with `--json`. MCP, internal hooks
and model-service protocol streams bypass terminal presentation.

`--agent` is the preferred public native-agent option; `--binding` remains an
alias where applicable. AST search uses `--top-k`, retaining `--k` as an alias.
Repeated `--check KIND TEXT` and `--reference TYPE ID RELATION` simplify Task
inputs. Previous parallel flags remain for compatibility and advanced metadata.
`-V` is the short form of the exclusive compiled `--version` operation.

The qualification below records its original scope, not validation of later
presentation changes. Current tests inventory every public help path and both
positions of the global output selector, then exercise real setup/config results.

## Functional command presentation

`projects list/show`, `index status/run`, `knowledge list/lint`, Memory record
commands, Task audit exports and references use the same global `--json`,
88-column help layout and human-first output as existing commands. Help works
without setup, including with `--json`. Required values, file paths, counts,
revisions and cursors have consistent metavariables and descriptions.

Memory creation requires one of `--body` or `--body-file`; `--body-file -` reads
UTF-8 from standard input. Update requires at least one field change. These
requirements, missing Knowledge incoming-reference scope and USER Memory combined
with PROJECT selectors are usage errors (exit 2), rejected before configuration
or service startup. Native validation and authorization still apply afterward.

`index run` distinguishes accepted work, processed requests, processing failure
and wait timeout. Acceptance never announces completion. `--wait-ms` uses
milliseconds; 0 returns after acceptance, and expiration exits 3 while shared
processing may continue. Ctrl+C stops waiting only. `index status` reports current
observations. `--module ast|knowledge` expresses the requested module while the
existing supervisor may coalesce work for the whole project.

Knowledge absence, an empty published corpus, clean lint and reported issues
have different text. Lint exits 3 without a corpus, 1 for findings and 0 for a
clean or empty corpus. Successful Task/Session exports confirm the JSON audit
file destination and exact record ID; no success is presented before atomic
publication. JSON mode preserves each service/result contract.

## Qualification

The affected Linux executable checks are:

```sh
cargo xtask cargo test --locked --offline -p graphit-app --bin graphit
cargo xtask cargo test --locked --offline -p graphit-app --test integration -- version_cli:: help_cli:: package::
cargo xtask cargo clippy --locked --offline -p graphit-app --bin graphit --test integration -- -D warnings
```

The actual Linux run passed 21 tests: eleven metadata/writer units, three
version integrations, three help integrations and four package integrations.
The first help fixture incorrectly treated Clap help precedence as a version
conflict; the corrected fixture exercises a real package operation conflict.
Strict Clippy for the affected executable and three integration targets passed
with warnings denied. Scoped formatting, diff checks and maintained links passed.
Version metadata/writer tests remain functional; actual package roundtrips and
native path handling are reused. Help tests exercise the compiled binary without
setup, invalid runtime configuration and nested package help. Native Windows and
macOS execution and the remaining CLI commands are separate integration work.
