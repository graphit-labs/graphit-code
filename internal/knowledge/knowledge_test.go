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
	defer os.RemoveAll(tempHome)

	origHome := os.Getenv("HOME")
	os.Setenv("HOME", tempHome)
	defer os.Setenv("HOME", origHome)

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

	// 2. EnsureContextSymlink
	EnsureContextSymlink("context-abc")
	linkDir := filepath.Join(brand.DotDir(), "knowledge", "context-abc")
	if _, err := os.Lstat(linkDir); err != nil {
		t.Errorf("expected symlink at %s, got error: %v", linkDir, err)
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
