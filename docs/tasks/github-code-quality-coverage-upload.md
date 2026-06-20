# GitHub Code Quality – Coverage Upload Setup

## Goal

Enable the GitHub Code Quality coverage upload feature so that coverage results appear automatically on pull requests.

## What Was Done

### 1. `Makefile` – Added `test-coverage` target

The existing `make test` uses `-cover` (prints summary to stdout only — no file generated). A new `test-coverage` target was added that:

- Runs the same two test passes (race + no-race)
- Passes `-coverprofile=coverage.out -covermode=atomic` to `go test`
- Appends the parsers coverage to the main `coverage.out` file
- Adds `test-coverage` to `.PHONY`

The `make test` target (used locally) is **unchanged**.

### 2. `.github/workflows/ci.yml`

Three changes:

1. **Permissions block** – Added `code-quality: write` at the workflow level (required by `actions/upload-code-coverage@v1`).
2. **Test step** – Changed from `make test` → `make test-coverage` in the `test` job.
3. **Two new steps after tests:**
   - `Convert coverage to Cobertura XML`: installs `gocover-cobertura` and converts `coverage.out` → `coverage.xml`
   - `Upload coverage report`: uses `actions/upload-code-coverage@v1` with `language: Go` and `label: code-coverage/go`. Skips upload on PRs from forks (security guard).

### 3. `.gitignore`

Added `coverage.xml` to gitignore (`.out` files were already ignored).

## Reference

- [GitHub Docs – Set up code coverage](https://docs.github.com/pt/code-security/how-tos/maintain-quality-code/set-up-code-coverage)
- `gocover-cobertura`: https://github.com/boumenot/gocover-cobertura

## Files Changed

- `Makefile`
- `.github/workflows/ci.yml`
- `.gitignore`
