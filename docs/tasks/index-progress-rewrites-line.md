# Task: `ast index` progress rewrites a single line instead of stacking one per refresh

**Status: completed** on 2026-08-07. A direct side effect of the progress reporter — it fixed
the 16-minute silence and created a second problem in its place.

## What the Engineer saw

```
  › Parsing: 22226/36824 file(s)
  › Parsing: 22586/36824 file(s)
  › Parsing: 22880/36824 file(s)
  … (25 more of them)
```

Indexing 36824 files took 677s and spat out ~28 lines of `Parsing:`. Each one is the same
sentence with a different number, and together they push everything that came before off the
screen — including `Grammar overrides:`, the line that says whether the index is using the right
grammar.

## Why it was like this

`indexProgressReporter` (`cmd/graphit/commands/runners.go`) called `p.Step`, and `Step` is an
`Fprintln`: always a new line. The 10s throttle existed precisely so there'd be few of them — it
was calibrated for a log, not a screen.

`internal/output` already had the right mechanism, and it just wasn't being used here: `Task`
has always spun a spinner with `\r\033[K`. But `Task` doesn't fit this case, and the reason
matters: outside a terminal, `Task.Update` **prints nothing at all**. Swapping `Step` for `Task`
would bring back the 16-minute silence for everyone who redirects the output — which is exactly
the case for the daemon and CI.

## What now exists

**`Printer.StepProgress`** — same line as `Step`, written with `\r\033[K` and no `\n`.
Consecutive calls overwrite each other. **Without a TTY, it falls back to `Step`**, because
there's no cursor to move in a file: there, each refresh really is its own line, and history is
the only thing the log has.

**The cursor stays parked at the end of that line**, so any subsequent print needs to clear it
first — otherwise the final summary lands on top of half a counter. Instead of spreading that
logic across all the call sites, every `Printer` method now writes through a single point,
`p.println` (and `p.printf`, for `Table`), which clears the transient line if there is one.
`Task`'s spinner was wired into the same state (`p.overwrite`), which incidentally fixes an old
case: a `Step` fired while the spinner was running used to write over it.

**`progressInterval(tty bool)`** separates the two cadences: 200ms with a cursor, the usual 10s
without one. The throttle is still wall-clock-based and still prints every phase change
immediately — none of that changed, only the interval.

**Truncation to terminal width.** A line longer than the screen wraps into two, and `\r` only
returns to the start of the last physical line: the ones above stay on screen as garbage. It
truncates by rune, not by byte.

## Files

| File | Change | Reason |
|---|---|---|
| `internal/output/printer.go` | Modified | `StepProgress`, `EndProgress`, `overwrite`, `println`/`printf`, `truncate`, `termWidth`; `IsTTY` moved from the test helper into production code |
| `internal/output/printer_helpers_test.go` | Modified | `IsTTY` removed from here (now lives in production code) |
| `cmd/graphit/commands/runners.go` | Modified | reporter uses `StepProgress`; `progressInterval`; `EndProgress` after the pipeline |
| `internal/output/progress_test.go` | Created | 6 tests for the transient line |
| `cmd/graphit/commands/index_progress_test.go` | Modified | test for the TTY-based interval |

## Verification

Besides the buffer tests, I ran the real path under a pty (`script -qec`): the 40 updates come
out as a single physical line, each preceded by `\r\033[K`, and the final `✓` clears the line
before printing. `go test -race ./internal/output ./cmd/graphit/commands` passes.

## What was left out

- **`Task.Done`/`Fail` no longer unconditionally emit `\r\033[K`** — they only clear it if there
  actually is a transient line. If the spinner never got to tick (a task under 80ms), there's now
  no escape sequence at all. This is the correct behavior and no test relied on the opposite, but
  it is an observable difference for anyone diffing output.
- The other progress consumers (`ast embed`, `wiki embed`) already used `Task.Update` and were
  left untouched. They have the opposite, known problem: without a TTY they report nothing.

## System knowledge

- `Printer` now has state (`progress`, under a mutex). `WithWriter` still returns a new
  `Printer`, so the copy starts clean — which is correct, since the transient line belongs to the
  stream, not the prefix.
- Lock ordering between `Task` and `Printer`: always `t.mu` → `p.mu`. The spinner and `Done`
  follow the same order; nothing locks `p.mu` before `t.mu`.
- `termWidth()` falls back to 80 when stdout is not a terminal, which is what happens under
  `go test` — that's why the truncation test can assert 80 columns without simulating a pty.
