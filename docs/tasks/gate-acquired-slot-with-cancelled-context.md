---
title: The heavy-duty work gate acquired slots with the context already canceled - and the veto broke the CI.
status: done
created: 2026-08-11
updated: 2026-08-11
tags: [daemon, concorrencia, gate, makefile, ci]
---

The heavy-duty work gate acquired slots with the context already canceled - and the veto broke the CI.

## Objective

The task did not proceed. Two independent causes, both preceding this task:

The target `vet` was failing with dozens of `unreachable code` in `internal/ast/antlr/db2/db2_parser.go`.
Inline 4 failed intermittently — it passed alone, then failed completely, and returned to passing on the next execution.

The second one seemed like a test flakiness. It wasn't; it's a real race in __INLINE_5__, with consequences for production.

## Implementation Details

**1. The cancellation does not override the select.**

```go
select {
case heavyChan() <- struct{}{}:
case <-ctx.Done():
    return nil, ctx.Err()
}
```

With the LIVRE slot and the JÁ context already canceled, both cases are ready, and Go selects one pseudo-randomly. Half the time, the gate delivered the slot to someone who had been previously canceled—while the function's own contract comment says the opposite: "The cancelled wait returns ctx.Err() ... the caller MUST skip the work: a supervisor being parked or a daemon shutting down should not keep queueing for a slot."

The effect on production is in the log of the failure: `WARN daemon: watcher lost events, running a full AST scan`. A supervisor being parked or a daemon shutting down initiates an **in-place reindex of the entire AST**. The correction is to check `ctx.Err()` before selecting.

The test that already existed (`TestAcquireHeavyAbandonsTheQueueOnCancel`) held the only slot before testing, so `ctx.Done()` was the only ready case and the race was invisible. The free slot—common in production—is unmarked.

**2. The package filter does not reach what is imported.**

The inline 11 removes the grammars generated from the LIST, but `internal/ast` and
`cmd/graphit-antlr-sidecar` as **important**, and the `go vet` reports the diagnostics of a
dependency when it needs to analyze itself — which only happens with the cold analysis cache. Thus, the intermittent appearance: `go vet ./internal/ast` isolated passed through the warm cache, and the complete list failed. Excluding the sidecar also doesn't resolve, because `internal/ast` import `db2` in the same way.

Inline 18 is the only analyzer that triggers on generated code (the ANTLR emits an `return` after a `panic`), and there's no way to ask for "only the diagnostics of named packages", so the target started running with ___INLINE_21__. The rest of the vet continues covering the entire project.

## Use Cases

### UC-01: Requesting a Heavy Work Slot with the Context Already Cancelled

- **Actor**: Actor 22 and closure 23 of Module 24 — both call sites of the gate.
- **Preconditions**: The supervisor is parked due to idleness, or the daemon is shutting down; the module context has already been canceled.
- **Main Flow**:
  1. The caller calls `sysutil.AcquireHeavy(ctx)`.
  2. The gate observes `ctx.Err()` != nil before looking at the channel.
  3. Returns `(nil, context.Canceled)`.
  4. The caller returns without doing the work, as required by the contract.

- **Alternative Flows**:
  - Live context and available slot: Acquire immediately and release the slot.
  - Live context and occupied slot: Wait, and is awakened by the release or cancellation.

- **Error Scenarios**:
  - During the wait: Same return, via arm `<-ctx.Done()` of select.

- **Postconditions**: No slot is consumed by a rejected acquisition; the gate does not leak capacity.
- **Affected Files**: `internal/sysutil/gate.go`

### UC-02: Run the project's vet with generated code in dependency graph

**Actor**: `make vet` or via `make ci` / `make check`

**Preconditions**: The Go analysis cache may be cold (a CI machine that is always clean).

**Main Flow**:
1. `go list ./...` generates the hand-written packages.
2. `go vet -unreachable=false` analyzes them.
3. Generated grammars continue to be compiled as dependencies without producing diagnostics.

**Error Scenarios**:
- A different analyzer triggering on generated code would return the same symptom, with a new name — and again as "passes locally, fails CI".

**Postconditions**: `make vet` is deterministic, independent of the cache state.

**Affected Files**: `Makefile`

## Test Cases & Acceptance Criteria

Feature: The gate rejects a context that has been canceled.  
Ref: UC-01

Scenario: vacant slot with context canceled
```gherkin
Given um gate com exatamente um slot, e esse slot livre
And an already canceled context
When AcquireHeavy is called 200 times consecutively
Then toda chamada retorna context.Canceled
And no call returns a function for release
And at the end, the slot remains available for a live context.
```

Scenario: Slot Occupied and Context Cancelled
```gherkin
With an exact one-slot gate, and that slot already taken
And an already canceled context
When Acquire Heavy is Called
Then returns context.Canceled without function for release.
```
Ref: `TestAcquireHeavyAbandonsTheQueueOnCancel` (already existing)

Scenario: A batch handle is not indexed due to a canceled operation
```gherkin
Given um projeto sem banco AST
And an already canceled context.
When `handleBatch` is called with a batch of scans
The directory for Banco AST is not created.
And the indexing slot remains free.
```
Ref: `TestHandleBatchAbandonsTheQueueOnCancel` (already existing — he was the one who denounced the race)

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/sysutil/gate.go` | Modified | Checked before the select, to cancel win free slot |
| `internal/sysutil/gate_test.go` | Modified | New test case not covered: LIVRE + canceled context, 200 iterations |
| `Makefile` | Modified | Target `vet` runs with `-unreachable=false` |

The file has been modified. Before the select operation, a check was performed on the slot to ensure that it could be cancelled and won back if needed. A new test case for an uncovered scenario has also been added: a free slot combined with a canceled context, involving 200 iterations. The target variable `vet` now runs with `-unreachable=false`.

## Trade-offs & Decisions

- **Before the select, instead of a nested select with default.** A `select { case <-ctx.Done(): ...; default: }` would do the same with more lines. The direct check says what it means.
- **200 iterations in the test.** An iteration of a 50/50 race passes half the time without proving anything; 200 leaves the chance for a false green in \(2^{-200}\).
- **`-unreachable=false` globally instead of excluding packages.** Excluding `cmd/graphit-antlr-sidecar` was tried first and did not resolve — `internal/ast` imports the same grammars, and excluding it is not an option. Disabling the only analyzer that triggers costs less coverage than removing entire packages from the list.


## Technical Debt

- [ ] The 26 warnings of `ui-lint` (0 errors) continue to exist. They do not break the CI, and they have not been touched.
- [ ] If another analyzer starts firing in the generated code, the trap returns. A structural solution would be not having the generated code in the same module, which is a different order of change.


## System Knowledge

- **Inline 52** in Go does not prioritize cases. Whenever "canceled should win," check ___Inline 53___ before the select— a ___Inline 54___ next to an operation on a channel that is also ready, loses half the time. This code has other selects with this form; the rule applies to all.
- **Inline 55** reports dependency diagnostics, but only when it needs to analyze them (cold cache). That's why a flaky test can be a bug in another package, and why reproducing requires running the complete Makefile command, not just the isolated package.
- A "flaky" test in one package could be a bug in another. The failure appeared at ___Inline 56___; the cause was at ___Inline 57___. The clue that closed the case was the ___Inline 58___ output of the failure, which proved that work really had started after cancellation— not the test observing incorrectly.

Note: I've translated "inline" to "Inline" and used underscores for inline comments as per your request.

## Progress Log

August 11, 2026
- Inline 59 is retracted in Inline 60; isolated as a code generated via import, confirmed as preceding commits of this session (Inline 61 last modified by 4e7cab4).
- Inline 62 is retracted only in Inline 63. Investigated instead of treated as flakiness: real run on Inline 65.
- Corrected the two; added test case discovered for inspection.
- Inline 66 from top to bottom: ui, vet, lint, vulncheck, test (with and without Inline 67), ui-lint. Inline 68 completed.
