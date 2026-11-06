---
title: Rust CI build caches
description: Restore and save compiled Rust, native engine and Windows package artifacts without skipping the native checks.
type: development
references:
  - target:
      type: task
      id: 01M5C6B85MCWH5HX932YVEKN3Z
    relation: implements
---

# Rust CI build caches

Maintainers use this page to diagnose the Rust matrix in
[the Code workflow](../../.github/workflows/ci.yml). The Linux, macOS and Windows
jobs keep the same [FIRST checks](rust-fast-guard.md) and
[native build contracts](native-runtime-packaging.md#build-and-package).
Cache hits never replace source checks, Cargo compilation checks or test execution.

## Preparation and save points

The workflow installs the platform tools, records their actual versions and runner
resources, restores Cargo sources, and runs `cargo fetch --locked`. All subsequent
Rust steps set `CARGO_NET_OFFLINE=true`; forwarded Cargo commands also pass
`--locked --offline`. The wrapper still resolves the official Ladybug dependency
through Cargo metadata and keeps its native build lock.

Preparation installs `cargo-nextest@0.9.151` with
[taiki-e/install-action](https://github.com/taiki-e/install-action), checksums
enabled and installer fallback disabled, before the offline steps. Test scheduling
is owned by the [xtask runner](rust-fast-guard.md); the cache does not choose test
filters, retries or concurrency.

| Cache | Contents | Saved after |
| --- | --- | --- |
| Cargo sources | `~/.cargo/registry` and `~/.cargo/git`, including unpacked sources | Successful locked fetch, before compilation |
| Windows packages | vcpkg's file binary-cache archives for `openssl:x64-windows`, `pkgconf:x64-windows` and their dependencies | Successful vcpkg install, before Rust compilation |
| Native engine | `target/native/ladybug`, including CMake's incremental build | Successful `cargo xtask native --offline`, before Clippy |
| Compiled Rust | Cargo fingerprints, build-script outputs, dependency/workspace libraries and the xtask executable under `target/debug` | Successful Clippy and `cargo xtask cargo test --workspace --no-run --locked --offline`, before tests |

The workflow uses the separate
[restore](https://github.com/actions/cache/tree/v4/restore) and
[save](https://github.com/actions/cache/tree/v4/save) actions. Save reuses the
restore step's `cache-primary-key` and identical paths. An exact hit is not
overwritten. Saving before execution retains a completed build when a later
test fails. There is no target cleanup between saving and running its executables.

The Rust cache deliberately selects `.fingerprint`, `build`, library/dependency
metadata extensions in `deps`, and `xtask`/`xtask.exe` with their hashed Cargo
artifacts in `deps/xtask-*`. It does not archive the
entire debug directory or its final application/test executables. Those binaries
remain on the current runner; a later job can rebuild/link them using the restored
libraries. This keeps repeated large executables out of the archive without
deleting workspace outputs before the tests run. Build-script native outputs
remain useful compiled inputs and are included.

The final `cargo xtask tests` runs the workspace through nextest and always runs
the separate `cargo test --workspace --doc --locked --offline` phase, including
when native tests fail. The runner aggregates errors. `--no-run` only prepares
executables; it is not a substitute for either phase. Cargo still checks freshness
and the wrapper still supplies the normal CLI and fixture paths. Doctest
compilation and execution happen in the final test step.

On Windows, a restored archive is consumed by the existing vcpkg `install` command
on every run. [vcpkg binary caching](https://learn.microsoft.com/en-us/vcpkg/users/binarycaching)
checks the package ABI and installs the matching binaries. `OPENSSL_DIR`, DLLs on
`PATH` and the `PKG_CONFIG` executable are then configured as on a cache miss.
The cache contains package archives, not a copied installed directory. It sets
`VCPKG_DEFAULT_BINARY_CACHE` and explicitly selects
`VCPKG_BINARY_SOURCES=clear;files,<absolute cache path>,readwrite` so an image's
other providers are not inherited. It does not use the removed `x-gha` provider.

## Manual Windows Core/FS feedback

The existing workflow accepts the fixed `suite` choices `standard` (the default)
and `core-fs`. Pushes, pull requests and manual `standard` runs retain the full
matrix and its existing jobs. A manual `core-fs` run starts only the Windows
Core/FS job, with a 20-minute job limit and a concurrency group separate from
standard runs on the same ref. Unsupported manual suite values fail before
preparation.

Push a topic ref such as `codex/native-regressions` containing the workflow and
the fixes, then dispatch that ref explicitly:

```sh
gh workflow run ci.yml --repo graphit-labs/graphit-code \
  --ref codex/native-regressions -f suite=core-fs
```

The workflow file must already exist on the repository's default branch;
[`--ref` selects its branch or tag version](https://cli.github.com/manual/gh_workflow_run).
The current push filters include `feature/**`, but exclude `codex/**`, so pushing
that topic alone does not start the standard matrix. A pull request targeting
`main` or `develop` still starts the standard jobs. See GitHub's
[dispatch prerequisites](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#workflow_dispatch).

The focal job installs the pinned Rust toolchain and the Git/hooks/shell tools
used by Core, restores the existing lockfile-keyed Cargo source cache and fetches
locked dependencies. It then runs offline:

```sh
cargo xtask cargo test -p graphit-core -p graphit-fs --locked --offline --no-fail-fast
```

The wrapper's exact package selection avoids the Ladybug dependency and App
fixture preparation. The job does not prepare C++, models or Docker, and does not
build App/Daemon fixture binaries. It restores and saves only Cargo sources;
partial compiled outputs never enter the standard compiled-cache namespaces.
A successful focal run qualifies Core/FS on that Windows ref; the standard
matrix remains the broader integration check.

## Invalidation

Compiled caches are separated by runner label, architecture and a digest of the
actual toolchain, native libraries and build options. The digest records Rust's
release and host triple, Cargo, C/C++ compiler, Clang, CMake and Protobuf versions,
runner image version, platform SDK and native-library package versions. Windows
also records the vcpkg checkout, MSVC toolset and Windows SDK. Relevant compiler,
linker, profile, target and native-tool environment settings contribute to it.
Native option families `GGML_*`, `LLAMA_*`, `CMAKE_*`, `CARGO_PROFILE_*`, `CUDA*`,
`CUDNN*` and `LBUG_*` also contribute when present. Their values are hashed without
printing them; logs show tool versions and option names.

The native key hashes the lockfile, root, xtask and `graphit-ast` manifests
(the latter selects the Ladybug binding),
Rust toolchain, Cargo configuration, this workflow and `xtask/src/ladybug.rs`,
which selects the engine options. Other member manifests do not select the native
engine build. Changing a test feature in Core or Daemon therefore leaves that
key unchanged when the native inputs remain the same.

The compiled Rust key has a compatibility digest containing those same inputs
plus member `build.rs` files. Its exact key additionally hashes every member
manifest and the source revision. The restore prefix ends after the compatibility
digest: changes to ordinary member manifests can restore existing dependencies,
then Cargo's fingerprints select which features, dependencies and sources need
rebuilding. Native binding, build-script, lockfile, configuration, toolchain and
ABI changes still invalidate that prefix.
Generated headers, `target`, temporary fixtures and build outputs are not inputs
to `hashFiles`.

Native and Windows package caches require their exact keys. Rust's primary key
can restore a compatible build from a different source revision or ordinary
member manifest. The v2 native/Rust key formats start a new cache population;
there is no restore path to the earlier v1 compiled entries.
There is no restore prefix that crosses a toolchain, target or ABI change.

The [native preparer](../../xtask/src/ladybug.rs) still resolves the fixed official
Ladybug package, configures and builds CMake, checks the selected compiler and
requires the archive and headers on every run. It has no complete ABI stamp;
ELF/COFF archive isolation uses modification times. The native selector and ABI
inputs therefore remain mandatory cache boundaries. Cargo freshness checks do
not certify external native ABI, and restoring a cache never skips preparation
or compilation checks.

Cargo sources are keyed separately by OS, architecture and lockfile. These are
sources rather than compiled binaries. Keeping their unpacked files also retains
the official Ladybug source tree used by the existing CMake preparation.

## Compilation resources

The Code repository is public. The
[documented standard runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
provide four CPUs and 16 GB RAM for `ubuntu-22.04` and `windows-2022`, and three
Arm CPUs and 7 GB RAM for `macos-15`. The matrix selects four compilation jobs on
Linux/Windows and two on macOS. The job prints its actual CPU count and RAM so
image changes can be checked against this choice. These settings control Cargo
and the native preparation; they do not change test concurrency.

These capacities are not a measured peak-memory bound for the full workspace.
Native runner execution must qualify the job settings; local workflow lint and a
small Linux build cannot establish Windows/macOS memory behavior.

## Diagnose a miss or a slow run

Read each restore action's key, hit/miss and archive size/transfer duration first.
A different runner image, toolchain, native package version, options or locked
input can legitimately produce a miss. A cache may also be absent or evicted by
the GitHub cache service; the normal build remains the recovery path. No cache
entry grants a successful check.

Cache storage and eviction follow the repository's GitHub service configuration,
not a new project quota. The [service reference](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#usage-limits-and-eviction-policy)
currently documents a configurable 10 GB default per repository and removal after
seven days without access; this is not a fixed per-entry limit. Large build-script outputs and libraries can still make
an archive expensive. Record the actual compressed CI size and transfer cost
before judging whole-workspace effectiveness; the shared local target can contain
many historical build graphs and is not a representative CI archive.

For Rust source changes under a compatible restore prefix, inspect Cargo's
`Compiling`/`Finished` output. For native preparation, distinguish CMake
configuration from actual object compilation. For Windows, distinguish vcpkg's
restored packages from newly built ports. Preserve compilation and test-harness
durations separately when comparing runs.

The Windows d802 run (`42208248699`) had no compiled/native cache. Its observed
preparation was 8m30.038s for the OpenSSL step and 35m36.936s for the native engine
before Clippy, including 1,076 C/C++ object compilation lines. Before tests the
engine preparation took 3.265s with no object recompilation. Test/fixture
compilation then took 42m38.126s; the native harness window was 84m04.018s.
The doctest window was 2m06.146s including compilation/startup, with 17.78s
reported harness runtime. These are historical phase measurements, not claimed
cache savings.

A focal local cold/restore/warm comparison can verify artifact reuse without
clearing the shared target: use one small existing package, an owned temporary
subdirectory of the canonical target, identical flags/jobs, offline dependencies
and the same source/toolchain. Measure archive save/restore separately. Do not
copy the full workspace target or infer a full CI speedup from this experiment.
Remote cache hit rates, transfer costs and complete matrix timings require a
published workflow and actual cold/warm CI runs; local checks do not measure them.

The local xtask focal used Rust 1.98, four build jobs, debug information and
incremental compilation disabled. The cold build compiled 34 crates in 7.091s
wall; the final restored build compiled none and took 2.421s wall. Its selected
archive was 25,005,533 bytes, saved in 0.510s and restored in 0.164s with local
GNU tar/zstd. The source/toolchain/flags were unchanged. These times include
wrapper overhead and concurrent host activity; they are not a statistical
benchmark or a fresh-checkout/network comparison.
