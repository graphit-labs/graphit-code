package knowledge

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestKnowledgePathsAndIgnore(t *testing.T) {
	tempHome, err := os.MkdirTemp("", "graphit-knowledge-test-*")
	if err != nil {
		t.Fatalf("failed to create temp home: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempHome) }()

	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// 1. WikiDir checks
	projWiki := WikiDir()
	expectedProjWiki := filepath.Join(brand.DotDir(), "knowledge", "project")
	if projWiki != expectedProjWiki {
		t.Errorf("expected %s, got %s", expectedProjWiki, projWiki)
	}

	wikiCtxProj := WikiDirForContext("")
	if wikiCtxProj != projWiki {
		t.Errorf("expected Project Wiki, got %s", wikiCtxProj)
	}

	wikiCtxOther := WikiDirForContext("context-abc")
	expectedOther := filepath.Join(tempHome, "."+brand.Brand, "knowledge", "context-abc")
	if wikiCtxOther != expectedOther {
		t.Errorf("expected %s, got %s", expectedOther, wikiCtxOther)
	}

	// 2. EnsureContextCopy
	EnsureContextCopy("context-abc")
	linkDir := filepath.Join(brand.DotDir(), "knowledge", "context-abc")
	info, err := os.Lstat(linkDir)
	if err != nil {
		t.Errorf("expected directory at %s, got error: %v", linkDir, err)
	} else if info.Mode()&os.ModeSymlink != 0 {
		t.Errorf("expected real directory at %s, got symlink", linkDir)
	} else if !info.IsDir() {
		t.Errorf("expected directory at %s, got file", linkDir)
	}

	// 3. NewKnowledgeIgnoreChecker
	checker := NewKnowledgeIgnoreChecker(tempHome)
	if checker == nil {
		t.Fatal("expected non-nil IgnoreChecker")
	}
	if !checker.IsIgnored("node_modules/some-file.js", false) {
		t.Error("expected node_modules file to be ignored")
	}
}
