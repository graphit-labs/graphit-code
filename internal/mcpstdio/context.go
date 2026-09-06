package mcpstdio

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// sanitizeContextName validates a user-supplied context name to prevent
// path traversal attacks. It strips directory components and rejects
// names that could escape the intended directory.
func sanitizeContextName(name string) (string, error) {
	if name == "" {
		return "", fmt.Errorf("context name is required")
	}
	clean := filepath.Base(name)
	if clean == "." || clean == ".." || clean == string(os.PathSeparator) {
		return "", fmt.Errorf("invalid context name %q", name)
	}
	if strings.ContainsAny(clean, "/\\") {
		return "", fmt.Errorf("invalid context name %q: must not contain path separators", name)
	}
	return clean, nil
}

func resolveProjectDir(projectDir string) (string, error) {
	if projectDir == "" {
		return "", fmt.Errorf("project_dir is required")
	}
	abs, err := filepath.Abs(projectDir)
	if err != nil {
		return "", fmt.Errorf("invalid project_dir %q: %w", projectDir, err)
	}
	if _, err := os.Stat(abs); err != nil {
		return "", fmt.Errorf("project_dir %q does not exist: %w", abs, err)
	}
	return abs, nil
}

// resolveProjectDirOptional accepts an absent project_dir and answers with "".
//
// An empty string means the GLOBAL scope throughout this package: a caller with no
// checkout on this machine — an agent reaching the server over HTTP — reaching Hub
// artifacts that were installed with no project. Every store those artifacts live in is
// already keyed by id and version rather than by project, so the only thing the caller
// has to supply instead of a project is the artifact's qualified identifier.
//
// It is deliberately a separate function rather than a relaxation of resolveProjectDir.
// Most tools genuinely need a project — indexing, linting, exporting, anything that
// writes — and for those an absent project_dir is a caller error that must keep failing
// loudly instead of resolving to something plausible.
func resolveProjectDirOptional(projectDir string) (string, error) {
	if projectDir == "" {
		return "", nil
	}
	return resolveProjectDir(projectDir)
}

func resolveArtifactScope(projectDir, contextName string) (string, error) {
	abs, err := resolveProjectDirOptional(projectDir)
	if err != nil {
		return "", err
	}
	if abs == "" && contextName == "" {
		return "", errNeedsArtifactReference()
	}
	return abs, nil
}

func errNeedsArtifactReference() error {
	return fmt.Errorf("without project_dir there is no project to answer about: name the artifact " +
		"in 'context' by its qualified identifier, for example 'my-artifact@1.2.0'")
}

func resolveWikiScope(projectDir, wikiScope, contextName string) (string, error) {
	abs, err := resolveProjectDirOptional(projectDir)
	if err != nil {
		return "", err
	}
	if abs != "" {
		return abs, nil
	}
	if wikiScope == "memory" {
		return "", nil
	}
	if contextName == "" {
		return "", errNeedsArtifactReference()
	}
	return "", nil
}

func errGlobalScopeIsReadOnly(what string) error {
	return fmt.Errorf("%s needs a project: without project_dir this server can only READ artifacts "+
		"installed globally, because writing would create a store with no owner to key it by", what)
}

// withProjectDir runs fn with the process sitting in projectDir.
//
// A working directory that cannot be read is NOT a reason to refuse: this function
// exists to move INTO projectDir, and the old directory only matters for putting it
// back afterwards. Returning an error there took down every memory tool on a server
// whose working directory had been deleted from under it — a directory the server
// never needed, and one it inherited rather than chose. Restoring to projectDir in
// that case leaves the process somewhere that exists, which is strictly better than
// where it was.
func withProjectDir(projectDir string, fn func() error) error {
	if projectDir == "" {
		return fn()
	}
	restoreTo, err := os.Getwd()
	if err != nil {
		restoreTo = projectDir
	}
	if err := os.Chdir(projectDir); err != nil {
		return fmt.Errorf("failed to chdir to %s: %w", projectDir, err)
	}
	defer func() { _ = os.Chdir(restoreTo) }()
	return fn()
}

// anchorToProject resolves a project-relative path against projectDir, which must
// already be absolute — every caller passes a resolveProjectDir result.
//
// The module path constructors return paths relative to a project root, because
// brand.DotDir() is the bare ".graphit". A server handling one project while
// sitting in another must therefore anchor them itself: left relative, such a
// path resolves against the server's own working directory and silently reads or
// writes a different project's data. Paths that are already absolute — global
// contexts under the home directory, environment overrides — pass through.
func anchorToProject(projectDir, path string) string {
	if path == "" || filepath.IsAbs(path) {
		return path
	}
	if projectDir == "" {
		return path
	}
	return filepath.Join(projectDir, path)
}

func astConfigForProject(projectDir, contextName string) ast.LadybugConfig {
	cfg := ast.LadybugConfigForContextIn(projectDir, contextName)
	cfg.StoreDir = anchorToProject(projectDir, cfg.StoreDir)
	if cfg.IcebugDir != "" {
		cfg.IcebugDir = anchorToProject(projectDir, cfg.IcebugDir)
	}
	return cfg
}

func openASTDB(projectDir, contextName string) (ast.GraphDB, error) {
	cfg := astConfigForProject(projectDir, contextName)

	if _, err := os.Stat(cfg.IcebugDir); os.IsNotExist(err) {
		return nil, fmt.Errorf("no AST database found at %s — index first with: %s ast index", cfg.IcebugDir, brand.BinName())
	}

	return ast.NewLadybugDBReadOnly(cfg), nil
}

func errEphemeralHasNoGraph() error {
	return fmt.Errorf("a live search session has no code graph of its own: its workspace holds no source, and the graphs it can query are the imported contexts — pass one as 'context'")
}

func openASTDBReadWrite(projectDir, contextName string) (ast.GraphDB, error) {
	// Opening read-write CREATES the store, so this is the structural guard: an
	// ephemeral workspace must not acquire a graph keyed by its session ID, and the
	// end-of-session index the AST skill mandates would otherwise do exactly that.
	//
	// The global scope is the same hazard with a different key: no project means no
	// identity, so the store would be filed under the hash of an empty path.
	if projectDir == "" {
		return nil, errGlobalScopeIsReadOnly("writing to a code graph")
	}
	if contextName == "" && store.IsEphemeralProject(projectDir) {
		return nil, errEphemeralHasNoGraph()
	}
	if contextName == "" {
		if _, err := store.EnsureProjectID(projectDir); err != nil {
			return nil, fmt.Errorf("ensuring project identity: %w", err)
		}
	}
	return ast.NewLadybugDB(astConfigForProject(projectDir, contextName)), nil
}

// memoryScopeFor redirects ephemeral or projectless requests to user memory and reports it.
func memoryScopeFor(ctx context.Context, userScope bool, projectDir string) (memory.MemoryScope, string, bool, error) {
	redirected := false
	if !userScope && projectDir == "" {
		userScope = true
		redirected = true
	}
	if !userScope && store.IsEphemeralProject(projectDir) {
		userScope = true
		redirected = true
	}

	if userScope {
		userID, err := memory.UserScopeIDForContext(ctx)
		if err != nil {
			return "", "", redirected, fmt.Errorf("cannot determine user identity: %w", err)
		}
		return memory.MemoryScopeUser, userID, redirected, nil
	}

	if projectDir == "" {
		return "", "", redirected, fmt.Errorf("a project memory scope is keyed by a project identity, and no project_dir was given")
	}
	lockPath := filepath.Join(projectDir, brand.LockFileName())
	lf, err := hub.LoadLockfile(lockPath)
	if err != nil || lf == nil {
		return "", "", false, fmt.Errorf("project not initialised at %s — run '%s init' first", projectDir, brand.BinName())
	}
	return memory.MemoryScopeProject, lf.Project.ID, false, nil
}

func memoryScopeNotice(userScope bool, projectDir string) string {
	if userScope {
		return ""
	}
	if projectDir == "" {
		return "note: no project_dir was given, and a project memory scope is keyed by a project identity — your user memory was used instead"
	}
	if !store.IsEphemeralProject(projectDir) {
		return ""
	}
	return "note: this is an ephemeral live search session, which has no project memory of its own — your user memory was used instead"
}

func newMemorySvc(ctx context.Context, userScope bool, projectDir string, stateful ...bool) (*memory.MemoryService, error) {
	if !userScope && projectDir != "" && !store.IsEphemeralProject(projectDir) && len(stateful) > 0 && stateful[0] {
		if _, err := store.EnsureProjectID(projectDir); err != nil {
			return nil, err
		}
	}
	scope, scopeID, _, err := memoryScopeFor(ctx, userScope, projectDir)
	if err != nil {
		return nil, err
	}

	ms, _ := memory.NewMemoryStore()
	svc := memory.NewMemoryService(scope, scopeID, ms).WithContext(ctx)
	if err := svc.EnsureInitialised(); err != nil {
		_ = err
	}
	return svc, nil
}

func resolveWikiDir(module, projectDir, contextName string) string {
	switch module {
	case "knowledge":
		return knowledge.ReadDirIn(projectDir, contextName)
	case "memory":
		scope := contextName
		if scope == "" {
			scope = "project"
		}
		if scope == "project" && (projectDir == "" || store.IsEphemeralProject(projectDir)) {
			scope = "user"
		}
		return memory.WikiDirFor(projectDir, scope)
	default:
		return ""
	}
}

func loadProjectConfig(projectDir string) config.ConfigMap {
	if projectDir == "" {
		return nil
	}
	lp := filepath.Join(projectDir, brand.LockFileName())
	if lf, err := hub.LoadLockfile(lp); err == nil && lf != nil {
		return lf.Config
	}
	return nil
}

func loadProjectLockInfo(projectDir string) (config.ConfigMap, []string) {
	if projectDir == "" {
		return nil, nil
	}
	lp := filepath.Join(projectDir, brand.LockFileName())
	if lf, err := hub.LoadLockfile(lp); err == nil && lf != nil {
		return lf.Config, lf.IDEs
	}
	return nil, nil
}

func resolveIDEFromProject(ide, projectDir string) string {
	projectCfg, ides := loadProjectLockInfo(projectDir)
	return config.ResolveProjectIDE(ide, nil, projectCfg, ides)
}

func scopeFromString(s string) bool {
	return s == "user"
}
