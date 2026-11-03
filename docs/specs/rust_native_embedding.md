---
title: Native EmbeddingGemma2 SDK library
type: specification
references:
  - target: {type: task, id: 01M5CF5RHWVG4GRQ9TS1P63FXG}
    relation: describes
---

# Native EmbeddingGemma2 SDK library

Project: graphit-code. Session: ses-c98a. Structural parent: tsk-c471,
requirement N3; inference migration front: tsk-e6ff.
Source owner: tsk-7e88; Main composition: tsk-73e6. The sealed SDK source
was integrated on 2026-10-11 with its existing BF16 qualification fixtures.
Status: synchronous SDK and worker are integrated and qualified in Main
on CPU Linux using the pinned BF16 model. Model-service integration and
consumer qualification are tracked separately in [Rust_shared_model_service](rust_model_service.md).
Source fmt passed after diagnostic formatting, and scoped strict Clippy
passed. Tracker's Source readback was NOFAULT, including the IO/batch
corrections; the reviewer did not execute runtime tests. Final delivery
and Main documentation readback are recorded by the composition owner.

The library provides synchronous local embeddings with explicit model
identity, weight precision, dimensions, limits and original inference
context. It reuses the existing embedding result and artifact reader.
ModelManifest work belongs to tsk-3832; worker and M/E integration,
provisioning, model selection and corpus identity remain separate work
in tsk-5b17/tsk-d232 under Root coordination.

## API and ownership

The Rust implementation is
[native_embedding/mod.rs](../../crates/graphit-ai/src/native_embedding/mod.rs).
The signatures below describe the real interface; Result carries
NativeEmbeddingError.

```text
NativeSdkRoot::new() -> Result<NativeSdkRoot>

NativeEmbeddingClient::load(
    source: GgufModelSource,
    options: NativeEmbeddingOptions,
    limits: EmbeddingLimits,
    context: InferenceContext<'_>,
) -> Result<NativeEmbeddingClient>

NativeEmbeddingClient::validate_request(
    options: NativeEmbeddingOptions,
    limits: EmbeddingLimits,
    texts: &[&str],
    input: NativeEmbeddingInput,
    context: InferenceContext<'_>,
) -> Result<()>

NativeEmbeddingClient::embed(
    &mut self, texts: &[&str], input: NativeEmbeddingInput,
    context: InferenceContext<'_>,
) -> Result<EmbeddingBatch<'_>>
```

`model()` and `revision()` borrow the client's captured identity;
`dimensions()` returns the selected width. EmbeddingBatch owns its
vectors and borrows the model identity. Its existing `vectors()` and
`into_vectors()` accessors are reused. Revision remains available from
the client; it is not an invented field in EmbeddingBatch.

NativeSdkRoot is an opaque, non-Send bootstrap guard. Construct it once
on the bootstrap thread and retain it until all model workers have
actually joined. C++ publishes a weak kernel reference under a mutex;
root destruction closes admission before releasing its shared_ptr.
Each NativeOwner retains its own shared_ptr<Kernel>, so backend free
occurs after the last owner is destroyed. A second initialization, or
owner creation after root close, fails. This ownership mechanism alone
does not implement M/E admission, drain, or all failure-path joins.

NativeEmbeddingClient is non-Send. Create, use and destroy it on the
model thread. `&mut self` serializes calls on that client. CXX opaque
UniquePtr owners and C++ SDK deleters manage model, context and batches;
there is no Rust raw pointer API or unsafe Send/Sync implementation.
The shared kernel does not establish shared weights across clients or
qualify parallel encodes.

## Model descriptor and verified staging

GgufModelSource contains `model: String`, `revision: String`,
`root: PathBuf`, `path: String`, `minimum_bytes: u64`,
`maximum_bytes: u64`, and `sha256: [u8; 32]`. It has no diagnostic or
serialization traits. It is data, not authorization to open a project
or an Installation resource. The caller must obtain the actual binding
and authorization before load.

Load rejects empty model/revision, zero minimum and maximum below
minimum. It canonicalizes the root and reuses the narrow crate-visible
[read_artifact](../../crates/graphit-ai/src/model_artifact.rs) function:
relative-path validation, canonical containment, regular-file/minimum
checks, 64 KiB reads, original-context checks and final SHA-256 check.
The consumer callback checks the cumulative maximum with checked
addition before each write. Integrity and reader errors retain the real
LocalRuntimeError cause; exceeding the maximum is ArtifactLimit.

Bytes are written to `model.gguf` in a private tempfile TempDir, flushed
and verified before the SDK opens the staged file. The native field
drops before the TempDir field: native mappings finish before directory
removal. Failed construction also releases these owned resources.
The descriptor and staging do not deliver global model provisioning,
active selection or an authority-bearing model lease.

## Options, limits and input recipe

NativeEmbeddingOptions requires canonical `provider: LocalProvider` and
`device_id: i32` alongside `threads: NonZeroU32`,
`max_tokens: NonZeroU32`, `dimensions: usize`, and
`precision: NativeWeightPrecision::{Bf16, F32}`. There are no discovered
defaults. Threads must fit i32; max_tokens cannot exceed 8192; dimensions
must be 128, 256, 512 or 768 and fit EmbeddingLimits.max_dimensions.
Native threads are a compute profile, not a CPU quota or RSS ceiling.

Whole-request validate_request runs before vector/prepared-string
allocation and is also available before a consumer loads a worker.
It calls the existing EmbeddingLimits input validation for text count,
raw UTF-8 bytes, dimensions and checked count × dimensions. It then
checks every nonempty text and cumulative prefix-plus-text bytes against
the same max_input_bytes, with each prepared text fitting i32 length.
Unicode, newline and tab are accepted; there is no invented whitespace
ban. An empty batch passes these checks and returns an empty batch after
the original checks; an empty individual text fails InvalidRequest.

This native adapter executes texts sequentially. The existing HTTP
request/response JSON ceilings and max_batch field do not create a
native HTTP operation or a native batch policy. Vector and prepared
string reservations are fallible. Failure on any item returns an error,
without publishing a partial EmbeddingBatch or executing later items.

| NativeEmbeddingInput | Exact prefix, including final U+0020 |
| --- | --- |
| Query | `task: search result \| query: ` |
| Document | `title: none \| text: ` |
| CodeQuery | `task: code retrieval \| query: ` |
| CodeDocument | `title: none \| text: ` |

The embedded GGUF tokenizer counts the complete prepared input with
special tokens. Inputs fitting the caller's max_tokens (at most 8192)
retain the original joint prefix-plus-text tokenization and encode path.
Query and CodeQuery above that bound fail TokenLimit before encode.

Long Document and CodeDocument inputs produce one vector for the complete
text. The same loaded SDK tokenizer encodes the role prefix with configured
specials and the content without automatically added specials; parsed special
tokens and Unicode byte-fallback IDs remain intact. Consecutive, nonoverlapping
content-token windows consume every content token once. Each window repeats
the native prefix/BOS and appends the configured EOS, all within max_tokens.
If the prefix and specials leave no room for a content token, TokenLimit is
returned before encode. No text is truncated and no extra model is loaded.

Each window uses noncausal mean pooling. Its raw 768-value sequence output is
copied into a double-precision accumulator weighted by the number of content
tokens consumed, excluding the repeated prefix and specials. The weighted
mean receives one final F32 L2 normalization; the existing Rust Matryoshka
reduction then applies. The formerly valid short vectors and recipe identity
remain unchanged; previously over-limit documents had no vector to preserve.

A context requests n_ctx, n_batch and n_ubatch for the complete first (largest)
window, up to the unchanged 8192-token limit, with one sequence. The SDK may
round context capacity for its kernels; this does not truncate the input. It is local to one native embed/encode_document call,
cleared and reused between that document's windows, then destroyed through
llama_free before returning to Rust and the RPC. The same scope releases the
context on native errors, cancellation, deadline and CXX exceptions. It is
constructed after the original callback-slot Borrow, so context destruction
finishes while that slot and RequestState are still valid. The next native
call creates a new context; the verified model, pin and SDK owner stay resident.
There is no retained growing context or replacement quota/cache pool. A batch
with multiple texts uses a context per text, retaining none between RPCs.

This releases SDK compute-buffer ownership between calls. The peak workspace
still depends on the selected SDK attention kernel; no exact idle RSS is
promised, since process allocators and the OS can retain other memory. Context
creation and graph reservation now repeat per native call, without reloading,
copying or rehashing the model. No isolated construction-cost claim is made.
All windows share the original absolute deadline and cancellation signal. Checks between windows can discard the entire result;
there is no partial vector/batch or per-window timeout renewal. Existing request
byte and output bounds apply to the full request. Windowing does not guarantee
that a long document will finish within the existing operation deadline.

The native profile uses noncausal ENCODE and mean pooling. CPU requires index
zero and disables GPU layers/offload. CUDA and Metal select a single device from
the pinned SDK's mature backend registry, use all model layers on that device
and enable KQV/operation offload. Metal requires macOS/index zero; CoreML is
unsupported for GGUF. Plugin discovery is serialized and occurs only for
Auto/CUDA/Metal demand. Auto may select CPU only when no supported accelerator
registers at the requested index; model-load/integrity/context failures do not
retry on CPU. `registered_device()` reports the requested and selected device,
including a finite fallback reason for Auto→CPU. This is configuration/registration
metadata, not proof that every native operation executed on GPU. Hardware execution
needs its own qualification; historical CPU receipts below remain CPU evidence.
CPU inputs fitting max_tokens retain `LLAMA_FLASH_ATTN_TYPE_DISABLED`;
accelerator operations request `LLAMA_FLASH_ATTN_TYPE_ENABLED` for both short
inputs and document windows. CPU documents entering the existing multi-window
branch retain AUTO. In the pinned SDK, the AUTO probe's one-token shape can
reject the accelerator's 512-wide flash kernel before evaluating the complete
window; ENABLED permits normal per-operation backend support checks instead.
The same request-local context and attention selection cover every window of
that document. There is no new
length threshold or public option. The CPU path retains the pinned SDK's
GEMM/BLAS backend where supported.

`GGML_SCHED_DEBUG=2` is the existing SDK's explicit local diagnostic setting.
When positive, the native bootstrap restores the SDK's public default log callbacks;
otherwise logging remains quiet. Its operation/backend trace distinguishes registered
CUDA model placement from actual per-operation CPU/CUDA scheduling. A selected
accelerator does not imply that unsupported tensor shapes execute entirely there:
the SDK may schedule individual operators on CPU without changing the selected
model device or silently retrying the whole request as a CPU configuration.

The SDK resolves AUTO from actual backend support and graph placement. For
noncausal attention without a KV cache, selected flash kernels convert F32 K
and V tensors to F16 internally while accumulating in F32; configured K/V types
remain F32. Model weights/pin, caller-selected threads, context/max_tokens
ceiling of 8192, complete content-token coverage and weights, raw MEAN pooling,
final F32 normalization and Rust Matryoshka reduction are unchanged. Kernel
selection is internal to the CPU implementation, not a model ABI or input-limit
change. Long-document values are not promised bit-for-bit equal to a disabled-
flash path. The [worker qualification](rust_native_embedding_worker.md) records
the passed full-payload Native regression and separate one-file Linux Source/NN
consumer. These results qualify their recorded inputs and profiles; the full
Source08 corpus, Release activation and native Windows/macOS remain pending.

The SDK abort callback remains cooperative between graph nodes or fusions;
it does not preempt a running native kernel or BLAS call. Checks before and
after encode and between windows still use the original deadline/cancellation,
discarding partial output on failure. Context and batch ownership remain local
to the request and are freed before its callback borrow ends.

The model's learned projection supplies 768 outputs. Native output is
checked finite, normalized with F32 L2 and rejected for nonfinite/zero
norm. For 128/256/512 dimensions, Rust takes that prefix and renormalizes
in F32. Token truncation and Matryoshka vector reduction are distinct.
Corpus/query consumers must preserve model revision, precision, recipe
and dimensions.

Load requires architecture `gemma-embedding2`, output width 768 and the
explicit native ftype: Bf16 requires LLAMA_FTYPE_MOSTLY_BF16; F32 requires
LLAMA_FTYPE_ALL_F32. A mismatch, including other quantization/precision,
fails before creating an inference context. BF16 was exercised in Q926b
and now in the affected private Source suite. Native F32 GGUF
has not been qualified. The official FP32 oracle is a separate runtime
and weight domain, and does not qualify that branch.

## Original context, cleanup and errors

RequestState clones the same Core Cancellation signal and keeps the
same absolute Instant from InferenceContext. Callbacks reconstruct the
context over those originals. Checks occur before artifact work/load,
in load progress, after load, before each item, in the native abort
callback, after native processing/copy, between document windows and after Rust MRL reduction.
Deadlines are not reset between items or SDK phases.

C++ Borrow attaches RequestState only during a synchronous operation
and clears the callback slot on scope exit, including error paths.
SDK model/context/batch owners use RAII. A nonzero llama_process status
returns the real native failure; the batch is freed before the local context,
and model weights remain available for a later call.
Post-copy original failure clears or discards the result rather than
publishing a vector. Callback cooperation does not promise hard CPU
preemption at an arbitrary instruction or a fixed cancellation latency.

NativeEmbeddingError exposes `kind()` and `native_status()` plus the
standard borrowed Error::source chain. Actual kinds are Context,
Configuration, Embedding, Artifact, ArtifactLimit, Native, Tokenizer,
TokenLimit { actual, maximum }, Output and Normalization. Existing
EmbeddingError carries common limit, invalid-request and allocation
failures. Native result failures preserve controlled kind/status;
CXX exceptions and artifact/IO failures preserve their real source.
native_status is Some only when the SDK result reports processed=true.
Admission, tokenizer and token-limit rejects that never processed an
input retain None; they do not invent a successful status zero.
When an original check fails on a native failure path, its Context kind
takes precedence while the underlying native cause remains retained.

Debug/Display of NativeEmbeddingError reveal only kind/status, not
model input, identity, paths or raw SDK diagnostics. Fixed native
exception codes remain available through source(); the SDK installs
quiet llama/ggml log callbacks at kernel initialization. Callers must
not turn retained causes or model payloads into secret-bearing logs.

## Cargo engine build boundary

The existing `cxx` bridge preserves model ownership, callbacks and owned outputs.
Its build script compiles only Graphit's adapter against the public headers
installed by the single Cargo `llama-cpp-sys-2` dependency. Engine compilation,
link flags and optional CUDA/Metal backends belong to the binding's native builder.
See [the exact binding/engine pin and build prerequisites](../development/llama-cargo.md).
The engine is upstream without patches; the upstream absent-logits correction
replaces Graphit's former patch. There is no local tarball, SDK, BLAS receipt or
second accelerator engine. Model artifacts, query/document recipes and vector
identity are unchanged by this build substitution.

## Tests and qualification domains

The staged [tests.rs](../../crates/graphit-ai/src/native_embedding/tests.rs)
contains two ordinary tests for request budgets/context and artifact
integrity/maximum/path/redaction, plus one ignored real-SDK test.
Both ordinary tests and the real-SDK test passed in the private workspace.
The following are the affected commands; the ordinary receipt used the
explicit skip form shown here:

```text
cargo test --offline --locked -j1 -p graphit-ai --lib native_embedding::tests -- --skip actual_sdk
cargo test --offline --locked -j1 -p graphit-ai --lib native_embedding::tests::actual_sdk -- --exact --ignored --nocapture
```

The ignored command requires GRAPHIT_EMBEDDINGGEMMA2_MODEL pointing to
the provisioned GGUF: revision bfcd298762cc34d0357ece5ebdd31791a3a374d8,
557950176 bytes, SHA-256
`68bae29d62fb8c7d23e98d21fd4662753ddd636e6b62b8a70dfe059a9844f216`.
The ordinary suite does not replace this dependency with a stub.

The real test exercised four roles against the pinned BF16
fixture, MRL dimensions, load/abort/post-copy cancel and deadline,
precision mismatch, 8193-token rejection, reuse after abort and two
owners with profiles one/two threads. It retires A through a real join,
then checks B's exact before/after values, joins B, drops root, checks
closed admission and verifies removed staging directories. Encodes are
sequential. This is actual private Source coverage, not product-consumer
or all-failure-path qualification.

Qualification evidence resides in the repository-relative private
directory `.build/rust-ai/embeddinggemma2-sdk-qualification`. These
receipts and the qualification handoff are not shipped API artifacts.
The filenames below refer to that evidence directory.

`source-unit-receipt.json`
records exit0, two PASS and runner wall0.273062s.
`source-actual-receipt.json`
records exit0 and real SDK execution, runner wall41.258865s. Its command
includes cache/Cargo work (5.05s reported) and test execution (36.14s
reported); the runner total is not isolated inference latency or a
performance comparison. Source fmt passed after diagnostic formatting.
`source-clippy-receipt.json`
records exit0 for `cargo clippy --offline --locked -j1 -p graphit-ai
--lib -- -D warnings`, runner wall204.128004s. The reported profile time
of3m24s includes SDK compilation in its own profile with CMake -j1;
it is not inference latency. This is a scoped library check, not an
all-targets/workspace claim. These statuses are not inferred from Q's
separate commands. Tracker's corrected Source readback was NOFAULT;
that review itself did not execute runtime tests.

The native-load callback fixture first obtains a verified staged model
through public load. Its request Instant is created before the native
phase/helper, preserving that phase's original deadline; artifact-reader
and native callback checks are separate exercised operations. The earlier
250ms diagnostic expired after about829ms before reaching callback2 and
did not establish the intended callback coverage. The passing fixture
uses the original 10s deadline and requires at least two load callbacks.
This does not reset a deadline within an operation or prove hard
preemption during every load instruction.

A two-item batch matched two single-item calls in order and cardinality.
The failure fixture cancels the original on the second item's callback,
requires exactly two observed callbacks and returns no batch; a following
batch with its own valid original context recovers. Later items are not
used to repair a failed batch or publish partial output.

The same-SDK BF16 fixture comparison uses a 1e-5 numerical adapter
regression tolerance. Comparison to the official FP32 oracle prints
cosine/max-absolute-error observations; it does not introduce a retrieval
quality, parity or SOTA gate. Q926b's already completed driver/oracle
qualification and current-vs-historical lifecycle domains are recorded
in `qualification.md` in the private evidence directory identified above.
Its four-role observed cosine range was 0.999995172–0.999998689 and max
absolute error 0.000208721–0.000359866, in CPU Linux only.

Q's long 8192-token run used the preserved single-owner Source before
the shared-kernel change: encode 199.117584s, runner 201.733402s and 757
callbacks. It was not repeated for the current kernel or this Source.
The SDK's 2128.25 MiB compute buffer report was not RSS or a RAM floor.
Do not replay this expensive historical case merely to transfer a
lifecycle claim. No performance gain, GPU, other OS, parallel encoding,
all-failure join, active provider/corpus or full M/E proof is declared.

## Document-window qualification (2026-10-19)

In the initial windowing qualification, the affected
`native_embedding::tests::actual_document_windows` focal ran once
on CPU Linux with the same pinned BF16 model and two fixture threads. Four
short roles at their exact token bounds matched the existing BF16 vectors
within 1e-5. Document and CodeDocument each used two windows with a test-only
max_tokens=64 and returned one finite L2-normalized 768-vector. Unicode and
parsed special tokens, a changed final tail, MRL128, query rejection and an
impossible prefix budget were exercised. Cancellation and deadline expiry
after the first window returned the original error without a partial batch;
the same owner then reproduced a short golden vector and removed its verified
staging directory on drop.

The test finished in 21.46s (driver 21.469031s), including artifact preparation,
load, all encodes, the deadline fixture and cleanup. This is not isolated
inference latency or an 8192-token benchmark. Two ordinary affected native
tests passed. The short comparison is numerical, not a bitwise guarantee;
window token coverage and raw weighting were also checked structurally, and
the changed-tail assertion is not an independent pooling oracle.

Run the real focal in its own process because NativeSdkRoot admits one root
per process. With GRAPHIT_EMBEDDINGGEMMA2_MODEL provisioned:

```text
cargo test --offline --locked -j4 -p graphit-ai --lib native_embedding::tests::actual_document_windows -- --exact --ignored --nocapture
```

Scoped library strict Clippy passed in 79.754267s driver time, separately
from inference. Source rustfmt and whitespace checks passed. The first build
failed on three test-only `unwrap_err` Debug bounds; the corrected build
passed and that failed log is retained. No native runtime failure occurred.

Private receipts are in `.build/native-document-windows-01`. No Source RPC,
release activation, GPU or other-platform execution qualifies this windowing
change yet. Existing byte limits, production max_tokens=8192 and operation
deadlines were not raised.

## Request-local context qualification (2026-10-19)

The context-lifetime correction removes the per-owner growing context while
retaining the resident verified model. Its affected existing
`native_embedding::tests::actual_document_windows` focal passed once with the
same BF16 pin, two fixture threads and max_tokens=64. An immediate short golden
after the two document roles now also exercises short → multi-window → short
on one owner. Existing four-role goldens, long-document/tail/MRL behavior,
query/no-progress errors, cancellation/deadline followed by short recovery and
staging cleanup all passed without weakening their assertions.

Build time was 10.394175s driver (10.31s Cargo), separately from the real test's
21.712582s driver (21.70s harness, including load, encodes and deadline fixture).
Two affected ordinary tests also passed; scoped AI-library strict Clippy passed
in 6.004483s driver time, with source rustfmt and whitespace checks passing.
These totals are not an isolated context-construction benchmark; the coordinator's old Source was still indexing
concurrently. Review confirmed local unique_ptr destruction before Borrow clears
the callback slot, and batch destruction before context destruction on failure.
No synthetic counter or RSS quota was added. The review did not execute tests.

Private receipts are in `.build/native-context-lifetime-01`. This library focal
and the RAII ownership proof do not establish idle RSS for a productive
8192-token workload. The installed Source/PID and package from the previous
integrated release are unchanged by this source correction; re-emission,
activation and productive NN/idle observation remain coordinator-owned.

## Source GGUF integration focal

The ignored App-library focal
`foreground_source::foreground_memory_jobs_tests::native_gguf_source::native_gguf_source_two_projects_and_long_document`
reuses the existing two-PROJECT foreground fixture, installation authority,
Memory seed and actual Models child. It selects the production pinned manifest
at 768 dimensions and 8192 tokens with the product CPU runtime. The provisioned
asset is hard-linked on its own filesystem, without a weight copy or download.

The focal compiles. With the Runtime futures boxed, execution has exercised
supervisor-driven AST, Memory and Knowledge publication and semantic searches
for both main and feature, including generations, finite distances and a pinned
768-dimensional model selection. A valid protected data operation without
credentials returned the exact MCP authentication error with no data payload.
The long Query returned the typed NativeTokenLimit on that same actual child.

The first long Document RPC used a fresh original request and the unchanged
120-second product deadline, then failed with ModelServiceError Deadline and
service_profile Deadline before returning a vector. This records the actual
operation failure; the relative time spent queued or encoding was not measured.
That failure preceded the document-window AUTO change. The existing real BF16
actual_document_windows focal then passed with AUTO requested for its long
windows: four unchanged short-role goldens, two long-document roles, Unicode
and special-token tails, finite normalized 768-vectors, MRL, cancellation,
deadline and context reuse. Its captured 64-token budget validates the affected
Native path; it does not qualify Source/default8192. Native AI library strict
also passed.

A subsequent real Source run with the resulting Models binary exercised both
projects and the exact denied-credential operation, then returned the typed
long Query limit and one finite normalized 768-vector for the complete long
Document, using default8192 and a fresh Original with the unchanged 120-second
deadline. This run took 181.201388 seconds and failed only at the final
cancellation oracle: its helper attempted to construct an Original from an
already-cancelled token, so policy construction failed before graph_entry::run
and was wrapped as io::ErrorKind::Other. It does not demonstrate a Source
cancellation defect or a complete focal pass. The foreground was cancelled and
joined after that assertion failure.

The corrected oracle creates the Original while its token is active, cancels
that same token, and passes the Original to the real graph_entry::run with a
valid project/version search. It requires exactly Interrupted and empty output;
the consumer checks the Original before discovery or transport. The complete
modified focal then passed in one execution: 1 test, exit 0, 114.194496 seconds,
with App compilation recorded separately. Both PROJECT/version publications
and real searches, denied selectors, the exact authentication error, long Query
limit and Document vector, Interrupted with empty output, successful foreground
join and closed child/Source listeners all passed. The recorded Models binary
was the qualified document-window AUTO release; default8192 and the original
120-second request deadline were unchanged. No idle RSS measurement was taken.
Earlier deadline failures, stack aborts and the invalid HTTP catalogue-status
assertion remain failed evidence and are not counted as functional passes.

Required consumer evidence is supervisor-driven publication for main/feature and
real Source HTTP/MCP searches through the CLI wire, including generations,
finite semantic distances, exact project/version selection and denied selectors.
A separate operation on that same actual child checks the complete long text:
the native query wire must report NativeTokenLimit, and Document must return
one finite normalized 768-vector under the unchanged request policy. The long
text is not claimed to be published as a Source NN entity. Cancellation and
foreground shutdown retain and await the real child and listener owners.

This bounded fixture does not qualify the earlier Code/main corpus or erase
its observed failures. Execution and any supersession of that corpus-specific
acceptance remain with the session coordinator.

## Documentation and integration boundary

Source-to-document comparison covers the actual API, precision guard,
limits, staging lifetime, callbacks/errors and build configuration.
Document-to-Source comparison uses the linked implementation and tests;
test assertions and Q results remain separate evidence domains.
Main AI_Models_Providers_and_Agent_CLIs, lines38–43, now describes the
historically activated typed ModelManifest backends ONNX and EmbeddingGemma2GGUF
(tsk-3832 completed r14). Native initialization and the model service
are documented in [Rust_shared_model_service](rust_model_service.md),
including the integrated GGUF service and its real single-query RPC proof.
That evidence is separate from this synchronous library's document-window
focal; productive Source indexing and querying require their own integrated
consumer evidence.

Root coordinates globals, pins/exports and heavy compilation. Consumers
must supply actual authority, model provisioning/selection, worker
ownership and joins, and corpus identity. This leaf does not change
Broker, Auth, Runtime, Data/Lance, Knowledge, AST, Core or FS. It adds no
universal grant, formal custody gate, Native resource reservation or RAM
floor.


## Current CUDA device fixture limitation

The explicit CUDA0 short-query fixture ran the actual pinned BF16 model on
Linux/RTX3060 and returned finite normalized output. Its predeclared maximum
absolute-error gate `1e-4` against the retained CPU golden failed: measured
`0.0002878215084852233`. Cosine `0.999997652768673` passed its separate
`0.99999` bound. A diagnostic run with the SDK's
`GGML_CUDA_CUBLAS_COMPUTE_TYPE=f32` produced the same numbers and failed the
same unchanged gate. Neither result qualifies Gemma CUDA numerical acceptance.
No production environment override or relaxed tolerance was introduced.
The successful paired BGE CUDA fixture is a separate model qualification.


The follow-up functional comparison retained the independently predeclared
cosine bound `>=0.99999`, extending it to all four existing role goldens and
one newly computed distractor document. All five comparisons passed finite
768-value shape and normalization; minimum cosine was `0.999991378726812`,
maximum coordinate drift `0.000517803244292736`. The general and code queries
preserved the complete CPU ordering of the same three documents and the
relevant first result. The initial coordinate-only bound was superseded because
backend identity is not the retrieval-quality contract; its failed receipts
remain failures. This is a small functional comparison, not a general quality
benchmark, all-platform proof or zero-loss claim. The specific kernel causing
the drift is not established by the unsuccessful F32-environment experiment.

The follow-up took 54.35 seconds in the test and 54.83 seconds wall time,
including separate CPU/GPU artifact staging and loading. One-second process
sampling observed CUDA libraries and up to 386 MiB of VRAM; neither that sampled
maximum nor the registration proves that every operation ran on GPU. No SDK,
weight, production computation policy or CPU golden was changed to obtain this
result. Only the one new distractor needed a fresh CPU reference.


The operation-sized context and accelerator ENABLED policy were then exercised
on Linux/RTX3060 with the same pinned BF16 model and SDK. The actual CUDA focal
passed all four role goldens and the distractor at the unchanged cosine bound,
with identical general/code retrieval ordering. It also completed an 8009-token
short document, Document and CodeDocument inputs with a full 8192-token first
window plus a partial second window, and cancellation after one completed window
followed by owner reuse. The SDK trace included CUDA FLASH_ATTN for the full
window and CPU scheduling for unsupported shapes. CPU document-window coverage
had already passed with the operation-sized context and was reused; the later
attention-policy change applies only to the selected accelerator.

The full-window focal first aborted during cuBLAS handle creation while the old
model-server retained another GPU model. The identical executable, inputs and
SDK settings passed after that second model process stopped (35.64 seconds test,
35.79 seconds wall). The failed receipt remains a failure: ENABLED is not a
promise that all models and desktop workloads fit concurrently in available
VRAM, and this representative input is not a reconstruction of the unavailable
historical AST RPC payload. The product does not hide allocation failures by
changing devices, truncating input, padding tokens or lowering the configured
window. No CPU golden, model precision, recipe or acceptance tolerance changed.
