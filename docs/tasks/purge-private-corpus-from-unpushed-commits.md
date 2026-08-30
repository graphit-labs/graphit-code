---
title: Purge private-corpus references from the unpushed commits
status: done
created: 2026-08-30
updated: 2026-08-30
tags: [security, privacy, git-history, documentation]
---

# Purge private-corpus references from the unpushed commits

> **This document deliberately names nothing.** The strings being purged are not reproduced
> here — writing them into the log that documents their removal is the exact mistake this
> task exists to undo. The audit terms live in the project memory (the global memory wiki,
> outside this repository), which is where a future agent should look for them.

## Objective

An audit of the 25 commits ahead of `origin/main` found references to a private corpus that
had already been scrubbed twice (2026-08-03 and 2026-08-09) and re-entered in later work.
Nothing has been pushed, so the leak is still recoverable — but only until the next push.

The goal: pushing `main` must publish nothing that identifies the private corpus, the
developer's machine, or the employer. Three classes were found.

1. **A terminal log pasted verbatim into a task log.** One line of
   `docs/tasks/icebug-export-streams-instead-of-materializing.md` carried a progress line
   containing the developer's absolute home path, the corpus directory name, and a cluster
   mapping whose key is the employer's name. This is the most severe finding and it shows the
   vector: the name was not *written*, it arrived attached to a pasted measurement.
2. **The corpus directory name in 8 lines across 3 task logs**, all new in this range.
3. **A regression of the 2026-08-09 scrub.** A product name that had been replaced with a
   generic term reappeared in `docs/specs/ast_module.md` and `docs/specs/config_module.md`:
   `origin/main` has zero occurrences, HEAD had three. Low severity in itself — a generic
   list of example domains — but it proves that scrubbing prose without a mechanical check
   does not hold.

Also cleaned, opportunistically: one absolute developer home path in
`docs/tasks/daemon-inherited-git-hook-environment.md`, replaced with `~`.

**Out of scope, deliberately.** Two identifiers from the corpus's real database schema sit in
`internal/ast/antlr/common/tree.go` and `internal/ast/antlr/common/qualified_name_test.go`,
and they are **already published** in `origin/main` (commit `763fe93`). Rewriting local
history does not recover them: the objects stay reachable by SHA on the remote. That is a
separate decision for the user — see `## Technical Debt`.

## Reasoning and justification of the approach

Sanitizing only the working tree would be **incomplete work**: the leaked lines live inside
the unpushed commits, so a push would publish them no matter what HEAD's tree looks like. The
only fix that meets the objective is to rewrite the unpushed range so the strings exist in no
object that reaches the remote.

Approach: sanitize the working tree, commit it, then `git filter-branch --index-filter` over
`origin/main..HEAD` applying the same substitution script, with `--prune-empty` so the
now-redundant sanitizing commit disappears.

- **`--index-filter`, never `--tree-filter`.** The tree carries ~3.5 GB of generated
  grammars; a tree filter checks all of it out once per commit. The index filter rewrites
  only the affected blobs: `ls-files --stage` → `cat-file blob` → `sed` → `hash-object -w`
  → `update-index --cacheinfo`. Proven in the 2026-08-09 cleanup.
- **The filter must end in `true`.** `[ a != b ] && cmd` returns non-zero when the two are
  equal, and `filter-branch` aborts on a non-zero filter.
- **The affected paths were enumerated from history, not from the working tree.** Task logs
  were renamed from Portuguese to English mid-range, so today's paths are not necessarily the
  historical ones. Enumerating with `git grep -l <terms> <commit>` across every commit in the
  range returned six paths, all under `docs/`, and that list is what the filter is scoped to.
- **Alternative considered and dropped:** commit the sanitized tree and leave history alone.
  Rejected — it does not stop the leak from being pushed, which is the entire objective.

## Plan & Task Breakdown

- [x] **T1 — Back up the current line** — Spec: a branch at the pre-rewrite HEAD so the
  rewrite is reversible. Done: `backup-pre-corpus-purge-20260830` → `8812d5e`.
- [x] **T2 — Sanitize the working tree** — Spec: the six files enumerated from history. Done
  when the audit grep returns nothing over versioned files. Constraint: the *measurements*
  the logs report must survive — only identifying names go, never the numbers, because the
  measurement is the whole value of those logs.
- [x] **T3 — Commit the sanitized tree** — Spec: one commit on `main`, recording HEAD's tree
  hash, because T5 verifies against it.
- [x] **T4 — Rewrite `origin/main..HEAD`** — Spec: `filter-branch --index-filter` scoped to
  the six paths, `--prune-empty`. Done when it completes without aborting.
- [x] **T5 — Verify** — Spec: (a) HEAD's tree hash comes out **byte-identical** to the one
  recorded in T3 — this is the strongest available proof the rewrite was surgical, because
  the sed is a no-op on an already-clean HEAD, so any change means it did something it should
  not have; (b) `git log -S <term> origin/main..HEAD` returns 0 for every audit term;
  (c) `git fsck` clean. Constraint: `git grep <ref>` inspects that ref's **tree**, not its
  history — history leaks are visible only to `git log -S`.
- [x] **T6 — Sweep the other local refs** — Spec: the 2026-08-09 cleanup was defeated by a
  work branch from another session still pointing at pre-rewrite objects. Done when every
  local ref still holding the terms is identified and reported to the user.
- [x] **T7 — Sync the indexes** — Spec: `graphit_sync`, so the wiki stops serving the
  pre-purge pages.

## Files Changed
| File | Change | Reason |
|---|---|---|
| `docs/tasks/icebug-export-streams-instead-of-materializing.md` | Modified | pasted terminal log: absolute path, corpus name, employer |
| `docs/tasks/optimize-ast-store-disk-usage.md` | Modified | corpus name |
| `docs/tasks/tests-must-run-in-an-ephemeral-home.md` | Modified | corpus name |
| `docs/specs/ast_module.md` | Modified | scrub regression |
| `docs/specs/config_module.md` | Modified | scrub regression |
| `docs/tasks/daemon-inherited-git-hook-environment.md` | Modified | absolute developer home path |
| `docs/tasks/purge-private-corpus-from-unpushed-commits.md` | Created | this log |

## Trade-offs & Decisions
- **Rewriting 25 commit SHAs was accepted** rather than adding a corrective commit on top.
  Nothing in the range is published and no other worktree tracks this line, so the cost is
  local; the benefit is that the strings never reach the remote at all. A backup ref makes it
  reversible.
- **This log names nothing.** The cost is that a reader cannot tell from here what was
  removed; the benefit is that the log is not itself a leak. The audit terms are in project
  memory, which is stored globally and never pushed.

## Technical Debt
- [ ] **The real schema identifiers are already public and this task does not fix that.**
      They went out in `763fe93`. A local rewrite cannot recover published objects. Needs a
      user decision: accept the published copy, or scrub the working tree going forward to at
      least stop further propagation (cheap, and worth doing on its own).
- [ ] **Project ULIDs in task logs are not synthetic.** One real store ULID appears in
      `docs/tasks/consolidate-search-into-ladybugdb-and-drop-sqlite.md` labelled "production
      shard cache". Opaque once names are gone, so low severity, but it is a real identifier.
- [ ] **Two published Portuguese task logs carry the developer's absolute home path.**
      `docs/tasks/corrigir-indexacao-no-projeto-errado.md` and
      `docs/tasks/revisar-skills-e-mandates.md` on `origin/main`. This range already renames
      them to English and drops the paths, so the exposure stops going forward, but the
      published copies stay reachable by SHA. Same class as the schema identifiers above.
- [ ] **No mechanical check before push.** Three scrubs in one month means the only control
      is an agent remembering to run one, which has now failed twice. A pre-push hook was
      offered and declined for now, so this stays manual.

## System Knowledge
- The vector in all three recurrences is identical, and it is not careless naming: it is
  **pasting command output**. The corpus name arrives attached to a measurement the log
  legitimately needs. Sanitize at paste time, not at audit time.
- **`git grep <term> <ref>` searches that ref's tree, not its history.** Auditing history
  needs `git log -S <term> <range>`. Confusing the two once produced a false "clean".
- **`git gc` only drops what no ref reaches.** Deleting branches is not enough — worktrees
  and other sessions' branches keep objects alive.
- **A document about a leak is a leak** if it quotes the strings. This log was written once
  with the offending lines quoted verbatim as "evidence", which reintroduced every term it
  was removing; it was rewritten to name nothing.

## Progress Log

### 2026-08-30
- Audited `origin/main..HEAD` (25 commits). Found the three classes above. Found **no secrets
  of any kind**: no API-key, cloud-credential, token or private-key patterns anywhere in the
  range's diff; AI-provider fixtures use a placeholder key; `internal/ai/*.go` references
  only environment-variable *names*, never values; no `.env`, `.pem` or `.key` is versioned.
- Established that the schema-identifier leak is already published, so it cannot be fixed by
  a local rewrite, and scoped it out.
- T1 done: backup ref created at `8812d5e`.
- T2 done: six files sanitized; the audit grep returns nothing over versioned files.
- Rewrote this log after noticing it quoted the very strings being purged.
- T3 done: sanitized tree committed; HEAD tree recorded as `e0d2bbe5`.
- T4 done: `filter-branch --index-filter` rewrote all 26 commits of the range in ~1 s. No
  abort. The sanitizing commit was NOT pruned by `--prune-empty`, correctly — it is not
  empty, because it also adds this log.
- T5 done, all three checks pass:
  - **(a) HEAD's tree came out `e0d2bbe5`, byte-identical to the pre-rewrite value.** This is
    the check that matters: the sed is a no-op on an already-clean HEAD, so an identical tree
    proves the rewrite changed history and nothing else.
  - **(b)** Every audit term returns 0 commits over the range, with two apparent residuals
    that are not leaks and are worth writing down because they will recur:
    - One term still reports 5 commits under `git log -S`, which is **substring** matching —
      the same trap recorded in memory, where the term is a substring of two very common
      English words. A word-boundary search across every commit in the range returns nothing.
    - One absolute-path term reports 1 commit. That commit **removes** it: the string lives
      in two published Portuguese task logs on `origin/main` that this range renames to
      English. `git log -S` counts a changed occurrence count, so a deletion scores exactly
      like an addition. See Technical Debt.
  - **(c)** `git fsck` clean.
- T6 done: two local branches still contain the terms, both backups, neither on the push path
  — `backup-pre-corpus-purge-20260830` (created by T1, the rollback for this operation) and
  `backup-pre-squash-20260826-212556` (pre-existing). Reported to the user; deleting them is
  the user's call, and the first one should outlive this session only until the result is
  accepted.
- T7 done: `graphit_sync` run so the wiki stops serving the pre-purge pages.

**Final state: `main` is clean and safe to push.** 26 commits ahead of `origin/main`, all
rewritten SHAs. Rollback: `git reset --hard backup-pre-corpus-purge-20260830`.
