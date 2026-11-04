---
references:
  - target: {type: task, id: 01M5C6B85MCWH5HX932YVEKN3Z}
    relation: implements
---

# Ladybug from Cargo and the captured graph adapter

Graphit uses the official `lbug = "=0.22.1"` registry package, with its checksum
pinned in `Cargo.lock`. The engine sources and headers are the `lbug-src` directory
inside that resolved package. No Graphit repository versions Ladybug sources,
archives, submodules, patches or copied headers. Graphit Libs no longer distributes
a Ladybug SDK. Lance and the GGUF runtime are independent dependencies.

## Build from source

Install the pinned Rust toolchain, C/C++20 compiler, CMake, Python 3.9+ and OpenSSL3
development files, plus the other workspace prerequisites described in
[native packaging](../development/native-runtime-packaging.md). Linux additionally
uses GNU binutils (`ld`, `nm`, `objcopy`, `ar`) for static symbol isolation and
LLVM `llvm-ar`/`llvm-ranlib` for the official bundled archive target. The latter
accepts paths containing spaces in upstream CMake’s MRI script; GNU ar
truncates them.
`CMAKE_AR`/`CMAKE_RANLIB` may explicitly select equivalent tools.

Upstream `cmake/BundleStaticLibrary.cmake` also reads its input archive list
with `file(STRINGS)` without a UTF-8 encoding. A tested build under
`target/Ladybug espaço` compiled the objects but failed when that operation
split the archive path at `ç`. LLVM ar cannot repair an already corrupted list.
No source patch or automatic alternate path is applied. Use a Cargo target
directory whose path is ASCII (spaces are supported), for example
`--target-dir 'target/Ladybug space'`. This build-path limitation does not alter
Unicode content or paths in the indexed graph. Native Windows/macOS execution
remains unqualified.
Windows builds use Ninja in the MSVC developer environment with the dynamic CRT
(`/MD`). Ninja lets the official CMake bundler add each dependency archive once.
`+crt-static` is rejected because the official Rust binding uses `/MD`.

```sh
cargo fetch --locked
# Prepares Ladybug, then runs Cargo with the matching library, headers and compiler:
cargo xtask cargo build --locked -p graphit-app -p graphit-daemon --bin graphit --bin graphit-model-server
cargo xtask cargo test --locked -p graphit-ast --lib icebug_query::tests::
cargo xtask cargo build --locked --release -p graphit-app -p graphit-daemon --bin graphit --bin graphit-model-server
# Optional preparation only; consumers should still use the wrapper:
cargo xtask native --offline
```

`make build`, `make check`, `make clippy`, `make grammars`, `make release`, the
registered `cargo xtask test` selectors and `cargo xtask gate` use the same
preparation. A bare `cargo build/test` is not the product build entrypoint and
must not be used to select an unrelated library through the upstream builder.
No developer needs to install or manually select a Ladybug SDK.

The xtask reads `cargo metadata --locked --format-version 1` with its resolved
graph, requires exactly registry `lbug 0.22.1`, and uses its `manifest_path`.
It does not scan Cargo's cache or guess `target` build hashes. Standard Cargo
fetching prepares the source; after `cargo fetch`, use `--offline` for no Cargo
network access. The configured engine disables optional backtraces/extensions
and grammar regeneration, and enables disconnected FetchContent; it requires
no separate engine/dependency download. Required native build tools remain host
prerequisites, not Cargo downloads.

CMake builds the official static engine and `single_file_header` out of tree,
under `<Cargo target directory>/native/ladybug/<target>/release/`. Rust dev, test
and release consumers reuse the same Release engine ABI. `--target`,
`--target-dir` and `CARGO_TARGET_DIR` are respected. Cross compilation additionally
requires `CMAKE_TOOLCHAIN_FILE`; hierarchical Cargo `build.target` and target-specific
Rust flags are resolved by `cargo-config2`. Multiple targets in one invocation and
Cargo `--config` overrides are rejected; use `.cargo/config.toml`, Cargo environment
variables or explicit `--target`/`--target-dir` instead. `CC`, `CXX`, their
target-specific cc-rs overrides, the toolchain, OpenSSL selection and
CMake's native generator apply during configuration. CMake owns incremental
rebuilds; unchanged inputs do not compile the engine again. No source or cache
file is modified. The generated amalgamated header stays in the build directory. Only the original
C API `lbug.h` is staged beside it, because the binding accepts one external
include directory. Other source headers are consumed in place, without copying
them into Graphit repositories. A standard file lock protects the shared native
directory through preparation and the consumer build, so another invocation cannot
reconfigure its engine while the binding or adapter is compiling.

When `CARGO_BUILD_TARGET` or `build.target` selects a different host, Cargo would
also cross-compile the `xtask` executable behind the alias. Run that executable
for the host explicitly, then select the consumer target after `--`:
`cargo run --locked -p xtask --target HOST -- cargo build --locked --target TARGET ...`.
Replace `HOST` with Cargo's host triple and `TARGET` with the destination triple;
the native CMake toolchain is still required. No cross/native platform result is
implied by configuring it.

The official binding receives its supported `LBUG_LIBRARY_DIR` and
`LBUG_INCLUDE_DIR` selectors from xtask; Graphit's existing `cxx` bridge receives
the same generated headers and the resolved official source include directories.
Both compile with CMake's selected C++ compiler and link the same library; the
binding does not build a second engine. Source preparation never invokes Cargo
from a `build.rs`. The product uses static Ladybug linkage; `LBUG_SHARED` is
rejected instead of silently changing the ABI. The product package includes its
native dependency closure and the Cargo package's licenses as before.

On Linux only, the existing archive symbol isolation is retained in xtask:
bundled strong C symbols are made local, and bundled ANTLR symbols (including
weak COMDAT signatures) receive private names. Public Ladybug C/C++ API names
remain intact. This avoids collisions with Graphit's separate ANTLR runtime and
Lance's C dependencies; it does not patch engine sources or replace its readers.
The isolated archive is regenerated only when CMake produces a newer input.
Native macOS/Windows linkage and execution need their own validation; Linux
execution does not qualify those platforms.

## Captured shards through the official extension API

`graphit-ast/native/icebug_query.cpp` creates only `Database(":memory:")`. Its
captured filesystem retains the same local-descriptor or authorized-S3 range
reader, namespace, publication and Original checks. Queries do not perform
`COPY FROM`, create a persistent `.lbug`, or materialize the graph into native
node storage.

The small Graphit adapter composes official `IceDiskNodeTable` and
`IceDiskRelTable` children through `ExternalTableHooks` and the official columnar
base classes. The extension slot is the upstream `StorageFormat::LANCE` enum;
it selects the Graphit factories only inside the ephemeral query catalog. The
payload is still Icebug Parquet/CSR, not a Lance dataset, and no Lance code or
protocol changes. The query API does not expose catalog DDL or extension loading.
Hook registration is process-wide; factories select the captured catalog by its
StorageManager identity, so concurrent queries cannot select each other's files.

Graphit owns shard selection, offset-aware semimask views, member-specific
property association/NULLs, local-to-captured node/edge identities and count/degree
composition. Official readers own Parquet decoding and CSR scanning. No copy of
the Icebug subsystem is compiled as an extension. Shards remain independent;
replacing a FILE changes only its publication and incident members.

Creating the ephemeral catalog does not open unrelated node-shard readers. Each
node label initializes its official tables and validates physical row counts on
first use, using the active transaction's context. Failed initialization publishes
no partial state. Creating the catalog also does not decode every relation's CSR. A relation
group validates its CSR through the official reader once when used, including
degree-only operations; node-only queries do not initialize unrelated CSR data.
Captured source/target row ranges discard unrelated parts before allocating scan
buffers. The official cached degree shortcut is used only for forward scans:
0.22.1's reverse degree API returns zero, so reverse traversal uses the official
reader and is never filtered with that value. These are operation-local states,
without a global cache or resource accounting.

In 0.22.1, the relation-source scan path bypasses the node scan-state hook and
creates a plain node state. The adapter keeps that operation's child reader state
when necessary and releases it after its morsel. Ordinary scans carry child state
in the hook-created scan state. This avoids patching the engine and must be
covered by real untyped relation-source queries, not just typed node scans.

## Memory and functional controls

Ladybug's own buffer pool, eviction, virtual reservations and buffer reuse are
used without Graphit engine changes. Explicit `ast.query.buffer_pool_bytes` and
query-thread settings continue reaching `SystemConfig`; zero selects the engine's
automatic memory policy. Response rows/bytes/depth, authorization, read-only
validation, cancellation and deadlines remain Graphit responsibilities.

Custom persistent WAL/checkpoint/recovery, detach and lazy-eviction patches have
no productive consumer in the current on-the-fly AST catalog and are removed.
Their mechanism-specific harnesses are retired. They are not fallback backends.
Functional deletion, isolation, public identities, Source and traversal coverage
remains with the Graphit progressive publication/query tests.

## Cypher function contracts

The query language comes from the official engine. In 0.22.1 use `size(text)`
for string length; `length` accepts recursive paths, not strings. Graphit does
not rewrite arbitrary Cypher or retain aliases from the former engine. Native
cancellation tests use a valid, CPU-bound `size(reverse(...))` expression so an
early binder rejection cannot accidentally qualify interruption.

The official engine also rejects `ORDER BY id(node)` because `INTERNAL_ID` is
not orderable. Use a public key such as `node.uid` or `node.entity_key` for
deterministic ordering, or select the specific entity being inspected. Without
`ORDER BY`, a parallel scan does not promise which row a `LIMIT` returns first.

## Affected verification

```sh
cargo test --locked -p xtask ladybug::tests::
cargo xtask cargo test --locked --offline -p graphit-ast --lib icebug_query::tests::
cargo xtask cargo test --locked --offline -p graphit-ast --lib antlr_engine::embedded::tests::existing_host_is_reused_for_plpgsql_bodies -- --exact
node scripts/test-native-packaging.mjs
```

The ignored product test
`source_server::remote_native::tests::official_ladybug_progressive_publication_and_remote_queries_s3`
runs through `scripts/test-record-storage-s3.py --require-parquet-ranges` with an explicit App test
executable produced by `cargo xtask cargo test --no-run`. It exercises temporary
STS credentials and a local S3-compatible service, incremental file
replacement/deletion, retained snapshots, reverse and multi-hop traversal,
Source, cancellation and changed-object rejection. The runner observes actual
Parquet query GET requests and requires a Range header. The negative fixture's
single intentional full GET is identified separately by its immediate one-byte
replacement PUT to the same original object; any additional full GET fails.
No provider account or
published native SDK is required. This contract alone does not declare native
platforms, remote CI or installed consumers qualified.
