package memory

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/store"
)

// A memory scope has exactly two directories, both global:
//
//	<global>/memory-raw/memory-<scope>-<id>/  the raw markdown — the truth
//	<global>/wiki/memory/<scope>/<id>/        the compiled wiki — what search opens
//
// There used to be a third: a replica of the wiki inside every project that read it,
// which is what `memory_search` actually opened. It bought nothing and cost a fan-out
// pass on every compile, a Windows-specific failure mode when a reader held the copy
// open, and a class of bug where a project answered from a replica nobody had
// refreshed — indistinguishable, from the outside, from a project with fewer
// memories. The wiki is now read where it is compiled.

// WikiDirFor is the compiled wiki of one scope, for a named project.
//
// The project matters only for the "project" scope, whose id comes from that
// project's lockfile. "user" is keyed by the git identity and an imported context by
// its own name, so both answer the same for every project on the machine.
func WikiDirFor(projectDir, scope string) string {
	scopeID := resolveScopeIDIn(projectDir, scope)
	if scopeID == "" {
		return ""
	}
	return MemoryWikiGlobalDir(scope, scopeID)
}

// WikiDir is WikiDirFor the working directory. Prefer WikiDirFor wherever the project
// is known.
func WikiDir(scope string) string {
	wd, _ := os.Getwd()
	return WikiDirFor(wd, scope)
}

// RawDir is the directory holding a scope's raw memories, for the project in the
// working directory.
func RawDir(scope string) string {
	return RawDirForScope(scope)
}

// resolveScopeIDIn returns the real scope identifier for a scope, for one project.
// For "project": the project's lockfile ID. For "user": the hash of the git identity.
// For a context scope: the scope name itself.
func resolveScopeIDIn(projectDir, scope string) string {
	switch scope {
	case "project":
		return store.ProjectID(projectDir)
	case "user":
		hash, err := UserScopeID()
		if err != nil {
			return ""
		}
		return hash
	default:
		return scope
	}
}

func resolveScopeID(scope string) string {
	wd, _ := os.Getwd()
	return resolveScopeIDIn(wd, scope)
}

// RawDirForScope locates the directory holding a scope's raw memories.
//
// It does NOT require anything to have been compiled first. It used to require the
// project replica to exist, and that made the raw store — the source of truth, the
// thing the remote syncs into — unreachable in a project that had not been compiled
// yet: no replica, so no raw dir, so no compile, so no replica. A fresh clone could
// not bootstrap its own memories.
func RawDirForScope(scope string) string {
	scopeID := resolveScopeID(scope)
	if scopeID == "" {
		return ""
	}
	return RawDirFor(scope, scopeID)
}

func RawDirFor(scope, scopeID string) string {
	return store.MemoryRawDir(scope, scopeID)
}

// ContextNamesFrom lists the imported memory contexts a raw directory root holds.
//
// Memory contexts have no per-project registry, and should not have one: a context's
// memories are a prefix in the shared memory bucket, and the set of local directories
// is the record of which prefixes this unit has. Scoping them per project would need a
// second record of the same fact.
//
// The name is recovered from the directory, which is the scope path with its separators
// flattened: `memory/<scope>/<id>` becomes `memory-<scope>-<id>`. A flattened name is
// ambiguous in general — a scope may contain a hyphen — but not for a context, whose
// scope and id are the SAME string, so the remainder splits into two equal halves.
// That equality is also what tells a context apart from the project and user scopes
// without matching their names.
func ContextNamesFrom(rawRoot string) []string {
	entries, err := os.ReadDir(rawRoot)
	if err != nil {
		return nil
	}
	var contexts []string
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		rest, ok := cutPrefix(e.Name(), "memory-")
		if !ok || strings.HasPrefix(rest, ".") {
			continue
		}
		name, ok := doubledName(rest)
		if !ok {
			continue
		}
		contexts = append(contexts, name)
	}
	return contexts
}

func cutPrefix(s, prefix string) (string, bool) {
	if !strings.HasPrefix(s, prefix) {
		return s, false
	}
	return s[len(prefix):], true
}

// doubledName reports the half of "<x>-<x>", which is how a context's directory name
// carries its name twice — once as the scope and once as the id.
func doubledName(s string) (string, bool) {
	if len(s) < 3 || len(s)%2 == 0 {
		return "", false
	}
	mid := len(s) / 2
	if s[mid] != '-' {
		return "", false
	}
	left, right := s[:mid], s[mid+1:]
	if left != right {
		return "", false
	}
	return left, true
}

// AllContextDirs lists the imported memory contexts on this machine, by name.
func AllContextDirs() []string {
	return ContextNamesFrom(store.MemoryRawRoot())
}

// EnsureScopeDirs creates the wiki directory of a scope so that a reader opening it
// finds an empty wiki rather than a missing path.
func EnsureScopeDirs(scope, projectDir string) error {
	dir := WikiDirFor(projectDir, scope)
	if dir == "" {
		return nil
	}
	return os.MkdirAll(dir, 0o755)
}

// MemoryWikiGlobalDir is the compiled wiki of one scope, by explicit id.
func MemoryWikiGlobalDir(scope, scopeID string) string {
	return store.MemoryWikiDir(scope, scopeID)
}

// contextWikiDir is the wiki of an imported context.
func contextWikiDir(contextName string) string {
	return filepath.Clean(MemoryWikiGlobalDir(contextName, contextName))
}
