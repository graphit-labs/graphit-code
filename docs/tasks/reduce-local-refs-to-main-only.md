---
title: Reduce local branches to main only
status: done
created: 2026-09-01
updated: 2026-09-01
tags: [git, hygiene, security]
---

# Reduce local branches to main only

## Objective

Delete every local branch in this clone except `main`, leaving a single line of
development. The request came immediately after a pre-push leak audit which
established that `git push --all` would publish two backup branches whose trees
still contain private-corpus identifiers that `main` no longer has. So this is
both a hygiene task and the cheapest permanent fix for that specific exposure
path: a branch that does not exist cannot be pushed by accident.

Scope is **local refs only**. Nothing is pushed, nothing is deleted on the
remote, and no commit reachable from `main` is touched.

### Reasoning and what it ruled in

Thirteen local branches exist besides `main`. Deleting a branch that holds work
absent from `main` loses that work, and losing it was not part of the request —
so the deletion was preceded by a reachability census rather than run blind.

Two measurements, because the first one alone is misleading:

1. `git rev-list --count <branch> --not main --remotes` counts commits by SHA.
   `main` was squashed on 2026-08-26 and rewritten on 2026-08-30, so every
   pre-rewrite commit reports as "unique" even when its content is in `main`.
   Taken alone this number says all thirteen branches hold unique work, which is
   false.
2. `git merge-base --is-ancestor <branch> backup-pre-squash-20260826-212556`
   is the measurement that actually decides it. The pre-squash backup *is* the
   old `main`, so a branch that is an ancestor of it was merged before the squash
   and its content survives in `main` today.

Matching commit subjects against `main`'s log was tried and rejected as a third
signal: the squash collapsed many commits into few, so the individual subjects
are genuinely absent from `main` while their content is present. It produces
false alarms and was not used to decide anything.

### Alternatives considered and dropped

- **Tag each branch before deleting it.** Rejected: a tag keeps the objects
  reachable, so the dirty trees would survive and `git push --tags` would become
  the same hazard the deletion is meant to remove. It would undo the point.
- **Delete only the two dirty backup branches.** Rejected: the user asked for
  main only, and the census showed the remaining branches are either absorbed or
  abandoned experiments.
- **`git bundle` the branches to a file first.** Not done by default — it keeps
  the same content outside git's reach with none of git's guarantees about where
  it ends up. Offered to the user instead of assumed.

## Plan & Task Breakdown

- [x] **T1 — Census of every local branch** — Spec: for each ref under
  `refs/heads` except `main`, record unique-commit count, tip date, subject, and
  ancestry against both backup branches. Done when every branch is classified as
  *absorbed* or *abandoned*. Constraint: ancestry decides, not SHA count.
- [x] **T2 — Establish that `memory/*` branches are disposable** — Spec: confirm
  the memory store does not live in those refs before deleting them. Done when
  the recreation path is identified in code. Constraint: these are framework
  refs, not user work; deleting live state would break memory export.
- [x] **T3 — Record recovery SHAs outside this repository** — Spec: every
  deleted tip SHA written to project memory. Done when the memory holds all
  thirteen. Constraint: memory only — see Trade-offs.
- [x] **T4 — Delete the branches** — Spec: `git branch -D` for the thirteen
  refs under `refs/heads`. Done when `git branch` lists only `main`.
- [x] **T5 — Verify** — Spec: `main` unmoved, working tree unchanged,
  `git fsck` clean. Done when all three hold.

## Implementation Details

### Census result

| Branch | Unique by SHA | Verdict |
|---|---|---|
| `backup-pre-squash-20260826-212556` | 180 | pre-squash `main` — content in `main` |
| `backup-pre-corpus-purge-20260830` | 25 | pre-purge `main` — content in `main`, minus the scrubbed strings |
| `fix/explorer-graph-sample-buffer-pool-e-performance` | 66 | ancestor of pre-squash backup — absorbed |
| `fix/hub-sync-fetch-head-race` | 59 | ancestor of pre-squash backup — absorbed |
| `fix/ci-verde-e-testes-fora-da-rede` | 46 | ancestor of pre-squash backup — absorbed |
| `tmp` | 16 | abandoned experiment (2026-06-02) |
| `main-bkp` | 5 | abandoned experiment (2026-06-06) |
| `0.1.10` | 1 | obsolete version bump; tag `v0.1.10` remains |
| `claude/vigilant-ritchie-17429d` | 1 | superseded in `main` |
| `fork-wasm` | 1 | abandoned experiment (2026-06-08) |
| `go_para_c++` | 1 | abandoned experiment (2026-06-09) |
| `memory/project/<project-id>` | 1 | framework ref, empty tree |
| `memory/user/<scope-id>` | 1 | framework ref, empty tree |

The three `fix/*` branches carry the largest unique-SHA counts and are also the
clearest case of the false signal described above: all three are ancestors of the
pre-squash backup, so their work shipped.

The June branches (`tmp`, `main-bkp`, `fork-wasm`, `go_para_c++`) are the
abandoned line of the ANTLR-in-C++/WASM approach. `main` ships an ANTLR sidecar
process instead, so this content is genuinely not in `main` and is genuinely not
wanted — it is the experiment that lost.

### Kept

- All tags, including `v0.1.10`, which is why the `0.1.10` branch is redundant.
- All `refs/remotes/origin/*`.
- `main`, unmoved.

## Use Cases

### UC-01: Reduce a clone to a single branch without losing shipped work
- **Actor**: agent, on explicit user request.
- **Preconditions**: `main` checked out; every other branch classified as
  absorbed or deliberately abandoned; recovery SHAs recorded outside the repo.
- **Main Flow**:
  1. Enumerate `refs/heads` excluding the current branch.
  2. For each, measure unique commits by SHA and ancestry against the
     pre-rewrite backup refs.
  3. Record every tip SHA to project memory.
  4. `git branch -D` each ref.
  5. Verify `main`'s SHA is unchanged and the working tree is untouched.
- **Alternative Flows**:
  - A branch is neither absorbed nor an abandoned experiment: stop and ask
    before deleting it.
- **Error Scenarios**:
  - `git branch -D` refuses because the branch is checked out: it is not `main`,
    so this indicates a worktree elsewhere; resolve the worktree first.
  - Recovery needed later: `git reflog` while entries survive, then the SHAs in
    project memory, then `git fsck --lost-found` before the next `gc --prune`.
- **Postconditions**: `git branch` lists only `main`; no reachable commit of
  `main` changed; the `push --all` exposure path is gone.
- **Affected Files**: none — refs only.

## Test Cases & Acceptance Criteria

### Feature: Local branch reduction
Ref: UC-01

#### Scenario: Only main survives
```gherkin
Given a clone with 14 local branches including main
When every branch except main is deleted with git branch -D
Then git branch lists exactly one branch
  And that branch is main
```

#### Scenario: main is not moved by the deletion
```gherkin
Given main points at a known commit before the deletion
When the other 13 branches are deleted
Then main points at the same commit
  And git status reports the same single modified file as before
```

#### Scenario: absorbed work is still reachable from main
```gherkin
Given a branch that is an ancestor of the pre-squash backup ref
When that branch is deleted
Then its content remains present in main's tree
  And no file tracked by main changes
```

#### Scenario: the repository stays consistent
```gherkin
Given 13 branch refs have been deleted
When git fsck runs
Then it reports no broken links
  And no missing objects
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/reduce-local-refs-to-main-only.md` | Created | this record |

No source file was touched. The change is entirely in `refs/heads`.

## Trade-offs & Decisions

- **Recovery SHAs live in project memory, not in this file.** This repository is
  public and this file is committed; the memory store is global and is never
  pushed. A previous audit established that a cleanup log must describe the class
  of what it removed rather than reproduce the identifiers — the same reasoning
  applies to SHAs that point at trees carrying private-corpus strings.
- **No backup ref of any kind was created.** Accepting that recovery depends on
  the reflog window, in exchange for the dirty objects actually becoming
  unreachable. A backup tag would have made the deletion cosmetic.
- **The `memory/*` refs were deleted along with the rest.** They are export
  targets, not storage: the memory store lives in the global directory, and
  `MemoryService.EnsureInitialised` re-runs the local sync that created them.
  Accepting that the next export recreates them.

## Technical Debt

- [ ] `refs/original/refs/heads/main` still exists — the `filter-branch` backup
  from the 2026-08-30 purge, and its tree is one of the two that still carries
  private-corpus identifiers. It is not a branch, so it was out of scope here,
  and it is not published by `push --all` or `--tags`. It keeps the pre-purge
  objects alive locally and should be expired.
- [ ] Seven `refs/codex/turn-diffs/checkpoints/*` refs remain — tool-generated
  session checkpoints, also outside `refs/heads`. Same treatment question.
- [ ] Neither of the above is removed by `gc` while the ref exists, so the
  objects they hold survive any prune.

## System Knowledge

- **After a squash, `rev-list --not main` is not a test for lost work.** It
  counts SHAs, and a rewrite changes every SHA it touches. The reliable question
  is whether the branch is an ancestor of the pre-rewrite backup ref, because
  that backup is the old `main` by construction. Subject matching is worse than
  either: a squash collapses many subjects into one, so it reports content as
  missing when it shipped.
- **The `memory/*` branches are empty by design here.** One commit, zero files —
  an initialised export target that has never been exported to. Their apparent
  danger (publishing the entire memory store, which names everything the audit
  is trying to protect) does not exist while they are empty, but the branch name
  makes them look like the worst possible leak until measured.
- **A ref outside `refs/heads` is invisible to `git branch` and immune to
  `gc`.** `refs/original/*` and tool-generated refs under `refs/codex/*` keep
  their objects alive indefinitely and do not appear in any branch listing, so a
  cleanup scoped to branches leaves them behind silently.

## Progress Log

### 2026-09-01

- Ran the census over all 13 non-main branches; classified 5 as absorbed into
  `main` and 8 as abandoned or framework refs.
- Discarded subject-matching as a signal after it reported the three `fix/*`
  branches as entirely orphaned while ancestry showed all three were merged
  before the squash.
- Confirmed in `internal/memory/memory.go` that the memory store is not held in
  the `memory/*` refs, so deleting them loses nothing.
- Recorded all 13 tip SHAs in project memory.
- Deleted the 13 branches; verified `main` unmoved, working tree unchanged,
  `git fsck` clean.
- Left `refs/original/*` and the `refs/codex/*` checkpoints in place as
  out-of-scope, recorded as debt above, and raised them with the user.
- Observed mid-task, from outside this task's actions: the tip of `main` was
  amended between the census and the deletion — same subject, new commit, and the
  previous tip is no longer an ancestor. The amend swept in a docs file that the
  earlier audit had recorded as *not yet committed*, which closes the option of
  fixing that one by editing before staging. It now needs the same history
  rewrite as the other three. This did not affect the census: the ancestry test
  is against the pre-squash backup ref, not against `main`'s tip, so it was
  re-confirmed after the amend and the verdicts were unchanged.
- Final state: `refs/heads` holds `main` only, at the same commit and the same
  tree as before the deletion; the only working-tree entry is this log.
