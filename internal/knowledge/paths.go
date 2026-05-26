package knowledge

import (
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
)

func globalKnowledgeContextDir(name string) string {
	d := brand.GlobalDir()
	if d == "" {
		return filepath.Join(brand.DotDir(), "knowledge", name)
	}
	return filepath.Join(d, "knowledge", name)
}

func WikiDir() string {
	return filepath.Join(brand.DotDir(), "knowledge", "project")
}

func WikiDirForContext(name string) string {
	if name == "" || name == "__project__" {
		return WikiDir()
	}
	return globalKnowledgeContextDir(name)
}

func EnsureContextSymlink(name string) {
	if name == "" || name == "__project__" {
		return
	}
	globalDir := globalKnowledgeContextDir(name)
	if err := os.MkdirAll(globalDir, 0o755); err != nil {
		return
	}
	linkDir := filepath.Join(brand.DotDir(), "knowledge", name)
	if err := os.MkdirAll(filepath.Dir(linkDir), 0o755); err != nil {
		return
	}
	_ = paths.SafeSymlink(globalDir, linkDir)
}

func InstalledContexts() []string {
	parentDir := filepath.Join(brand.DotDir(), "knowledge")
	entries, err := os.ReadDir(parentDir)
	if err != nil {
		return nil
	}
	var names []string
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		n := e.Name()
		if n == "project" {
			continue
		}
		indexPath := filepath.Join(parentDir, n, "index.md")
		if _, err := os.Stat(indexPath); err == nil {
			names = append(names, n)
		}
	}
	return names
}
