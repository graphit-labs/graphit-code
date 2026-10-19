---
title: Rust CLI version
type: specification
status: partial
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-75a5}
    relation: implements
  - target: {type: task, id: tsk-6db1}
    relation: uses
  - target: {type: task, id: tsk-6a69}
    relation: relates_to
---

# Rust CLI version

The Rust application prints its compiled brand and build version with:

```sh
graphit --version
```

The output is exactly `<brand> <build-version>` followed by one LF. A development
build with no metadata inputs prints `graphit dev`. There is one space, no color
or prefix, and no output on stderr on success. This command works in an empty
directory without setup, configuration, credentials or a project.

| Result | Exit code | Output |
|---|---|---|
| Successful version write and flush | 0 | Version line; stderr empty |
| Missing, unknown, duplicate, extra or invalid argument | 2 | Empty stdout; the Clap argument diagnostic on stderr |
| Metadata, allocation or output error | 1 | Static diagnostic; stdout may already contain a partial write |

The central [Clap CLI](rust_cli_help.md) accepts `--version` as an exclusive
option. Help and native package commands use the same parser. Other commands
are connected as their Rust implementations are completed.

## Build metadata

[build.rs](../../crates/graphit-app/build.rs) uses the same private
[metadata validator](../../crates/graphit-app/src/build_metadata.rs) as the
application. The build inputs are:

| Input | Behavior |
|---|---|
| `GRAPHIT_BUILD_BRAND` | Defaults to `graphit`; explicit values are 1–64 ASCII bytes without whitespace or controls and must pass Core `Brand::new` |
| `GRAPHIT_BUILD_VERSION` | Defaults to `dev` in every profile; explicit values are 1–128 printable ASCII bytes without whitespace |

Brand spelling is canonicalized by Core; version spelling is preserved. An
explicit version is a build token and does not need to be semver. Invalid inputs
fail the build with static diagnostics without echoing the value.

The optimization profile does not select a public version. A local optimized
build may keep `dev`. The tag-triggered public release workflow supplies its
explicit version; packaging preserves the executable's compiled identity.

For example, a release can be built with:

```sh
GRAPHIT_BUILD_BRAND=Graphit-CLI GRAPHIT_BUILD_VERSION=v0.9.4 cargo build --release -p graphit-app --bin graphit
```

This POSIX shell example produces `graphit-cli v0.9.4`. On other shells, set the
same two environment variables using that shell's syntax.

The build script emits the compiled `GRAPHIT_COMPILED_BUILD_BRAND` and
`GRAPHIT_COMPILED_BUILD_VERSION` constants. The binary reads these with `env!`;
changing metadata variables at runtime does not change version output. The
package version is a library/build detail and is not the product's release
version. The command does not consult Git or configuration.

## Implementation and tests

[main.rs](../../crates/graphit-app/src/main.rs) passes native arguments and stdout
through the [central parser](../../crates/graphit-app/src/cli.rs). After argument
validation, the [version renderer](../../crates/graphit-app/src/version.rs)
writes one plain line through the Core terminal
printer. Its maximum line is 194 bytes. Write and flush failures produce an
error; partial output is not retried or rolled back.

Eleven unit tests cover metadata selection and validation, maximum output,
partial writes, flush errors and diagnostics. Invalid arguments are tested at
the actual Clap entrypoint rather than a retired manual parser. The three
[integration tests](../../crates/graphit-app/tests/version_cli.rs) run the real
Cargo-built binary with `std::process::Command` and `TempDir` fixtures:

- Version works without setup and leaves an empty working directory intact.
- Compiled metadata overrides runtime variables and works with invalid synthetic
  configuration files, without modifying the fixture.
- Unsupported arguments return exit 2, empty stdout and a Clap diagnostic,
  including native invalid strings where the operating system supports them.

Run the affected checks with:

```sh
cargo xtask cargo test --locked -p graphit-app --bin graphit --test version_cli
cargo xtask cargo clippy --locked -p graphit-app --bin graphit --test version_cli -- -D warnings
```
