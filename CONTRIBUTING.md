# Contributing to Graphit Code

Graphit combines a Go CLI and daemon, Rust-backed LanceDB storage, LadybugDB/Icebug graphs, a
React Observatory, agent adapters, and a large documentation corpus. Contributions should preserve
the contracts between those surfaces rather than changing one in isolation.

## Prerequisites

| Tool | Version or requirement | Purpose |
|---|---|---|
| Go | Version declared by `go.mod` (currently 1.26.6) | CLI, daemon, MCP, storage orchestration, and tests |
| Node.js | 22+ | Observatory development, lint, tests, and build |
| Rust and Cargo | current stable, maintainers only | Explicit `make fetch-lancedb` source rebuilds |
| GNU Make | current | Reproducible repository workflows |
| C/C++ toolchain, `pkg-config`, and ICU development libraries | platform current | CGO/native dependencies |
| `golangci-lint` | repository/CI-compatible current release | Go static analysis |

The normal build downloads the platform's pinned native bundle and verifies it before use. Platform
setup differs for the remaining build tools; follow the CI workflow when a local package name is
unclear.

## Start a development checkout

```bash
git clone https://github.com/<your-username>/graphit-code.git
cd graphit-code
make setup-lbug
make lancedb-native
make build-local
make test
```

Run the Observatory separately when working on the frontend:

```bash
cd internal/ui
npm ci
npm run dev
```

## Repository map

| Path | Responsibility |
|---|---|
| `cmd/` | CLI entry points and command surfaces |
| `internal/ast/` | parsing, Icebug graph construction, Lance retrieval, source reads |
| `internal/knowledge/`, `internal/wiki/` | documentation ingestion and compiled wiki retrieval |
| `internal/memory/` | persistent project/user memory and revision lifecycle |
| `internal/task/` | deterministic task ownership, audit, checks, and completion gates |
| `internal/hub/` | artifact registry, shared storage, and agent adapters |
| `internal/daemon/` | supervision, filesystem synchronization, and background work |
| `internal/ui/` | embedded React Observatory |
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

Use `gofmt` for Go and the repository's frontend formatter/linter for TypeScript and CSS. Keep code
comments only for invariants, constraints, or risks that are not evident from the code. Put user and
architecture documentation in the docs tree.

## Tests

Run the smallest relevant package while iterating, then the repository targets appropriate to the
change:

```bash
go test ./internal/<package>
make test
make lint
make build-local
```

For Observatory changes:

```bash
cd internal/ui
npm run lint
npm test
npm run build
```

Tests must be hermetic: temporary isolated state, no network or external service, no real user home
mutation, and bounded resource use. Test current behavior and non-obvious invariants; do not add
environment-gated suites, production test switches, historical regression narratives, or duplicated
coverage.

`make ci` is the broad local gate. Native release builds are platform-specific because the LanceDB
bridge cannot be cross-compiled.

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
