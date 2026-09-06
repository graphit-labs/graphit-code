package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/projectlock"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func envOr(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}

type ImportedContext struct {
	Name       string `yaml:"name" json:"name"`
	SourcePath string `yaml:"source_path" json:"source_path"`
	StoreDir   string `yaml:"store_dir" json:"store_dir"`
	ImportedAt string `yaml:"imported_at" json:"imported_at"`
	// Version is the Hub version this context is pinned to, and "local" for a
	// context imported from a directory on this machine.
	//
	// It is reported because a caller with no project has no lockfile to read it
	// from: the qualified id@version is the only way such a caller can name one
	// version of an artifact rather than whichever is newest.
	Version string `yaml:"version,omitempty" json:"version,omitempty"`
	// Reference is what to pass as `context` to reach this store — the qualified
	// identifier for a Hub context, the plain name otherwise.
	Reference string `yaml:"reference,omitempty" json:"reference,omitempty"`
}

func sanitizeContextName(name string) string { return store.SanitizeName(name) }

// ContextDirIn resolves a context name to its store directory for one project.
//
// Every store is global; what differs is which record says this project may query
// it. A context's dir answers in three ways:
//
//  1. a Hub-installed context — a shared, version-scoped store under
//     HubContextsRoot(), claimed by the project's lockfile;
//  2. a context in the project's registry with an explicit source path — what
//     `hub link` records when it points at a sibling project's own store;
//  3. the global context store, ~/.<brand>/ast/context/<name>/graph.icebug, which
//     is where `ast install` indexes and where an ordinary registry entry resolves to.
//
// Case 3 answers whether or not the project registered the context. That is
// deliberate: resolution is not authorisation, and a caller that asks for a context
// by name gets the one store that name can mean. Membership decides what `ast list`
// reports, not what a direct request can reach.
func ContextDirIn(projectDir, name string) string {
	return store.ASTContextDirIn(projectDir, name)
}

func AddImportedContext(projectDir, name, sourcePath string) (ImportedContext, error) {
	globalDir := store.ASTContextDir(name)

	if err := os.MkdirAll(globalDir, 0o755); err != nil {
		return ImportedContext{}, fmt.Errorf("create global context dir: %w", err)
	}

	ictx := ImportedContext{
		Name:       name,
		SourcePath: sourcePath,
		StoreDir:   globalDir,
		ImportedAt: time.Now().Format(time.RFC3339),
	}

	if err := store.AddContext(projectDir, store.KindAST, store.ContextRecord{
		Name:       name,
		SourcePath: sourcePath,
		Origin:     projectlock.OriginLocal,
	}); err != nil {
		return ImportedContext{}, err
	}
	return ictx, nil
}

// LinkImportedContext records a context whose store belongs to someone else — a
// sibling project's graph, reached in place.
//
// What is recorded is the SIBLING'S DIRECTORY, not its store: the store location is
// derived from it on every read, so the link follows the sibling if it reindexes or
// runs `init` and re-keys. Recording the store path froze it at link time.
func LinkImportedContext(projectDir, name, siblingDir string) error {
	return store.AddContext(projectDir, store.KindAST, store.ContextRecord{
		Name:       name,
		SourcePath: siblingDir,
		Origin:     projectlock.OriginLink,
	})
}

// RemoveImportedContext drops a project's claim on a context.
//
// The store is left alone. It is global and another project may have imported the
// same one; `ast install --reset` is how a store itself is discarded.
func RemoveImportedContext(projectDir, name string) error {
	return store.RemoveContext(projectDir, store.KindAST, name)
}

// ListImportedContexts enumerates the contexts available to the project in the
// current working directory. Prefer ListImportedContextsIn where the project is
// known.
func ListImportedContexts() map[string]ImportedContext {
	wd, _ := os.Getwd()
	return ListImportedContextsIn(wd)
}

// ListImportedContextsIn enumerates every context projectDir can query.
//
// Two records are merged, and neither is a directory scan. A Hub-installed context
// is claimed by the project's lockfile; a locally imported or linked one is claimed
// by the project's context registry. Both stores are global, so walking the project
// would find nothing — which is the point: there is one copy of each graph, and the
// project holds only the list of which ones are its own.
func ListImportedContextsIn(projectDir string) map[string]ImportedContext {
	result := map[string]ImportedContext{}
	projectNames := loadProjectIDNamesFromRegistry()
	for name, rec := range store.ListContexts(projectDir, store.KindAST) {
		storeDir := store.ASTContextDirIn(projectDir, name)
		if !contextStoreBuilt(storeDir) {
			// Claimed but never built, or collected after the last project using it
			// dropped it. Reporting it would offer a graph that cannot be opened.
			continue
		}
		displayName := rec.Name
		if readable, ok := projectNames[name]; ok {
			displayName = readable
		}
		reference := name
		if rec.IsHub() && rec.Version != "" {
			reference = name + "@" + rec.Version
		}
		result[name] = ImportedContext{
			Name:       displayName,
			SourcePath: rec.SourcePath,
			StoreDir:   storeDir,
			Version:    rec.Version,
			Reference:  reference,
		}
	}
	return result
}

func contextStoreBuilt(dir string) bool {
	for _, p := range []string{
		filepath.Join(dir, "graph.icebug", "schema.cypher"),
		filepath.Join(dir, "schema.cypher"),
	} {
		if _, err := os.Stat(p); err == nil {
			return true
		}
	}
	return false
}

func loadProjectIDNamesFromRegistry() map[string]string {
	return map[string]string{}
}
