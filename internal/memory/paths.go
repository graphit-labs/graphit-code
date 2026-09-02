package memory

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// A memory scope's artifacts, all global:
//
//	s3://<bucket>/<prefix>/memory/<scope>/<id>  the Lance table — the truth, when there is a bucket
//	<global>/memory-table/memory-<scope>-<id>/  the Lance table — the truth, when there is not
//	<global>/wiki/memory/<scope>/<id>/          the compiled wiki — what search opens
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

// MemoryTableURI is where the Lance table of the scope at scopePath lives.
//
// `scopePath` is the REMOTE layout — what MemoryService.ScopePrefix returns — and `localDir` is
// where the table goes when there is no bucket to put it in.
//
// 🔒 IT REUSES `remotePrefix` RATHER THAN REBUILDING THE KEY, deliberately. The mapping from a
// scope to its remote location is not the identity: an imported context lives under
// `memory/project/<name>`, because a context IS another project's prefix. A second normalisation
// rule here would put two tables where there is one scope, and the two would diverge silently.
//
// `cfg.Prefix` has to be spelled out because the two clients disagree about who applies it:
// `s3store.Store.Key` prepends it internally, while LanceDB is handed a URI and talks to S3
// directly. A URI built without it addresses a different prefix from every other object this
// project writes — and answers as an empty store rather than failing.
//
// The local form is not a fallback in the sense the project forbids: there is one store, one
// schema and one code path, and only the URI string differs. A missing bucket has always been
// local-only mode rather than an error, and a table that could exist only in S3 would take memory
// away from every unit without a bucket.
func MemoryTableURI(scopePath, localDir string) string {
	if cfg := config.HubS3Config(); cfg.Configured() {
		return s3store.URI(cfg.Bucket, s3store.JoinKey(cfg.Prefix, remotePrefix(scopePath)))
	}
	return localDir
}

// TableDirFor is the local table directory of a scope. It is named from the (scope, scopeID) pair
// so that a scope can never own two differently-named directories.
func TableDirFor(scope, scopeID string) string {
	return store.MemoryTableDir(scope, scopeID)
}

// TableURIFor is where the project or user scope's table lives, for callers outside the service.
func TableURIFor(scope, scopeID string) string {
	return MemoryTableURI("memory/"+scope+"/"+scopeID, TableDirFor(scope, scopeID))
}

// TableURIForScope is TableURIFor with the scope id resolved from the working directory. It answers
// empty when the scope has no identity yet, which a caller must treat as "nothing to compile" rather
// than as a local path.
func TableURIForScope(scope string) string {
	scopeID := resolveScopeID(scope)
	if scopeID == "" {
		return ""
	}
	return TableURIFor(scope, scopeID)
}

// ContextTableURI is where an imported context's table lives.
//
// Separate from TableURIFor rather than folded into it, because a context is the one scope whose
// remote location is not built from its own name: it lives under `memory/project/<name>`, since a
// context IS another project's prefix. Making the caller pick which helper it wants keeps that from
// being guessed from the shape of the arguments.
func ContextTableURI(name string) string {
	return MemoryTableURI("memory/project/"+name, TableDirFor(name, name))
}

// ContextNamesFrom lists the imported memory contexts a memory WIKI root holds.
//
// Memory contexts have no per-project registry, and should not have one: a context's
// memories are a prefix in the shared memory bucket, and the set of local artifacts
// is the record of which prefixes this unit has. Scoping them per project would need a
// second record of the same fact.
//
// 🔒 THE WIKI IS THAT RECORD, AND IT IS THE ONLY CANDIDATE THAT ALWAYS EXISTS. A scope has two
// artifacts and the other one is conditional: with a bucket configured — the normal case — the table
// is `s3://…/memory/project/<name>` and NOTHING is written locally for it. The wiki is always local,
// because being local is what it is for: it is what a search opens.
//
// The name is recovered from the layout `wiki/memory/<scope>/<id>`, where a context is the scope
// whose two halves are the SAME string. That equality is what tells a context apart from `project`
// and `user` without matching their names — and, before the layout mattered, it was also what let a
// context's name survive containing a hyphen.
func ContextNamesFrom(wikiRoot string) []string {
	entries, err := os.ReadDir(wikiRoot)
	if err != nil {
		return nil
	}
	var contexts []string
	for _, e := range entries {
		name := e.Name()
		if !e.IsDir() || strings.HasPrefix(name, ".") {
			continue
		}
		if info, err := os.Stat(filepath.Join(wikiRoot, name, name)); err != nil || !info.IsDir() {
			continue
		}
		contexts = append(contexts, name)
	}
	return contexts
}

// AllContextDirs lists the imported memory contexts on this machine, by name.
//
// 🔒 IT ENUMERATES THE WIKI ROOT, and it used to enumerate the raw markdown store's root. When that
// store was retired the root stopped existing, so this answered EMPTY — with no error, because a
// missing directory and a machine with no imported contexts are the same answer from os.ReadDir. The
// UI's wiki picker consequently listed no memory contexts at all: see uiserver.discoverModules,
// which resolves each name it gets from here to MemoryWikiGlobalDir(name, name) — the same directory
// this now recognises them by, which is why the two cannot disagree any more.
//
// The table root is NOT the replacement, and that is the part worth reading twice: it holds a
// directory per scope only when there is no bucket, so keying the listing on it would have fixed the
// undefined root and left the listing empty in the configuration everything actually runs in.
func AllContextDirs() []string {
	return ContextNamesFrom(store.MemoryWikiRoot())
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
