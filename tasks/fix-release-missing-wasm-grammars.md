# Fix: Release builds missing WASM grammars → AST indexing broken

## Problem

Release builds (v0.1.12 and earlier) shipped without tree-sitter WASM grammar
files in the embedded runtime. This caused `graphit init` and `graphit sync` to
report "AST: 0 files up to date" and never create the LadybugDB, search index,
or shard cache files in `.graphit/ast/project/`.

The dev version worked because local builds (`make install`) ran on machines
where `make build-grammars` had already been executed.

## Root Cause

1. The `.wasm` grammar files (`internal/ast/grammars/*.wasm`) are gitignored
   (compiled artifacts built from C sources via zig).
2. The release CI (`.github/workflows/release.yml`) never ran `make build-grammars`
   before the platform build targets.
3. The `bundle_ast` Makefile function conditionally copies `.wasm` files:
   ```makefile
   @if ls internal/ast/grammars/*.wasm 1>/dev/null 2>&1; then \
       cp internal/ast/grammars/*.wasm cmd/launcher/runtime/ast/grammars/; \
   fi
   ```
   Since no `.wasm` files existed in CI, none were bundled.
4. Without grammars, `initBuiltinGrammars()` returns an empty map → `tsExtMap`
   is empty → `HasTreeSitterForExtension()` always returns false →
   `collectFiles()` returns 0 files.

## Fix

Added `zig` installation and `make build-grammars` step to all three platform
build jobs in `.github/workflows/release.yml`:

- **Linux**: `zig` added to `apt-get install` + new "Build WASM grammars" step
- **macOS**: `zig` added to `brew install` + new "Build WASM grammars" step
- **Windows**: `mingw-w64-x86_64-zig` added to MSYS2 packages + new step

## Hotfix for existing v0.1.12 installs

```bash
# Copy grammars from source tree to runtime directory
mkdir -p ~/.graphit/runtime/v0.1.12/ast/grammars
cp internal/ast/grammars/*.wasm ~/.graphit/runtime/v0.1.12/ast/grammars/
```

## Files Changed

- `.github/workflows/release.yml` — added zig + build-grammars to all build jobs

## Verification

After copying grammars to the runtime directory, `graphit sync` successfully
indexed 347 files and created all expected AST artifacts.
