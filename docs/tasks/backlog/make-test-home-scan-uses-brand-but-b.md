# `make test` home scan uses $(BRAND), but the test binary always uses the default "graphit"

# `make test` home scan uses `$(BRAND)`, but the test binary always uses the default `graphit`

Observed on 2026-08-18 while verifying the task `docs/tasks/tests-must-run-in-an-ephemeral-home.md`. Not fixed: narrow scope (only affects white-label builds) and the task that revealed it was already closed and verified.

## The defect

Two halves of the same mechanism derive the directory name in ways that only coincide in the default build.

1. `internal/brand/testhome.go` creates the ephemeral homes in `TestHomeRoot()` = `filepath.Join(os.TempDir(), Brand+"-test-homes")`.
2. The Makefile `test` target (line ~529) scans `rm -rf "$${TMPDIR:-/tmp}/$(BRAND)-test-homes"`.

`Brand` is `var Brand = "graphit"` (internal/brand/brand.go:10) and only changes via `-ldflags -X`, which the Makefile assembles in `LDFLAGS` (line 29). **The `test` target does not pass `LDFLAGS`** — only `-race -coverprofile=... -covermode=atomic -p 4`. So inside any test binary `Brand` is always `"graphit"`, regardless of the make variable value.

Consequence: `make test BRAND=acme` scans `/tmp/acme-test-homes`, which is never created, while `/tmp/graphit-test-homes` grows without bound — ~330 MB per run on this machine.

`internal/brand/testhome_test.go` asserts the literal `"graphit-test-homes"` and passes, which matches what the code does; its comment is what becomes wrong ("this is the path make test sweeps" is only true when `BRAND=graphit`).

## How to reproduce

```
rm -rf /tmp/graphit-test-homes /tmp/acme-test-homes
make test BRAND=acme        # or just: go test ./internal/brand/
ls -d /tmp/*-test-homes     # shows graphit-test-homes, not acme-test-homes
```

## What has already been ruled out

- **Not `TMPDIR`**: both sides use the same variable, with the same default.
- **Not `init()` failing to run**: it runs and creates the directory; only the name diverges.
- **Not the test being wrong**: the literal assertion reflects the real behavior of the test binary. The problem is the Makefile interpolation promising something the binary doesn't do.

## Possible exits (choose one, don't stack)

1. **Scan via glob**: `rm -rf "$${TMPDIR:-/tmp}"/*-test-homes`. One line, covers any brand, and nothing else on the system uses that suffix. Risk: a glob in `rm -rf` over `/tmp`, which deserves to be read twice.
2. **Don't derive the name from the brand**: fix the directory as `graphit-code-test-homes` (or another literal) in `TestHomeRoot()`, and use the same literal in the Makefile and the test. Test homes are not a brand artifact — no end user ever sees them — so deriving them from `Brand` buys nothing and is the root of the divergence.
3. **Pass `LDFLAGS` in the `test` target**: aligns both sides, but makes tests run under a different brand than the source default, which changes the behavior of 5 tests in `brand_test.go` that reassign `Brand`. Probably the worst of the three.

Preference of the reporter: **2**.

## How to know it worked

- `make test BRAND=acme` followed by `ls -d /tmp/*-test-homes` shows exactly one directory, and the next round of `make test BRAND=acme` deletes it before starting.
- `internal/brand/testhome_test.go` remains green, and the comment about "the path make test sweeps" becomes true.
