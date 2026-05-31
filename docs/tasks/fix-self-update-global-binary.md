# Fix self-update for global binary installs

## Summary

Fixed `graphit self-update` failing when the binary is installed in a system-wide
directory like `/usr/local/bin/`. Two distinct bugs were resolved:

1. **Permission denied on temp file creation** — The temp file was created in the
   binary's directory (`filepath.Dir(currentExe)`), which requires root for
   `/usr/local/bin/`. Now falls back to `os.TempDir()` when the binary's
   directory is not writable.

2. **`sudo` loses user config** — `self-update` had a `PreRunE: requireSetup`
   gate that checks `hub.repo` from `~/.graphit/config.json`. Under `sudo`,
   `$HOME=/root` so the config isn't found. Removed this gate since `self-update`
   only needs `brand.GitHubRepo`/`brand.SelfUpdateURL` (build-time constants).

3. **Cross-filesystem `os.Rename`** — When the temp file falls back to
   `os.TempDir()` (potentially a different filesystem), `os.Rename()` fails with
   `EXDEV`. Added `isCrossDevice` detection with cross-platform support
   (Unix/Windows) and a `copyFile` fallback in `AtomicReplace`.

## Files Changed

- `cmd/graphit/commands/lifecycle.go` — Removed `requireSetup` from self-update;
  added temp-dir fallback.
- `internal/updater/updater.go` — `AtomicReplace` now handles cross-filesystem
  renames via `copyFile` fallback.
- `internal/updater/crossdev_unix.go` — [NEW] Unix `isCrossDevice` (checks
  `syscall.EXDEV`).
- `internal/updater/crossdev_windows.go` — [NEW] Windows `isCrossDevice` (checks
  `ERROR_NOT_SAME_DEVICE`).
- `internal/updater/updater_test.go` — Added `TestAtomicReplaceCrossDir` and
  `TestCopyFile`.

## Verification

- `make ci` — All checks passed (lint, vet, tests, build).
