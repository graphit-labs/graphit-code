# Installer Improvements: default dir, --dir flag, PATH warning, remove macOS quarantine block

**Date:** 2026-06-21  
**Status:** Complete

## What

Improved `install.sh` and `install.ps1` to follow best practices for user-space installations and user experience.

## Changes

### `install.sh`

- **Default install dir changed**: `/usr/local/bin` → `$HOME/.local/bin`  
  `/usr/local/bin` requires `sudo` on most distros and is a system-level dir. `$HOME/.local/bin` is the XDG standard for user-space binaries (already in PATH on modern Ubuntu/Fedora/Arch via `~/.profile` or systemd).
- **Added `--dir <path>` flag**: Allows custom install directory via `bash -s -- --dir /custom/path`.  
  Also accepts `--dir=<path>` form. Includes `--help`.
- **Removed macOS quarantine block** (`xattr -d com.apple.quarantine` + `codesign --sign -`):  
  The `com.apple.quarantine` attribute is only set by GUI apps (Finder, Safari, browsers). `curl` in terminal **never** applies quarantine. The `codesign --sign -` (ad-hoc) block was also a no-op for curl-based installs — it only matters for `.dmg`/`.pkg` distribution via Gatekeeper notarization. Removing it simplifies the script with no functional loss.
- **PATH warning**: If the install dir is not in `$PATH`, prints a clear warning with copy-paste `export PATH` snippets for bash, zsh and fish.
- **Auto-create install dir**: `mkdir -p "$INSTALL_DIR"` before moving binary (with sudo fallback).
- **`DLBIN` variable**: Introduces `DLBIN="${INSTALL_DIR}/${BIN_NAME}"` for cleaner code.

### `install.ps1`

- **Default install dir changed**: `~\.graphit\bin` → `$env:USERPROFILE\.local\bin`  
  Aligns with the Linux/macOS convention (`$HOME/.local/bin`) for users working cross-platform.
- **Added `-Dir` parameter**: Clean PowerShell param (`[string]$Dir`) with priority order: `-Dir` > `$env:GRAPHIT_INSTALL_DIR` > default.  
  To use: `iex "& { $(irm .../install.ps1) } -Dir 'C:\Tools\graphit'"`
- **PATH management**: Unchanged — Windows installer already auto-adds to user PATH (no manual step needed, unlike POSIX shells).
- **Updated usage comment** at top to document the new `-Dir` parameter.

## Why macOS quarantine block was removed

| Scenario | Quarantine applied? |
|---|---|
| Download via `curl` in Terminal | ❌ No |
| Download via Safari/browser | ✅ Yes |
| Download via Finder (drag-drop .dmg) | ✅ Yes |

Since our install flow is `curl ... \| bash`, quarantine is never applied and the `xattr`/`codesign` commands were always no-ops. Removing them reduces confusion and eliminates a potential error surface (`codesign` may not be available in CI/minimal macOS environments).

## Verification

- `bash -n install.sh` — syntax check passes
- Tested logic flow manually: `--dir`, `--help`, PATH check, mkdir
- PowerShell param binding verified: `-Dir` takes priority over `$env:GRAPHIT_INSTALL_DIR`
