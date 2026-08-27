---
Title: Merge Native LanceDB Resolution with Other Libraries' Standards
status: done
created: 2026-08-25
updated: 2026-08-25
tags: [build, native, lancedb, runtime, makefile]
---

Unify Native LanceDB Resolution with Other Libraries Standards

## Objective

The Engineer fixed the principle: everything necessary for RUNNING should come from the RUNTIME of
The global target "`~/.graphit/runtime/<version>/`" and the build should not have different processes.
For those who need links, there are three distinct native processes today:

Lib | Origin | Placement for Link/Testing
|---|---|---|
Here is the translation:

Pre-built (release tarball from GitHub) inside the module Go (`go mod download` + extraction in `$GOMODCACHE/go-ladybug@vX/lib`), resolved by Cgo `${SRCDIR}/lib`
ONNX Runtime is pre-built (official release) | `/tmp/onnxruntime-cache`; the CODE resolves at runtime: exe dir → `brand.RuntimeDir(version)` → `LD_LIBRARY_PATH` (`internal/ai/embedding_local.go`, same as YAMLs of queries and extensions)
LanceDB | Build from source with role (no usable release exists: v0.1.2 does not implement FTS — measured; requires 3 configuration patches and statically linked toolchain) | Inline checkout, gitignored, via cgo static inline `-L${SRCDIR}/../../.native` in `internal/lancestore/cgo_lancedb.go:22`

The LanceDB is the only one whose artifact must be built from source (obvious - there's nothing to avoid).
Download) And the only local project when everything else resolves globally is. The
The launcher already extracts `liblancedb_go.so` from `~/.graphit/runtime/<version>/` along with the payload, then.
The artifact exists globally after any installation; only the source tree's link does not search for it.

**The Approach Thinking.** Not for ready consumption (it doesn't exist), but the LOCATION
It can follow the same philosophy of ONNX/YAML extensions: use what is extracted by the last installation.
O alvo `lancedb-native` passa a resolver em cascata: `.native/` existente (no-op, guarda barata
The current step is → copy/link from INLINE_0 → only then build with cargo. The path of
link cgo permanece `.native/` (contrato documentado em `cgo_lancedb.go`: mudar um sem o outro
The code breaks the link, so no changes to the Go code are necessary, and CI remains deterministic.

**Alternativas descartadas.**
Point the Go build directly at the runtime global: link all local builds to the last installed version,
Breaking the CI's hermetic integrity and violating the own _INLINE_0_'s contract comment.
Replace LanceDB with upstream release: v0.1.2 lacks FTS/hybrid (the quality floor of the search)
It depends on that — rejected with registered measurement in the Makefile.

Known and assumed risk. The extracted Lib in `runtime/dev/` does not carry metadata of
**INLINE_0** originates from. If the pin changes between installation and fallback, there is theoretical risk of
Mismatched FFI that fails at runtime, not during build. Mitigations for this task:

(1) Explicit warning in the code.
output do make nomeando ambas as origens; (2) os builds por cargo passam a gravar um stamp
At the side of the library for future verification; (3) the pin changes rarely and has always been
deliberate operation documented ("keep it and the toolchain pinned together")

## Plan & Task Breakdown

- [ ] T1 - Resolution Cascade on Target `lancedb-native`. Spec: target in `Makefile`;
  ordem `.native/` presente → `$(HOME)/.$(BRAND)/runtime/dev/$(LANCEDB_LIB_NAME)` (symlink em
Unix, copying to Windows) → Build package; warning naming origin; signed SHA hash stored in builds
  por cargo. **FEITO** — `LANCEDB_RUNTIME_SOURCE ?= $(if $($(BRAND_ENV)_GLOBAL_DIR),…,$(HOME)/.$(BRAND))/runtime/dev`
  respeita o override de global dir; `fetch-lancedb` grava `lancedb_go_build.sha` com o
The waterfall emits an explicit warning of unregistered origin.
- [ ] **T2 - End-to-end Verification.** Spec: remove `.native/`, run `make lancedb-native`.
  conferir que a lib veio do runtime dev, `go build -tags lancedb ./...` e teste focado verdes;
Repeat with `INLINE_0` present to prove the NOP (No Operation) in action. **DONE** - The first attempt revealed a bug: `INLINE_1`.
The failure occurs when `INLINE_0` is not defined; corrected with `INLINE_1` before creation.
  symlink criado para `~/.graphit/runtime/dev/liblancedb_go.so`, `go build -tags lancedb
  ./internal/lancestore ./internal/ast` exit 0, `go test -tags lancedb ./internal/lancestore`
  PASS (FFI carrega via symlink) e, com `.native/` presente, o alvo sai sem fazer nada.
- [ ] T3 - Documentation and Memory. Spec: comments in the Makefile section for LanceDB;
Note in `docs/architecture/storage_layout.md` (or Start Guide) about the cascade; memory
Updated graph with decision. **DONE** – comments from target and INLINE_0
They describe the waterfall; INLINE_0 gained an entire paragraph on unified resolution and the stamp.
The SHA algorithm's memory, `INLINE_0`, is updated in place with the final decision.

## Files Changed

| File | Change | Reason |
|---|---|---|
The inline 0 is modified by resolving cascata with a SHA stamp on target __inline_1__.
| `docs/tasks/unify-lancedb-native-resolution.md` | Created | este log |

## Progress Log

### 2026-08-25
Task opened after correction/evaluation of T17 (commit `fcaa9d7`) and direction from Engineer:
"How is it done with other libraries that require linking for testing? Shouldn't it be the same?"
  dois processos diferentes".
Context restored in this session: `.native/` and `/tmp/lancedb-native-cache` had disappeared and
There is no path entry; the suite `-tags lancedb` only linked after manual copy of
  `~/.graphit/runtime/dev/liblancedb_go.so` — exatamente o caso que a cascata automatiza.

### 2026-08-25 (fechamento)
Cascata verified from head to toe, documentation and memory completed. Task ready for deployment.
"Be true to yourself."
