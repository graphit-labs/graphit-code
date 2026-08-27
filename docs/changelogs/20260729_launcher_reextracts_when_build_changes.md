# Launcher re-extracts when the build changes

**Date:** 2026-07-29
**Scope:** `cmd/launcher/main.go`, `cmd/launcher/runtime_stamp_test.go`
**Origin:** Engineer ran `graphit ast index . --reset` on a 36823-file corpus and the core that ran was from a build prior to the fix he wanted

---

## The symptom that wasn't what it seemed

A `--reset` on a PL/SQL corpus failed on schema (`Binder exception: Table Table does not
exist`) and left the database at 4096 bytes — empty, because `--reset` deletes before and rebuild
aborted before swap. This error already has a fix in the repo since `763fe938`, in two layers: the pair filter in `newRebuildIndex` and the `nodeTables` guard in `initSchemaForLabels`.

The binary was the other one. `/usr/local/bin/graphit` was from today 18:36, and
`~/.graphit/runtime/v0.1.27/graphit-core` — the one that actually executes — was from yesterday 23:29.

## The cause

```go
shouldExtract := false
if _, err := os.Stat(coreBinPath); os.IsNotExist(err) {
shouldExtract = true
} else if versionSafe == "dev" {          // <- the gate
shouldExtract = devStampChanged(appDir)
}
```

The BuildID comparison existed and worked. It only ran when version was literally
`dev` — and `dev` is the source default, which survives in a build **without** Makefile ldflags.
`make install` produces `VERSION ?= v0.1.27` (`Makefile:12`), so the condition fell to `os.Stat`,
found a core already in place and executed the old one indefinitely. `BUILD_ID` itself is
correct: `Makefile:26` generates a new UUID on each make invocation.

Two consequences beyond the obvious. An interrupted extraction stayed **permanent** on a versioned
build: a truncated core remains, `os.Stat` finds it, and nothing else is checked. And since
`writeLauncherStamp` is only called on extraction, the global stamp never moved — so the daemon,
which restarts when it changes (`daemon.go`, on `VersionCheckInterval` tick), also never
knew it should exit.

## The fix

The decision stops looking at version and core existence, and compares **both stamps**
against this launcher's BuildID:

```go
func shouldExtractRuntime(appDir, runtimeDir string) bool {
return !stampMatchesBuild(runtimeStampPath(runtimeDir)) ||
!stampMatchesBuild(launcherStampPath(appDir))
}
```

The new one is `<runtimeDir>/.build-id`, written on extraction alongside the global one. Missing or different in
either ⇒ re-extract. The two answer different questions: the one inside the
directory says which build is on disk, the global says which build the live daemon thinks is
current.

`devStampChanged` left, replaced by `stampMatchesBuild` — which now applies to every version,
not just `dev`.

**Directory remains per version.** I had proposed a per-build path and was wrong: with a
per-build directory, `cleanupOldRuntimes` would on **every** `make install` delete the
directory from which the live daemon and `graphit-mcp` processes load `liblbug.so`, `libonnxruntime.so`,
ICU, grammars via `dlopen` and ANTLR sidecars via `exec`. Today that almost never fires because
version almost never changes. Keeping the name per version preserves that blast radius.

## Tests

`cmd/launcher/runtime_stamp_test.go`:

- `TestShouldExtractRuntime` — seven entries: no stamp, both from this build, each missing
  separately, each from another build separately, both from another build.
- `TestCoreBinaryAloneDoesNotSatisfyTheCheck` — core on disk without stamp requires re-extraction; it was
  exactly what the old condition read as success.
- `TestStampsRoundTripAcrossARebuild` — the regression that motivated everything: extract, satisfied; swap
  BuildID keeping version (what `make install` does), requires re-extract; re-extract, satisfied.

`gofmt` clean on both files, `go vet ./cmd/launcher/` clean, `golangci-lint run
--new-from-rev=HEAD` with 0 issues, package green.

## What remains open

- **Order inside the extraction block.** Today it's `RemoveAll(runtimeDir)` and extract on top. With
  the fixed check this now happens on every `make install`, and there's a window where
  libraries aren't there for a late `dlopen` from a live process. Publishing via `rename`
  instead of deleting in place closes most of that.
- **Nobody knows who is using a runtime.** `cleanupOldRuntimes` deletes by name, without checking
  usage, and `graphit-mcp` processes hold no lock. A shared flock on
  `<runtimeDir>/.inuse` taken by every core process would truly solve it.
- **The `--reset` that deletes before knowing whether rebuild works.** The empty graph in this report is
  a consequence of that, not the launcher.
- **`ast index` reports `0 files indexed` over an empty graph**, because the manifest still considers
  everything up to date. Full manifest with empty graph should force rebuild.
