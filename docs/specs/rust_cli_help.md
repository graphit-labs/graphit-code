---
title: Rust CLI help
description: Public help, compiled version and native package commands through one Clap parser.
type: specification
updated: 2026-10-11
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
passed directly to their typed producer. Help uses Clap's
standard rendering and precedence, including its native help subcommand.

`--version` is exclusive with an operation and preserves the exact compiled
version line. Invalid or conflicting arguments exit 2 with Clap diagnostics.
Version metadata and output errors exit 1; partial writes are not retried.
Failure to print a help or error response also exits 1. The manual argument parser,
its obsolete parsing unit and fixed-size help rendering plan were retired.

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
