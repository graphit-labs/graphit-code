---
title: The model arrives at setup, not inside the binary
status: done
created: 2026-08-14
updated: 2026-08-14
tags: [launcher, embedding, setup, build, docs]
---

# The model arrives at setup, not inside the binary

## Problem

Every released launcher carried the embedding model. `fetch-model` downloaded
`model.onnx` from Hugging Face, gzipped it at `-9`, `bundle_model` copied it into
`cmd/launcher/runtime/models/`, `//go:embed runtime/*` swallowed it, and
`extractRuntime` decompressed it on first run. `deduplicateModels` then moved the
extracted copy into `~/.graphit/models/coderankembed/` and left a symlink behind, so
that a version upgrade would not keep a second 132 MB copy.

Measured on the last local build:

| | bytes | |
|---|---|---|
| `model.onnx` raw | 138,619,279 | 132.2 MiB |
| `model.onnx.gz` at `-9` | 107,753,463 | 102.8 MiB — embedded |
| `tokenizer.json` | 711,649 | 0.7 MiB — embedded |
| **carried in the binary** | **108,465,112** | **103.4 MiB** |
| launcher, before | 518,586,455 | 494.6 MiB |
| launcher, after | 410,121,343 | 391.1 MiB |

**20.9% of the Linux launcher was model weights** — more than everything else it
carries put together. The weights version on a different schedule from the code, so
every release, and every CI build, paid to move a file that had not changed. The
elaborate extract-then-symlink dance existed only to undo the damage of embedding it
in the first place.

## What changed

The download already existed and already looked in the right two places:
`ModelManager.EnsureModel` checks a `models/` directory next to the executable, then
the shared cache, then downloads. Nothing about that needed inventing — the model
simply had to stop being in the binary, and something had to fetch it at a moment the
user is expecting to wait.

### The model leaves the build

- **Makefile** — `MODEL_REPO`, `MODEL_CACHE`, the `fetch-model` target and the
  `bundle_model` macro are gone, along with the `fetch-model` prerequisite and the
  `$(call bundle_model)` line on all four of `build-linux`, `build-darwin`,
  `build-windows` and `build-windows-native`. A comment in its place says why, so the
  next person to look for `fetch-model` finds the reason instead of a gap.
- **`cmd/launcher/dedup.go`** and its test are deleted. `deduplicateModels` only ever
  had work to do when the runtime held an extracted model; with no model to extract it
  would return at its first `os.Stat` forever. Its `moveFile`/`copyFile` helpers had no
  other callers. This also removes the one place that needed `os.Symlink` to work,
  which was the piece most likely to misbehave on Windows without Developer Mode.
- **`extractRuntime`** loses its `.gz` branch. `model.onnx.gz` was the only compressed
  asset in the bundle — the other tarballs in the Makefile are build-time downloads
  that never reach `cmd/launcher/runtime/` — so the branch was unreachable, along with
  the `bytes`, `compress/gzip` and `io` imports.
- **CI and release workflows** no longer cache `/tmp/coderankembed-cache`: three
  `Cache CodeRankEmbed model` steps in `release.yml` and one path in `ci.yml`.

### The model arrives at setup

`graphit setup` gained a final step, `ensureEmbeddingModel`
(`cmd/graphit/commands/model_setup.go`). It reports one of three outcomes:

```
  › ✓ Embedding model downloaded to /home/u/.graphit/models/coderankembed
  › ✓ Embedding model already present at /home/u/.graphit/models/coderankembed
  ✗ Embedding model download failed: <err>
    › Model cache: /home/u/.graphit/models/coderankembed
    › Every other setting was saved — fix the network and run 'graphit setup' again
    › Behind a proxy? set HTTP_PROXY / HTTPS_PROXY. Air-gapped? place model.onnx and
      tokenizer.json in the cache directory above
```

**A failure fails the setup**, with a non-zero exit status. An installation without the
model cannot embed anything, so it cannot answer a semantic query at all; a setup that
reported success would defer that discovery to some later search that quietly came back
on keyword hits with no explanation.

This was reversed during review. The first implementation warned and finished, on the
reasoning that `EnsureModel` retries on first use and hybrid search degrades to
FTS-only in the meantime — a slower first search rather than a broken install. The
Engineer's call is that a half installation should not report success, and it is the
better one: the degradation is invisible at the point where it matters, and "semantic
search silently isn't semantic" is a worse failure than an install that stops and says
what to fix.

Two things make failing cheap, and both were already true:

- **It is the last step.** The hub URL, the memory URL, the IDE and the CLI are written
  before it, and both repositories are already initialised, so re-running setup after
  fixing the network costs one pass through the prompts and loses nothing.
- **The message names the fix**: the cache directory, the proxy variables, and the
  air-gapped route of dropping the two files in by hand. `ai.ModelCacheDir` is exported
  for exactly this — a failed `EnsureModel` returns no path, and the message needs one.

It is deliberately stricter than the hub and memory clones in the same command, which
only warn. Those retry on the next command against a remote that may be momentarily
unreachable; this is a fixed asset at a fixed URL that the tool needs in order to do
its job. A future reader should not "fix" the inconsistency.

### Progress, and who decides how often to speak

`ModelManager.OnProgress` is a new optional `ProgressFunc`. It fires once with zero
bytes before the transfer starts, so a caller can announce a 132 MB download before
there is anything to report, and then once per read — every 32 KB, so roughly four
thousand times for the model.

**It does not throttle itself**, because how often a line may be repainted is a
property of where the line is going, not of the download. That policy lives in
`modelProgress`, which captures `output.IsTTY()` once and then decides:

| | limit | rendering |
|---|---|---|
| terminal | one repaint per 100 ms | `StepProgress` rewrites one line in place, with a 24-cell bar |
| redirected | one line per 10% | `Step` appends, no bar — a bar means nothing in a log |

Both paths end at 100%: the final read is always reported even when it lands inside a
tenth that was just announced, or a redirected run would stop at 90-something percent
every time.

The decision is split from the printing into `modelProgress.next`, which takes the
clock as a parameter and returns the line plus whether to emit it. A rate limit tested
through real sleeps is slow and flaky at once; this way both throttles are ordinary
table tests.

Rendering is in `internal/output/progress.go` — `HumanBytes`, `ProgressBar`,
`DownloadLine` — beside the `StepProgress` machinery it feeds. `DownloadLine` degrades
honestly: an undeclared `Content-Length` is a real case, and it then prints the byte
count with no percentage and no bar rather than a bar that would have to invent a
position.

### Platforms

Nothing in the new path is platform-specific. The download is `net/http`, the paths go
through `filepath.Join`, and the cache is `os.UserHomeDir()` + `brand.DotDir()`. The
TTY check is `golang.org/x/term`, already used by every other line the printer writes.
The bar uses the same block characters the printer already repeats across the terminal
width in `Table`, so it makes no new assumption about the console encoding. Windows in
fact loses its one fragile step, the symlink in `deduplicateModels`.

## Documentation

`docs/specs/ai_engine.md` was wrong about this area in three ways that predate this
change, all now corrected — it was the subject of a backlog item, now removed:

1. It described the proxy embedding client as connecting to "external services (like
   Google Gemini API or OpenAI Embeddings) if configured by the user" and delegating
   "over HTTPS". It is a UNIX socket to our own daemon's `EmbedServer`, which holds a
   local ONNX client. **There is no external embedding provider anywhere in the
   codebase**, and the spec now says so explicitly so the assumption does not come
   back.
2. The cache path was given as `~/.graphit/models/coderank-embed-137m/`. The constant
   is `models/coderankembed`.
3. The download was attributed to "the Graphit Labs repository". It is Hugging Face.

Also updated: `docs/architecture/architecture_overview.md` (what the launcher does and
does not embed), `docs/architecture/storage_layout.md` (the models directory, and that
it is keyed by nothing — one copy per machine), `docs/guides/getting_started.md` and
`docs/guides/cli_reference.md` (the new setup step and that a failure aborts it),
`docs/guides/troubleshooting.md` (the ~90 MB figure was wrong, plus sections on
upgrades and air-gapped machines), `docs/guides/private_brand_customization.md` (a new
air-gapped section, since shipping the weights beside the binary is now an opt-in
rather than what the project does), and two `README.md` lines.

## Verification

- `go build -tags fts5 ./...` and `go vet` on the touched packages — clean.
- `go test -tags fts5 ./cmd/launcher/... ./internal/output/... ./internal/ai/... ./cmd/graphit/commands/...` — pass.
- New tests: progress reporting in `internal/ai/model_progress_test.go` (opens at
  zero, ends at the full size, monotonic, names the final file rather than the `.tmp`,
  reports no total when the server declares none, silent on a cache hit, both bundle
  files named); the two throttles and the renderers in
  `cmd/graphit/commands/model_setup_test.go` and `internal/output/progress_test.go`.
- The fatal contract is pinned in `cmd/graphit/commands/model_setup_test.go`:
  `ensureEmbeddingModel` returns the error and still returns the cache directory for
  the message, and a pre-seeded cache is reported as a hit rather than a download. Both
  redirect the home directory — `USERPROFILE` on Windows, `HOME` elsewhere — and fail
  the cache creation locally, so neither touches the network. What is *not* covered by a
  test is `newSetupCmd` itself propagating that error: its `RunE` reads four prompts
  from stdin and clones two git repositories, so an end-to-end test of it would be a
  fixture of its own. The propagation is a bare `return fmt.Errorf(...)`.
- The `//go:embed runtime/*` glob still resolves with no `models/` directory present,
  checked by building `./cmd/launcher` against a runtime tree of the shape the Makefile
  now produces.

The size figures above are arithmetic on the embedded bytes, not a measurement of a
new release build: `go:embed` stores file contents verbatim, so the binary shrinks by
exactly what stopped being embedded. A full `make build-linux` was not run here, since
it needs the UI bundle, liblbug and the ONNX Runtime archives.

## Follow-up worth knowing

The model is accepted on **size alone** — `modelONNXMinSize`, `tokenizerJSONMinSize`.
A truncated download is caught; a corrupted one that is large enough is not, and
surfaces later as an ONNX load failure. Now that the file always arrives over the
network rather than out of a binary whose integrity the release process vouched for,
a checksum is worth more than it used to be.
