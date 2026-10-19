# Contributing to Graphit Code

Graphit combines a Rust CLI and daemon, Lance storage, Ladybug/Icebug graphs, a
React Observatory, agent adapters, and a large documentation corpus. Contributions should preserve
the contracts between those surfaces rather than changing one in isolation.

## Prerequisites

| Tool | Version or requirement | Purpose |
|---|---|---|
| Rust and Cargo | Toolchain pinned in `rust-toolchain.toml`, with Clippy and rustfmt | Product executables, native SDKs, and tests |
| Node.js | 22+ | Observatory development, lint, tests, and build |
| GNU Make | current | Reproducible repository workflows |
| C/C++ toolchain, CMake, `pkg-config`, and Protobuf compiler | platform current | Native SDK and grammar builds |
| Git | Current release with reftable support | Checkout operations and test fixtures |

Keep the matching `graphit-broker` checkout beside `graphit-code`, as described in
[Rust workspace checkout](docs/development/rust-workspace.md). Cargo builds Lance, Ladybug and
the maintained native parsers from their pinned sources. Local embedding and rerank use the
shared llama.cpp/cxx GGUF runtime; follow [native runtime packaging](docs/development/native-runtime-packaging.md)
for installation and platform prerequisites. Ordinary Cargo builds embed the versioned UI assets
and do not require Node.js.

## Start a development checkout

```bash
git clone https://github.com/<your-username>/graphit-code.git
git clone https://github.com/graphit-labs/graphit-broker.git
cd graphit-code
make build
make run ARGS="--help"
cargo xtask install-hooks
```

Run the Observatory separately when working on the frontend:

```bash
cd ui
npm ci
npm run dev
```

After UI changes, run `make ui` from the repository root and include the generated assets under
`crates/graphit-app/assets/ui/` with the React source changes.

## Repository map

| Path | Responsibility |
|---|---|
| `crates/graphit-app/` | `graphit` CLI, MCP, HTTP and embedded UI composition |
| `crates/graphit-core/`, `crates/graphit-fs/` | captured configuration, identity, filesystem and request contracts |
| `crates/graphit-data/`, `crates/graphit-ast/` | storage, parsing, graphs, retrieval and source reads |
| `crates/graphit-knowledge/` | documentation ingestion, wiki generation and retrieval |
| `crates/graphit-records/` | Memory, Task, Dream, audit, ownership and fencing |
| `crates/graphit-hub/`, `crates/graphit-auth/` | artifact contracts and authorization |
| `crates/graphit-daemon/` | supervision, watchers, services and `graphit-model-server` |
| `crates/graphit-ai/`, `crates/graphit-runtime/`, `crates/graphit-process/` | models, agents and owned process execution |
| `ui/` | embedded React Observatory |
| `xtask/`, `scripts/` | Rust checks, Git hooks, UI generation and packaging |
| `crates/graphit-ast/grammars/`, `crates/graphit-ast/src/antlr_engine/` | native parser inputs/builders, licenses and notices; dependency patches/builds are maintained in Graphit Libs |
| `docs/guides/` | maintained user workflows and references |
| `docs/specs/`, `docs/architecture/`, `docs/decisions/` | mechanism, architecture, and decisions |
| `docs/site/` | GitHub Pages product and documentation entry point |

## Make a change

Keep a change focused and verify the whole public contract it affects:

- Configuration changes need a default, resolution behavior, environment spelling, secret policy,
  and updates to the [Configuration Reference](docs/guides/configuration.md).
- Filesystem changes need explicit ownership, ignore behavior, watcher impact, cleanup semantics,
  and updates to [Filesystem, State, and Watchers](docs/guides/filesystem_contract.md).
- A capability exposed through more than one surface must remain consistent across CLI, MCP,
  Observatory, adapters, and documentation.
- Task completion, memory recall, and artifact identity are deterministic state contracts. Preserve
  fencing, idempotency, audit history, and explicit failure behavior.
- Agent-generated synthesis must remain distinguishable from BM25, vector, RRF, graph, source,
  memory, and Task operations that do not need a coding-agent CLI.

Use `cargo fmt` for Rust and the repository's frontend formatter/linter for TypeScript and CSS. Keep code
comments only for invariants, constraints, or risks that are not evident from the code. Put user and
architecture documentation in the docs tree.

## Tests

Run affected registered suites and their actual consumers while iterating:

```bash
make test TEST=graphit-ast/source_ranges
make fmt-check
```

Select the suite appropriate to the change; the AST selector above is an example. `make test`
requires `TEST` and does not silently run the workspace. Use `cargo xtask test PACKAGE/TARGET ...`
for multiple selectors. Direct package tests and focused Clippy runs are appropriate when the
changed behavior has no registered selector. The maintained selector catalog and hook commands
are documented in [Rust checks and Git hooks](docs/development/rust-fast-guard.md).

For Observatory changes:

```bash
make ui-lint
make ui-test
make ui
```

Tests must own temporary isolated state and avoid real user home mutation. Integration fixtures
may use owned local services and loopback networking when required by the actual contract;
document their prerequisites and join their processes on failure and cancellation. Keep concrete
input/output limits and test current behavior and non-obvious invariants; do not add
environment-gated suites, production test switches, historical regression narratives, or duplicated
coverage.

The maintained [CI workflow](.github/workflows/ci.yml) runs Rust contracts on Linux, macOS and
Windows with `cargo xtask gate` and `cargo xtask verify-guards`. Its UI job
uses Node.js 22 and `make ui`, `make ui-lint` and `make ui-test`; a separate job validates all workflow
files with the checksum-verified official actionlint binary. The obsolete Go quality jobs have been
retired. See [Rust checks and Git hooks](docs/development/rust-fast-guard.md) for the current commands.

The [release workflow](.github/workflows/release.yml) runs the Rust gate before native packaging.
Both workflows require an explicit `GRAPHIT_BROKER_REF` paired with the Code revision. Release
builds and native runtime qualification remain platform-specific; a local Linux result does not
qualify macOS, Windows or a hosted CI run. Follow the native packaging guide for `make release`,
product version metadata and installable assets.

## Documentation and UI

All versioned project content is English. Public claims must describe implemented behavior and
state limitations directly. When commands, tools, defaults, paths, screenshots, or security
boundaries change, update the relevant guide, specification, README, and Pages copy in the same pull
request.

The Pages site must retain the current release token consumed by its publication workflow. Verify
responsive layout, keyboard operation, reduced-motion behavior, local assets, and every link after a
site change.

## Pull requests

Open a focused pull request against `main` and complete the template. Include:

1. the observable outcome and intended boundary;
2. the relevant issue or Graphit Task;
3. exact validation evidence;
4. configuration, filesystem, security, adapter, and documentation impact;
5. screenshots for meaningful visual changes.

Use clear commit subjects. Conventional Commit prefixes such as `feat`, `fix`, `docs`, `refactor`,
`test`, `build`, `ci`, and `chore` are welcome, but the subject must describe the actual outcome.

For questions and design discussion, use
[GitHub Discussions](https://github.com/graphit-labs/graphit-code/discussions). Report reproducible
bugs and scoped proposals through the repository's issue forms.
