---
title: Rust native runtime packaging
description: Build native dependencies and package the Rust executables.
type: development
references:
  - target:
      type: task
      id: tsk-da6d
    relation: corrects
  - target:
      type: task
      id: tsk-cab0
    relation: corrects
  - target:
      type: task
      id: tsk-bb62
    relation: implements
  - target:
      type: task
      id: tsk-effc
    relation: implements
---

# Native runtime packaging

The Rust workspace selects official Lance 14.0.2 from the Cargo registry, without
patches. Ladybug/lbug 0.22.1 also comes from the official Cargo registry.
The existing xtask prepares Ladybug directly from the `lbug-src` directory in
the Cargo-resolved package, generating headers and one static Release engine
outside the source tree. See [official Lance integration](lance-upstream.md) and
[Ladybug build and adapter](../specs/rust_ladybug_sdk.md).

Use `cargo xtask cargo build/test/check ...` or the maintained Make targets.
The workspace checks Clippy warnings, except argument-count and type-complexity
style suggestions. Keep contracts explicit; do not introduce parameter wrappers
only to meet a seven-argument threshold. Compilation errors and functional,
security and cancellation tests remain required.
The wrapper supplies the official binding's external-library selectors and the
same engine, headers and compiler to Graphit's `cxx` adapter. CMake reuses its
ordinary build artifacts; there is no published Graphit Ladybug SDK, installer
or manual SDK selection. After fetching Cargo dependencies and installing build
tools, the wrapper supports `--offline`. Cross compilation requires an explicit
CMake toolchain. The installed package needs none of these build tools.
ANTLR retains its maintained C++ parsers and small `cxx` bridge. Local and remote
Icebug use the same captured Graphit adapter over official Parquet/CSR readers.

The GGUF engine is built once by the normal Cargo dependency described in
[llama.cpp through Cargo](llama-cargo.md). CPU and optional CUDA/Metal use the
same unmodified upstream engine and CXX ABI. No Graphit llama SDK, source archive,
patch, separate accelerator build or OpenBLAS dependency remains.

## Model artifacts

EmbeddingGemma and BGE reranker v2-m3 use GGUF through the Cargo-built engine.
The canonical reranker producer is Graphit Libs
`models/bge-reranker-v2-m3/build.py`; that directory contains the recipe,
source/converter/dependency pins and licenses, without model weights. It calls
the official converter with F16 and verifies the qualified output checksum.
Code and Broker provisioning consume the separately qualified model artifact
on demand. Disabling reranking must not discover, download or prepare it.
The producer receipt establishes artifact integrity; inference and provisioning
remain separate affected checks. Local runtime packaging no longer provisions
or distributes ONNX Runtime.

Real model qualification consumes explicit local inputs. The BGE smoke and
worker tests read `GRAPHIT_BGE_RERANKER_V2_M3_MODEL`, naming the pinned F16 GGUF
file. Reference qualification also requires `GRAPHIT_BGE_RERANKER_REFERENCE`,
naming the separately captured upstream F32 reference. For example:

```sh
export GRAPHIT_BGE_RERANKER_V2_M3_MODEL=/path/to/bge-reranker-v2-m3-F16.gguf
cargo test --locked -p graphit-ai --lib gguf_rerank::tests::actual_v2_m3_f16_cpu_pairs -- --ignored --exact --nocapture
GRAPHIT_BGE_RERANKER_REFERENCE=/path/to/reference.json cargo test --locked -p graphit-ai --lib gguf_rerank::tests::actual_v2_m3_f16_matches_f32_reference -- --ignored --exact --nocapture
```

The embedding fixture uses `GRAPHIT_EMBEDDINGGEMMA2_MODEL`; the daemon's real
installed-child fixture uses `GRAPHIT_TEST_GEMMA2_MODEL`. These real-model tests
are explicitly selected with `--ignored`. Ordinary contracts, build, workflow
lint or archive assembly do not qualify model inference.

The standard Rust CI prepares locked Cargo dependencies and runs offline.
The tagged Linux release qualification builds the model-server executable and
checks the pinned EmbeddingGemma BF16 and BGE F16 files by size and SHA-256.
The models stay in the runner's temporary directory. `cargo xtask qualify`
selects the native NN, rerank and Broker/S3 journeys. For an explicit Linux run
without publishing, select `platform=linux-amd64` and `native_integrations=true`
in the Release workflow.
See [Rust checks and Git hooks](rust-fast-guard.md) for the local equivalent.

To qualify the complete Linux, Windows and macOS packages without creating a release,
run the **Release** workflow manually on the desired Code ref:

```sh
gh workflow run release.yml --repo graphit-labs/graphit-code --ref main
```

The default `platform=all` builds the three packages. To repeat only an affected
native job while preserving valid results for unchanged platforms, select
`linux-amd64`, `darwin-arm64` or `windows-amd64`:

```sh
gh workflow run release.yml --repo graphit-labs/graphit-code --ref main -f platform=windows-amd64
```

This dispatch uses the same three-platform matrix as tag builds: `ubuntu-22.04`
(AMD64/CUDA), `windows-2022` (AMD64) and `macos-15` (ARM64). It runs the existing
source-boundary and formatting checks through `cargo xtask check`, builds the UI
and Release executables, runs package/install smokes,
and uploads complete packages as workflow artifacts. The manual dispatch does
not repeat the complete Debug gate, its Linux Hub fixture, Debug model-server
or embedding-model download. Main CI and tag releases retain their complete
formatting, Clippy and functional gates. The Linux job prepares its own supported CUDA toolchain;
it can produce a replacement GPU-capable package when local build caches have
been removed. A successful CUDA build does not prove GPU execution on a hosted
runner without a GPU.
On Windows and macOS it also points `GRAPHIT_TEST_MODEL_SERVER` at the installed executable for the
existing bootstrap/authentication, on-demand error, cancellation and shutdown
tests. These process tests require no model download and do not qualify model
inference or the platform's service manager.
On all three platforms the manual build also runs the existing
`graphit-records --lib git_records::tests` filter in Release, exercising native
Lance commit tags across subsequent head updates. Package production and these
focused consumers omit `--target` and use the same native Release layout;
the installed model-child consumers also reuse the Release profile. This scope
qualifies the affected snapshot/package paths, not the entire product suite.
The dispatch does not publish a GitHub release or container image. Cargo
resolves the Broker revision recorded in the lockfile. Record the workflow's
completed checks as qualification evidence.
Tag pushes retain the Linux, Windows and macOS release matrix and publication.

BGE F16 still requires an explicitly supplied qualified local artifact; its
successful source build does not establish a published model asset URL. That
publication dependency is separate from the usual embedding NN gate.

## Linux GNU allocator

The Graphit App selects `tikv-jemallocator` **0.7.0** as its global allocator on
`cfg(all(target_os = "linux", target_env = "gnu"))`. The locked
`tikv-jemalloc-sys` version is
`0.7.1+5.3.1-0-g81034ce1f1373e37dc865038e1bc8eeecf559ce8`.
The upstream `override_allocator_on_supported_platforms` feature retains the
unprefixed allocation symbols in the executable, covering linked native
`malloc`/`free` callers as well as Rust allocation; ordinary C++ `new`/`delete`
use those same allocation functions. The `background_threads` feature enables
jemalloc's existing asynchronous page purging. Reclamation follows its normal
decay behavior; freed memory need not disappear from RSS immediately.

This addresses the observed GNU libc retention of freed arenas after daemon
background work. It does not establish the cause of every earlier memory peak.
There are no custom arena limits or periodic application trimming. The dependency
is built and linked statically by its upstream Cargo builder, without another
shared runtime library. The Broker and separate model-server do not depend on
the App allocator. Windows, macOS and musl retain their existing allocator; this
Linux change is not native qualification of those platforms.

For a Linux release, inspect the final executable's dynamic symbols with
`readelf --dyn-syms --wide graphit`: `malloc`, `calloc`, `realloc`, `free`,
`aligned_alloc` and `posix_memalign` must be defined by the executable rather
than left undefined. `ldd graphit` must not require a separately installed
`libjemalloc.so`. After exercising the affected daemon paths, assess the running
process rather than interpreting file size or virtual mappings as live memory.
The [0.7.0 allocator manifest](https://github.com/tikv/jemallocator/blob/main/jemallocator/Cargo.toml)
and [upstream feature documentation](https://github.com/tikv/jemallocator/blob/main/jemalloc-sys/README.md#cargo-features)
describe the supported override and background behavior; Cargo.lock records the
actual released source checksums.

### MCP stdio allocations

`graphit mcp --stdio` remains in the Graphit executable. The CLI releases its
Clap command/help tree after argument validation and before dispatching a
long-running command. Parsed arguments own their data; keeping the parser alive
for the lifetime of the stdio connection serves no consumer. This also applies
to managed-role and Live MCP entrypoints without changing their admission.

When stdout is a writable Linux pipe, all three entrypoints use Tokio's native
nonblocking pipe writer inside the I/O runtime. This removes the extra copy and
up-to-2-MiB retained buffer of Tokio's blocking stdout adapter; RMCP still owns
framing, serialization and backpressure. Reopening the inherited descriptor
through procfs keeps its original file-status flags intact. The writer does not
open a reader, so a disconnected client still produces `BrokenPipe`. Files,
terminals, inaccessible procfs and other platforms keep standard Tokio stdout;
this capability selection does not suppress write errors.

The proxy retains the live Source catalog required for dispatch and selection.
RMCP 3.5.1 already shares each tool's input/output schema through `Arc`;
`SourceProxy::list_tools` clones the SDK tool descriptors, not the schema trees.
There is no additional catalog cache or replacement transport. SDK buffers and
the existing reqwest pool retain their normal reuse, and the Linux allocator
keeps its background reclamation for the daemon as well as the proxy. There is
no application purge, per-call trimming, or new allocator thread/arena limit.

For a focused Linux comparison, use isolated Release clients with the same
catalog, input, response sizes and idle interval. Record private memory, PSS and
the kernel RSS high-water mark separately. Inspect anonymous and file-backed
mappings: an executable that has been replaced can retain unshared clean file
pages, while relocated executable data is private even before a tool call.
Control file-page sharing between the compared images and identify sampled
private/PSS peaks as sampled, rather than treating RSS as exclusive heap usage.
Exercise initialization, tools/list, repeated small and large responses, idle
reclamation and EOF; ordinary command execution and its security checks remain
part of the functional qualification.

The focused Linux qualification on 2026-10-23 used normal Release builds
(`opt-level=3`, thin LTO, one codegen unit), the same running Source, 64 tools,
five small calls and three 3,171,080-byte Source responses per client. Both
executables had a read-only image mapping in the test client and one warmed MCP
peer to control file-page sharing. Three runs per image preserved the catalog
and schemas, exact payloads and successful EOF. Measurements below are MiB;
peaks of private memory and PSS were sampled every 50 ms, not kernel maxima.

| Observation | Before | After |
| --- | ---: | ---: |
| Private memory after initialization, range | 12.1–12.2 | 11.9–12.0 |
| Sampled private peak, range | 52.3–55.4 | 48.5–51.6 |
| Sampled PSS peak, range | 67.3–70.8 | 64.9–68.0 |
| Private memory after 45 seconds idle, extended pair | 17.8 | 15.4 |
| PSS after 45 seconds idle, extended pair | 33.2 | 31.8 |

The extended pair was stable between 30 and 45 seconds. Fourteen seconds was
not sufficient for reclamation in every run of either executable. These are
workload-specific observations, not memory budgets or a claim of lower PSS in
every phase: initialization PSS increased with the new image's file-page working
set, and kernel RSS high-water marks overlapped. Approximately 6.8 MiB of private
executable data, including relocations, was already present before any tool call
in both images. Large-response decoding/serialization and reusable SDK buffers remain;
anonymous mappings include live allocations and allocator retention, without a
per-allocation heap profile. The historical 160.8-MiB peak from the original PID
was not reproduced or attributed to a leak. No allocator setting was changed.

## Build and package

Cargo fetches the shared Broker dependencies from the Git revision recorded
in `Cargo.lock`. Development and standard CI build from a Code checkout alone.
The tagged Linux release workflow selects the same revision when it checks out
the explicit Broker/S3 fixture.

CI and release use GCC/libstdc++ 13 with CMake, OpenSSL and Protobuf on Linux.
macOS uses its native C++20 toolchain and explicit Homebrew OpenSSL prefix.
Windows uses the native toolchain, Protobuf/unzip and `openssl:x64-windows`
(import libraries for linking and DLLs on PATH for tests/runtime). They prepare
current Git for the Core Git contract. Windows producers enable
`git config --global core.longpaths true` before Cargo prepares its Git
dependencies and upstream submodules. Apply the same Git setting for local
Windows builds so the standard Cargo cache can contain the upstream filenames.
Native Windows/macOS execution still needs its own actual runner result; local
workflow lint is not platform qualification.

Windows App help/version subprocess tests keep `env_clear()` and explicitly
restore only `SYSTEMROOT` and the prepared loader `PATH`. This preserves native
DLL resolution for Cargo build artifacts while clearing inherited application
configuration and provider credentials. Release packaging validates and stages
each executable's actual dependency closure as described below.

`make build` builds graphit from graphit-app and graphit-model-server from
graphit-daemon using Cargo's locked dependencies. `make grammars` builds the AST
crate and its native grammars. `make ui` regenerates the shipped React assets
after UI changes; normal Rust builds reuse the checked-in assets.

Ladybug's CMake build always receives a positive parallel job count. An explicit
`CMAKE_BUILD_PARALLEL_LEVEL` takes precedence over `CARGO_BUILD_JOBS`; both must
be positive integers. Without either setting, Rust's
`std::thread::available_parallelism()` supplies the available CPU count, including
applicable affinity/cgroup constraints. The builder never passes an unbounded
`--parallel` option. These settings control compilation, not runtime memory.

`make run ARGS="--help"` builds both executables before running graphit with the
supplied arguments. It prepends the selected target's absolute `debug` directory
to that command's `PATH`, so the daemon can resolve `graphit-model-server` through
its captured search path. `CARGO_TARGET_DIR` selects the target directory; the
default is `target`. Model selection and inference still follow the captured
model configuration.

Local optimized builds may keep the default `dev` identity:

```text
mkdir -p target
cargo xtask cargo build --locked --release -p graphit-app -p graphit-daemon --bin graphit --bin graphit-model-server --message-format=json-render-diagnostics > target/release-artifacts.jsonl
```

For a public release, set an explicit product version other than `dev` in
`GRAPHIT_BUILD_VERSION`; Cargo package metadata is not the product version.
The tag-triggered workflow passes `github.ref_name` to that metadata producer.
Set the intended public version before invoking `make release`, which also
packages the executables.

`make release` builds both release executables and calls:

```text
bash scripts/package-release.sh linux-amd64 target/release-artifacts.jsonl .build/dist
```

Release uses optimized code generation and Thin LTO. The application crate uses
16 codegen units so LLVM can compile it in parallel; other packages retain one.
It uses `strip = "symbols"` to remove static symbol/name tables as well as debug
information from the final executables. This preserves dynamic exports, unwind
handling and the existing Ladybug/ANTLR native link configuration. Native SDK
archives must retain their linkable symbols. The Ladybug engine prepared from
the locked official Cargo sources uses the matching Release configuration.

Removing symbol tables does not remove statically linked library code or make
the executables fully static. Keep `graphit-runtime` beside them. Linux packages
include the resolved redistributable C++/GCC, OpenSSL 3, zlib and zstd shared
libraries under `graphit-runtime/lib`. The platform's glibc, libm and dynamic
loader remain required. GPU providers retain their documented driver/runtime
prerequisites. Native dependencies must be checked on each supported platform
rather than inferred from a Linux build.

Packaging takes the JSONL output of the successful Cargo `--release` build, not
a directory of arbitrary executables. It selects exactly one non-test optimized
`graphit` and `graphit-model-server` artifact with debug assertions disabled;
missing, ambiguous, Debug/test or mismatched executable paths fail before any
runtime download or archive publication. A stripped development executable is
not evidence of a Release build. Artifact paths stay local and are not included
in product provenance.

### Linux shared libraries

Linux packaging runs on the native Debian/Ubuntu build host and requires
`patchelf`, `ldd`, `readelf` and `dpkg-query` in addition to Cargo and Node.js.
It inspects the actual executable dependency closure before
publishing an archive. Missing tools, unresolved or unsupported libraries,
ambiguous package ownership, missing notices and mismatched SONAMEs fail the
package. A dependency cannot silently fall back to an unrelated host library.

The package retains `libstdc++.so.6`, `libgcc_s.so.1`, `libssl.so.3`,
`libcrypto.so.3`, `libz.so.1` and `libzstd.so.1` when present in that closure.
Only staging copies are modified with the maintained
[PatchELF](https://github.com/NixOS/patchelf) tool. Executables receive
`$ORIGIN/graphit-runtime/lib` as their RUNPATH; copied libraries receive
`$ORIGIN`, so indirect dependencies also resolve within the bundle. The
operating system's loader performs normal ELF loading; there is no shell
launcher, replacement interpreter or persistent `LD_LIBRARY_PATH` change.
Packaging checks the resulting resolution without inherited loader overrides.
The existing installers copy the complete runtime directory with the executables.

`graphit-runtime/linux-libraries.json` records each SONAME, installed distribution
package/version, corresponding source package/version, RUNPATH, and hashes before
and after transformation. It also records executable input/output hashes, host
library names and required GLIBC symbol versions. It contains no builder paths.
The original Cargo files remain unchanged. Packaged Linux executable bytes now
differ deliberately from those inputs because of the RUNPATH transformation;
the earlier byte-identical package qualification is historical, not proof of this
new transformation.

Distribution copyright notices are retained under
`graphit-runtime/licenses/linux/<package>/copyright`, with the canonical license
texts they reference under `common-licenses`. This includes the GCC runtime
exception notices. Source package names and exact versions allow maintainers to
retrieve the matching distribution sources, for example with
`apt-get source '<source_package>=<source_version>'` from the build distribution's
source repositories. Keep these notices and source provenance with redistributed
libraries; this step does not rewrite their licenses.

This is a glibc Linux bundle, not a promise to run on every Linux distribution or
on musl. Its recorded GLIBC symbol requirements must be satisfied by the target
host; copying libstdc++ does not lower that requirement. The currently inspected
local Code and Broker bundles require GLIBC 2.39, including their bundled ELF
libraries. The earlier GLIBC 2.43 observation belonged to a superseded local build.
The current local result does not establish the
baseline of a future Ubuntu 22.04 CI artifact. Host certificate stores and any
selected GPU drivers remain platform prerequisites. Windows and macOS packages
do not use this Linux transformation and require their own native qualification.

Packaging requires Cargo and Node.js. The script
runs `cargo metadata --locked --offline --format-version 1` for the release target.
It requires exactly one resolved official `lance` 14.0.2 and `lbug` 0.22.1 in the returned
resolve graph. The selected `manifest_path` supplies each package directory;
there is no sibling-checkout search, source download or build in this step.
Both database SDKs must resolve from crates.io, without a Graphit fork.
Missing metadata, ambiguity, another version, a missing license or missing
Ladybug license fails before archive publication. The published Lance crate
omits its repository license, so Code retains the unmodified upstream notice in
`crates/graphit-data/licenses/lance.LICENSE`. Packaging copies it without a source
download. Ladybug similarly retains its upstream MIT notice at
`crates/graphit-ast/licenses/ladybug.LICENSE`; both published crates omit the
repository-level license. Neither SDK requires Graphit fork provenance.

### Windows and macOS shared libraries

`scripts/package-platform-runtime.mjs` inspects the actual executables and their
redistributable dependency closure with native build tools. It rejects unknown
external libraries, incompatible architectures and missing redistributables;
the package cannot silently depend on a developer's installed libraries.

On Windows x64, `dumpbin` selects required release VC143 CRT DLLs from
`VCToolsRedistDir/x64/Microsoft.VC143.CRT` and OpenSSL 3 DLLs from
`OPENSSL_DIR/bin`. They are installed beside the executables, following
[Microsoft's application-local deployment](https://learn.microsoft.com/en-us/cpp/windows/choosing-a-deployment-method?view=msvc-170).
There is no requirement to install Visual Studio or run `vc_redist` on the
client. Windows 10/11 UCRT and system API DLLs remain operating system components,
including the COM/WinRT `combase.dll` described in the
[Windows API requirements](https://learn.microsoft.com/en-us/windows/win32/api/roapi/nf-roapi-roregisteractivationfactories);
debug CRT and other external dependencies are rejected. The Rust, Ladybug and
llama.cpp builds retain their consistent MSVC dynamic-CRT selection rather than
mixing `/MT` and `/MD`. Use the separate default installation directories for
Code and Broker when deploying their Windows packages. The native Windows
producers select Visual Studio 2022 explicitly, matching the VC143 SDK and
redistributable closure.

On macOS arm64, `otool` inspects the load commands. OpenSSL 3 dylibs selected
from `OPENSSL_DIR/lib` are bundled under `graphit-runtime/lib`, and
`install_name_tool` rewrites their references relative to the loading file.
Builder rpaths are removed. The package receives and verifies ad-hoc code
signatures after these transformations; this is not Developer ID signing or
Apple notarization. `libSystem`, system `libc++` and Apple frameworks, including
Accelerate, remain supplied by macOS. The manifest records the actual maximum
minimum macOS version required by the executables and bundled dylibs, rather
than inferring compatibility from the build runner's name.

`graphit-runtime/windows-libraries.json` or `macos-libraries.json` records
the packaged files, hashes, origin and system dependencies. Their notices are
under `graphit-runtime/licenses/<platform>/`. These files must remain with the
package. Packaging contracts use controlled tool output on Linux; only the
native CI build/package/install jobs can qualify execution on Windows/macOS.
The release smoke tests start the installed CLI outside the checkout with no
OpenSSL, Homebrew or vcpkg paths in the environment. They also start the packaged
model server without arguments and require its own `InvalidInput` diagnostic,
which verifies the loader and entrypoint, not model inference.
The macOS jobs use `macos-15` because GitHub is
[retiring the macOS 14 runners on November 2, 2026](https://github.com/actions/runner-images/issues/13518).

The archive contains the actual executables, native licenses, provenance and
the Cargo.lock digest. GGUF acceleration is part of the selected engine build. Cargo notices use this layout:

```text
graphit-runtime/licenses/cargo/lance/LICENSE
graphit-runtime/licenses/cargo/lbug/LICENSE
graphit-runtime/licenses/cargo/lbug/lbug-src/third_party/...
```

Linux GNU archives also preserve `LICENSE-APACHE` and `LICENSE-MIT` from both
resolved allocator crates under their corresponding `licenses/cargo/` names,
plus `tikv-jemalloc-sys/jemalloc/COPYING` for the statically linked native code.
Their exact Cargo versions and registry origins are recorded in `native_sources`;
missing or empty notices stop publication. These files are copied from the
resolved, locked crate sources rather than reconstructed license text.

Ladybug third-party LICENSE/NOTICE/COPYING files retain their paths below its
resolved package. ANTLR and maintained Tree-sitter notices keep their repository
paths below `graphit-runtime/licenses/crates/graphit-ast`. No builder, C++/cxx
linking or model selection is changed by notice collection.

The package copies engine MIT/jsonhpp and binding MIT/Apache notices from the
exact Cargo-resolved `llama-cpp-sys-2` source under
`graphit-runtime/licenses/cargo/llama-cpp-sys-2`. NVIDIA redistribution notices
are retained for optional CUDA builds. Missing or empty notices fail
before publication. No packaging step downloads or builds the engine.

`graphit-runtime/build.json` records `native_sources.lance`,
`native_sources.lbug` and `native_sources.llama-cpp-sys-2`, each with the exact
Cargo `version` and `source`. It stores no manifest path or checkout location.
The databases resolve from crates.io and the binding from its fixed public Git
revision. Package production rejects unexpected sources instead of publishing
host-specific paths or substituting another engine.

For a local validation composition, `build.json` describes the primary CLI
producer and its exact Cargo lock and native sources. The composition receipt
retains the original Cargo emission and input and packaged hashes of each
executable. An inherited model server retains its original producer, lock
digest and native-source provenance; it is not relabeled as part of the new
CLI build. The maintained public release packager still requires the executable
pair from its successful joint Cargo emission. A local composition does not
establish a published release or qualification of another platform.

The self-update preparation consumer requires the same Cargo notice paths,
Graphit provenance and native-source metadata. A missing binding origin fails
preparation; the updater does not require future binding releases to keep the
current build's version or Git pin.
The tar.gz has an outer .sha256 file. The extracted directory contains graphit
and graphit-model-server together; Windows names retain .exe.

`make install PREFIX=/desired/prefix` builds the Release package and installs
both executables and the complete runtime in `prefix/bin` through `install.sh`
(or `install.ps1` on Windows).
It does not copy the development executables. The caller chooses a writable
prefix; the command does not elevate privileges. Use `make run` for development.

The release workflow builds on each target's native runner. Its recipes do not
turn a Linux fixture or successful archive assembly into macOS, Windows or GPU
execution evidence.

## Install a product release

The public install.sh and install.ps1 download the selected release archive and its
SHA-256 sidecar, verify the archive before extraction, and install both executables
with graphit-runtime beside them. Keep that directory with the binaries when moving
a manual installation. The Unix installer defaults to ~/.local/bin and accepts
--dir and --version; PowerShell accepts -Dir and -Version.
The Unix installer also accepts `--archive` (PowerShell: `-Archive`) for an already generated local
package, with its adjacent `.sha256` file. It uses the same checksum validation
and installation path without contacting GitHub. `--archive` and `--version`
are mutually exclusive:

```sh
sh install.sh --archive .build/dist/graphit-linux-amd64.tar.gz --dir "$HOME/.local/bin"
```

The Windows installer requires the platform manifest, both declared executables
and every declared app-local DLL before it changes the installation directory.
The macOS package must include its runtime manifest as well. Model files remain
separate from the runtime package and follow the configured on-demand preparation.

On Unix, runtime files are installed before the executables, using temporary sibling
files and rename. Existing processes keep their original mapped inodes; the
Unix installer does not truncate a running executable or shared library. Restart
the Graphit services after installation to activate the new binaries. Copying
only an unprocessed Cargo executable bypasses the package's relative RUNPATH
and does not install its runtime dependencies.
The runtime directory also contains the product license,
Cargo.lock digest and this guide. Installers do not change model selection.
The Unix installer replaces the two executables and their packaged runtime files,
preserving unrelated installed files. Code self-update also preserves the
independent Broker namespaces `graphit-runtime/broker` and
`graphit-runtime/licenses/broker` through staging and rollback using hardlinks.
CUDA builds package the engine and CUDA
user-space runtime beside the executables with relative loader paths; there is no separate accelerator module
or backend manifest. CPU remains the default device.

For the maintained Docker build, extract the Linux release under .build/release.
The build context includes both executables and graphit-runtime. Container startup,
configuration and health behavior remain a separate functional integration from
archive assembly; including the files alone does not qualify those behaviors.

## Rust self-update preparation in progress

The independent [Rust preparation component](../specs/rust_self_update_preparation.md)
in tsk-20d9 is registered in the App. It selects the exact release assets, streams
and verifies SHA256, and validates the complete package in an owned private stage
through the real HTTP, filesystem, gzip and TAR producers. Internal file links
are checked after confined resolution; Coordinator authorial files are required.
The App21d ELF passed ten initial focal tests in 2.75 seconds. App22c passed the
modified negatives unit in 0.76 seconds and the new extraction-worker ownership
unit in 2.04 seconds; the nine unchanged tests reuse their App21d evidence. All
these executions had zero failures or ignored tests. The preparation specification
links both functional receipts and the subsequent App24g Clippy qualification
(23.26 seconds, normal App, -D warnings). Formatting and the focal diff check
passed. Preparation alone does not qualify installed replacement or restart;
the integrated consumer has separate evidence below.
The [installed consumer contract](../specs/rust_self_update.md) tracks the
registered apply module and compiled CLI/policy composition. App24c passed the
ordinary filesystem/locking cases and development CLI no-effects/help/PROJECT
rejection. After correcting the fixture to emit regular TAR entries for the real
ELF, App24f passed actual installed-daemon replacement/restart with a new PID and
startup failure on an occupied port with a retained recovery backup (123.77 seconds
of test execution). The owned daemons were stopped and joined; no system service
was installed. This historical Linux journey does not qualify the current GGUF
model service, published-release installation or Windows/macOS runtime. tsk-19ca remains
tracked separately from preparation tsk-20d9; affected Clippy, formatting and
functional results are recorded without a general gate campaign.
The contract links the durable receipts and limitations.

## Affected checks

```text
make test TEST=graphit-ast/source_ranges
node scripts/test-native-packaging.mjs
shellcheck -x install.sh scripts/package-release.sh
actionlint .github/workflows/release.yml
```

For the GGUF notice change, the same fixture accepts label fragments to select
the affected package, missing/corrupt notice cases and installer consumer:

```text
node scripts/test-native-packaging.mjs "release consumer" "Cargo llama notice" "Unix product installer preserves"
```

For the Cargo engine/CUDA dependency closure and notice checks, select:

```text
node scripts/test-native-packaging.mjs "release consumer" "Cargo shared engine" "Unix product installer preserves"
```

The Node suite checks the scripts' Linux archive/cache/integrity/packaging contract with controlled payloads and Cargo metadata. Resolved source fixtures live outside the product checkout and no vendor directory is supplied. These are consumer tests, not proof of a remotely published fork or native execution. This is Linux execution with controlled archives and actual HTTP/TAR/filesystem producers, not Windows/macOS runtime or remote publication.

Actual runtime loading and product-artifact evidence are recorded separately
under the delivery's validation receipt. Select affected Rust suites with TEST;
`make test` does not silently execute the whole workspace.

The release also carries the maintained Coordinator authorial files in
`graphit-runtime/agents/coordinator`. They are copied from the same Markdown
templates compiled into the product; `coordinator install` captures and validates
them through the existing publication SDK. Keep this directory beside the
executables so a user does not need a checkout of the Graphit source repository.
