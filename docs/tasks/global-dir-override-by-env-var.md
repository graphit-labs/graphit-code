---
title: The global brand directory becomes overridable by <BRAND>_GLOBAL_DIR
status: done
created: 2026-08-24
updated: 2026-08-24
tags: [brand, storage, configuration, environment, white-label]
---

# The global brand directory becomes overridable by `<BRAND>_GLOBAL_DIR`

## Objective

Make the location of the **global brand directory** overridable by an environment
variable named after the brand — `GRAPHIT_GLOBAL_DIR` on a default build,
`<BRAND>_GLOBAL_DIR` on a white-label one. When the variable is set and non-empty it
**prevails** over the default `~/.<brand>`.

Today the location is derived in one function, `brand.GlobalDir()`
(`internal/brand/brand.go:80`), as `os.UserHomeDir() + "/." + Brand`, with no way to
move it. Everything the framework compiles lives there — code graphs, both wikis,
the memory store, the embedding model, the Hub clones, the extracted runtime — so an
operator whose home is small, network-mounted, or backed up cannot relocate any of it.

### Reasoning

Two facts from the existing code decided the shape of this change:

1. **`brand.GlobalDir()` is already the single resolver for 32 call sites** (verified
   by graph query: `MATCH (caller)-[:CALLS]->(t) WHERE t.name = 'GlobalDir'`), among
   them `store.Root()`, which is what every AST/wiki/memory store hangs off. Putting
   the override inside `GlobalDir()` therefore reaches all of them at once.
2. **It is NOT the only place the global directory gets built.** `os.UserHomeDir()` is
   called directly in six other places that then join `brand.DotDir()` by hand — the
   global config, the frameworks/artifacts dirs, the model cache, the two grammar
   search paths, and the launcher. An override that only touched `GlobalDir()` would
   move the stores and leave the config, the models, the grammars and the extracted
   runtime behind in the real home — a split state that is worse than no override.
   "Prevails" has to mean *all of it*, so those sites get routed through
   `brand.GlobalDir()` as part of this task.

The launcher is the sharpest case: it extracts the core binary to
`~/.<brand>/runtime/<version>` while `brand.RuntimeDir(version)` reads the same path.
If one honoured the variable and the other did not, the binary would be written to one
directory and looked for in another.

There is precedent for the mechanism in this repository: `ai.ModelsDir()`
(`internal/ai/model_manager.go:99`) already reads `brand.EnvVar("MODEL_CACHE")` as a
first-wins override, for the same operator reason. This task follows that shape and
subsumes it — `GRAPHIT_MODEL_CACHE` keeps its meaning as the narrower knob.

### Justification — why an environment variable and not a config key

`graphit config` cannot express this. The global config file **lives inside the global
directory** (`internal/config/config.go:AppDir()` → `<global>/config.json`), so a key
that names the directory could only be read after the directory had already been
located. The environment is the only layer that resolves before the filesystem does.

This also means the variable deliberately does **not** flow through
`config.ResolveConfig`, whose env layer already maps `key` → `GRAPHIT_<KEY>`. It is
read directly in `internal/brand`, which `internal/config` imports — the reverse would
be an import cycle.

### What this task is NOT

It is **not** the mechanism that isolates the test suite. That remains `HOME`, decided
in `internal/brand/testhome.go` and unchanged here: several things a test can write
into the operator's home never route through `GlobalDir()` at all (git's own
`~/.gitconfig`, `~/Library/LaunchAgents`, `~` expansion of user-supplied paths), and a
subprocess spawned by a test is not a test binary, so only an inherited environment
reaches it. The new variable is an *operator* knob.

The interaction runs the other way, and it is a hazard this task must close: an
operator who exports `GRAPHIT_GLOBAL_DIR` in their shell would have the test suite
inherit it, and every test would write into their real override directory — precisely
the pollution the ephemeral home exists to prevent. So `testhome.go` unsets it.

## Plan & Task Breakdown

- [x] **T1 — `GlobalDir()` honours the variable** — Spec: `internal/brand/brand.go`.
  `GlobalDir()` returns the trimmed value of `os.Getenv(EnvVar("GLOBAL_DIR"))` when it
  is non-empty, before consulting `os.UserHomeDir()`. Done when the override wins and
  an unset/blank variable changes nothing. Constraint: a **relative** value must not
  drift — the daemon chdirs into `GlobalDir()` (`internal/daemon/daemon.go:438`), so
  resolving a relative value against the live cwd would yield a different directory on
  every subsequent call. Resolve it against the working directory captured at process
  start.
- [x] **T2 — the test home unsets it** — Spec: `internal/brand/testhome.go`. The `init()`
  that points `HOME` at a throwaway directory also removes `<BRAND>_GLOBAL_DIR` from
  the environment. Done when a test binary launched with the variable exported still
  resolves its global directory under the isolated home. Constraint: it must be
  `Unsetenv`, not a rewrite to the isolated home — subprocesses inherit either way, and
  unsetting keeps `HOME` the single mechanism the recorded decision names.
- [x] **T3 — every hand-built global path routes through `GlobalDir()`** — Spec:
  `internal/config/config.go:AppDir()`, `internal/paths/paths.go:buildPaths()`,
  `internal/ai/model_manager.go:ModelsDir()`, `internal/ast/antlr_adapter.go:antlrGrammarSearchDirs()`,
  `internal/ast/treesitter_dynload.go:searchDirs()`, `cmd/launcher/main.go:main()`.
  Done when none of them composes `home + DotDir()` itself. Constraint: `ModelsDir()`
  keeps `GRAPHIT_MODEL_CACHE` as the *first* check; the two AST grammar sites currently
  hardcode the literal `".graphit"` instead of `brand.DotDir()`, which this also fixes.
- [x] **T4 — tests** — Spec: `internal/brand/global_dir_env_test.go` plus an assertion in
  the existing `internal/brand/testhome_test.go`. Done when the override, the blank
  value, the relative value and the test-home unset are each covered.
- [x] **T5 — documentation** — Spec: `docs/architecture/storage_layout.md` (where the
  root comes from), `docs/specs/config_module.md` (why it is not a config key),
  `docs/guides/private_brand_customization.md` (the name follows the brand). Done when
  a reader can find the variable from any of the three entry points.

## Implementation Details

**`internal/brand/brand.go`** — `GlobalDir()` now checks
`strings.TrimSpace(os.Getenv(EnvVar("GLOBAL_DIR")))` before `os.UserHomeDir()`. An
absolute value is returned `filepath.Clean`ed; a relative one is joined onto
`processStartDir`, a new package variable initialised from `os.Getwd()`. Package
variables initialise before `init()` functions, so it holds the working directory the
process started in, ahead of any `os.Chdir`. Nothing is created or stated — the function
stays a pure resolver.

**`internal/brand/testhome.go`** — the isolating `init()` adds
`os.Unsetenv(EnvVar("GLOBAL_DIR"))` next to the `HOME`/`USERPROFILE`/`XDG_CONFIG_HOME`
assignments. The doc comment above it was corrected in the same pass: it listed the
global config, the model cache and the launcher as sites that *do not* route through
`GlobalDir()`, which stopped being true in T3. It now names the three that genuinely do
not — `~/.gitconfig`, `~/Library/LaunchAgents`, and `~` expansion of user-supplied paths
— which is what still makes `HOME` the right isolation mechanism.

**The six hand-built sites**, all now `brand.GlobalDir()`:

| Site | Before | After |
|---|---|---|
| `config.AppDir()` | `UserHomeDir() + DotDir()`, error from `UserHomeDir` | `GlobalDir()`, error when it is empty |
| `paths.buildPaths()` | `home` joined three times | one `globalDir` local |
| `ai.ModelsDir()` | `UserHomeDir() + DotDir() + models` | `GlobalDir() + models`; `GRAPHIT_MODEL_CACHE` still checked first |
| `ast.antlrGrammarSearchDirs()` | literal `".graphit"` | `brand.DotDir()` for the project dir, `GlobalDir()` for the global one |
| `(*DynGrammarLoader).searchDirs()` | literal `".graphit"` | same |
| `launcher main()` | `UserHomeDir() + DotDir()` | `GlobalDir()`, with the same "Error getting home directory" exit path |

The two AST sites were also the only remaining places that hardcoded the literal
`".graphit"` instead of deriving it from `Brand`, so a white-label build had been looking
for user grammars in the wrong directory. Fixed as a side effect.

**Verified afterwards** that no production code composes the global directory by hand any
more: the only remaining `os.UserHomeDir()` calls outside tests are `GlobalDir()` itself,
`ide.expandHome` (expanding `~` in a path the *user* supplied) and
`daemon.launchAgentsDir` (`~/Library/LaunchAgents`, macOS's directory, not ours). Both of
the latter are correct as they stand.

## Use Cases

### UC-01: Operator relocates the global directory

- **Actor**: operator running any `graphit` command, the daemon, or the MCP server.
- **Preconditions**: `<BRAND>_GLOBAL_DIR` is exported in the environment the process
  inherits; the path is writable or creatable.
- **Main Flow**:
  1. Any code needing the global root calls `brand.GlobalDir()`.
  2. `GlobalDir()` reads `os.Getenv(brand.EnvVar("GLOBAL_DIR"))` and trims it.
  3. The value is non-empty, so it is returned — absolute values verbatim (cleaned),
     relative values joined onto the working directory captured at process start.
  4. `os.UserHomeDir()` is never consulted.
  5. Every derived path — `store.Root()`, `config.AppDir()`, `ai.ModelsDir()`,
     `brand.RuntimeDir()`, `paths.buildPaths()`, the grammar search dirs and the
     launcher's runtime dir — resolves under the new root.
- **Alternative Flows**:
  - Variable set to whitespace only → treated as unset; the home-based default applies.
  - `GRAPHIT_MODEL_CACHE` also set → the model cache follows *that*, not the global dir;
    it is the narrower, more specific override and stays first.
- **Error Scenarios**:
  - The path cannot be created or written → the failure surfaces at the first store
    operation with its own error, as it would for the home-based default. `GlobalDir()`
    itself does not stat or create anything.
  - `os.UserHomeDir()` fails **and** the variable is unset → `GlobalDir()` returns `""`,
    the pre-existing behaviour, and callers degrade as they already do.
- **Postconditions**: every compiled artifact of the framework is read from and written
  under the overridden directory, and the real `~/.<brand>` is untouched.
- **Affected Files**: `internal/brand/brand.go`, `internal/config/config.go`,
  `internal/paths/paths.go`, `internal/ai/model_manager.go`,
  `internal/ast/antlr_adapter.go`, `internal/ast/treesitter_dynload.go`,
  `cmd/launcher/main.go`.

### UC-02: The test suite ignores an operator's override

- **Actor**: the Go test binary, at package initialisation.
- **Preconditions**: the binary is a test binary (`testing.Testing()` is true); the
  operator may or may not have `<BRAND>_GLOBAL_DIR` exported.
- **Main Flow**:
  1. `internal/brand`'s `init()` creates a throwaway home under
     `<tmp>/<brand>-test-homes/`.
  2. It points `HOME`, `USERPROFILE` and `XDG_CONFIG_HOME` at it.
  3. It calls `os.Unsetenv(EnvVar("GLOBAL_DIR"))`.
  4. `brand.GlobalDir()` finds no override and derives the root from the isolated
     `HOME`.
- **Alternative Flows**:
  - A test that deliberately wants the override sets it with `t.Setenv`, which runs
    after `init()` and is restored on cleanup.
- **Error Scenarios**:
  - The throwaway home cannot be created → `init()` panics, as it already does; falling
    back to the real home is the bug this exists to prevent.
- **Postconditions**: no test, and no subprocess a test spawns, writes into the
  operator's global directory — overridden or not.
- **Affected Files**: `internal/brand/testhome.go`.

### UC-03: Launcher and core agree on where the runtime is

- **Actor**: the `graphit` launcher binary.
- **Preconditions**: `<BRAND>_GLOBAL_DIR` may be set; a core binary needs extracting or
  is already extracted.
- **Main Flow**:
  1. The launcher resolves its application directory with `brand.GlobalDir()`.
  2. It composes `<global>/runtime/<version>` and extracts the core binary there.
  3. It execs the core with the current environment, so the variable is inherited.
  4. The core resolves `brand.RuntimeDir(version)` to the same path.
- **Alternative Flows**:
  - Variable unset → both sides resolve `~/.<brand>/runtime/<version>`, unchanged.
- **Error Scenarios**:
  - `GlobalDir()` returns `""` (no home, no override) → the launcher reports the home
    error and exits, as before.
- **Postconditions**: the binary is written to and executed from one directory.
- **Affected Files**: `cmd/launcher/main.go`.

## Test Cases & Acceptance Criteria

### Feature: `<BRAND>_GLOBAL_DIR` overrides the global brand directory
Ref: UC-01

#### Scenario: The variable wins over the home directory
```gherkin
Given HOME is set to a temporary directory "/tmp/home-a"
  And GRAPHIT_GLOBAL_DIR is set to "/tmp/elsewhere/store"
When brand.GlobalDir() is called
Then it returns "/tmp/elsewhere/store"
  And the value does not contain "/tmp/home-a"
```

#### Scenario: An unset variable leaves the default untouched
```gherkin
Given HOME is set to a temporary directory "/tmp/home-a"
  And GRAPHIT_GLOBAL_DIR is not set
When brand.GlobalDir() is called
Then it returns "/tmp/home-a/.graphit"
```

#### Scenario Outline: A blank value is treated as unset
```gherkin
Given HOME is set to a temporary directory "/tmp/home-a"
  And GRAPHIT_GLOBAL_DIR is set to "<value>"
When brand.GlobalDir() is called
Then it returns "/tmp/home-a/.graphit"

Examples:
  | value |
  |       |
  | "   " |
  | "\t"  |
```

#### Scenario: The variable name follows a white-label brand
```gherkin
Given brand.Brand is reassigned to "acme"
  And ACME_GLOBAL_DIR is set to "/tmp/acme-store"
When brand.GlobalDir() is called
Then it returns "/tmp/acme-store"
  And GRAPHIT_GLOBAL_DIR is ignored even when also set
```

#### Scenario: A relative value does not drift when the process changes directory
```gherkin
Given GRAPHIT_GLOBAL_DIR is set to a relative path "relative-store"
When brand.GlobalDir() is called and its result recorded
  And the process changes its working directory to that result
  And brand.GlobalDir() is called again
Then both calls return the same absolute path
```

#### Scenario: Every derived path follows the override
```gherkin
Given GRAPHIT_GLOBAL_DIR is set to "/tmp/elsewhere/store"
When the global root, the rules dir, the Hub rules dir and the versioned runtime dir are resolved
Then each of them is inside "/tmp/elsewhere/store"
```

### Feature: The test suite is immune to an exported override
Ref: UC-02

#### Scenario: A test binary ignores an operator's override
```gherkin
Given the operator exported GRAPHIT_GLOBAL_DIR before running the suite
When the test binary initialises internal/brand
Then GRAPHIT_GLOBAL_DIR is unset in the test process
  And brand.GlobalDir() resolves under the isolated test home
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/brand/brand.go` | Modified | `GlobalDir()` reads the override first; process-start cwd captured for relative values |
| `internal/brand/testhome.go` | Modified | Unset the override so an operator's shell cannot leak into the suite |
| `internal/brand/global_dir_env_test.go` | Created | Override, blank value, white-label name, relative value, derived paths |
| `internal/brand/testhome_test.go` | Modified | Assert the override is unset inside a test binary |
| `internal/config/config.go` | Modified | `AppDir()` routes through `brand.GlobalDir()` |
| `internal/paths/paths.go` | Modified | `buildPaths()` routes through `brand.GlobalDir()` |
| `internal/ai/model_manager.go` | Modified | `ModelsDir()` falls back to `brand.GlobalDir()`; `GRAPHIT_MODEL_CACHE` still first |
| `internal/ast/antlr_adapter.go` | Modified | Grammar search dir from `brand.GlobalDir()`; drops the hardcoded `".graphit"` |
| `internal/ast/treesitter_dynload.go` | Modified | Same, for tree-sitter grammars |
| `cmd/launcher/main.go` | Modified | Runtime extraction directory from `brand.GlobalDir()` |
| `docs/architecture/storage_layout.md` | Modified | Where the root comes from, and how to move it |
| `docs/specs/config_module.md` | Modified | Why this is not a config key |
| `docs/guides/private_brand_customization.md` | Modified | The variable name follows the brand |

## Trade-offs & Decisions

- **Override inside `GlobalDir()`, plus routing the strays through it** — rather than a
  new `brand.GlobalDirOverride()` that each site consults. One resolver is what makes
  "prevails" checkable; the recorded reason a `GlobalDir()` guard was rejected for
  *test isolation* does not apply here, because that argument was about sites the guard
  could not reach, and this task moves those sites onto it.
- **Relative values resolved against the process-start cwd** — not against `$HOME`, and
  not left to `filepath.Abs` at call time. `filepath.Abs` is the tempting one and it is
  wrong: the daemon chdirs into the global directory, so the second call would resolve
  the same relative value one level deeper. Capturing the cwd in a package variable
  costs one line and has no ordering hazard, since package variables initialise before
  `init()`.
- **Blank is treated as unset** — an exported-but-empty variable is far more often an
  unset-with-a-typo than a request to use the process's working directory.
- **`GlobalDir()` still does not create the directory** — it is a pure resolver, and the
  callers that need the directory to exist already `MkdirAll` it (`config.AppDir()` does,
  `store` does). Creating it here would make a read-only path query a side effect.

## Technical Debt

- [ ] `internal/hub/adapters/ide/base.go:expandHome` and
  `internal/daemon/scheduler_darwin.go:launchAgentsDir` still call `os.UserHomeDir()`
  directly. Verified deliberate, not missed — one expands `~` in a *user-supplied* path,
  the other names `~/Library/LaunchAgents`, which is macOS's directory and not ours — but
  they are the two sites a future reader will have to re-classify when auditing this again.
- [ ] The Makefile sweeps `$(BRAND)-test-homes` while the test binary always uses the
  compiled-in default `graphit` — a pre-existing white-label mismatch recorded in
  memory, untouched here.

## System Knowledge

- `brand.Brand` is a **mutable global**, so anything derived from it is only stable at
  the moment it is read. That is why `EnvVar("GLOBAL_DIR")` is evaluated per call rather
  than cached in a package variable: a white-label test that reassigns `Brand` must see
  the new variable name immediately.
- The global config file lives inside the global directory, which is the structural
  reason this override cannot be a config key. Any future "where do we keep X" knob has
  the same shape.
- The launcher is a separate binary from the core and resolves the runtime directory
  independently; they agree only because both go through `internal/brand`.
- A first run against a fresh global directory **downloads the embedding model** — the
  smoke test's throwaway root reached 117 MB within seconds. Worth knowing before pointing
  the variable at a volume where that is unwelcome, and worth saying in any migration
  advice: an empty override is not a cheap experiment.
- `daemon/daemon.pid` holds the pid **and** a start timestamp on the following line. A
  naive read that strips whitespace concatenates the two into a number that matches no
  process — parse the first line only.

## Progress Log

### 2026-08-24
- Opened the task. Read `internal/brand/{brand.go,testhome.go}`, the 32 `GlobalDir()`
  callers and the 13 direct `os.UserHomeDir()` callers from the code graph, plus the
  recorded decision behind the ephemeral test home.
- Scoped the change to one resolver plus six hand-built sites, and wrote the plan above.
- T1 landed: `GlobalDir()` honours `<BRAND>_GLOBAL_DIR`, with the process-start cwd
  captured for relative values.
- T2 landed: the test-home `init()` unsets the variable, and its stale rationale comment
  was corrected in the same pass — T3 invalidated three of the examples it named.
- T3 landed: `config.AppDir()`, `paths.buildPaths()`, `ai.ModelsDir()`, both AST grammar
  search paths and the launcher all resolve through `brand.GlobalDir()`. Confirmed by
  search that no production code composes `home + DotDir()` any more.
- T4 landed: `internal/brand/global_dir_env_test.go` (override wins, unset default, blank
  values, white-label name, relative value across a chdir, derived paths) plus an
  assertion in `TestTestHomeIsIsolated` that the variable is unset in a test binary.
  `go test ./internal/brand/ ./internal/config/ ./internal/paths/ ./internal/ai/` — all ok.
- T5 landed: `docs/architecture/storage_layout.md` gained a "Where the root is" section,
  `docs/specs/config_module.md` gained "The one setting that is not a config key",
  `docs/guides/private_brand_customization.md` gained the white-label naming rule, and
  `docs/guides/troubleshooting.md` opens with a note that every `~/.graphit` in it should
  be read as the override when one is set.
- **Verified — full suite:** `make test` → 47 packages `ok`, 0 FAIL, exit 0. Both passes
  (race-enabled project code, and the generated parsers without race).
- **Verified — real binary, real filesystem.** Built `.build/graphit-local` and ran
  `GRAPHIT_GLOBAL_DIR=<tmp>/alt-global graphit-local config list` from a directory that is
  not a project. Result: `daemon/{daemon.log,daemon.pid,mcp.key,mcp.port,.spawn.lock}`,
  `logs/graphit.log`, `memory-raw/` and `models/coderankembed/` were all created **under
  the override**, and a `find ~/.graphit -maxdepth 1` diff taken before and after was
  **empty** — the real global directory was not touched at all.
- **Verified — inheritance.** The daemon that run spawned carried
  `GRAPHIT_GLOBAL_DIR=<tmp>/alt-global` in `/proc/<pid>/environ`, which is the mechanism
  UC-03 depends on: the child resolves the same root because it inherits the variable, not
  because anything passes it explicitly.
- Cleaned up after the smoke test: SIGTERM to that daemon (pid confirmed against its own
  cmdline and environ first) and removed the temporary global directory. The operator's
  own `graphit-core daemon` was left running.
