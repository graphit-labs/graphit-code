package mcpstdio

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
)

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

func withProjectDir(projectDir string, fn func() error) error {
	origWd, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("failed to get current directory: %w", err)
	}
	if err := os.Chdir(projectDir); err != nil {
		return fmt.Errorf("failed to chdir to %s: %w", projectDir, err)
	}
	defer func() { _ = os.Chdir(origWd) }()
	return fn()
}

func openASTDB(projectDir, contextName string) (ast.GraphDB, error) {
	origWd, _ := os.Getwd()
	if err := os.Chdir(projectDir); err != nil {
		return nil, fmt.Errorf("cannot chdir to %s: %w", projectDir, err)
	}
	defer func() { _ = os.Chdir(origWd) }()

	var cfg ast.LadybugConfig
	if contextName != "" {
		cfg = ast.LadybugConfigForContext(contextName)
	} else {
		cfg = ast.DefaultLadybugConfig()
	}

	if _, err := os.Stat(cfg.DBPath); os.IsNotExist(err) {
		return nil, fmt.Errorf("no AST database found at %s — index first with: %s ast index", cfg.DBPath, brand.BinName())
	}

	return ast.NewLadybugDBReadOnly(cfg), nil
}

func openASTDBReadWrite(projectDir, contextName string) (ast.GraphDB, error) {
	origWd, _ := os.Getwd()
	if err := os.Chdir(projectDir); err != nil {
		return nil, fmt.Errorf("cannot chdir to %s: %w", projectDir, err)
	}
	defer func() { _ = os.Chdir(origWd) }()

	var cfg ast.LadybugConfig
	if contextName != "" {
		cfg = ast.LadybugConfigForContext(contextName)
	} else {
		cfg = ast.DefaultLadybugConfig()
	}

	return ast.NewLadybugDB(cfg), nil
}

func newMemorySvc(userScope bool, projectDir string) (*memory.MemoryService, error) {
	var scope memory.MemoryScope
	var scopeID string

	if userScope {
		scope = memory.MemoryScopeUser
		hash, err := memory.UserHashFromGit()
		if err != nil {
			return nil, fmt.Errorf("cannot determine user identity: %w", err)
		}
		scopeID = hash
	} else {
		scope = memory.MemoryScopeProject
		lockPath := filepath.Join(projectDir, brand.LockFileName())
		lf, err := hub.LoadLockfile(lockPath)
		if err != nil || lf == nil {
			return nil, fmt.Errorf("project not initialised at %s — run '%s init' first", projectDir, brand.BinName())
		}
		scopeID = lf.Project.ID
	}

	ms, _ := memory.NewMemoryGitStore()
	svc := memory.NewMemoryService(scope, scopeID, ms)
	if err := svc.EnsureInitialised(); err != nil {
		_ = err
	}
	return svc, nil
}

func resolveWikiDir(module, projectDir, contextName string) string {
	origWd, _ := os.Getwd()
	_ = os.Chdir(projectDir)
	defer func() { _ = os.Chdir(origWd) }()

	switch module {
	case "knowledge":
		if contextName != "" {
			return knowledge.WikiDirForContext(contextName)
		}
		return knowledge.WikiDir()
	case "memory":
		if contextName != "" {
			return memory.WikiDir(contextName)
		}
		return memory.WikiDir("project")
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
