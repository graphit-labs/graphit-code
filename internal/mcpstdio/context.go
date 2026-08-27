package mcpstdio

import (
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
	// Extra safety: reject if it still contains separators (shouldn't after Base)
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
	return filepath.Join(projectDir, path)
}

// astConfigForProject builds the Ladybug config for projectDir with a DBPath that
// does not depend on the working directory.
//
// Everything here is resolved against projectDir, and that is the whole point. The
// project's own graph is keyed by the project's identity, and a context — Hub or
// locally imported — is claimed by the project's lockfile and registry, so neither
// is discoverable without knowing which project is asking. Resolving either from the
// working directory answers for whichever project the server was started in, and it
// answers silently: a graph opens, it is simply not the one that was requested.
//
// The backend connects lazily, on its first query, so a wrong path here is invisible
// until a write lands in another project's graph and reports success.
func astConfigForProject(projectDir, contextName string) ast.LadybugConfig {
	cfg := ast.LadybugConfigForContextIn(projectDir, contextName)
	cfg.DBPath = anchorToProject(projectDir, cfg.DBPath)
	return cfg
}

func openASTDB(projectDir, contextName string) (ast.GraphDB, error) {
	cfg := astConfigForProject(projectDir, contextName)

	if _, err := os.Stat(cfg.DBPath); os.IsNotExist(err) {
		return nil, fmt.Errorf("no AST database found at %s — index first with: %s ast index", cfg.DBPath, brand.BinName())
	}

	return ast.NewLadybugDBReadOnly(cfg), nil
}

// errEphemeralHasNoGraph is what a write to an ephemeral session's own code graph
// gets. Imported contexts are unaffected — reading and installing those is most of
// what a live search does.
func errEphemeralHasNoGraph() error {
	return fmt.Errorf("a live search session has no code graph of its own: its workspace holds no source, and the graphs it can query are the imported contexts — pass one as 'context'")
}

func openASTDBReadWrite(projectDir, contextName string) (ast.GraphDB, error) {
	// Opening read-write CREATES the store, so this is the structural guard: an
	// ephemeral workspace must not acquire a graph keyed by its session ID, and the
	// end-of-session index the AST skill mandates would otherwise do exactly that.
	if contextName == "" && store.IsEphemeralProject(projectDir) {
		return nil, errEphemeralHasNoGraph()
	}
	return ast.NewLadybugDB(astConfigForProject(projectDir, contextName)), nil
}

// memoryScopeFor decides which memory scope a request actually gets, which is not
// always the one it asked for.
//
// An ephemeral workspace never gets a project scope. Asking for one is not a caller
// error — the mandate tells an agent to search project memory before its first
// response, so every live search session asks — but opening it is destructive in a way
// that is easy to miss: the scope is created on first use, and creating it means an
// orphan branch and a worktree in the SHARED memory repository, named after a session
// that exists for one search. Nothing reclaims them.
//
// So the request is redirected to the user scope, and the caller is told. The user's
// memory is the only memory such a session legitimately has: it is about the user,
// applies everywhere, and is frequently the only place a constraint was written down.
// Refusing outright would be more literal and less useful — it would fail the first
// call of every session and lose any memory the search was about to record.
//
// The bool reports the redirect so a tool can say so rather than quietly answering a
// different question.
func memoryScopeFor(userScope bool, projectDir string) (memory.MemoryScope, string, bool, error) {
	redirected := false
	if !userScope && store.IsEphemeralProject(projectDir) {
		userScope = true
		redirected = true
	}

	if userScope {
		hash, err := memory.UserScopeID()
		if err != nil {
			return "", "", redirected, fmt.Errorf("cannot determine user identity: %w", err)
		}
		return memory.MemoryScopeUser, hash, redirected, nil
	}

	lockPath := filepath.Join(projectDir, brand.LockFileName())
	lf, err := hub.LoadLockfile(lockPath)
	if err != nil || lf == nil {
		return "", "", false, fmt.Errorf("project not initialised at %s — run '%s init' first", projectDir, brand.BinName())
	}
	return memory.MemoryScopeProject, lf.Project.ID, false, nil
}

// memoryScopeNotice is the sentence a tool adds when the scope it served was not the
// scope it was asked for, and "" when it was.
func memoryScopeNotice(userScope bool, projectDir string) string {
	if userScope || !store.IsEphemeralProject(projectDir) {
		return ""
	}
	return "note: this is an ephemeral live search session, which has no project memory of its own — your user memory was used instead"
}

func newMemorySvc(userScope bool, projectDir string) (*memory.MemoryService, error) {
	scope, scopeID, _, err := memoryScopeFor(userScope, projectDir)
	if err != nil {
		return nil, err
	}

	ms, _ := memory.NewMemoryStore()
	svc := memory.NewMemoryService(scope, scopeID, ms)
	if err := svc.EnsureInitialised(); err != nil {
		_ = err
	}
	return svc, nil
}

// resolveWikiDir returns the absolute wiki directory of a module for one project.
//
// No chdir, and no anchoring. Both used to be necessary: the resolvers returned
// ".graphit/knowledge/project" relative to the working directory, and one of them
// stat'ed a project-local replica to decide whether it existed — so a server serving
// one project while sitting in another read the wrong project's wiki unless it moved
// itself first. Every wiki is now global and keyed by identity, so the project is
// simply an argument.
func resolveWikiDir(module, projectDir, contextName string) string {
	switch module {
	case "knowledge":
		// ReadDirIn, not WikiDirForContextIn, because an ephemeral session has no
		// documentation wiki of its own — the sets it can read are the ones it
		// selected, reached by name. Putting the rule here rather than in each tool
		// means every knowledge and wiki tool inherits it.
		return knowledge.ReadDirIn(projectDir, contextName)
	case "memory":
		scope := contextName
		if scope == "" {
			scope = "project"
		}
		// An ephemeral session has no project memory, so a request for one is served
		// from the user scope — the same redirect memoryScopeFor performs. It has to
		// happen here too, or the two disagree: a search would return user memory
		// slugs and reading one of them back would resolve to a directory that does
		// not exist.
		if scope == "project" && store.IsEphemeralProject(projectDir) {
			scope = "user"
		}
		return memory.WikiDirFor(projectDir, scope)
	default:
		return ""
	}
}

func loadProjectConfig(projectDir string) config.ConfigMap {
	lp := filepath.Join(projectDir, brand.LockFileName())
	if lf, err := hub.LoadLockfile(lp); err == nil && lf != nil {
		return lf.Config
	}
	return nil
}

func loadProjectLockInfo(projectDir string) (config.ConfigMap, []string) {
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
