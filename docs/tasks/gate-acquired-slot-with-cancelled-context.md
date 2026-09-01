---
title: The heavy-work gate acquired a slot with the context already cancelled — and vet broke CI
status: done
created: 2026-08-11
updated: 2026-08-11
tags: [daemon, concorrencia, gate, makefile, ci]
---

# The heavy-work gate acquired a slot with the context already cancelled — and vet broke CI

## Objective

`make ci` was not passing. Two independent causes, both predating this task:

1. The `vet` target failed with dozens of `unreachable code` in `internal/ast/antlr/db2/db2_parser.go`.
2. `TestHandleBatchAbandonsTheQueueOnCancel` failed intermittently — it passed in isolation, failed
   with the whole package, and passed again on the next run.

The second looked like test flakiness. It was not: it is a real race in `sysutil.AcquireHeavy`,
with a consequence in production.

## Implementation Details

**1. `internal/sysutil/gate.go` — the cancellation did not win the select.**

```go
select {
case heavyChan() <- struct{}{}:
case <-ctx.Done():
    return nil, ctx.Err()
}
```

With the slot FREE and the context ALREADY cancelled, both cases are ready, and Go picks a ready
case pseudo-randomly. About half the time the gate handed the slot to someone who had already been
cancelled — and the function's own contract comment says the opposite: "A
cancelled wait returns ctx.Err() ... the caller MUST skip the work: a supervisor being parked
or a daemon shutting down should not keep queueing for a slot".

The effect in production is in the failure log: `WARN daemon: watcher lost events, running a full
AST scan`. A supervisor being parked or a daemon shutting down started a **full AST reindex on the
way out**. The fix is to check `ctx.Err()` before the select.

The test that already existed (`TestAcquireHeavyAbandonsTheQueueOnCancel`) held the only slot
before testing, so `ctx.Done()` was the only ready case and the race was invisible. The free slot —
the common case in production — was the uncovered one.

**2. `Makefile` — the package filter does not reach what is imported.**

`GO_PKGS_SKIP` takes the generated grammars out of the LIST, but `internal/ast` and
`cmd/graphit-antlr-sidecar` **import** them, and `go vet` reports the diagnostics of a
dependency when it has to analyze it itself — which only happens with a cold analysis cache. Hence
the appearance of intermittency: `go vet ./internal/ast` in isolation passed with a warm cache, and
the full list failed. Excluding the sidecar does not solve it either, because `internal/ast` imports
`db2` all the same.

`unreachable` is the only analyzer that fires on generated code (ANTLR emits a `return` after a
`panic`), and there is no way to ask vet for "only the diagnostics of the named packages", so the
target now runs with `-unreachable=false`. Everything else in vet still covers the whole project.

## Use Cases

### UC-01: Requesting a heavy-work slot with the context already cancelled
- **Actor**: `SyncModule.handleBatch` and the `cycle()` closure of `ast.RunEmbeddingLoop` — the gate's two call sites.
- **Preconditions**: the supervisor is being parked for idleness, or the daemon is shutting down; the module context has already been cancelled.
- **Main Flow**:
  1. The caller calls `sysutil.AcquireHeavy(ctx)`.
  2. The gate observes `ctx.Err()` != nil before looking at the channel.
  3. It returns `(nil, context.Canceled)`.
  4. The caller returns without doing the work, as the contract requires.
- **Alternative Flows**:
  - Live context and free slot: acquires immediately and returns the release.
  - Live context and taken slot: waits, and is woken by the release or by the cancellation.
- **Error Scenarios**:
  - Cancellation during the wait: same return, through the `<-ctx.Done()` arm of the select.
- **Postconditions**: no slot is consumed by a refused acquisition; the gate does not leak capacity.
- **Affected Files**: `internal/sysutil/gate.go`.

### UC-02: Running the project's vet with generated code in the dependency graph
- **Actor**: `make vet`, directly or via `make ci` / `make check`.
- **Preconditions**: the Go analysis cache may be cold (a clean CI machine always is).
- **Main Flow**:
  1. `go list ./...` minus `GO_PKGS_SKIP` produces the hand-written packages.
  2. `go vet -unreachable=false` analyzes them.
  3. The generated grammars are still compiled as dependencies, without producing a diagnostic.
- **Error Scenarios**:
  - A different analyzer starting to fire on the generated code would bring the same symptom back, under another name — and again as "passes locally, fails in CI".
- **Postconditions**: `make vet` is deterministic, independent of the cache state.
- **Affected Files**: `Makefile`.

## Test Cases & Acceptance Criteria

### Feature: The gate refuses a cancelled context
Ref: UC-01

#### Scenario: free slot and cancelled context
```gherkin
Given a gate with exactly one slot, and that slot free
  And an already cancelled context
When AcquireHeavy is called 200 times in a row
Then every call returns context.Canceled
  And no call returns a release function
  And at the end the slot is still available to a live context
```

#### Scenario: taken slot and cancelled context
```gherkin
Given a gate with exactly one slot, and that slot already taken
  And an already cancelled context
When AcquireHeavy is called
Then it returns context.Canceled with no release function
```
Ref: `TestAcquireHeavyAbandonsTheQueueOnCancel` (already existing)

#### Scenario: a cancelled handleBatch does not index
```gherkin
Given a project with no AST database
  And an already cancelled context
When handleBatch is called with a rescan batch
Then the AST database directory is not created
  And the indexing slot stays free
```
Ref: `TestHandleBatchAbandonsTheQueueOnCancel` (already existing — it was the one denouncing the race)

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/sysutil/gate.go` | Modified | `ctx.Err()` checked before the select, so the cancellation wins over the free slot |
| `internal/sysutil/gate_test.go` | Modified | New test for the uncovered case: FREE slot + cancelled context, 200 iterations |
| `Makefile` | Modified | The `vet` target runs with `-unreachable=false` |

## Trade-offs & Decisions

- **`ctx.Err()` before the select, instead of a nested select with a default.** A
  `select { case <-ctx.Done(): ...; default: }` would do the same with more lines. The direct check
  says what it means.
- **200 iterations in the test.** One iteration of a 50/50 race passes half the time without
  proving anything; 200 puts the chance of a false green at 2⁻²⁰⁰.
- **`-unreachable=false` globally instead of excluding packages.** Excluding `cmd/graphit-antlr-sidecar`
  was tried first and did not solve it — `internal/ast` imports the same grammars, and excluding it
  is not an option. Turning off the only analyzer that fires costs less coverage than
  removing entire packages from vet.

## Technical Debt

- [ ] The 26 `ui-lint` warnings (0 errors) remain. They do not break CI, and they were not touched.
- [ ] If another vet analyzer starts firing on the generated code, the trap comes back. The
  structural solution would be not having the generated code in the same module, which is a change
  of another order.

## System Knowledge

- **`select` in Go does not prioritize cases.** Whenever "cancelled must win", check `ctx.Err()`
  before the select — a `<-ctx.Done()` next to a channel operation that is also ready
  loses the coin toss half the time. This code has other selects of that shape; the rule holds
  for all of them.
- **`go vet` reports diagnostics from dependencies**, but only when it has to analyze them (cold
  cache). That is why a vet problem can pass locally and fail in CI, and why
  reproducing it requires running the full Makefile command, not the package in isolation.
- **A "flaky" test in one package can be a bug in another.** The failure showed up in
  `internal/daemon`; the cause was in `internal/sysutil`. The clue that closed the case was the
  `WARN` in the failure output, which proved that the work had REALLY started after the
  cancellation — it was not the test observing wrongly.

## Progress Log

### 2026-08-11
- `make ci` failed at `vet`; cause isolated as generated code reached through an import, and
  confirmed as predating this session's commits (`db2_parser.go` last changed in
  4e7cab4).
- `make ci` failed at `test`, only in `TestHandleBatchAbandonsTheQueueOnCancel`. Investigated
  instead of treated as flakiness: a real race in `AcquireHeavy`.
- Both fixed; a test for the discovered case added.
- `make ci` green end to end: ui, vet, lint, vulncheck, test (with and without `-race`),
  ui-lint. `make install` completed.
