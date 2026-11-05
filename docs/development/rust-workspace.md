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

Graphit Code builds from its own checkout. Cargo resolves `broker-now-wire` and
the Broker crates used by joint protocol tests from a single pinned Git revision.
The shared Now crate validates the Serde payload and has no SQL, authorization or
transport dependency. A sibling checkout is unnecessary.

```sh
git clone https://github.com/graphit-labs/graphit-code.git
cd graphit-code
cargo fetch --locked
cargo metadata --locked --offline --format-version 1
cargo --offline xtask gate
```

The Code checkout selects Rust 1.98.0 with Clippy and rustfmt. Native builds also
use the toolchain described in [native runtime packaging](native-runtime-packaging.md).
Cargo prepares registry and Git dependencies before the offline run. The
[workspace manifest](../../Cargo.toml) and lockfile identify the reviewed Broker
source revision, including the codec consumed by the daemon.

The [standard CI](../../.github/workflows/ci.yml) fetches the same locked sources
then runs the source checks, Clippy and workspace tests offline on Linux, macOS
and Windows. The tagged Linux
[release workflow](../../.github/workflows/release.yml) checks out the Broker
revision in the lockfile when it needs the explicit Broker/S3 qualification
fixture. See [Rust checks and Git hooks](rust-fast-guard.md) for the distinction
between local protocol checks and native qualification.

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
