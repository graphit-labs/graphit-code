# Fix .astignore negation patterns not being respected

## Date
2026-06-22

## Problem
Negation patterns (`!`) in `.astignore` were not working. When a directory like
`internal/ast/antlr/` was excluded, negated children such as
`!internal/ast/antlr/common/` and `!internal/ast/antlr/*/driver.go` were never
evaluated because `filepath.Walk` returned `filepath.SkipDir` on the parent
directory, skipping the entire subtree.

## Root Cause
In `internal/ast/writer.go:collectFiles`, the walker checked `IsIgnored(rel, true)`
for directories and immediately returned `filepath.SkipDir` — never giving negation
patterns a chance to re-include children.

## Changes

### `internal/ignorer/ignorer.go`
- Added `negationPrefixes []string` field to `IgnoreChecker` struct
- During construction (`New()`), collect negation pattern prefixes from all ignore
  files (`.gitignore`, custom files, default patterns)
- Added `ShouldDescend(dirRelPath string) bool` method that checks whether any
  negation prefix falls under the given directory
- Added `readNegationPrefixesFromFile()` and `negationToPrefix()` helpers

### `internal/ignorer/ignorer_test.go`
- Added `TestShouldDescend` — tests with a `.astignore` mimicking the real scenario
- Added `TestShouldDescendWithDefaultPatterns` — tests negation via default patterns
- Added `TestNegationToPrefix` — unit tests for the prefix extraction helper

### `internal/ast/writer.go`
- Changed `collectFiles` directory-skip logic: if a directory is ignored but
  `ShouldDescend` returns true, the walker enters it and lets per-file/per-dir
  `IsIgnored` checks filter individual entries correctly.

## Testing
- `go test ./internal/ignorer/ -v` — all 5 tests pass
- `go test ./internal/ast/ -v -run TestAstIgnore` — passes
- `go build ./...` — clean build
