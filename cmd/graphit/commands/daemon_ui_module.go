package commands

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/uiserver"
)

// daemonUIModule serves the unified UI for as long as the daemon runs.
//
// It is a GLOBAL module, not a per-project one, for the same reason the memory sync and the
// embedding server are: there is one UI, it switches projects from the client, and every handler
// behind it already takes `project_dir` per request. A per-project instance would mean one port
// per project and a browser that has to be told which to open.
//
// Enabled by `modules.daemon_ui`, which is opt-in — see config.DaemonServesUI for why.
//
// NOTE: this lives in the command package, not in internal/daemon, because internal/uiserver
// already imports internal/daemon (for the daemon status endpoint) and putting it there is an
// import cycle. Satisfying daemon.WatchModule is all that is required, and that interface is
// two methods.
//
// Being a WatchModule buys the two things a hand-rolled goroutine would have to reimplement:
// SuperviseGlobal restarts it if it dies, and its failures reach the daemon log instead of a
// discarded error. It fits because UnifiedServer.Start already has the exact `Start(ctx) error`
// shape and already blocks until the context is cancelled.
type daemonUIModule struct {
	// repoPath is the project the UI opens on. Empty means "resolve it at start time", which is
	// the normal case: the daemon has chdir'd to the global directory by then, so it cannot use
	// its working directory.
	repoPath string
}

func newDaemonUIModule(repoPath string) *daemonUIModule { return &daemonUIModule{repoPath: repoPath} }

func (m *daemonUIModule) Name() string { return "ui" }

// Start builds the server and blocks until ctx is cancelled.
//
// The AST backend and hub service are constructed HERE rather than passed in, because this module
// owns them for its whole lifetime and closing them is part of stopping. The daemon holds
// neither — its per-project modules open their own — so there is nothing to share.
func (m *daemonUIModule) Start(ctx context.Context) error {
	repoPath := m.repoPath
	if repoPath == "" {
		repoPath = resolveDaemonUIRepoPath()
	}

	reg, err := hub.NewRegistryManager(ctx)
	if err != nil {
		// Offline is a normal Hub state — no bucket configured, or a network briefly gone — and it
		// must not stop the UI, which is mostly a view of local stores.
		reg, _ = hub.NewRegistryManager(ctx)
	}
	hubSvc := hub.NewHubService(reg)

	astDB := daemonUIASTBackend(repoPath)
	defer func() { _ = astDB.Close() }()

	ide := config.ResolveIDE("", nil, config.LoadProjectConfig(repoPath))

	srv, err := uiserver.NewUnifiedServer(hubSvc, ide, astDB, repoPath, daemonUIProjectName(repoPath))
	if err != nil {
		return fmt.Errorf("ui server init: %w", err)
	}

	return srv.Start(ctx)
}

// resolveDaemonUIRepoPath picks the project the daemon-hosted UI opens on.
//
// The daemon deliberately chdirs to the global directory, so os.Getwd is useless here. The first
// active registered project is the closest thing to "the project the user means"; with none
// registered, the global directory is a valid neutral root — the UI still serves the Hub, the
// wiki explorer and the project switcher, and the client sends project_dir on every call anyway.
func resolveDaemonUIRepoPath() string {
	if mgr, err := hub.NewGlobalLockManager(); err == nil {
		if active, err := mgr.ListActiveProjects(); err == nil && len(active) > 0 {
			return active[0].Dir
		}
	}
	return brand.GlobalDir()
}

// daemonUIASTBackend opens the project's graph READ-ONLY, which is what a viewer wants: the
// daemon's own sync modules are the writers, and two writers on one graph is a lock fight.
func daemonUIASTBackend(repoPath string) ast.GraphDB {
	storeDir := store.ASTProjectDir(repoPath)
	return ast.NewLadybugDBReadOnly(ast.LadybugConfig{
		StoreDir:  storeDir,
		IcebugDir: filepath.Join(storeDir, "graph.icebug"),
		ReadOnly:  true,
	})
}

func daemonUIProjectName(repoPath string) string {
	name := filepath.Base(repoPath)
	data, err := os.ReadFile(filepath.Join(repoPath, brand.LockFileName()))
	if err != nil {
		return name
	}
	var lockData map[string]any
	if json.Unmarshal(data, &lockData) != nil {
		return name
	}
	proj, ok := lockData["project"].(map[string]any)
	if !ok {
		return name
	}
	if n, ok := proj["name"].(string); ok && n != "" {
		return n
	}
	return name
}
