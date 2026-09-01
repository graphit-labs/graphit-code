---
title: Unify LanceDB native resolution with the pattern used by the other libs
status: done
created: 2026-08-25
updated: 2026-08-25
tags: [build, native, lancedb, runtime, makefile]
---

# Unify LanceDB native resolution with the pattern used by the other libs

## Objective

The Engineer fixed the principle: everything needed to RUN must come from the runtime of the
target global path (`~/.graphit/runtime/<versão>/`) and the build must not have different processes
for libs that need linking. Today there are three distinct native processes:

| Lib | Origin | Placement for linking/testing |
|---|---|---|
| `liblbug` | prebuilt (GitHub release tarball) | inside the Go module (`go mod download` + extraction into `$GOMODCACHE/go-ladybug@vX/lib`), resolved by cgo `${SRCDIR}/lib` |
| ONNX Runtime | prebuilt (official release) | `/tmp/onnxruntime-cache`; the CODE resolves at runtime: exe dir → `brand.RuntimeDir(version)` → `LD_LIBRARY_PATH` (`internal/ai/embedding_local.go`, same pattern as query YAMLs and extensions) |
| LanceDB | **built from source with cargo** (no usable release exists: v0.1.2 does not implement FTS — measured; requires 3 configuration patches and a pinned toolchain) | **`.native/` per checkout**, gitignored, via static cgo `-L${SRCDIR}/../../.native` in `internal/lancestore/cgo_lancedb.go:22` |

LanceDB is the only one whose artifact has to be built from source (unavoidable — there is nothing
to download) AND the only one placed project-locally when everything else resolves through global
paths. The launcher already extracts `liblancedb_go.so` into `~/.graphit/runtime/<versão>/` alongside
the payload, so the global artifact exists after any installation; only the source tree's link does
not look for it.

**Reasoning behind the approach.** We cannot consume a ready-made release (there is none), but the
PLACEMENT can follow the same philosophy as ONNX/YAMLs/extensions: use whatever the last
installation extracted. The `lancedb-native` target now resolves as a cascade: existing `.native/`
(no-op, the cheap guard we already have) → copy/link from `~/.graphit/runtime/dev/` → only then
build with cargo. The cgo link path stays `.native/` (a contract documented in `cgo_lancedb.go`:
changing one without the other breaks the link), so no Go code change is needed and CI stays
deterministic.

**Alternatives discarded.**
- Point cgo directly at the global runtime: couples every local build to the last installed
  version, breaks CI hermeticity, and violates `cgo_lancedb.go`'s own contract comment.
- Swap LanceDB for the upstream release: v0.1.2 has no FTS/hybrid (search's quality floor depends
  on it) — rejected with the measurement recorded in the Makefile.

**Known and accepted risk.** The lib extracted into `runtime/dev/` carries no metadata about the
`LANCEDB_SHA` it came from. If the pin changes between the installation and the fallback, there is a
theoretical risk of an FFI mismatch that fails at runtime, not at build time. Mitigations in this
task: (1) an explicit warning in the make output naming both origins; (2) cargo builds now write a
`LANCEDB_SHA` stamp next to the lib for future verification; (3) the pin changes rarely and has
always been a deliberate, documented operation ("keep it and the toolchain pinned together").

## Plan & Task Breakdown

- [x] **T1 — Resolution cascade in the `lancedb-native` target.** Spec: target in `Makefile`;
  order `.native/` present → `$(HOME)/.$(BRAND)/runtime/dev/$(LANCEDB_LIB_NAME)` (symlink on
  unix, copy on windows) → cargo build; warning naming the origin; SHA stamp written by cargo
  builds. **DONE** — `LANCEDB_RUNTIME_SOURCE ?= $(if $($(BRAND_ENV)_GLOBAL_DIR),…,$(HOME)/.$(BRAND))/runtime/dev`
  honors the global dir override; `fetch-lancedb` writes `lancedb_go_build.sha` with the
  `LANCEDB_SHA`; the cascade emits an explicit warning about unrecorded provenance.
- [x] **T2 — End-to-end verification.** Spec: remove `.native/`, run `make lancedb-native`,
  check that the lib came from the dev runtime, `go build -tags lancedb ./...` and a focused test
  green; repeat with `.native/` present to prove the no-op. **DONE** — the first attempt exposed a
  bug: `ln` fails when the whole `.native/` directory does not exist; fixed with `mkdir -p` before
  creation. Afterwards: symlink created to `~/.graphit/runtime/dev/liblancedb_go.so`, `go build -tags lancedb
  ./internal/lancestore ./internal/ast` exit 0, `go test -tags lancedb ./internal/lancestore`
  PASS (FFI loads via the symlink) and, with `.native/` present, the target exits without doing anything.
- [x] **T3 — Documentation and memory.** Spec: Makefile comments in the LanceDB section;
  a note in `docs/architecture/storage_layout.md` (or the getting-started guide) about the cascade;
  Graphit memory updated with the decision. **DONE** — the comments on the target and on
  `LANCEDB_LIB_DIR` describe the cascade; `storage_layout.md` gained a paragraph about the unified
  resolution and the SHA stamp; memory `01M0XAJ3ENS1VJT28E8W7FBW3F` updated in place with the final
  decision.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `Makefile` | Modified | resolution cascade + SHA stamp in the `lancedb-native` target |
| `docs/tasks/unify-lancedb-native-resolution.md` | Created | this log |

## Progress Log

### 2026-08-25
- Task opened after the T17 fix/performance work (commit `fcaa9d7`) and the Engineer's direction:
  "how is it done with the other libs that need linking for the test? it should be the same, not
  have two different processes".
- Context restored in this session: `.native/` and `/tmp/lancedb-native-cache` had disappeared and
  there is no cargo on the PATH; the `-tags lancedb` suite only linked again after a manual copy of
  `~/.graphit/runtime/dev/liblancedb_go.so` — exactly the case the cascade automates.

### 2026-08-25 (closing)
- Cascade verified end to end, documentation and memory completed. Task ready for its own
  commit.
