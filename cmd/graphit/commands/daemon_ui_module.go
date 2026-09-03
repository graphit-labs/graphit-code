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

type daemonUIModule struct {
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

func resolveDaemonUIRepoPath() string {
	if mgr, err := hub.NewGlobalLockManager(); err == nil {
		if active, err := mgr.ListActiveProjects(); err == nil && len(active) > 0 {
			return active[0].Dir
		}
	}
	return brand.GlobalDir()
}

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
