---
title: Rust CLI model installation and selection
description: Typed model commands in the Rust executable with native installation paths and cancellation.
type: specification
updated: 2026-10-29
references:
  - target: {type: task, id: tsk-5f7d}
    relation: implements
  - target: {type: task, id: tsk-27c1}
    relation: uses
  - target: {type: task, id: tsk-13b9}
    relation: uses
---

# Rust CLI models

The Linux Rust executable exposes the existing model producer through its single
Clap parser:

```sh
graphit models install embeddinggemma2
graphit models install bge-reranker-v2-m3
graphit models select embeddinggemma2-bfcd298762cc34d0357ece5ebdd31791a3a374d8/model-128.json
graphit models --help
```

Installation supports pinned EmbeddingGemma2 BF16 and BAAI BGE Reranker V2 M3
F16 bundles. `--file` captures a prepared GGUF under the original request and
verifies its exact digest and size; it accepts either model. Both models can
download their published artifacts through immutable repository revisions. An
existing valid bundle is verified and reused before network access. Downloads
use the same bounded streaming client, exact byte count and SHA-256 validation;
the response ceiling admits either pinned artifact. Redirects remain restricted
to the existing exact HTTPS Hugging Face origins, without credentials or cookies.

Installing a model preserves an existing selection for the same task, including
an explicitly empty model configuration. A missing task selection uses its
canonical slot: `embedding` or `rerank`. Selecting an installed manifest is
explicit and updates only that task's canonical slot. Merely installing or
selecting a reranker does not enable reranking: effective search configuration
remains opt-in, and a disabled search does not prepare, download or load it.

Selection takes an installed manifest path relative
to the model directory. The model producer still checks the captured artifact,
manifest, backend, recipe and configuration before changing the selection. See
[AI models](../guides/ai_models.md) for the model and MRL contracts.

Both operations accept `--global-dir PATH` and `--timeout SECONDS`, including after
the operation arguments. Timeout is a positive number of seconds, default 3600.
The captured deadline and Ctrl-C cancellation are passed to the producer. A
deadline that cannot be represented by the platform fails before file operations.
The producer requires a finite deadline before preparing the installation or
writing its identity. A cancellation-only service context is rejected before
these effects; an already cancelled context retains its cancellation cause.

Installation paths use the compiled Brand. An explicit global directory takes
precedence over the Brand's `GLOBAL_DIR` environment variable; otherwise the
default is the Brand directory in the current user's home directory. Relative
overrides resolve from the current working directory. Home lookup uses Rust's
[standard library](https://doc.rust-lang.org/std/env/fn.home_dir.html). Path
resolution preserves native paths and performs no Git operation.

Help and argument validation precede installation setup, configuration reads and
signal registration. Unknown models, missing arguments and zero or invalid
timeouts exit 2 with Clap diagnostics. Help exits 0 on stdout even when the
installation path is unavailable. Valid operations return the producer's exit
code; a missing manifest exits 1 with a diagnostic. Local installation and
selection do not log in or modify an Auth profile. A valid operation can initialize
the native installation identity before discovering a missing manifest; that
failure does not select a model.

The parser passes `models::Arguments` directly to `models::run`. There is no
second argument parser in the model producer. The existing installation,
selection and artifact validation engine is shared by both models. Selection
reads the captured small manifest first to determine its task, captures the
bundle once with the matching slot, revalidates the original observation and
binds its artifacts and runtime before saving configuration.

Payload transfer checks the original deadline, cancellation and admission around
each bounded read/write. The captured source stream verifies its descriptor and
path observations before and after reads, and installation privacy and identity
are rechecked at phase boundaries and before publication. The installer hashes
the exact copied bytes against the pinned size and SHA-256; subsequent App fences
revalidate the captured bindings instead of rereading that same GGUF. Manifest
binding and the artifact publisher retain their own digest and copy verification.
No longer rewalking the installation's ancestors for every 64 KiB block does not
change the operation's deadline, authorization or integrity requirements.

## Reranker preparation

The default reranker is `BAAI/bge-reranker-v2-m3`, upstream revision
`953dc6f6f85a1b2dbfca4c34a2796e7dde08d41e`, licensed under Apache-2.0.
The default published distribution is
[`sinjab/bge-reranker-v2-m3-F16-GGUF`](https://huggingface.co/sinjab/bge-reranker-v2-m3-F16-GGUF),
fixed at commit `1458e3304258a8a11773000c7d0c192abfe78291` and file
`bge-reranker-v2-m3-F16.gguf`. The distribution commit is distinct from the
BAAI model revision above. Its model card identifies BAAI, Apache-2.0 and
llama.cpp/GGUF-my-repo conversion, but does not pin the converter revision.
Its exact artifact digest and size match the F16 artifact already qualified in
Graphit; selecting this download source changes neither model identity nor
precision, tokenizer or scoring recipe.

The canonical preparation builder is
`graphit-libs/models/bge-reranker-v2-m3/build.py`. It uses the official llama.cpp
converter at `4fbc76dec51d0add466f0210855c0596589b60d4` to produce the F16 GGUF with
`convert_hf_to_gguf.py <captured upstream directory> --outtype f16 --outfile bge-reranker-v2-m3-F16.gguf`.
The product manifest keeps the embedded XLM-RoBERTa pair tokenizer, noncausal
attention, RANK pooling and the original classifier's single raw logit.
`max_tokens = 8192` covers the entire query/document pair, including four special
tokens; truncation removes tokens from the longer side on the right. Sigmoid is
an explicit alternative recipe, not part of the default score.

The F16 artifact is exactly 1,159,774,912 bytes with SHA-256
`3c2de408d2c0a85a9472dc09f9d5a22c9b73743c6343952c15053299c777c298`.
Graphit's pinned converter requirements include NumPy 2.2.6, Transformers 4.57.6
and PyTorch 2.11.0. These pins, source file digests and the Apache-2.0 model card
must accompany artifact preparation. The artifact digest and byte length are
pinned in `model_provision.rs`; installation rejects another conversion or
quantization. The GGUF remains a
model artifact and does not introduce another native bridge or a new installer.

## Qualification

The affected executable checks use:

```sh
cargo xtask cargo test --locked --offline -p graphit-app --bin graphit --test models_cli --test help_cli --test version_cli --test package
cargo xtask cargo clippy --locked --offline -p graphit-app --bin graphit --test models_cli --test help_cli --test version_cli --test package -- -D warnings
```

The previously qualified executable suite passed all 25 tests on Linux (11 binary units,
three help, four models, four package and three version tests). Scoped Clippy
with `-D warnings`, formatting and links also passed. The model producer's
earlier real BF16 qualification remains separate from these routing checks. These tests cover
compiled help and errors, explicit/environment/home installation paths and a real
missing-manifest failure. They do not claim a new model download, inference,
corpus integration or native execution on Windows and macOS.

### Published BGE provisioning

The Linux provisioning suite passed 13 default contract tests, including the
immutable BGE source, its full response-length admission, truncated responses,
HTTPS redirect admission, cancellation, deadlines, integrity and installation
selection. The two real-artifact tests remain opt-in. The BGE real-artifact test
was also executed successfully with the existing pinned F16 file: it streams
all 1,159,774,912 bytes over loopback HTTP through the actual download sink,
then installs the captured artifact and exercises standard Remote reuse,
canonical slot selection, preservation of the embedding slot and an explicit
empty selection. `search.rerank = false` remains unchanged.

The default loopback fixture holds a deliberately partial body until the caller
has returned and its owner releases teardown. It does not auto-close the body
on a second five-second timer. Teardown also wakes its owned listener if no
client connected, so a failed zero-send regression cannot strand the server
join. Request checks still reject Authorization and Cookie headers.

```sh
cargo xtask cargo test --locked --offline --release -p graphit-app --test model_provision
GRAPHIT_BGE_RERANK_TEST_FILE=prepared/bge-reranker-v2-m3-F16.gguf \
  cargo test --locked --offline --release -p graphit-app --test model_provision -- \
  --ignored --exact actual_pinned_rerank_installs_reuses_and_selects_its_canonical_slot
```

Public immutable Hugging Face metadata, the initial GET redirect and the CDN
HEAD response confirmed the repository commit, file length and artifact digest.
The existing canonical file was freshly hashed and matches that publication.
No second external GGUF download or model inference was performed. Existing
CPU/CUDA inference and ranking evidence applies to these identical bytes;
this provisioning qualification does not claim new GPU, Windows/macOS or
remote CI execution.
