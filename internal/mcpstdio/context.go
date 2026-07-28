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
// no longer depends on the working directory.
//
// Anchoring here rather than chdir'ing around the constructors is what makes the
// path correct: the backend connects lazily, on its first query, long after any
// chdir would have been undone — so a DBPath left relative resolved against
// whichever project the server happened to sit in, and one project's nodes landed
// in another project's graph while indexing reported success. The constructors
// themselves never read the working directory; they only return relative strings.
func astConfigForProject(projectDir, contextName string) ast.LadybugConfig {
	var cfg ast.LadybugConfig
	if contextName != "" {
		cfg = ast.LadybugConfigForContext(contextName)
	} else {
		cfg = ast.DefaultLadybugConfig()
	}
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

func openASTDBReadWrite(projectDir, contextName string) (ast.GraphDB, error) {
	return ast.NewLadybugDB(astConfigForProject(projectDir, contextName)), nil
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

// resolveWikiDir returns an absolute wiki directory for the module.
//
// The chdir stays because memory.WikiDir stats the project link directory to
// decide whether it exists, and that probe has to run against projectDir. Only
// the resolved path escapes the block, so it is anchored before returning:
// otherwise the caller received ".graphit/knowledge/project" and read whichever
// project the server was started in.
func resolveWikiDir(module, projectDir, contextName string) string {
	origWd, _ := os.Getwd()
	_ = os.Chdir(projectDir)
	defer func() { _ = os.Chdir(origWd) }()

	var dir string
	switch module {
	case "knowledge":
		if contextName != "" {
			dir = knowledge.WikiDirForContext(contextName)
		} else {
			dir = knowledge.WikiDir()
		}
	case "memory":
		if contextName != "" {
			dir = memory.WikiDir(contextName)
		} else {
			dir = memory.WikiDir("project")
		}
	default:
		return ""
	}
	return anchorToProject(projectDir, dir)
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
