---
title: Strip the core binary
status: done
created: 2026-08-14
updated: 2026-08-14
tags: [build, launcher, size]
---

# Strip the core binary

Follow-up to [the model leaving the binary](model-downloaded-at-setup-not-embedded.md).
That removed 103 MB of model weights; this takes 64.6 MiB more off what was left, by
linking the core without its debug tables.

## Result, measured on a real `make build-linux`

| | binary | release `.tar.gz` |
|---|---|---|
| before | 409,914,777 B — 390.9 MiB | 115,748,880 B — 110.4 MiB |
| after | 342,154,657 B — 326.3 MiB | 87,105,007 B — 83.1 MiB |
| **cut** | **64.6 MiB — 16.5%** | **27.3 MiB — 24.7%** |

The core itself goes from 270,062,928 to 202,306,224 B — **257.6 → 192.9 MiB, −25.1%**.
It is 66% of the payload, which is why a quarter of it moves the whole artifact this
much.

Across today's two changes the Linux launcher went from 494.6 MiB to 326.3 MiB.

## The change

A new `STRIP_LDFLAGS ?= -s -w`, appended to the core's ldflags in all four build
targets. `make build-linux STRIP_LDFLAGS=` gives a debuggable build back, and the
variable is a `?=` so the environment can override it per build.

`graphit-mcp` was already built with `-s -w` in every target; only the core was
missing it.

## No runtime capability is lost

`-s -w` omits DWARF and the symbol table external debuggers read. It does **not**
touch the Go runtime's `pclntab`, which is what produces function names and line
numbers at runtime.

Verified rather than assumed: the same program built both ways produces byte-identical
output for

- `debug.Stack()`, with function names and `file:line`
- `runtime.Caller`, `runtime.FuncForPC(pc).Name()`
- `debug.ReadBuildInfo()`

and the `-X` values survive, confirmed independently on `graphit-mcp` — `go version -m`
shows all eight `-X` settings intact beside the `-s -w` it has always carried.

The codebase has exactly one consumer of any of this: `internal/daemon/project.go:215`,
`fmt.Errorf("panic: %v\n%s", r, debug.Stack())` — the `runProtected` that catches
module panics and the thing that reported the tokenizer crash-loop. It behaves
identically. Nothing imports `runtime/pprof` or `net/http/pprof`, and no handler sets
`AddSource`, so there is no profiling or log-source path to degrade either. (Had there
been, `runtime/pprof` symbolises from the `pclntab` too, so it would also have been
fine.)

## What is lost

Source-level debugging of a **released** binary. Measured on the artifacts:

| | before | after |
|---|---|---|
| `file` | with debug_info, not stripped | stripped |
| `nm` | 2464 symbols | 1 |
| `go tool nm` | 2465 | 2 |
| `.debug_*` sections | 8 | 0 |

So delve can still attach to a running process but without variable inspection or
source stepping, gdb is degraded, and `go tool nm` on a shipped artifact no longer
answers questions like whether a given symbol made it in. If a release ever needs deep
debugging, rebuild it at the same tag with `STRIP_LDFLAGS=` or publish an unstripped
artifact alongside.

One platform note: this is `-s -w` at link time, not `strip(1)` afterwards. Running
`strip` over an already-built Go binary can break it; the linker flag simply does not
emit the sections. `codesign --sign -` in `install-darwin` is unaffected.

## Verification

- Real `make build-linux`; the sizes above are of the produced artifact.
- The built launcher run under a throwaway `HOME`: extracts its runtime and reports
  its version.
- Earlier in the day, the same stripped core answered `ast query --hybrid` against the
  real project graph — LadybugDB, FTS5, sqlite-vec and ONNX inference in one command.
- `make -n build-linux` shows `-s -w` on the core build line.

## ICU was measured, and deliberately left alone

While looking for the size, ICU turned out to be dead weight on Linux — 72.8 MiB on
the machine used, loaded by nothing in the bundle. It was removed, and then **put back
at the Engineer's request** so that this change ships the strip alone; the ICU decision
is his to make across all three platforms at once, not Linux-only as a side effect of
a size pass.

Nothing broke while it was out — `ast query` and `ast query --hybrid` both worked
against a real graph with the ICU files deleted from an extracted runtime. The full
evidence, the value at stake, and the method for checking darwin and windows are in
`docs/tasks/backlog/verificar-se-macos-e-windows-realmente-precisam-da-icu-no-bu.md`,
and the Makefile points at it above the `build-darwin` target.

## Compressing the embeds was measured and rejected

The obvious next idea, and the machinery for it existed until the model left. It does
not help the download: the release already ships `.tar.gz`, and gzipping the core a
second time yields 97.0% of the first pass. Per-member compression would put the
binary near 110 MiB but leave the tarball at roughly what it is now — marginally
worse, since per-member gzip loses the cross-file redundancy a single stream exploits.
It would only shrink the binary on disk, at ~20 s of `gzip -9` per release build and
~1 s of decompression per version bump.

Worth revisiting only if disk footprint is the complaint in its own right: an
installed machine holds the payload twice, once inside the binary and once extracted.
The real fix for that is downloading the runtime at setup the way the model now is,
which is a far larger change and makes the air-gapped story harder.
