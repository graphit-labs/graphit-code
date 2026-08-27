---
title: One registry for context membership, and a versioned knowledge store
status: done
created: 2026-08-14
updated: 2026-08-14
tags: [store, lockfile, hub, knowledge, ast, refactor]
---

# One registry for context membership, and a versioned knowledge store

## Objective

Context membership was recorded in two places, and which place depended on the artifact's
origin in a way that followed no principle. Unify it: `graphit.lock.json` is the only
record, `.graphit/contexts.json` is deleted, and a Hub knowledge artifact becomes
version-keyed like its AST counterpart.

The user's directive was clear: "Everything needs to go into the lock, it shouldn't have this"
contexts.json mais"*, with the added constraint that a local install or link records its
path **relative to the project**.

## Implementation Details

### What the split actually was

Measured before changing anything, because the explanation that had been written down was
a rationalisation:

| | lockfile entry | resolved from |
|---|---|---|
| `ast install` local | no | `contexts.json` |
| `hub link` (ast) | yes | `contexts.json` (`origin=link` was skipped by the lockfile resolver) |
| `hub install` (ast) | yes | **the lockfile**, via `ast.HubContextsForProject` |
| local knowledge import | no | `contexts.json` |
| `hub install` (knowledge) | yes | `contexts.json` |

`hub.Install` writes `lf.Artifacts[artType][realID]` with a `Version` for **every**
artifact type — generic code after the type switch. So the split was never about which
record held the version. It followed the store path:

- `<global>/ast/hub/<context-id>/<version>/` contains the version, so resolving an AST
  context requires reading it, and the lockfile is where it is.
- `<global>/wiki/knowledge/context/<name>/` had no version in the path, so knowledge
  resolution never needed the lockfile and never learned to read it.

**The defect that hid behind it:** a Hub knowledge context was not versioned at all. Two
projects pinned to different versions shared one directory, `wiki.ResetDir` wiped it on
each install, and the last one silently won — while both lockfiles recorded a version
nothing enforced.

### The format moved to a leaf package

The dependency argument that had justified the split was that `hub` owns the lockfile
format and `hub` imports `ast`, `knowledge` and `memory`. It was already being bypassed:
`HubContextsForProject` parsed the lockfile with an anonymous struct, the same trick
`store.ProjectID` uses.

So `internal/projectlock` now owns the format — a leaf that imports only `internal/git`
and `ulid` (`internal/git` imports only `brand`, so there is no cycle). Moved out of
`hub`: `ArtifactType` and its ten constants, `ProjectIdentity`, `ArtifactMeta` (was
`LockfileArtifactMeta`), `Lockfile`, `Load`, `Save`, `resolveProjectIdentity`.

`internal/hub/lockfile.go` became **type aliases**, not wrappers:

```go
type (
	ArtifactType         = projectlock.ArtifactType
	ProjectIdentity      = projectlock.ProjectIdentity
	Lockfile             = projectlock.Lockfile
	LockfileArtifactMeta = projectlock.ArtifactMeta
)
```

An alias makes the types identical, so `map[hub.ArtifactType]` and
`map[projectlock.ArtifactType]` are the same type and **all 77 references across 20 files
kept compiling untouched**.

> **Naming hazard, recorded because it cost a mistake:** `internal/lockfile` already
> exists and is *advisory file locking* (`Acquire`, `TryAcquire`, `Release`). Creating the
> format package under that name overwrote it; it was restored from git and the new
> package is `internal/projectlock`. The two are unrelated and easy to confuse.

### One path in the lockfile, always relative

`ArtifactMeta` gained exactly one field:

```go
// Stored RELATIVE TO THE PROJECT, slash-separated.
SourcePath string `json:"source_path,omitempty"`
```

Relative because the lockfile is committed and shared — an absolute path is only true on
the machine that wrote it, and a teammate's clone would get a record pointing into
somebody else's home directory. `projectlock.RelSourcePath` and `SourceDir` convert in
each direction, falling back to absolute in the one case `filepath.Rel` refuses
(different Windows volumes).

**There is no `DBPath` field, and that is the more consequential half.** The old
`contexts.json` stored a store path for a link, and both values were derivable from the
source directory — `store.ASTProjectDBPath(absSource)` and
`store.KnowledgeProjectDir(absSource)`. Storing them was a second copy of one fact, and a
copy that went stale: the moment the sibling ran `init` and re-keyed its store, the frozen
path pointed at the old location. Now the lockfile records the sibling's *directory* and
every store location is derived on read.

Also added: `VersionLocal = "local"`, the `Origin*` constants, `IsLocal()`.

### The registry became a view

`internal/store/registry.go` was rewritten over `projectlock`. `ContextRecord` is now a
view over a lockfile artifact entry rather than a stored shape of its own:

```go
type ContextRecord struct {
	Name       string // ProjectID for a Hub artifact, the directory name otherwise
	ArtifactID string // the lockfile key
	Version    string
	Origin     string
	ProjectID  string
	SourcePath string // ABSOLUTE on read, relative on disk
}
```

`ImportedAt` and `DBPath` are gone. The public signatures — `ListContexts`,
`ContextNames`, `LookupContext`, `HasContext`, `AddContext`, `RemoveContext` — were kept,
so the sixteen production call sites barely changed.

New `store.ContextNameFor(artifactID, projectID)`: a Hub artifact published by a project
is named after that project, because that is what keys its store. The rule used to be
`ast.HubContextID`; it belongs to the package that owns naming.

`LookupContext` accepts the name, its sanitised form, **or** the Hub artifact ID, because
callers echo back whichever they were shown.

`AddContext` is now only for local imports and links. `hub install` already writes its own
entry with the version, hash and members it resolved, and calling `AddContext` afterwards
would overwrite that with less.

### Paths moved to where paths live

New `internal/store/contextpaths.go`:

```go
func ASTContextDBPathIn(projectDir, name string) string
func KnowledgeContextDirIn(projectDir, name string) string
```

Hub → the version-keyed store; link → the sibling's own store, derived; local import →
its own context directory. `ast.ContextDBPathIn` and `knowledge.WikiDirForContextIn` are
now one line each. Scattering this logic across `ast` and `knowledge` is how the two came
to disagree in the first place.

New `store.KnowledgeHubDir(contextID, version)` and `KnowledgeHubRoot`, mirroring
`ASTHubDir`. `hub.Install` of `TypeKnowledge` writes there and no longer registers
anything.

### The merge collapsed

`ast.ListImportedContextsIn` read two sources and needed a rule for which won a name
clash. It now reads `store.ListContexts` alone — a clash cannot arise. That left four
things dead in `internal/ast/hubstore.go`, all removed: `HubContextRef`,
`HubContextsForProject`, `LookupHubContext`, `HubStoreExists`.

### A bug this refactor introduced, and its fix

`prep.writeLockfile` built a fresh `Lockfile` and saved it. That was harmless while
membership lived in a separate file and is not harmless now: it would erase every artifact
entry. It merges instead — loads, replaces `Project` and `IDEs`, preserves `Artifacts`.
Production ordering happened to be safe, which is exactly the kind of accident worth not
depending on.

## Use Cases

### UC-01: Install a knowledge artifact from the Hub
- **Actor**: a developer running `hub install <id>`
- **Preconditions**: the artifact is published with a compiled wiki
- **Main Flow**:
  1. `store.ContextNameFor(realID, entry.ProjectID)` gives the context name
  2. `wiki.ResetDir(store.KnowledgeHubDir(name, resolvedVersion))` — version-keyed
  3. `SafeCopyDir`, then `wiki.BuildDBFromCache`
  4. The generic lockfile write after the type switch records the claim
- **Alternative Flows**: installing a different version writes a **different** directory;
  both remain on disk and each project reads the one its lockfile pins
- **Error Scenarios**: a build failure surfaces and the install fails
- **Postconditions**: nothing is written to `.graphit/`
- **Affected Files**: `internal/hub/service.go`, `internal/store/store.go`

### UC-02: Resolve a context by name
- **Actor**: any knowledge or AST read
- **Preconditions**: the project's lockfile claims the context
- **Main Flow**:
  1. `store.LookupContext` finds the entry by name, sanitised name or artifact ID
  2. `ASTContextDBPathIn` / `KnowledgeContextDirIn` derive the location from the origin
- **Alternative Flows**: an unclaimed name falls back to the one store that name could
  mean — resolution is not authorisation
- **Error Scenarios**: a claimed context whose store was never built is omitted from
  listings, because offering it would offer a context whose every query fails
- **Postconditions**: no path was read from disk records
- **Affected Files**: `internal/store/contextpaths.go`, `internal/ast/config.go`, `internal/knowledge/paths.go`

### UC-03: Import a local repository as a context
- **Actor**: a developer running `ast install <path> --context <name>`
- **Main Flow**:
  1. The store directory is created
  2. `store.AddContext` writes a lockfile entry with `version: local`, `origin: local`
     and `source_path` relative to the project
- **Error Scenarios**: no project directory, or an empty name, is refused
- **Postconditions**: the entry survives a clone if the source sits at the same relative
  place
- **Affected Files**: `internal/ast/config.go`, `internal/store/registry.go`

### UC-04: Link a sibling project's graph or wiki
- **Actor**: a developer running `hub link`
- **Main Flow**:
  1. The sibling's store is checked for existence
  2. The **sibling's directory** is recorded, relative, with `origin: link`
  3. Reads derive the sibling's store from that directory every time
- **Alternative Flows**: the sibling reindexes or runs `init` and re-keys — the link
  follows, where a stored store path would have frozen
- **Affected Files**: `internal/hub/service.go`, `internal/ast/config.go`

### UC-05: Drop a claim
- **Actor**: `knowledge remove --context`, `ast remove --context`, `hub uninstall`
- **Main Flow**: the lockfile entry is deleted, matched by artifact ID, sanitised name or
  context name
- **Postconditions**: the store is untouched — it is shared, and another project may
  claim the same one
- **Affected Files**: `internal/store/registry.go`

## Test Cases & Acceptance Criteria

### Feature: One registry
Ref: UC-01, UC-03

#### Scenario: Membership is written to the lockfile and nowhere else
```gherkin
Given a project with no contexts
When a context is added
Then the lockfile names it
  And no "contexts.json" exists anywhere in the project
```

#### Scenario: A re-import overwrites rather than duplicating
```gherkin
Given a context imported from one directory
When the same name is imported from a different directory
Then exactly one entry exists
  And it records the second directory
```

### Feature: Paths in the lockfile are relative
Ref: UC-03, UC-04

#### Scenario: A link is recorded relative to the project
```gherkin
Given a project at "<base>/app" and a sibling at "<base>/lib"
When the sibling is linked
Then the lockfile bytes contain a source path of "../lib"
  And they contain no absolute path
```

#### Scenario Outline: A recorded path round-trips
```gherkin
Given a project directory and a source described as "<source>"
When the path is recorded and read back
Then the absolute directory is recovered

Examples:
  | source                        |
  | inside the project            |
  | a sibling outside the project |
  | an absolute cross-volume path |
```

### Feature: A link is derived, never frozen
Ref: UC-04

#### Scenario: The store comes from the sibling's directory
```gherkin
Given a linked sibling
When the context's graph path is resolved
Then it is the sibling's own project store, derived from the recorded directory
```

### Feature: A Hub context is version-keyed
Ref: UC-01, UC-02

#### Scenario: A knowledge artifact resolves to a versioned directory
```gherkin
Given a lockfile claiming a knowledge artifact at version "1.2.0"
When its wiki directory is resolved
Then it is the hub directory for that context and that version
```

#### Scenario: Two versions do not share a directory
```gherkin
Given the same artifact resolved at "1.0.0" and at "2.0.0"
Then the two directories differ
```

#### Scenario: A Hub context is named after its publishing project
```gherkin
Given a lockfile entry keyed "acme-graph" with project id "01ACME"
When contexts are listed
Then the context is named "01ACME"
  And looking it up by "acme-graph" also finds it
```

### Feature: An empty project claims nothing
Ref: UC-02

#### Scenario: No lockfile is not an error
```gherkin
Given a directory with no lockfile
When contexts are listed
Then the result is empty
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/projectlock/projectlock.go` | Created | the lockfile format, as a leaf |
| `internal/projectlock/projectlock_test.go` | Created | relative-path round-trip, `IsLocal` |
| `internal/hub/lockfile.go` | Modified (rewritten) | type aliases so 77 references keep working |
| `internal/hub/registry.go` | Modified | `ArtifactType` and its constants removed |
| `internal/hub/service.go` | Modified | versioned knowledge dir; link records the sibling directory; no second registry write |
| `internal/store/registry.go` | Modified (rewritten) | membership from the lockfile |
| `internal/store/contextpaths.go` | Created | `ASTContextDBPathIn`, `KnowledgeContextDirIn` |
| `internal/store/store.go` | Modified | `KnowledgeHubDir`, `KnowledgeHubRoot` |
| `internal/ast/config.go` | Modified | one source; `LinkImportedContext` takes a directory |
| `internal/ast/hubstore.go` | Modified | four dead symbols removed |
| `internal/knowledge/paths.go` | Modified | delegates to the store |
| `internal/livesearch/prep/workspace.go` | Modified | `writeLockfile` merges instead of replacing |
| `internal/store/registry_test.go` | Modified | asserts the second registry is absent |
| `internal/ast/hubstore_test.go` | Modified | link derivation, relative recording, properties of the removed symbols |
| `internal/hub/lockfile_test.go` | Modified | identity test moved to `projectlock` |
| `docs/architecture/storage_layout.md` | Modified | one home for membership; versioned knowledge path |
| `docs/guides/retrieval_architecture.md` | Modified | no `contexts.json` |
| `docs/specs/hub_collaboration.md` | Modified | a link records a directory, not a `db_path` |
| `docs/tasks/centralize-stores-in-global-dir.md` | Modified | Progress Log: the trade-off is reversed |
| `docs/tasks/one-registry-for-context-membership.md` | Created | this log |

## Trade-offs & Decisions

**Type aliases instead of updating 77 references.** The alternative was a mechanical
rename across 20 files, which is more diff for the same result and more chance of a
mistake in files this change has no reason to touch. The cost is one indirection: `hub`
now re-exports a format it does not own, which the doc comment says plainly.

**A new package name rather than reusing `internal/lockfile`.** Not a choice so much as a
correction — that name was taken by advisory file locking, and I overwrote it before
noticing. `internal/projectlock` is unambiguous.

**No `DBPath`, at the cost of one extra derivation per read.** Deriving means a lookup and
a path join where a stored value would have been a map read. That is nothing measurable,
and it removes an entire class of staleness.

**`AddContext` no longer serves Hub installs.** It could have been made to merge into the
existing entry, but every field it would write is one `hub install` already resolved
better. Narrowing it says which caller it is for.

**Knowledge Hub stores are keyed by version with no migration.** An existing unversioned
`wiki/knowledge/context/<name>/` from a previous Hub install is simply not found any more,
and the artifact must be reinstalled. Acceptable in development; a release would need a
one-time move.

## Technical Debt

- [ ] **No migration for either change.** A project with a `contexts.json` loses its local
  imports and links silently — the file is not read, not warned about, not deleted. A
  release needs a one-time import of it into the lockfile, and the same for unversioned
  knowledge stores.
- [ ] **`hub` re-exports a format it does not own.** The aliases are a compatibility shim.
  New code should use `projectlock` directly, and the shim should eventually go, which
  means touching those 77 references after all.
- [ ] **`store.AddContext` can write a lockfile for a project that has none**, creating an
  identity as a side effect of an import. That was also true of `contexts.json`, but the
  lockfile is a more meaningful thing to bring into existence by accident.
- [ ] **Nothing verifies that a claimed version's store exists** at claim time. A lockfile
  pinning a version whose directory was never built resolves to a path that is not there,
  and only the listing filters it out.
- [ ] **A transient `SIGSEGV` was observed once** in `go test ./internal/ast/`, not
  reproducible in two subsequent runs nor in the full suite. The package uses CGO and has
  a history of flakes; recorded here rather than dismissed.

## System Knowledge

- **`internal/lockfile` is advisory file locking, `internal/projectlock` is the
  `graphit.lock.json` format.** Confusing them is easy and I did it once, destructively.
- **`hub.Install` writes a lockfile artifact entry for every type**, in generic code after
  the type switch. Any per-type registration in the switch is either redundant with it or
  fighting it.
- **A type alias, not a defined type, is what makes a map key interchangeable.**
  `type ArtifactType = projectlock.ArtifactType` keeps `map[hub.ArtifactType]` assignable;
  `type ArtifactType projectlock.ArtifactType` would not.
- **The context name for a Hub artifact is its publishing project's ID**, not the artifact
  ID, because that is what keys the store. An artifact published outside any project falls
  back to the artifact ID.
- **`store.ProjectID`, `store.IsEphemeralProject` and the old `HubContextsForProject` all
  read the lockfile with anonymous structs.** That pattern was the workaround for a
  dependency inversion that this change removed at the root; new readers should use
  `projectlock.Load`.

## Progress Log

### 2026-08-14
- Measured the two-registry split and found the recorded justification did not hold: both
  origins carry a version in the lockfile, and the split followed the store path instead.
- Moved the format to `internal/projectlock` after overwriting `internal/lockfile` by
  mistake and restoring it from git.
- Rewrote `store/registry.go` as a view over lockfile entries; added
  `store/contextpaths.go` so every store location is derived.
- Made the knowledge Hub store version-keyed, closing the silent last-write-wins between
  two versions of the same artifact.
- Recorded `source_path` relative to the project, per the user's constraint, and dropped
  `DBPath` entirely once it proved derivable.
- Collapsed `ListImportedContextsIn` to one source and deleted the four symbols that left
  dead.
- Caught and fixed `prep.writeLockfile` erasing artifact entries.
- Full suite green, `make vet` clean, `make lint` 0 issues.
