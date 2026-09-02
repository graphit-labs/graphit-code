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
//	<global>/memory-raw/memory-<scope>-<id>/    the raw markdown — RETIRING, see T2.4
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

// TableDirFor is the local table directory of a scope, mirroring RawDirFor so the two artifacts of
// one scope are always named from the same pair.
func TableDirFor(scope, scopeID string) string {
	return store.MemoryTableDir(scope, scopeID)
}

// TableURIFor is where the project or user scope's table lives, for callers outside the service.
func TableURIFor(scope, scopeID string) string {
	return MemoryTableURI("memory/"+scope+"/"+scopeID, TableDirFor(scope, scopeID))
}

// TableURIForScope is TableURIFor with the scope id resolved from the working directory, mirroring
// RawDirForScope. It answers empty when the scope has no identity yet, which a caller must treat as
// "nothing to compile" rather than as a local path.
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

// RawScope is one scope found in the raw store root, named by its directory.
type RawScope struct {
	Scope   string
	ScopeID string
	Dir     string
}

// RawScopesIn lists every scope the raw store root holds, whichever project it belongs to.
//
// 🔒 IT EXISTS SO THE RAW STORE CAN BE RETIRED WITHOUT STRANDING ANYBODY'S MEMORIES. The store is
// GLOBAL and shared by every project on the machine, while `graphit memory migrate` resolves one
// scope from a project's lockfile — so migrating project by project can only reach the projects that
// are still checked out and registered. Measured on the machine this was written on: five scopes in
// the raw store, three of them belonging to projects the global lock no longer knows, two of those
// holding 15 real memories. Deleting the store after migrating only the current project would have
// destroyed them.
//
// The scope's TARGET does not need the project either: a table's URI is built from the scope id, so
// a directory name is enough to migrate what it holds.
//
// The name is the flattened scope path — `memory-<scope>-<id>` — and it is parsed the same way
// ContextNamesFrom parses it: an imported context has scope == id, so its remainder splits into two
// equal halves, and anything else is `project` or `user` followed by an id that contains no hyphen.
func RawScopesIn(rawRoot string) []RawScope {
	entries, err := os.ReadDir(rawRoot)
	if err != nil {
		return nil
	}
	var out []RawScope
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		rest, ok := cutPrefix(e.Name(), "memory-")
		if !ok || strings.HasPrefix(rest, ".") {
			continue
		}
		dir := filepath.Join(rawRoot, e.Name())
		if name, doubled := doubledName(rest); doubled {
			out = append(out, RawScope{Scope: name, ScopeID: name, Dir: dir})
			continue
		}
		scope, id, found := strings.Cut(rest, "-")
		if !found || scope == "" || id == "" {
			continue
		}
		out = append(out, RawScope{Scope: scope, ScopeID: id, Dir: dir})
	}
	return out
}

// TableURIForRawScope is where a scope found in the raw store belongs in object storage.
func TableURIForRawScope(s RawScope) string {
	if s.Scope == s.ScopeID {
		return ContextTableURI(s.ScopeID)
	}
	return TableURIFor(s.Scope, s.ScopeID)
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
