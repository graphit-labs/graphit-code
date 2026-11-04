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
Cargo fetches the shared Now wire crate from the locked Broker revision.

Maintainers use the same commands on Linux, macOS and Windows:

- `cargo xtask check`: native SDK boundary audit and formatting. The pre-commit hook runs this command and stops at the first failure.
- `cargo xtask gate`: native SDK boundary audit, formatting, strict workspace Clippy and workspace tests, including documentation tests. The CI matrix runs this offline gate after dependency preparation.
- `cargo xtask qualify`: run the Linux native GGUF and Broker/S3 journeys with explicit fixtures.
- `cargo xtask test PACKAGE/TARGET ...`: run only selected registered suites, in catalog order. Empty, unknown or repeated selectors are rejected.
- `graphit git install`: snapshot current records and register the commands declared in `.graphit/project.json`, using Git 2.54+ native configuration. `graphit git sync` reconciles events without a snapshot.

On Windows, the existing `check` and `gate` commands invoke Cargo fmt separately
for each local package manifest. This
preserves the `fmt --all` coverage without exceeding the Windows process command-line
limit. Package selection comes from locked Cargo metadata, not a maintained file list.
Prepare dependencies with `cargo fetch --locked`. Run the standard suite with
`cargo --offline xtask gate`. Its tests create temporary resources and use loopback
for network contracts. Real provider inference and externally provisioned services
are selected explicitly.

Rust runs cases within each test target in parallel, using the CPUs available to
the process. Set `RUST_TEST_THREADS` to choose a different test concurrency for a
local run. `CARGO_BUILD_JOBS` controls compilation separately.

Pre-commit runs the staged whitespace check, `actionlint` and `cargo xtask check`.
Pre-push runs `cargo xtask check`, strict all-target workspace Clippy through
`cargo xtask cargo`, and `cargo test --locked -p xtask --bin xtask`. These checks
validate source boundaries, formatting, compilation and the build/test runner itself.
Functional suites run through explicit affected selectors during development;
the complete `cargo xtask gate` stays in CI.
Neither hook repeats the complete integration matrix. Reuse valid evidence for
unchanged code instead of rerunning it after each failed push.

During a work unit, run only affected package suites and consumers. The manual selection catalog includes daemon watcher, supervisor and service command filters.

The Hub publication positive-flow fixtures use a 30-second request captured once per writer for durable filesystem operations. The mixed batch and Kiro update/forget tests verify that the same Original and deadline survive their flow. Injected boundary errors must be consumed and report `Other`, so expiry cannot masquerade as a successful fault case. Existing recovery opens a fresh writer as a separate operation; expiry and cancellation tests retain their own short or cancelled requests. This fixture duration does not set a production timeout.

## GitHub Actions CI

The [CI workflow](../../.github/workflows/ci.yml) keeps four independent jobs:

- `rust-contracts`: Linux, macOS and Windows fetch the locked dependencies, then
  run `cargo xtask gate` with Cargo offline. The standard job has no model
  download, model-server provisioning or external storage service setup.
- `ui-build`: Node.js 22 installs the UI dependencies, runs `node scripts/build-rust-ui.mjs` and checks that the three shipped assets have no diff. `make ui-lint` and `make ui-test` run the maintained lint and test scripts.
- `workflow-lint`: the official actionlint 1.7.12 Linux binary is verified against its pinned SHA-256 before extraction, then runs `actionlint -no-color .github/workflows/*.yml`.
- `native-packaging`: ShellCheck validates the maintained installer/packaging scripts and Node runs their existing package contracts.

## Native qualification

The separate [release workflow](../../.github/workflows/release.yml) builds and
checks the distributed packages. Its tagged Linux job also prepares the pinned
EmbeddingGemma2 BF16 and BGE F16 GGUF files, the model server and the owned
Broker/S3 fixture before running `cargo xtask qualify`. For a manual Linux
run, set the `native_integrations` input to select these checks. See
[native packaging](native-runtime-packaging.md).

For the Linux qualification, build the model server in Code and set
`GRAPHIT_TEST_MODEL_SERVER`, `GRAPHIT_EMBEDDINGGEMMA2_MODEL` and
`GRAPHIT_BGE_RERANKER_V2_M3_MODEL` to the absolute paths of the executable
and verified models. In the explicitly selected Broker
checkout, prepare the loopback MinIO and `mc` tools using
`scripts/prepare-s3-test-tools.sh`, then build the fixture with
`cargo build --locked -p broker-presence --example hub_storage_fixture`.
Add the prepared tools to `PATH` and set
`GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN` to the absolute executable path.
Use `cargo xtask qualify` to run these journeys or select
`graphit-app/ast-generic-main-native-nn` or `graphit-app/role-hub-file`
through `cargo xtask test`. Missing fixture paths fail the selected check.

The same qualification covers compiled Hub FILE installation, registry changes,
remote catalog and standalone authorization against the owned Broker/S3 fixture.
To run only this group after setting `GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN`:

```sh
cargo xtask cargo test --locked --offline -p graphit-app --test source_executable native_publication::file_installation -- --ignored
```

The PostgreSQL Now qualification uses the locked Broker control fixture and
its pinned PostgreSQL image on Linux. After `cargo fetch --locked`, export the
Broker package directory reported by Cargo, then select the qualification targets.

```sh
export GRAPHIT_BROKER_CONTROL_SOURCE="$(cargo metadata --locked --offline --format-version 1 |
  python3 -c 'import json, pathlib, sys; print(next(pathlib.Path(p["manifest_path"]).parent for p in json.load(sys.stdin)["packages"] if p["name"] == "broker-control"))')"
cargo xtask cargo test --locked --offline -p graphit-daemon --features postgres-qualification --test now_publisher --test now_subscription -- --ignored
cargo xtask cargo test --locked --offline -p graphit-app --features postgres-qualification --test foreground_now native_foreground_now_two_projects_and_owned_shutdown -- --exact --ignored
```

The combined Foreground Now and Memory cases also use the verified model and
model-server paths above. Ordinary tests cover the Now protocol and lifecycle
using their own loopback fixtures.

The launcher tests exercise the captured service command through a private
executable and check its arguments, working directory and result. The native
Linux desktop checks additionally use a private D-Bus instance. Run these
checks explicitly on a host with `dbus-daemon` and `dbus-run-session`.

```sh
cargo xtask cargo test --locked --offline -p graphit-app --test daemon_launcher native_private_bus_without_a_manager_is_unavailable -- --ignored --exact
cargo xtask cargo test --locked --offline -p graphit-app --test foreground_tasks tray:: -- --ignored
```

The artifact relocation test across filesystems uses `/dev/shm`, `unshare`,
`mount`, and `umount` on Linux. Run it explicitly where the host permits user
and mount namespaces.

```sh
cargo test --locked --offline -p graphit-core --lib artifact_store::tests::publication_between_devices_uses_local_wrappers_and_recovers_relocations -- --exact --ignored
```

Standard SCIP integration tests use a private Docker command fixture that
emits real SCIP data. The native image checks require Linux, Docker and the
compatible `ghcr.io/graphit-labs/graphit-scip-rust:v1` image. Prepare the image
before running these checks.

```sh
cargo xtask cargo test --locked --offline -p graphit-app --lib real_docker_custom_image_full_delta_source_and_mount_contract -- --ignored
cargo xtask cargo test --locked --offline -p graphit-app --lib foreground_rust_scip_calls_and_physical_change_rejection -- --ignored
```

UI tests use isolated Vitest workers, up to four according to the CPUs
available to Node. Override the worker count with Vitest's options, for example
`npm --prefix ui test -- --minWorkers=1 --maxWorkers=1`.
To generate the optional coverage report, run `npm --prefix ui test -- --coverage`.

The Task Browser qualification starts a Rust fixture on an ephemeral port,
seeds the authorized records and passes its session cookie to the selected UI
test. Prepare the UI dependencies, then run the qualification from the workspace
root. The standard Rust and UI suites exercise their respective Task contracts
with their own fixtures.

```sh
GRAPHIT_TASK_BROWSER_UI_DIR="$PWD/ui" cargo xtask cargo test --locked --offline -p graphit-app --lib task_browser_ui_reads_authorized_native_records -- --ignored
```

For an interactive browser inspection, run one ignored `hosting` test at a
time. Point `GRAPHIT_UI_BROWSER_PROBE_FILE` to a new file in a private temporary
directory. The fixture writes its origin and selection there. The observer
signals completion with a sibling `.done` file.

```sh
GRAPHIT_UI_BROWSER_PROBE_FILE="$probe" cargo xtask cargo test --locked --offline -p graphit-app --test foreground_tasks hosting::hosted_record_references_signed_oauth_and_browser_probe -- --exact --ignored
```

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
workspace tests in the CI gate.

`cargo xtask test graphit-app/git_inspection_`

The Git selector exercises the real SDK inspection path, scoped authorization,
staged and unstaged changes, conflicted indexes, and cancellation with owned
worker cleanup. Its library filter is available through the test selector catalog and covered by
the CI workspace gate.
The runner's existing failure propagation applies to this filter; it does not
create a separate hook harness or run all App tests.

`cargo xtask test graphit-ast/postgresql_semantic graphit-core/project_source_publication_order`

These selectors check [retained PostgreSQL body trees and their real scopes](../specs/rust_ast_antlr_semantics.md), and [writer preparation before the first Source capture](../specs/project_source_capture.md). The Source tests preserve rejection of a late directory change; they do not waive revalidation. Both targets are available through explicit selectors and covered by the
CI workspace gate.

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

Rust runs cases within each test target concurrently according to the machine's
available CPUs. Set `RUST_TEST_THREADS` to override that for a local run. CI uses
independent Linux, macOS, and Windows jobs. Its Cargo build job setting controls
compilation separately from test execution.

The SDK audit keeps private module, signature and unsafe-call boundaries. Its unit tests exercise structural violations; adding a legitimate SDK function does not fail an unrelated aggregate count assertion.

Focused hook verification and selected suites complement the workspace gate. Read platform coverage from the CI run for the revision under test.

The CI and release Rust jobs prepare the native build tools and current Git
before compilation. Cargo fetches the locked sources. Only the Linux S3
qualification uses a separate Broker checkout, pinned to the same revision.
