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

// resolveArtifactScope resolves the pair (project_dir, context) for a read that can be
// served either from a project or from a global install.
//
// The rule it enforces is the one an agent gets wrong: without a project there is no
// "own" graph or wiki to fall back on, so the context is not optional there. Left
// unchecked, an empty pair does not fail — it resolves to a store keyed by the hash of
// an empty path and answers with nothing, which reads as "the artifact is empty" rather
// than "you did not say which artifact".
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

// resolveWikiScope is resolveArtifactScope for the wiki tools, where the two wikis
// differ on whether a project is needed.
//
// The memory wiki does not need one: its user scope is keyed by the machine, so a
// project-less caller has a real scope to read rather than a fallback. The knowledge
// wiki does, unless a context names the artifact to read instead.
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

// errGlobalScopeIsReadOnly is what a write gets in the global scope.
//
// Opening a graph or a wiki read-write CREATES it, and a project-less caller has no
// identity to key one by: the store would be filed under the hash of an empty path,
// where nothing would ever find it again and nothing would reclaim it. Same reasoning
// that already refuses an ephemeral workspace a graph of its own.
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
	// The global scope has no directory to move into, and moving anywhere would be
	// worse than staying: the whole reason this function exists is that code below it
	// resolves things from the working directory, and in the global scope every path
	// is absolute by construction.
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
	// Nothing to anchor to in the global scope. Joining with "" is a no-op that LOOKS
	// harmless and is not: it leaves the path relative, which is precisely the state
	// this function exists to remove, and the caller then resolves it against the
	// server's own working directory.
	if projectDir == "" {
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
	//
	// The global scope is the same hazard with a different key: no project means no
	// identity, so the store would be filed under the hash of an empty path.
	if projectDir == "" {
		return nil, errGlobalScopeIsReadOnly("writing to a code graph")
	}
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
	// The global scope has the same shape as an ephemeral workspace and for the same
	// reason: a project memory scope is keyed by a project identity, and there is none
	// here. The user scope is keyed by the machine, so it is available and it is the
	// only memory such a caller legitimately has — refusing outright would fail the
	// first call of every session the mandate tells an agent to make.
	if !userScope && projectDir == "" {
		userScope = true
		redirected = true
	}
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

	// Unreachable for an empty projectDir — the redirect above already took it — and
	// guarded anyway, because the join would produce a relative path that resolves
	// against this server's working directory.
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

// memoryScopeNotice is the sentence a tool adds when the scope it served was not the
// scope it was asked for, and "" when it was.
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
		//
		// The global scope redirects for the same reason, and the two conditions are
		// kept apart because the sentence the caller is shown differs.
		if scope == "project" && (projectDir == "" || store.IsEphemeralProject(projectDir)) {
			scope = "user"
		}
		return memory.WikiDirFor(projectDir, scope)
	default:
		return ""
	}
}

// loadProjectConfig reads one project's configuration out of its lockfile.
//
// The empty projectDir guard is not defensive tidiness. filepath.Join("", "<lockfile>")
// is a RELATIVE path, so without it a global-scope call reads the lockfile of whatever
// directory this server happens to be sitting in and applies that project's
// configuration — its Hub bucket, its module switches — to a request that named no
// project at all.
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
