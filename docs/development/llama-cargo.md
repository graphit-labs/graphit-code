---
title: llama.cpp through Cargo
description: Locked upstream engine, binding provenance, build prerequisites, devices and model contracts.
content-type: reference
audience: developers
updated: 2026-10-31
references:
  - target: {type: task, id: 01M5C6B85MCWH5HX932YVEKN3Z}
    relation: relates_to
---

# llama.cpp through Cargo

Graphit Code and Broker use `llama-cpp-sys-2` version `0.1.161` at the fixed
binding commit `424d1ffe6c1c4212d5da8e6075f473e33e35fb7f` in
[utilityai/llama-cpp-rs](https://github.com/utilityai/llama-cpp-rs/pull/1175).
This is an unmerged binding update, not a published crates.io release or an
official Rust binding from ggml-org. Its MIT/Apache-2.0 binding builds the
unmodified MIT upstream engine `620d2225cc436ab0b001e3200e590e3b286ef9ab`.
That engine includes EmbeddingGemma2 and the upstream
[absent-logits correction](https://github.com/ggml-org/llama.cpp/commit/ed019cd8b94a8194881e3d2448075156939d32a1).
The registry binding `0.1.160` does not include either requirement; it must not
be substituted merely because it is the latest published version.

Cargo.lock fixes the Git source. `cargo fetch --locked` fetches the binding and
its upstream engine submodule into Cargo's ordinary cache. Graphit ships no
engine source archive, patches, downloader, SDK or second compilation. The
binding's native CMake/bindgen builder owns engine compilation and link flags.
Graphit's existing `cxx` adapter uses public headers from
`DEP_LLAMA_GGML_CMAKE_DIR`; generated Rust bindings and C++ share that engine.

Build prerequisites are Rust, C/C++, CMake and libclang including its resource
headers, plus the workspace Protobuf compiler and development headers for its
other native dependencies. These are contributor/CI tools, not prerequisites
for installed users.
CPU uses the upstream GGML CPU backend and its configured worker threads;
OpenBLAS is no longer a build or runtime dependency. The dependency disables
curl, examples, tools, server and common helper compilation. Optional upstream
KleidiAI and externally fetched CCCL sources are not enabled by Graphit; enabling
those upstream options introduces network downloads not prepared by `cargo fetch`.
Models continue to use their existing separately verified download mechanism.

## Devices and build commands

Configuration still defaults to CPU. `provider.embedding` and `provider.rerank`
have independent local execution selection; see [local execution](../specs/rust_local_runtime.md).
The `graphit-ai/cuda` Cargo feature enables the binding's existing `cuda-no-vmm`
and `dynamic-link` features. Linux CUDA builds use the Cargo-built shared engine
and NVIDIA runtime/cuBLAS redistributables, staged by the existing package producer.
The package resolves those libraries through relative loader paths; users do not
install llama.cpp or the CUDA toolkit. VMM is disabled so CPU startup does not
require an NVIDIA driver. Actual GPU use needs a compatible host driver.
Set `CUDA_PATH` to the build toolkit when packaging a CUDA build; the producer
uses only that explicit toolkit and the matching Cargo build output, not a search
for checkouts or inherited loader overrides. CPU-only builds remain statically
linked to the engine. There is no separately compiled Graphit backend plugin.
The package includes both original NVIDIA CUDA Runtime and cuBLAS redistribution
license texts in its combined NVIDIA notice. No CUDA compiler or model
weights are shipped. macOS enables the same engine's Metal backend through its
Cargo target dependency; CPU selection still prevents GPU model offload.
Explicit unavailable devices fail; auto reports the chosen device and CPU fallback.

```sh
cargo fetch --locked
# CPU build
cargo xtask cargo build --locked --release -p graphit-app -p graphit-daemon --bin graphit --bin graphit-model-server
# CUDA build, with a supported NVIDIA toolkit and host compiler in the build environment
cargo xtask cargo build --locked --release -p graphit-app -p graphit-daemon --features graphit-app/cuda,graphit-daemon/cuda --bin graphit --bin graphit-model-server
# Broker, from its checkout
cargo build --locked --release -p broker-server --features cuda --bin graphit-broker
```

Linux CUDA release builds use CUDA 12.9.1 on Ubuntu 24.04 with GCC 13 and
`CMAKE_CUDA_ARCHITECTURES=75-real;80-real;86-real;89-real;90-virtual`.
CI prepares that build toolchain with a pinned third-party CUDA action; this is
independent of Cargo's engine sources. A newer host glibc is not made compatible
by choosing an older GCC alone: build in the supported distribution instead of
patching CUDA or llama.cpp headers. CPU builds require no NVIDIA toolkit.

The selected binding's shared-asset copier only selects `*.so` and hardlinks
those entries. With upstream's default versioned symlinks, a later native build
can fail with `AlreadyExists` on a broken copied link. Both workspaces therefore
set the official CMake variable
[`CMAKE_PLATFORM_NO_VERSIONED_SONAME=ON`](https://cmake.org/cmake/help/latest/variable/CMAKE_PLATFORM_NO_VERSIONED_SONAME.html)
through the binding's existing `CMAKE_*` forwarding. Private engine libraries
are real `libllama.so`/`libggml*.so` files. This changes library names, not engine
source, model recipes or the C++ ABI. Cargo.lock and package provenance identify
the exact engine; Graphit does not expose these private files as a system ABI.
The package producer always selects the actual Cargo build output, not the
binding's convenience copies in `target/release` or `deps`.

The existing package producers consume Cargo metadata, the successful Cargo
JSON artifact stream (including its native build output), and the actual Release
executables. Engine/binding notices are copied from the resolved Cargo source;
no checkout search, old source archive or native SDK receipt is used. Existing
native dependency closure and relative loader-path checks remain in force.

## Models and lifecycle

EmbeddingGemma2 keeps its BF16/F32 artifacts, 768-dimensional normalized output,
query/document prefixes and document windows. BGE remains
`BAAI/bge-reranker-v2-m3` GGUF F16 with true query/document pair classification.
Neither model revision nor vector recipe changes merely because the engine build
changes. Per-content checks and normal model-space compatibility remain intact.

`search.rerank=false` remains the default: configuring a model does not prepare,
load or infer it. Model loading remains demand-driven and
`models.idle_timeout_seconds` unloads idle embedding and rerank workers. Original
cancellation/deadlines and actual worker/process joins are retained. Broker
provider selection continues to route inference exclusively through Broker;
this engine substitution does not alter remote protocols or authorization.

Native Windows/macOS, other GPUs and remote CI require their own execution
evidence. A successful local CPU/CUDA fixture does not qualify those platforms.

## Execution evidence and limits

The real BF16 CPU query fixture passes, including in a container without an
NVIDIA driver using the CUDA-enabled executable. BGE F16 passes CPU and CUDA
paired scoring, score association, ordering, cancellation/reuse and worker join.
The CUDA pair fixture also passes at 8,192 tokens. The diagnostic EmbeddingGemma2
CUDA fixture passes retained-vector angular/ranking checks, the 8,009-token
single-window case and Unicode document windows. Its first quiet attempt returned
a native error; the diagnostic rerun did not reproduce it and no cause has been
established. No retry or silent CPU fallback was added.

The four CPU recipes retain cosine similarity of at least 0.9999931 to the
previous engine's BF16 vectors, within the existing CPU/GPU compatibility contract
of 0.99999. Query coordinates match exactly; document/code recipes have maximum
absolute differences of 0.000516. Both reference retrieval orders are unchanged.
Comparison with the independent F32 oracle remains at least 0.9999947. The original
engine fixture is retained for compatibility; a separate measured upstream fixture
provides the strict per-coordinate regression reference. No existing vectors or
model identity were changed. This is numerical compatibility, not a claim of
identical outputs on every input or device.

The complete Linux Code and Broker packages were installed with their private
Cargo-built engine libraries and CUDA runtime dependencies; no upstream library
installation was needed on the client. Installed CLI queries and an isolated
MCP client (67 tools) exercised AST, Knowledge, Memory and Task against Broker.
AST hybrid retrieval with three reranked results, Knowledge hybrid retrieval,
Knowledge textual retrieval with an actual rerank score, and Memory/Task hybrid
retrieval returned existing records without reindexing.

The installed Broker had an explicit two-thread CPU override. A Knowledge hybrid
request with three larger candidates reached the 120-second MCP forwarding
deadline; without rerank the reproduction took 12.391 seconds. A focal native
probe of one 2,876-token pair spent 41.6 milliseconds creating its context and
32.325 seconds encoding. Reusing contexts therefore does not address the dominant
cost. An isolated upstream BLAS probe improved that pair, but a larger 6,889-token
pair still took 95.5 seconds on two CPU threads. No BLAS dependency, altered
FlashAttention policy, truncation, quantization or deadline increase was added.

With explicitly authorized `auto` execution, the existing packaged CUDA backend
processed the fixed three-document Broker rerank request in 3.983 seconds, with
the returned indices and scores associated correctly. The hybrid MCP query also
completed with rerank; its candidates had changed as background indexing advanced,
so that timing is functional evidence rather than a controlled speedup. CPU remains
the product default. Omitting Broker `upstream.threads` selects half the available
logical CPUs rounded up; an explicit positive value overrides it. Device and
threads are independent settings for embedding and rerank. Large F16 reranking on
a small CPU allocation still has no universal latency guarantee.

After installing the final Code and Broker packages and restarting their services,
an isolated MCP client negotiated the 67-tool catalogue. Knowledge FTS completed
in 3.814 seconds, hybrid without rerank in 21.138 seconds, and hybrid with rerank in
23.539 seconds. Each returned three distinct records; hybrid results included
compatible semantic distances, disabled rerank emitted no scores, and enabled
rerank returned three finite scores in descending order. The fixed three-document
Broker request completed in 5.361 seconds with the same scores as before final
activation. These functional timings include different startup/background load
and live candidates, so they are not a controlled performance comparison. Both
models reported `Auto` selecting `CUDA`; the four existing stdio proxy processes
survived daemon replacement. The affected suites passed 79 tests in total; this
does not qualify native Windows/macOS or remote CI.

Broker local semantic recipe and cache identities no longer include device or
thread count. The default is derived from the verified model manifest; a deployment
may explicitly name its unchanged recipe with `upstream.recipe`, versioning the
label when weights, precision or the actual recipe change. This keeps compatible
persisted embeddings reusable when moving execution between CPU and GPU, without
rewriting datasets. Manifest and weight validation remain mandatory.

The original initial admission denials did not retain an exact typed cause. A
separate reproduced defect invalidated Runtime service leases when an unrelated
provider or inactive profile replaced the shared auth document. Runtime gates now
compare the selected provider/profile and credential revisions; selected-token
rotation, logout, policy/key revocation, installation/privacy checks, cancellation
and deadlines still reject the old lease. Transport, malformed discovery/JWKS and
configuration errors remain failures but are no longer labelled permission denials.
A genuinely expired access token still requires the normal profile refresh; no
TTL or authentication policy was changed. Lexical Knowledge membership scans now
project document columns instead of reading vectors from the unified dataset.

The installed-library CPU idle test passed in 15.50 seconds: embedding unloads
while a peer is active, both model owners are released, and both reload with the
same vector/config identity before joined shutdown. Three cold/disabled-rerank
contracts passed without preparing the missing bundles; the real opt-in RPC
check passed in 3.63 seconds. The packaged-child integration passed in 23.62
seconds and its five nonignored bootstrap/cancellation/completion contracts
passed after selecting the complete package consistently. Compilation and Cargo
lock waits are separate from these test durations.
