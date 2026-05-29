package memory

import (
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

func GlobalBaseDir() string {
	g := brand.GlobalDir()
	if g == "" {
		return filepath.Join(brand.DotDir(), "memory")
	}
	return filepath.Join(g, "memory")
}

func GlobalScopeDir(scope string) string {
	localPath := ProjectLinkDir(scope)
	if _, err := os.Stat(localPath); err == nil {
		return localPath
	}
	return ""
}

func WikiDir(scope string) string {
	return GlobalScopeDir(scope)
}

func RawDir(scope string) string {
	return WorktreeRawDirForScope(scope)
}

func WorktreeRawDirForScope(scope string) string {
	wikiDir := GlobalScopeDir(scope)
	if wikiDir == "" {
		return ""
	}
	scopeID := filepath.Base(wikiDir)
	if scopeID == "" || scopeID == "." || scopeID == "/" {
		return ""
	}
	return WorktreeRawDir(scope, scopeID)
}

func WorktreeRawDir(scope, scopeID string) string {
	d := brand.GlobalDir()
	if d == "" {
		d = brand.DotDir()
	}
	wtBase := filepath.Join(d, "memory-wt")
	branch := fmt.Sprintf("memory/%s/%s", scope, scopeID)
	safe := strings.NewReplacer("/", "-", " ", "_").Replace(branch)
	return filepath.Join(wtBase, safe)
}

func ProjectLinkDir(scope string) string {
	return filepath.Join(brand.DotDir(), "memory", scope)
}

func EnsureScopeDirs(scope, projectDir string) error {
	if projectDir != "" {
		linkPath := filepath.Join(projectDir, ProjectLinkDir(scope))
		if err := os.MkdirAll(filepath.Dir(linkPath), 0o755); err != nil {
			return err
		}
	}
	return nil
}

func EnsureContextCopy(contextName, projectDir string, logger *slog.Logger) {
	log := slogutil.Resolve(logger)
	if projectDir == "" {
		return
	}

	wikiDir := MemoryWikiGlobalDir(contextName, contextName)
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		log.Warn("context copy: mkdir failed", "dir", wikiDir, "error", err)
	}

	copyPath := filepath.Join(projectDir, ProjectLinkDir(contextName))
	if err := os.MkdirAll(filepath.Dir(copyPath), 0o755); err != nil {
		log.Warn("context copy: mkdir parent failed", "dir", filepath.Dir(copyPath), "error", err)
	}
	if err := paths.SyncCopyDir(wikiDir, copyPath); err != nil {
		log.Warn("context copy failed", "src", wikiDir, "dst", copyPath, "error", err)
	}
}

func AllContextDirs() []string {

	memDir := filepath.Dir(ProjectLinkDir("project"))
	entries, err := os.ReadDir(memDir)
	if err != nil {
		return nil
	}
	var contexts []string
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		name := e.Name()

		if name == "project" || name == "user" || strings.HasPrefix(name, ".") {
			continue
		}
		contexts = append(contexts, name)
	}
	return contexts
}
