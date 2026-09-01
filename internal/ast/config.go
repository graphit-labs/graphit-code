package ast

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
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

// AddImportedContext records a locally imported graph against a project and returns
// where its store lives.
//
// It creates the store directory and the project's registry entry, and nothing else.
// It used to also symlink the global directory into `<project>/<dotdir>/ast/<name>`,
// which existed only so that a directory scan could rediscover the import — a second
// record of a fact the registry states directly, and one that made `ast list` depend
// on the working directory.
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
	// One source. This used to merge the lockfile's Hub artifacts with a separate
	// per-project registry, and the merge needed a rule for which won a name clash.
	// Membership is one record now, so a clash cannot arise.
	for name, rec := range store.ListContexts(projectDir, store.KindAST) {
		storeDir := store.ASTContextDirIn(projectDir, name)
		// A store is BUILT only once it holds a bundle: either the icebug schema
		// directly (local contexts) or the mount cache (Hub contexts). The
		// directory itself is created by registration, so its existence proves
		// nothing — exactly why the old ladybugdb file check was dropped with the
		// file-based store.
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

// contextStoreBuilt reports whether a context store directory holds a mountable
// bundle — schema.cypher + icebug.json, either directly or under graph.icebug/.
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
	names := map[string]string{}
	registryPath := filepath.Join(brand.GlobalDir(), "hub.registry.json")
	data, err := os.ReadFile(registryPath)
	if err != nil {
		return names
	}
	var cache struct {
		Projects map[string]struct {
			Name string `json:"name"`
		} `json:"projects"`
	}
	if json.Unmarshal(data, &cache) == nil {
		for id, proj := range cache.Projects {
			if proj.Name != "" {
				names[id] = proj.Name
			}
		}
	}
	return names
}
