# Ignore defaults above the project root do not apply: in a monorep, the repository .gitignore does not delete anything from a subproject

# Ignore patterns above the project root do not apply

Discovered on 2026-08-22 by removing the git dependency from the ignore mechanism
(`docs/tasks/hub-em-s3-icebug-e-lancedb.md`, section "Four corrections coming from an actual `setup`").
**It's not regression** — it never worked; the correction only made the behavior honest and explicit.
It is stated in test: `TestAnIgnoreFileAboveTheProjectDoesNotApply` on
`internal/ignorer/ignorer_test.go`.

## The defect, and why it's silent

`internal/ignorer/ignorer.go` calculates the *domain* of each pattern in `domainForFile`:

```go
rel, err := filepath.Rel(rootPath, dir)   // rootPath = a raiz do PROJETO
return strings.Split(filepath.ToSlash(rel), "/")
```

If the ignore file is **above** `rootPath`, `rel` turns `../..` and the domain is
`["..", ".."]`. The `gogitignore` sets a standard by comparing the domain against the first
segments of the ** project-related** path, and no actual path starts with `..` — then the
default **never home**.

Before 2026-08-22 the collection boundary was the result of looking for a `.git` going up, so
in a monorep the `.gitignore` of the repository root **was read and was inert**. Nothing was wrong,
nothing logged, and counting indexed files was the only evidence.

## O impacto concreto

A project in `repo/packages/app` (with its own lockfile) within a repository whose
`.gitignore` in the root has `node_modules/`, `dist/`, `target/`:

- these standards **do not** exclude anything from the AST index of the subproject;
- `DefaultAstIgnorePatterns` (`internal/ast/astignore.go`) covers only the output of itself
  tool — the brand directory and the lockfile — so ** there is no safety net **;
- result: `node_modules` integer enters the subproject code graph, with a node `File` per
  file and the grammar of JS/JSON complaining about each.

The knowledge side is less exposed because `DefaultKnowledgeIgnorePatterns` already lists
`node_modules/`, `vendor/`, `dist/`, `build/` explicitly.

## How to play

```bash
mkdir -p /tmp/mono/packages/app/node_modules/left-pad
cd /tmp/mono && git init -q .
printf 'node_modules/\n' > .gitignore
printf '{}' > packages/app/graphit.lock.json
printf 'module.exports=1\n' > packages/app/node_modules/left-pad/index.js
```

A checker built with `rootPath = /tmp/mono/packages/app` responds
`IsIgnored("node_modules/left-pad/index.js", false) == false`.

## What has already been discarded by measurement

- **It is not the collection border.** Collect to repository root (the old behavior)
  does not solve: the file is read and the pattern is inert by the domain. That's exactly what the
  investigation measured. - **It is not the `gogitignore`.** The library is correct; the domain we passed is that it does not
  describes nothing. - **It's not the semantics of negation.** `!` works (verified with actual denials of the
  `.astignore` of this repository, `common/` and `*/driver.go`).

## Possible outputs (choose one, do not stack)

1. **Calculate the domain against the root of the COLLECTION, and marry paths relative to it.** It's the
   true correction. Requires that `IsIgnored` receive (or derive) a path relative to the root of the
   collection and not to the project — which changes the public contract used by `fswatch`, `internal/ast` and
   `internal/knowledge`. Larger, and the only one that gives the behavior anyone expects from
   gitignore. 2. **Rewrite the patterns when uploading from a file above the project**: for each pattern of a
   file in `../..`, discard what cannot reach the project and re-anchor the rest with mastery
   `nil`. cheap and local, but the rewrite has subtle cases (patterns anchored with initial `/`,
   denials, `**`). 3. **Accept and compensate**: add `node_modules/`, `vendor/`, `dist/`, `build/`,
   `target/` to `DefaultAstIgnorePatterns`, aligning with what knowledge already does. It does not correct the
   mechanism, but removes the worst symptom with three lines. It is valid as a palliative even if 1 is
   chosen later.

Preference of those who registered: a **3 now** (cockroach, take your foot off the pump) and a **1 when there is
space** to tamper with the contract.

## How to know it worked

- In the above playback scenario, `IsIgnored("node_modules/left-pad/index.js", false) == true`.
- `TestAnIgnoreFileAboveTheProjectDoesNotApply` needs to be **rewritten or removed** — it
  states the current behavior on purpose, then it will fail when this is corrected, and this
  failure is the expected sign, not a regression.
- Existing tests remain green: `TestProjectGitignorePlusCustomIgnoreBothApply`,
  `TestCustomIgnoreWorksWithNoGitAnywhere`,
  `TestIgnoreFilesAreCollectedUpToTheProjectRootWithoutGit`, `TestANestedIgnoreFileAppliesFromInside`.
