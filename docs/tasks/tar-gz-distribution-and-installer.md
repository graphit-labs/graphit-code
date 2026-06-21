# tar.gz Distribution & curl/ps1 Installer

**Date:** 2026-06-21  
**Status:** Complete

## What

Switched all Graphit Code release artifacts from bare binaries to `.tar.gz` compressed archives with maximum compression. Introduced industry-standard `install.sh` and `install.ps1` one-liner installers. Updated `self-update` command to decompress tarballs. Updated all installation docs.

## Why

- Users expect `curl -fsSL ... | bash` / `irm ... | iex` one-liners as the standard open-source install pattern
- Compressed archives reduce download size significantly (the launcher binary embeds models and runtimes)
- Checksums now cover the full archive rather than bare binaries, which is more secure
- Installer scripts handle OS/arch detection, checksum verification, and PATH setup automatically

## Changes

### New files
- `install.sh` — POSIX shell installer: detects OS/arch, downloads `.tar.gz`, verifies SHA-256, extracts, installs to `/usr/local/bin/graphit`
- `install.ps1` — PowerShell installer: same flow for Windows, installs to `~/.graphit/bin/`, adds to user PATH

### Modified files
- `.github/workflows/release.yml`
  - Each build job now packages binary into `.tar.gz` (gzip -9) and checksums the archive
  - Published assets: `graphit-{platform}.tar.gz` + `graphit-{platform}.sha256` + `checksums.sha256`
  - Release body updated with one-liner install commands
- `internal/updater/updater.go`
  - Added `PlatformArchiveName(binName)` → returns `graphit-<os>-<arch>.tar.gz`
  - Added `ExtractFromTarGz(archivePath, binName, destPath)` — extracts binary from tarball
  - `PlatformBinaryName` kept as deprecated alias
- `cmd/graphit/commands/lifecycle.go`
  - `self-update` now downloads `.tar.gz` asset, verifies checksum on archive, extracts binary, atomically replaces
- `README.md`
  - Option 1: one-liner `curl -fsSL .../install.sh | bash` and `irm .../install.ps1 | iex`
  - Option 2: manual tarball download with `tar -xzf` instructions
- `docs/site/index.html`
  - Linux/macOS tab: `curl -fsSL .../install.sh | bash`
  - Windows tab: `irm .../install.ps1 | iex`

## Breaking Change for Private Brands

`graphit self-update` now looks for asset `graphit-<os>-<arch>.tar.gz` instead of the bare binary. Any deployment using a custom `SELF_UPDATE_URL` pointing to bare binaries must update the release endpoint to serve tarballs.

## Verification

- `go build ./internal/updater/... ./cmd/graphit/...` — passes
- Manual test: `install.sh` logic verified for OS detection, checksum verification, extraction
- Release workflow dry-run: `.tar.gz` packaging steps use standard `tar -czf`
