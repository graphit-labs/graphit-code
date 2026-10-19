---
title: Rust workspace checkout
content-type: guide
audience: developers
status: local-build-qualified-ci-unrun
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-85fa}
    relation: implemented_by
  - target: {type: task, id: tsk-e89c}
    relation: uses
  - target: {type: task, id: tsk-60d0}
    relation: supports
  - target: {type: task, id: tsk-df7c}
    relation: implemented_by
  - target: {type: task, id: tsk-ca81}
    relation: uses
---

# Rust workspace checkout

Keep `graphit-code` and `graphit-broker` in the same parent directory. Cargo loads
the `broker-now-wire` 0.1.0 crate from the Broker checkout. It contains the shared,
validated Now data format, with Serde and ULID dependencies; it does not depend on
Broker SQL, authorization or transport services.

From a directory where neither checkout exists:

```text
git clone https://github.com/graphit-labs/graphit-code.git graphit-code
git clone https://github.com/graphit-labs/graphit-broker.git graphit-broker
cd graphit-code
cargo metadata --locked --offline --format-version 1
```

The Code checkout selects Rust 1.98.0 with Clippy and rustfmt. Native SDK builds
also need a C/C++ toolchain. Offline Cargo commands require their registry
dependencies to have been downloaded already; the peer crate itself is local.
If Cargo reports that `broker-now-wire/Cargo.toml` is missing, check the sibling
layout and that the Broker checkout contains the maintained crate.

The edge is declared in the [workspace manifest](../../Cargo.toml) and consumed by
the [daemon manifest](../../crates/graphit-daemon/Cargo.toml). Cargo resolves and
locks it normally. Native dependency bundles are a separate build input.

The [CI workflow](../../.github/workflows/ci.yml) and
[release workflow](../../.github/workflows/release.yml) check out Code at its
tested revision and Broker from `graphit-labs/graphit-broker` into these sibling
directories. Both require the repository variable `GRAPHIT_BROKER_REF` to name the
Broker commit or tag qualified with that Code revision; an absent value fails
before checkout. Rust steps run from `graphit-code`. The declared ref must contain
the reviewed producer source and be readable by the checkout credential. Neither
workflow selects `main` implicitly or substitutes an unpublished local commit.

Local source and build qualification do not prove that changes are published on
the CI refs or that the hosted Linux, macOS and Windows jobs have executed.
The shared codec does not qualify the websocket service or its Runtime/CLI/MCP
consumers; their authentication, project access and lifecycle remain separate.

Run the [affected checks and Git hooks](rust-fast-guard.md) for each work unit.

The Task browser uses the three versioned files in
[the shipped UI directory](../../crates/graphit-app/assets/ui/index.html). After
changing React source, install its locked dependencies and regenerate them:

```text
npm --prefix ui ci
node scripts/build-rust-ui.mjs
```

Include the generated files with the source change. The existing UI CI job runs
that generator once and rejects differences in `index.html`, `logo.svg` and
`favicon.svg`. Its lint, tests and built UI artifact remain enabled. Ordinary
Cargo builds embed these versioned bytes and need neither Node nor a Vite build.
The local workflow check does not establish a hosted CI run.
