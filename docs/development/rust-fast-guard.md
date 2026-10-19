---
references:
  - target: {type: task, id: tsk-93b3}
    relation: implements
  - target: {type: task, id: tsk-31cc}
    relation: implements
  - target: {type: task, id: tsk-b559}
    relation: implements
  - target: {type: task, id: tsk-86bc}
    relation: implements
  - target: {type: task, id: tsk-bd23}
    relation: implements
  - target: {type: task, id: tsk-f48e}
    relation: implements
  - target: {type: task, id: tsk-1559}
    relation: implements
  - target: {type: task, id: tsk-fa7c}
    relation: implements
  - target: {type: task, id: tsk-7af8}
    relation: implements
  - target: {type: task, id: tsk-98e7}
    relation: implements
  - target: {type: task, id: tsk-368b}
    relation: supports
  - target: {type: task, id: tsk-25a0}
    relation: implements
  - target: {type: task, id: tsk-2f3c}
    relation: implements
  - target: {type: task, id: tsk-08ae}
    relation: implements
  - target: {type: task, id: tsk-8b93}
    relation: derived_from
  - target: {type: task, id: tsk-7424}
    relation: relates_to
  - target: {type: task, id: tsk-a0eb}
    relation: supports
  - target: {type: task, id: tsk-b96c}
    relation: implements
  - target: {type: task, id: tsk-c775}
    relation: implements
  - target: {type: task, id: tsk-9f12}
    relation: implements
  - target: {type: task, id: tsk-9840}
    relation: implements
  - target: {type: task, id: tsk-9d27}
    relation: implements
  - target: {type: task, id: tsk-5372}
    relation: implements
  - target: {type: task, id: tsk-0395}
    relation: implements
  - target: {type: task, id: tsk-b210}
    relation: implements
  - target: {type: task, id: tsk-df9e}
    relation: implements
  - target: {type: task, id: tsk-e4a9}
    relation: implements
  - target: {type: task, id: tsk-2ccd}
    relation: implements
  - target: {type: task, id: tsk-9df1}
    relation: implements
  - target: {type: task, id: tsk-d76c}
    relation: implements
  - target: {type: task, id: tsk-0ac6}
    relation: implements
  - target: {type: task, id: tsk-3f4b}
    relation: implements
---

# Rust checks and Git hooks

Use the [Rust workspace layout](rust-workspace.md) before running Cargo commands.
The shared Now wire crate comes from the sibling Broker checkout.

Maintainers use the same commands on Linux, macOS and Windows:

- `cargo xtask check`: native SDK boundary audit and formatting. The pre-commit hook runs this command and stops at the first failure.
- `cargo xtask gate`: native SDK boundary audit, formatting, strict workspace Clippy and workspace tests, including documentation tests. CI and explicit full qualification run this integration gate.
- `cargo xtask test PACKAGE/TARGET ...`: run only selected registered suites, in catalog order. Empty, unknown or repeated selectors are rejected.
- `graphit git install`: snapshot current records and register the commands declared in `.graphit/project.json`, using Git 2.54+ native configuration. `graphit git sync` reconciles events without a snapshot.

Pre-commit runs the staged whitespace check, `actionlint` and `cargo xtask check`.
Pre-push runs `cargo xtask check`, strict all-target workspace Clippy through
`cargo xtask cargo`, and `cargo test --locked -p xtask --bin xtask`. These checks
validate source boundaries, formatting, compilation and the build/test runner itself.
Functional suites run through explicit affected selectors during development;
the complete `cargo xtask gate` stays in CI and is available for explicit qualification.
Neither hook repeats the complete integration matrix. Reuse valid evidence for
unchanged code instead of rerunning it after each failed push.

During a work unit, run only affected package suites and consumers. The manual selection catalog includes daemon watcher, supervisor and service command filters.

The Hub publication positive-flow fixtures use a 30-second request captured once per writer for durable filesystem operations. The mixed batch and Kiro update/forget tests verify that the same Original and deadline survive their flow. Injected boundary errors must be consumed and report `Other`, so expiry cannot masquerade as a successful fault case. Existing recovery opens a fresh writer as a separate operation; expiry and cancellation tests retain their own short or cancelled requests. This fixture duration does not set a production timeout.

## GitHub Actions CI

The [CI workflow](../../.github/workflows/ci.yml) keeps four independent jobs:

- `rust-contracts`: Code and Broker are checked out as siblings; Linux, macOS and Windows run `cargo xtask gate` from Code. On Linux, the job first builds Broker's `broker-presence` example `hub_storage_fixture`, checks that its emitted executable exists and exports its absolute path as `GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN` through `GITHUB_ENV` for the existing gate.
- `ui-build`: Node.js 22 installs the UI dependencies, runs `node scripts/build-rust-ui.mjs` and checks that the three shipped assets have no diff. `make ui-lint` and `make ui-test` run the maintained lint and test scripts.
- `workflow-lint`: the official actionlint 1.7.12 Linux binary is verified against its pinned SHA-256 before extraction, then runs `actionlint -no-color .github/workflows/*.yml` without bootstrapping Go.
- `native-packaging`: ShellCheck validates the maintained installer/packaging scripts and Node runs their existing package contracts.

The separate [release workflow](../../.github/workflows/release.yml) runs the same Rust gate before native packaging; its Linux build prepares and exports the same owned Broker fixture first. Native platform qualification remains part of the migration. Static workflow validation does not establish that the Rust/UI suites or hosted platform matrix executed.

The Linux gate includes the ignored Role FILE Hub journey through the registered
`graphit-app/role-hub-file` selector. It starts the owned Broker storage fixture
and verifies genuine Role service credentials and PROJECT grants; it needs no
external Broker account or agent binary. The fixture build is preparation for
the explicit `cargo xtask gate`, not the fast pre-push checks. macOS and Windows retain the existing
Linux-only test exclusion.

For a local Linux gate or selected run, build the same example from Code with
`cargo build --locked --manifest-path ../graphit-broker/Cargo.toml -p broker-presence --example hub_storage_fixture`,
then set `GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN` to the emitted example's absolute
executable path before `cargo xtask test graphit-app/role-hub-file` or the existing
explicit full gate. Missing or invalid fixture paths fail the runner; they do not skip
the test. These instructions do not supply other suites' independent resources.

The Linux gate also runs the registered
`graphit-app/ast-generic-main-native-nn` selector. Before that gate, both CI and
release build the normal `graphit-model-server` executable and prepare the pinned
EmbeddingGemma2 BF16 GGUF. They verify the model's SHA-256 and byte length, then
export the executable and model paths as `GRAPHIT_TEST_MODEL_SERVER` and
`GRAPHIT_EMBEDDINGGEMMA2_MODEL` through `GITHUB_ENV`. The revision, hash and size
are declared in those workflows; release packaging happens after the gate.

The test uses that GGUF through the daemon model fixture with explicit CPU
execution. Local invocations must provide the same two absolute resource paths;
the model must already be available, while CI downloads its pinned artifact.
No agent executable, provider account or paid plan is required. Preparing those
resources does not execute or requalify the native NN journey; hosted CI and
the existing platform obligations remain separate from workflow lint.

## Validate hook execution

`cargo xtask test graphit-app/git_hooks_executable` runs the real compiled Graphit
dispatcher in temporary repositories. It checks native registrations, command order,
configuration edits without reinstall, live stdout/stderr, fail-fast behavior,
arguments/stdin, third-party preservation and process cancellation. It never pushes.
The same integration target is included in the workspace gate; there is no separate
xtask hook runner or generic guard harness. [Git hooks](../guides/git-hooks.md)
defines the production shell/context and Task/Memory contract.

## Validate selected behavior

Use `cargo xtask test PACKAGE/TARGET ...` to run only selected registered suites in the current workspace, reusing the normal Cargo cache. Empty, unknown or repeated selectors fail before executing tests. Selectors follow catalog order. Only selecting `graphit-app/git_hooks_executable` runs its real Git fixtures; other selectors retain their own scope.

For example:

`cargo xtask test graphit-daemon/watcher graphit-daemon/supervisor graphit-daemon/services`

`cargo xtask test graphit-runtime/roles`

`cargo xtask test graphit-data/lance_source_stream`

The Source stream target checks real SDK creation, replacement and deletion,
including late errors, schema and key validation, stale CAS, recovery and named
branches. It is registered in the explicit test selector catalog and covered by the
workspace tests in the pre-push gate. The five affected cases passed on Linux;
the test runtime was 0.06 seconds with the existing build cache. This guard
does not qualify the foreground App consumer or other platforms.

`cargo xtask test graphit-app/git_inspection_`

The Git selector exercises the real SDK inspection path, scoped authorization,
staged and unstaged changes, conflicted indexes, and cancellation with owned
worker cleanup. Its library filter is available through the test selector catalog and covered by
the pre-push workspace gate.
The runner's existing failure propagation applies to this filter; it does not
create a separate hook harness or run all App tests.

`cargo xtask test graphit-ast/postgresql_semantic graphit-core/project_source_publication_order`

These selectors check [retained PostgreSQL body trees and their real scopes](../specs/rust_ast_antlr_semantics.md), and [writer preparation before the first Source capture](../specs/project_source_capture.md). The Source tests preserve rejection of a late directory change; they do not waive revalidation. Both targets are available through explicit selectors and covered by the
pre-push workspace gate. Their current native qualification is Linux; Windows and macOS remain unqualified.

`cargo xtask test graphit-knowledge/markdown_export graphit-knowledge/published_lint`

The Knowledge selectors validate Markdown export plans and lint of a published snapshot. Each runs its named integration target, without repeating the package library tests.

`cargo xtask test graphit-app/models_cli graphit-app/help_cli graphit-app/version_cli graphit-app/package`

The App selectors exercise the compiled CLI: model arguments and installation
paths, help before setup, compiled version, and native package commands. They
select the four named integration targets through the same runner.

`cargo xtask test graphit-app/source_executable`

The Source selector runs the compiled executable against a private installation
and real project storage, including source from indexed data and the physical
file fallback. Its fixtures currently run on Linux; this selector does not
qualify native Windows or macOS execution.

The services filter runs five normal tests. Its Linux systemd user test is opt-in and requires an active user manager:

`cargo test -p graphit-daemon --lib services::tests::native_systemd_execstart_preserves_literal_path_and_arguments -- --ignored`

The native test installs, starts, restarts and removes its own temporary unit. A missing user manager does not qualify native behavior. Windows and macOS service adapters require their own native execution.

The selector names and Cargo arguments live in `xtask/src/main.rs`. The runner does not copy the product workspace, build additional runners or inject test code into product modules. Child command failures stop the selection and reach the caller; hook failures stop subsequent configured commands. Commands wait for their direct children. This does not claim supervision of independent descendant process groups.

Build and test parallelism follow Cargo and Rust defaults or the caller's
environment. The runner does not override `CARGO_BUILD_JOBS` or
`RUST_TEST_THREADS`.

The SDK audit keeps private module, signature and unsafe-call boundaries. Its unit tests exercise structural violations; adding a legitimate SDK function does not fail an unrelated aggregate count assertion.

Focused hook verification and selected suites do not substitute for the final integration gate or native platform tests. Linux execution is qualified by the current delivery; Windows and macOS require their own execution before claiming platform coverage.

The CI and release Rust jobs prepare their native build dependencies and current
Git before compiling. `GRAPHIT_BROKER_REF` remains an explicit paired source selection;
use `GRAPHIT_BROKER_READ_TOKEN` when the Broker repository is private. The checkout
does not persist that credential and records the actual Broker commit used. These
settings must refer to available sources; local-only revisions are not remote CI proof.

The Rust jobs disable incremental/debug-symbol output for development/test builds
and share Code's target directory with the owned Broker fixture. Release-profile
product binaries retain their existing optimization settings. This reuses the
common dependencies without a second native target tree.
