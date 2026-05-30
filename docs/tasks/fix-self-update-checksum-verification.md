# Fix Self-Update Checksum Verification

## Objective

Fix the `graphit self-update` command which was failing with:

```
✗ Checksum verification failed: reading checksum file: checksum for ".graphit-update-1907810890" not found in /tmp/graphit-checksums-3016442266
```

## Root Cause

Two issues combined:

1. **Wrong filename lookup**: `VerifyChecksum` used `filepath.Base(tmpPath)` to look up the
   checksum entry, but `tmpPath` is a random temp file like `.graphit-update-1907810890`.
   The checksums file only contains entries for real asset names like `graphit-linux-amd64`.

2. **Wrong architecture**: The system used a single `checksums.sha256` file listing all
   platform binaries. This required searching for the correct entry by name and downloading
   a file with checksums for every platform even though only one is needed.

## Solution

Switched to **per-asset checksum files**. Each release binary now has its own `.sha256` file:

- `graphit-linux-amd64` → `graphit-linux-amd64.sha256`
- `graphit-darwin-arm64` → `graphit-darwin-arm64.sha256`
- `graphit-windows-amd64.exe` → `graphit-windows-amd64.exe.sha256`

The checksum URL is derived by appending `.sha256` to the binary download URL. No need
to search for a separate checksums asset in the release.

The `.sha256` file format supports both:
- `hash  filename` (standard sha256sum output)
- `hash` (hash only)

## Files Changed

| File | Change |
|------|--------|
| `internal/updater/updater.go` | Replaced `VerifyChecksumNamed` + `extractChecksum` with `readChecksumFile` (reads first hash from per-asset file). Removed unused `filepath` import. |
| `internal/updater/updater_test.go` | Replaced `TestVerifyChecksumNamed` with `TestVerifyChecksumPerAsset` covering hash+filename, hash-only, mismatch, and empty file cases. |
| `cmd/graphit/commands/lifecycle.go` | Replaced `FindAsset(release, "checksums.sha256")` with `checksumURL := binaryURL + ".sha256"`. Simplified to use `VerifyChecksum`. |

## Key Decisions

- Per-asset `.sha256` files instead of a single multi-file `checksums.sha256` — simpler,
  each platform downloads only its own checksum.
- `readChecksumFile` reads only the first non-empty line's first field — supports both
  `hash  filename` and `hash` formats.
- Kept `VerifyChecksum(filePath, checksumFilePath)` signature unchanged — callers don't
  need to know the internal format.

## Use Cases

### UC-01: Self-update with per-asset checksum verification

- **Actor:** User running `graphit self-update`
- **Preconditions:** A newer release exists with binary assets and per-asset `.sha256` files
- **Main Flow:**
  1. User runs `graphit self-update`
  2. System fetches latest release metadata
  3. System finds the platform binary asset URL
  4. System derives checksum URL by appending `.sha256` to binary URL
  5. System downloads binary to a temp file
  6. System downloads the per-asset `.sha256` file
  7. System reads the hash from the `.sha256` file, computes SHA-256 of the temp file, compares
  8. System atomically replaces the current executable
- **Error Scenarios:**
  - `.sha256` file not available → download fails with HTTP error
  - Checksum mismatch → update aborted, temp files cleaned up
  - Empty `.sha256` file → "empty checksum file" error
- **Postconditions:** Binary updated to latest version
- **Affected Files:** `internal/updater/updater.go`, `cmd/graphit/commands/lifecycle.go`

## Test Cases & Acceptance Criteria

### TC-01: Per-asset checksum with hash+filename format (Ref: UC-01)

```gherkin
Given a binary file with known content
And a .sha256 file containing "hash  filename"
When VerifyChecksum is called
Then verification succeeds
```

### TC-02: Per-asset checksum with hash-only format (Ref: UC-01)

```gherkin
Given a binary file with known content
And a .sha256 file containing only the hash
When VerifyChecksum is called
Then verification succeeds
```

### TC-03: Checksum mismatch (Ref: UC-01)

```gherkin
Given a binary file with known content
And a .sha256 file containing a wrong hash
When VerifyChecksum is called
Then verification fails with "checksum mismatch" error
```

### TC-04: Empty checksum file (Ref: UC-01)

```gherkin
Given a binary file
And an empty .sha256 file
When VerifyChecksum is called
Then verification fails with "empty checksum file" error
```

## Verification

- `make ci` passes with zero errors
- All 4 test scenarios pass in `TestVerifyChecksumPerAsset`
- Existing `TestDownloadAndChecksum` still passes (backward-compatible)

## Impact on Release Pipeline

The release pipeline (CI/CD) must now generate per-asset `.sha256` files instead of
(or in addition to) a single `checksums.sha256`. For each binary, generate:

```bash
sha256sum graphit-linux-amd64 > graphit-linux-amd64.sha256
```
