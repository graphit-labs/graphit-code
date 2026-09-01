---
title: Translate the remaining Portuguese task logs and scrub machine-specific identifiers from history
status: in-progress
created: 2026-09-01
updated: 2026-09-01
tags: [docs, i18n, git, security]
---

# Translate the remaining Portuguese task logs and scrub machine-specific identifiers from history

## Objective

Two things the user asked for in one request, which share a target set of files
and therefore share a task log.

**1. English-only, enforced.** The project convention is that everything written
here is in English — code, comments, docs, task logs, file names, no exceptions.
A previous pass rewrote 28 task logs, but it did not finish the job: a survey
found **605 lines carrying strong Portuguese markers across 20 task logs**. The
user named one file (`generated-instructions-drift-preamble-and-canonical-query-templates.md`,
the worst at 267 lines) and asked for "that one and whichever else needs it", so
the scope is all 20.

**2. The four leaking lines, removed from history.** The pre-push audit earlier
today found four lines in the 53 commits ahead of `origin/main` that carry
machine-specific identifiers the public repository must not have. All four are
now committed, so editing the working tree is not enough — the blobs are in the
history and a push sends them.

### Reasoning and what it ruled in

The two halves are one task because they overlap: the file with the most
Portuguese is also one of the files with a leaking line. Translating it rewrites
the line anyway, so splitting the work would mean touching it twice and risking
a merge of two half-finished states.

They are still **two commits**, because they answer to different reviewers: the
translation is editorial and belongs in one reviewable diff, and the scrub is a
security change whose diff should be readable on its own.

The order is forced by the verification, not by preference. The tree has to be
clean and committed *before* the history rewrite, because the proof that the
rewrite was surgical is that `HEAD`'s tree comes out **byte-identical**: the
substitution script is a no-op on an already-clean `HEAD`, so an identical tree
means the rewrite touched history and nothing else. Reversed, there is nothing to
compare against.

### Justification of the approach, and the alternatives dropped

- **`git filter-branch --index-filter`, not `--tree-filter`.** The tree carries
  ~3.5 GB of generated grammars; `--tree-filter` checks the whole thing out once
  per commit. The index filter reads only the affected blobs:
  `ls-files --stage` → `cat-file blob` → `sed -f` → `hash-object -w` →
  `update-index --cacheinfo`.
- **Not a new commit on top.** For a leak in commits that have not been pushed,
  a corrective commit fixes the tip and ships the dirty blobs of every earlier
  commit along with it. Rewriting is the only thing that works while the range is
  still local; once pushed, nothing on the client side works at all.
- **Paths enumerated from the history, not from the current tree.** Task logs
  were renamed from Portuguese to English filenames inside this range, so today's
  paths are not the historical ones. The path list comes from walking
  `git rev-list` and grepping each commit.
- **The substitution script lives in `/tmp`, never in the repository** — a script
  written inside the repo becomes part of the tree being cleaned, and it contains
  exactly the strings being removed.
- **Translation is delegated in parallel for the smaller files, done directly for
  the two large ones.** 18 of the 20 files have between 4 and 38 Portuguese lines,
  which is independent per-file work; the two largest are the ones where
  technical fidelity is most at risk and are handled directly.

## Plan & Task Breakdown

- [x] **T1 — Survey the extent of the Portuguese** — Spec: count strong-marker
  lines per tracked `.md`. Done when every affected file is listed with a count.
  Constraint: weak markers (`que`, `com`, `para`, `uma`) also match English docs
  quoting Portuguese commit subjects or identifiers, and inflate the result — the
  survey must use a strong-marker set and be spot-checked by reading matches.
- [ ] **T2 — Translate the two large files directly** — Spec:
  `generated-instructions-drift-preamble-and-canonical-query-templates.md` (267
  lines) and `redesign-frontend-experience.md` (72). Done when no Portuguese
  prose remains. Constraint: identifiers, paths, code blocks, numbers, Gherkin
  keywords and measured values are preserved verbatim; only prose changes.
- [ ] **T3 — Translate the remaining 18 files** — Spec: the scattered Portuguese
  lines in the files listed under Implementation Details. Done when the survey
  returns zero for each. Constraint: same as T2, plus no absolute path and no
  project ULID may be introduced by the rewrite.
- [ ] **T4 — Scrub the four leaking lines in the working tree** — Spec: replace
  the full project ULID and its prefix form with a pseudonym, and the developer
  home path with a portable form, in the four files. Done when a tree grep for
  both classes returns zero. Constraint: the mapping line — the one that connects
  the pseudonym to the real prefix — must not survive in any form.
- [ ] **T5 — Commit the two changes separately** — Spec: translation commit, then
  scrub commit. Done when the tree is clean. Constraint: commit messages in
  English, and the scrub message names the class, never the strings.
- [ ] **T6 — Rewrite the history over `origin/main..HEAD`** — Spec:
  `filter-branch --index-filter` with the `/tmp` script. Done when both classes
  return zero in every commit of the range, checked with a word boundary.
  Constraint: the filter must end in `true` — a bare `[ a != b ] && cmd` returns
  non-zero when the two are equal and aborts the run.
- [ ] **T7 — Verify** — Spec: `HEAD`'s tree byte-identical to the pre-rewrite
  value; both classes at zero across the range; `git fsck` clean; the build and
  the test suite unaffected. Done when all four hold.

## Implementation Details

### Portuguese survey (T1) — the 20 files

| File | PT lines | Total | Handling |
|---|---|---|---|
| `generated-instructions-drift-preamble-and-canonical-query-templates.md` | 267 | 495 | direct |
| `redesign-frontend-experience.md` | 72 | 214 | direct |
| `wiki-document-as-the-unit-and-one-compile-n-replicas.md` | 38 | 375 | delegated |
| `explorer-opens-code-at-entity-line.md` | 34 | 299 | delegated |
| `embedded-language-parsing.md` | 27 | 560 | delegated |
| `embedding-crash-loop-due-to-tokenizer-panic.md` | 24 | 238 | delegated |
| `hub-on-s3-icebug-and-lancedb.md` | 19 | 2564 | delegated |
| `make-test-slowness-measured-and-132mb-download-hidden-in-tests.md` | 18 | 138 | delegated |
| `search-index-copy-load-and-late-vector-index.md` | 17 | 126 | delegated |
| `trim-captured-names-in-treesitter-adapter.md` | 16 | 288 | delegated |
| `icebug-remote-graph-on-s3-feasibility.md` | 16 | 168 | delegated |
| `fts-build-starved-by-a-flat-buffer-pool-ceiling.md` | 15 | 297 | delegated |
| `search-index-mixed-vector-batch.md` | 10 | 118 | delegated |
| `ast-explorer-hide-labels-and-file-source-404.md` | 6 | 272 | delegated |
| `explorer-500-on-large-graph-limit-after-expansion.md` | 6 | 320 | delegated |
| `consolidate-search-into-ladybugdb-and-drop-sqlite.md` | 4 | 256 | delegated |
| `daemon-cross-pipeline-resource-gate.md` | 4 | 386 | delegated |
| `fix-indexing-in-wrong-project.md` | 4 | 270 | delegated |
| `the-parse-cache-stops-paying-for-the-same-string-twice.md` | 4 | 394 | delegated |
| `unify-lancedb-native-resolution.md` | 4 | 91 | delegated |

### The two identifier classes being scrubbed (T4, T6)

Named by class, not reproduced — this file is committed to a public repository.

1. **The private project's store ULID**, in two forms: the full 26-character
   identifier, and a truncated prefix. The prefix looks opaque and is not: one
   ecosystem lookup resolves it to a name and a path, and one line of prose
   mapping prefix to pseudonym removes even that step. Both forms are replaced by
   a pseudonym. The ULID of this project is *not* in scope — it is already
   published in the committed lockfile.
2. **An absolute path containing the developer's username**, in two files. Both
   become `~`-relative, which is portable and reveals nothing.

Four lines, four files: two files carry class 1 (three lines total, one of them
being the mapping line), two carry class 2 (one line each).

## Use Cases

### UC-01: Enforce the English-only convention across the documentation tree
- **Actor**: agent, on user request or on discovering a violation.
- **Preconditions**: the convention is recorded in project memory; the tracked
  `.md` set is known.
- **Main Flow**:
  1. Survey tracked markdown with a strong Portuguese-marker set.
  2. Spot-check matches by reading them, to separate prose from incidental
     matches (quoted commit subjects, Portuguese identifiers in code).
  3. Rewrite the prose in English, preserving identifiers, paths, code blocks,
     numbers and Gherkin keywords verbatim.
  4. Re-run the survey; a count above zero means a file was missed.
- **Alternative Flows**:
  - A Portuguese string is a quoted historical commit subject or a code
    identifier: leave it, and say so — translating it would falsify the record.
- **Error Scenarios**:
  - The survey reports a file that contains no Portuguese prose: the marker set
    is matching code or quotations; tighten it rather than editing the file.
- **Postconditions**: no Portuguese prose in any tracked markdown file.
- **Affected Files**: the 20 listed above.

### UC-02: Remove a machine-specific identifier from unpushed history
- **Actor**: agent, before a push.
- **Preconditions**: the identifier is present in commits ahead of the remote and
  absent from the remote itself; the range is unpushed.
- **Main Flow**:
  1. Sanitise the working tree and commit, so `HEAD` is clean.
  2. Record `git rev-parse HEAD^{tree}`.
  3. Enumerate affected paths by walking the history, not the current tree.
  4. Write the substitution script outside the repository.
  5. Run `filter-branch --index-filter` over the range, filter ending in `true`.
  6. Assert `HEAD`'s tree is byte-identical to the recorded value.
  7. Assert both classes return zero in every commit of the range, with a word
     boundary.
- **Alternative Flows**:
  - The identifier is also in a commit message: add `--msg-filter` with the same
    script.
- **Error Scenarios**:
  - `HEAD`'s tree changed: the rewrite altered more than history. Restore from
    the `refs/original` backup and narrow the script.
  - `filter-branch` aborts on the first commit: the filter's last command
    returned non-zero on a no-op path.
  - A `git log -S` hit that is actually a deletion or a substring of an English
    word: confirm with a word-boundary grep before treating it as a leak.
- **Postconditions**: both classes at zero across the range; the tip's content
  unchanged; the range is safe to push.
- **Affected Files**: four task logs.

## Test Cases & Acceptance Criteria

### Feature: English-only documentation
Ref: UC-01

#### Scenario: no Portuguese prose remains in the surveyed files
```gherkin
Given 20 task logs carrying 605 lines with strong Portuguese markers
When each file has been rewritten in English
Then the survey reports zero marker lines for every one of the 20 files
```

#### Scenario: technical content survives translation untouched
```gherkin
Given a task log containing code blocks, file paths and measured values
When its prose is translated to English
Then every code block is byte-identical to before
  And every file path is unchanged
  And every numeric measurement is unchanged
```

#### Scenario: a quoted Portuguese commit subject is preserved
```gherkin
Given a task log quoting a historical commit subject written in Portuguese
When the file is translated
Then the quoted subject is left in Portuguese
  And the surrounding prose is in English
```

### Feature: Identifier scrub across unpushed history
Ref: UC-02

#### Scenario: the tip is clean before the rewrite
```gherkin
Given four task logs containing a project ULID or a developer home path
When the working tree is sanitised and committed
Then a grep of the tip for either class returns no match
```

#### Scenario: the rewrite is surgical
```gherkin
Given the tip's tree hash was recorded before the rewrite
When filter-branch runs the substitution over the 53-commit range
Then the tip's tree hash is byte-identical to the recorded value
```

#### Scenario: every commit in the range is clean
```gherkin
Given the range has been rewritten
When each commit is grepped for both classes with a word boundary
Then no commit in the range returns a match
```

#### Scenario: a deletion is not mistaken for a leak
```gherkin
Given git log -S reports a commit for a scrubbed term
When that commit is grepped with a word boundary
Then a commit that only removes the term reports no match
  And it is not counted as a leak
```

#### Scenario: the repository stays consistent and buildable
```gherkin
Given the history has been rewritten
When git fsck runs
Then it reports no broken links
When the build and the test suite run
Then both pass as they did before the rewrite
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/english-only-task-logs-and-identifier-scrub.md` | Created | this record |
| the 20 files listed above | Modified | translated to English |
| 4 of those 20 | Modified | machine-specific identifiers replaced |

## Trade-offs & Decisions

- **Two commits, not one.** The translation is a large editorial diff and the
  scrub is four lines that a reviewer must be able to see on their own. Combined,
  the security-relevant change disappears inside the translation.
- **The identifiers are described by class in this file, never reproduced.** A
  cleanup log that quotes the strings it removes reintroduces every one of them
  in a new file — the same mistake was caught before commit on 2026-08-30. The
  exact values live in project memory, which is global and never pushed.
- **The history rewrite is accepted even though it invalidates every SHA in the
  range.** The alternative is shipping the dirty blobs; the range is unpushed, so
  nobody else holds these SHAs, and the cost is confined to this clone.

## Technical Debt

- [ ] The absolute-path class is **already in `origin/main`** (two files, six
  commits in its history) from before this range. Rewriting locally does not
  recover that — the SHAs stay reachable on the remote. Cleaning it going forward
  stops the propagation, which is what this task does; the published copies need
  a decision that is not this task's.
- [ ] No automated guard prevents the next recurrence. The user declined a hook on
  2026-08-30, so the pre-push survey stays manual, and four audits have now found
  it re-broken within days each time.

## System Knowledge

- **A survey for Portuguese must use strong markers only.** Including `que`,
  `com`, `para` and `uma` inflates the count by matching English documents that
  quote Portuguese commit subjects or reference Portuguese identifiers — one file
  scored 1768 lines that way with 19 genuinely Portuguese lines in it.
- **Both leaking classes arrive pasted, not typed.** The corpus *name* is at zero
  across the range because the convention is applied when writing prose. What
  slips through is the ULID, which arrives glued to a measurement, and the
  absolute path, which arrives glued to a terminal transcript. The moment to
  sanitise is the paste, not the audit.

## Progress Log

### 2026-09-01

- Confirmed the English-only convention in project memory before starting.
- Surveyed tracked markdown: 605 strong-marker lines across 20 task logs, two of
  which account for 339 of them.
- Opened this log with the full plan before the first edit.
- T3, four delegated files finished: `embedding-crash-loop-due-to-tokenizer-panic.md`
  (24 → 0 marker lines), `make-test-slowness-measured-and-132mb-download-hidden-in-tests.md`
  (18 → 1), `search-index-copy-load-and-late-vector-index.md` (17 → 0) and
  `hub-on-s3-icebug-and-lancedb.md` (19 → 10). Every file kept its exact line count,
  so each rewrite was line-for-line; no absolute path and no ULID was introduced.
- Three deliberate exceptions, all reasoned rather than missed:
  - `hub-on-s3-icebug-and-lancedb.md` keeps 10 marker lines because all 10 sit INSIDE
    ``` fences (verified by fence-state scan): the `local:/export:/consumidor:` and
    `antes:/depois:` chain diagrams, the `"uid" IN ('u2')` quoting measurement, the
    `| era |` / `| era | became |` tables emitted inside ```markdown / ```plaintext,
    and the `resultado = linhas(primeira) + ...` formula. The preservation rule for
    fenced content outranks the translation, so they were left byte-for-byte. The
    inline `2 × primeira` on the prose line below the formula was left for the same
    reason — it names a term defined inside that fence.
  - The quoted heading label `"FASE E"` in the T15 name-correction note was left in
    Portuguese: it records what the section was WRONGLY called, so translating it
    would erase the mislabel the note exists to document.
  - `make-test-slowness-measured-and-132mb-download-hidden-in-tests.md` keeps one
    marker line: the cross-reference to `docs/tasks/busca-devolve-so-arquivos-e-index-nao-reconstroi.md`.
    It is a file path, so it was preserved — but that file no longer exists; a prior
    pass renamed it to `search-returns-only-files-and-index-not-rebuilt.md`. See
    Technical Debt.
- Format placeholders and identifiers left as-is on purpose: `TIPO`, `TIPO_REVERSE`,
  `nodes_<tipo>.parquet`, `<tipo>_<de>_<para>`, `indices_<tabela>.parquet`, `IN [valor]`.

## Technical Debt (added 2026-09-01)

- [ ] `make-test-slowness-measured-and-132mb-download-hidden-in-tests.md` line 122 points
  at `docs/tasks/busca-devolve-so-arquivos-e-index-nao-reconstroi.md`, which was renamed to
  `search-returns-only-files-and-index-not-rebuilt.md`. The reference is broken. Not fixed
  here because the scope was translation and file paths were to be preserved verbatim;
  fixing it is a one-line edit plus a repo-wide check for other references to the old name.
- [ ] The four files carry pre-existing corruption from the earlier machine-translation
  pass — `INLINE_N` / `___INLINE_N__` placeholder tokens, garbled English sentences, and
  in `hub-on-s3-icebug-and-lancedb.md` around lines 583-600 and 1660-1668 a Portuguese
  table followed by two English duplicates of itself plus translator notes left in the
  prose. None of this was touched: it is not Portuguese, and removing it is restructuring,
  not translation. The clean originals are on `backup-pre-squash-20260826-212556`.

---

## CHANGE OF DIRECTION — the premise of T2/T3 is wrong (2026-09-01)

**The remaining Portuguese is not the defect. It is the least of the defects.**

The four parallel translation passes each reported, independently and without being asked,
that the *already-English* parts of these files are corrupted — and measurement confirms it
across the whole documentation tree, not just the 20 surveyed files:

| Damage | Extent |
|---|---|
| `INLINE_N` / `inline N` placeholders where inline code spans used to be | **1,163 occurrences across 42 files** |
| Baked-in translator meta-commentary in the prose | **24 files** |
| Table rows that lost their leading pipe and their first cell | several files |
| Identifiers translated as if they were prose | see below |

The identifier damage is the part that makes these documents actively wrong rather than
merely ugly. Measured examples:

- A SQL fixture declares a table and a column, and the Gherkin step below it asserts on the
  **English translations of those identifiers** — so the scenario now contradicts the fixture
  it is testing.
- A Portuguese word for pattern-matching was rendered as "marriage"; a word for a host
  grammar as "grammar hostess".
- A numbered list lost its `1.` marker while the `2.` two lines below survived.

**Cause.** The pass that rewrote the task logs into English inside this range — the commit
range's own `docs(tasks): rewrite 28 task logs in English` — did the rewriting through a
mechanism that destroyed inline code spans, translated identifiers, and left its own working
notes in the output. Finishing the translation on top of that produces a document that reads
fluently and states things that are false.

**The originals survive, and they are clean.** The pre-squash backup ref holds 165 task logs
with **zero** placeholder tokens. The branch that carried it was deleted earlier today at the
user's request; the objects were still unreferenced-but-present and are now pinned under
`refs/recover/` so that a `gc` cannot take them. This is the reason the identifier scrub
(T6) has **not** been run: `filter-branch` is followed by `reflog expire` + `gc --prune`,
which is precisely what would destroy the only copy.

### Consequence for the plan

T2 and T3 as specified — "translate the remaining Portuguese" — are no longer the right
work. Two options, and the choice is the user's because they differ in scope by an order of
magnitude:

- **A. Finish as asked.** Translate the remaining Portuguese, leave the corruption. Cheap,
  and it leaves ~42 engineering records that are fluent and partly false.
- **B. Restore and retranslate.** Take the clean Portuguese originals from the recovery ref
  for the affected files, then translate them properly — preserving code spans, identifiers
  and table structure. Larger, and it is the only version that ends with correct records.

The identifier scrub (T4–T7) is independent of this choice and blocked only on the recovery
objects being either used or explicitly released.

### Progress Log addition

- Four translation passes ran in parallel over 18 files and completed their brief; their
  edits are in the working tree, uncommitted.
- Each reported the corruption independently. Measured it tree-wide: 1,163 placeholders in
  42 files, meta-commentary in 24.
- Verified the deleted backup objects still exist and pinned them under
  `refs/recover/pre-squash-originals` and `refs/recover/pre-corpus-purge`. Confirmed the
  originals hold zero placeholders.
- **Held T6 deliberately.** Running the history rewrite now would take the reflog and the
  unreferenced objects with it.
- Raised both options with the user rather than choosing.

---

## DECISION: option B — restore the originals and retranslate (2026-09-01)

The user chose B. T2 and T3 are replaced by the plan below; T4–T7 (the identifier scrub) are
unchanged and still queued behind it.

### What the measurement of the damage settled

| | |
|---|---|
| Files to restore and retranslate | **43** |
| Lines in the clean originals | **11,080** |
| Lines in today's corrupted versions | 9,568 |
| **Content destroyed outright by the earlier pass** | **1,512 lines — 13%** |

The worst single case lost **620 lines** of a 3,184-line log; another lost more than half of
itself (89 → 41). This is not formatting damage. It is deletion, and it is the reason B is
the only option that ends with usable records.

### The filename mapping — 23 of the 43 were renamed

Restoring is not copying by path. The earlier pass renamed the files from Portuguese to
English as it rewrote them, so 20 files map by identical path and **23 had to be matched by
content**. Rename detection found only 5 even at a 20% similarity threshold, because the
translation changed too much of the text for git to see the relationship.

The 23 were matched on a language-independent fingerprint — backtick-quoted identifiers,
decimal numbers, and source file names — taking the best overlap. 21 of the 23 matched at
72–100%. One false positive was caught and corrected by hand: a 15-line backlog item scored
100% against a 3,184-line log purely because the small file's few markers are all contained
in the big one. **Lesson: a containment-shaped overlap score cannot be trusted when the
candidates differ in size by two orders of magnitude — check the title, not just the score.**

### The originals predate the corpus purge, so 3 of them carry leaks

Checked before restoring anything, because the recovery ref is from 2026-08-26 and the purge
was 2026-08-30. **3 of the 43 originals carry an identifier from the two scrubbed classes** —
two occurrences of the project ULID in one file, and a developer home path in two others. The
agents restoring those three sanitise as they translate, and the whole set is re-verified
afterwards. The remaining 40 are clean.

### Revised plan

- [x] **T2' — Map current filename to original** — Spec: fingerprint match for the 23 renamed;
  verify each pair by title. Done, one false positive corrected.
- [x] **T3' — Check the originals for leaked identifiers before restoring** — Spec: grep all
  43 for both scrubbed classes. Done: 3 affected, named above.
- [ ] **T4' — Restore and retranslate, 43 files in 6 parallel batches** — Spec: each batch
  reads its originals from `refs/recover/pre-squash-originals`, produces a faithful English
  translation, and carries forward anything the current version has that the original lacks.
  Batches balanced by original line count; the 3,184-line log gets a batch to itself. Done
  when zero placeholder tokens and zero translator meta-commentary remain, and the line count
  of each file is at least that of its original. Constraint: filenames stay as they are —
  English names are correct and renaming again would break inbound links.
- [ ] **T5' — Verify the restoration** — Spec: placeholder count zero; per-file line count no
  longer below the original; no absolute path and no ULID anywhere; knowledge lint no worse.
- [ ] **T4–T7 — the identifier scrub** — unchanged, and still last, because the history
  rewrite ends in a `gc` that would take the recovery objects with it.

### One file has legitimate post-squash content

`data-format-key-value-nodes.md` carries a `2026-08-31` entry that its original cannot have.
It is the only one, verified by diffing the set of post-squash dates in each pair. Its batch
is told to preserve that section.
### `generated-instructions-drift-preamble-and-canonical-query-templates.md` — translated 2026-09-01
Translated in place from clean Portuguese (not a corrupted file — no placeholder tokens).
495 lines before, 495 after; `git numstat` reports 426 insertions / 426 deletions, so heading
levels, table shape, checkbox state (9 checked / 15 open) and blank-line layout are unchanged.
Verified byte-identical: the single Cypher fence, all 798 backticks, and every number.
Sanitised as required: the developer home path became `/home/you/.../mandate.go` and
`home/you/...`, keeping the missing-leading-slash form distinct because that missing slash is
the defect the passage documents. One metavariable was translated — `ENDS WITH '<arquivo>'`
became `'<file>'` — following the precedent already set for `LOAD EXTENSION '<caminho>'`.
The two ULIDs in the file (`01M1DGYP5JG0ZNYSVYHDB1M5RK`, `01KZWFC40QFEP8TDCVBV3MT51Z`) are this
project's own memory ids, not the `<private-corpus>` private-corpus class, so they were left as they are.

---

## T4'/T5' DONE — the restoration, measured (2026-09-01)

Six batches ran in parallel over the 43 files, each translating from
`refs/recover/pre-squash-originals` rather than repairing the corrupted current version.

| | before | after |
|---|---|---|
| `INLINE_N` placeholder tokens | **1,163 across 42 files** | **0** |
| files with translator meta-commentary | **24** | **0** |
| lines in the 43 files | 9,568 | **11,118** |
| lines in the clean originals | 11,080 | — |
| files below their original's length | 43 | **0** |

The **1,512 destroyed lines are back**, and the total now sits slightly above the originals'
because English prose re-wraps at different points. Every batch verified structurally rather
than by eye: fence bodies byte-compared against the original, the multiset of backticked spans
compared, heading sequence, table row counts, checkbox states and numeric tokens all matched.

### What the batches found that the survey had not

Restoring from the originals recovered content, and it also **reversed statements the corrupted
versions had inverted** — these had been sitting in the tree as confident falsehoods:

- A capped-expansion alternative recorded as "became faster" where the original measured it
  **slower** (two separate files).
- A DB2 parenthesis explanation reversed: the corrupted text said the parentheses are "not ANTLR
  grouping, but tokens"; the original says they **are** grouping, which is the whole reason the
  `CREATE TABLE` pattern fails to match.
- A Vue containment measurement changed from `div` to `<slot>`.
- "20 tree-sitter grammars in CGO" rendered as "two".
- A debt note inverted from "does **not** declare `t.Parallel()`" to "so it declares it".
- A decision recorded as "superseded in the third pass" flipped to "made in the third pass" —
  the opposite of what the surrounding text supports.

None of these would have been caught by finishing the translation on top of the damage, which is
the argument for option B stated as evidence rather than as preference.

### Corrections made to the batches' own output

- One batch replaced **this project's own** store ULID with the `<private-corpus>` pseudonym. That
  ULID is published in the committed lockfile and names graphit-code itself; substituting it made
  the sentence false. Restored.
- Nine lines of Portuguese remained inside fenced blocks in the largest log — shell comments,
  pipeline diagrams, and a formula. The byte-for-byte fence rule the batches were given is
  correct for code and wrong for prose that merely lives in a fence, so these were translated by
  hand, keeping the formula's terms consistent with the prose that references them.

### The link rot the renames left behind

The earlier pass renamed 23 files from Portuguese to English and **did not repoint the documents
that referenced them**. 19 dead references were found and fixed, resolved by fingerprint match
and verified by title. One filename was itself a mistranslation — a log referenced
`grammar-override-via-config-and-them.md`, where `and-them` is `e-o-merge` run through the
translator. And one file still carried a Portuguese name with English content; it was renamed and
its inbound references repointed.

## Remaining, and deliberately not invented

- [ ] **6 references point at documents that no longer exist** — deleted backlog items and one
  renamed spec whose target cannot be identified from the reference alone. Listed below rather
  than guessed at, because pointing a reference at the wrong document is worse than leaving it
  visibly dead: `backlog/comentarios-e-nomes-que-ainda-descrevem-os-artefatos-removid.md`,
  `backlog/release-workflow-ainda-passa-default-hub-repo-removido.md`,
  `backlog/the-canonical-bounded-traversal-planner-ignores-its-own-maxh.md`,
  `backlog/verificar-se-macos-e-windows-realmente-precisam-da-icu-no-bu.md`,
  `fix-memory-sync-race-condition.md`, `otimizar-espaco-im-disco-do-store-ast.md`.
- [ ] Two verbatim quotations of the engineer speaking Portuguese are preserved as quotations.
  Translating recorded speech would misattribute words.
- [ ] Portuguese comments inside quoted Go source stay as they are — they are what the source
  file actually says.
