package memory

import (
	"context"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/store"
)

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

func resolveScopeIDIn(projectDir, scope string) string {
	switch scope {
	case "project":
		return store.ProjectID(projectDir)
	case "user":
		if config.HubS3Config().Configured() {
			subject, err := hubaccess.TrustedSubject(context.Background())
			if err != nil {
				return ""
			}
			return subject.UserID
		}
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

// MemoryTableURI maps a validated logical scope to its authoritative v2 S3 prefix.
func MemoryTableURI(scopePath, localDir string) string {
	if cfg := config.HubS3Config(); cfg.Configured() {
		parts := strings.Split(strings.Trim(scopePath, "/"), "/")
		if len(parts) != 3 || parts[0] != "memory" {
			return ""
		}
		var prefix string
		switch parts[1] {
		case "project":
			if hubaccess.ValidateProjectID(parts[2]) != nil {
				return ""
			}
			prefix = hubaccess.ProjectMemoryPrefix(parts[2])
		case "user":
			if hubaccess.ValidateSubjectID("user", parts[2]) != nil {
				return ""
			}
			prefix = hubaccess.UserMemoryPrefix(parts[2])
		default:
			return ""
		}
		return s3store.URI(cfg.Bucket, s3store.JoinKey(cfg.Prefix, prefix))
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

// ContextTableURI resolves an imported project's memory table by immutable project ID.
func ContextTableURI(projectID string) string {
	return MemoryTableURI("memory/project/"+projectID, TableDirFor(projectID, projectID))
}

// ContextNamesFrom lists the imported memory contexts a memory WIKI root holds.
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

func contextWikiDir(contextName string) string {
	return filepath.Clean(MemoryWikiGlobalDir(contextName, contextName))
}
