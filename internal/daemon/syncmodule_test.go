package daemon

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// SyncModule — Name

// wikiSources returns the source paths the project's wiki was built from. The wiki
// itself lives in the global store, keyed by the project's identity.
func wikiSources(t *testing.T, projectDir string) map[string]bool {
	t.Helper()
	m := knowledge.LoadManifest(store.KnowledgeProjectDir(projectDir))
	out := make(map[string]bool, len(m.SourceHashes))
	for path := range m.SourceHashes {
		out[path] = true
	}
	return out
}

func TestSyncModule_ReindexKnowledge_NoDocs(t *testing.T) {
	tmpDir := t.TempDir()
	m := NewSyncModule(tmpDir, "")
	ctx := context.Background()
	// No docs directory and no README: nothing to index, so this returns without
	// running the pipeline at all.
	m.reindexKnowledge(ctx, nil)

	if got := wikiSources(t, tmpDir); len(got) != 0 {
		t.Errorf("a project with no documentation produced a wiki from %v", got)
	}
}

func TestSyncModule_ReindexKnowledge_WithDocs(t *testing.T) {
	tmpDir := t.TempDir()
	docsDir := filepath.Join(tmpDir, "docs")
	if err := os.MkdirAll(docsDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(docsDir, "test.md"), []byte("# Test\n\nCorpo.\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	// Outside the docs tree: in scope as the root README, and only as that.
	if err := os.WriteFile(filepath.Join(tmpDir, "README.md"), []byte("# Projeto\n\nPorta.\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tmpDir, "NOTAS.md"), []byte("# Notas\n\nSoltas.\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	m := NewSyncModule(tmpDir, "")
	m.reindexKnowledge(context.Background(), nil)

	got := wikiSources(t, tmpDir)
	// Source paths are relative to the project, not to the docs directory — the
	// daemon hands the pipeline the project root and narrows it with a scope.
	for _, want := range []string{filepath.Join("docs", "test.md"), "README.md"} {
		if !got[want] {
			t.Errorf("%s is not in the wiki; indexed: %v", want, got)
		}
	}
	if got["NOTAS.md"] {
		t.Error("a root document that is not the README was indexed")
	}
}

// A project that has not created its docs tree yet still has a front page, and the
// daemon has to build a wiki containing it rather than returning early.
func TestSyncModule_ReindexKnowledge_ReadmeWithoutDocsTree(t *testing.T) {
	tmpDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmpDir, "README.md"), []byte("# Projeto\n\nSem docs.\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	m := NewSyncModule(tmpDir, "")
	m.reindexKnowledge(context.Background(), nil)

	if got := wikiSources(t, tmpDir); !got["README.md"] {
		t.Errorf("README.md is not in the wiki; indexed: %v", got)
	}
}

func TestSyncModule_ReindexAST(t *testing.T) {
	tmpDir := t.TempDir()
	storeDir := store.ASTProjectDir(tmpDir)
	if err := os.MkdirAll(storeDir, 0o755); err != nil {
		t.Fatal(err)
	}

	m := NewSyncModule(tmpDir, storeDir)
	ctx := context.Background()
	m.reindexAST(ctx, nil, nil, nil) // nil scope = full scan
}

func TestSyncModule_ImplementsActivityReporter(t *testing.T) {
	var _ ActivityReporter = (*SyncModule)(nil)

	m := NewSyncModule(t.TempDir(), "")
	called := false
	m.SetActivityCallback(func() { called = true })

	if m.onActivity == nil {
		t.Fatal("expected onActivity to be set")
	}
	m.onActivity()
	if !called {
		t.Error("expected the wired callback to run")
	}
}

func TestScopeDir(t *testing.T) {
	tests := []struct {
		name     string
		wtBase   string
		branch   string
		contains string
	}{
		{
			name:     "simple branch",
			wtBase:   "/tmp/wt",
			branch:   "main",
			contains: "main",
		},
		{
			name:     "branch with slashes",
			wtBase:   "/tmp/wt",
			branch:   "memory/project/abc",
			contains: "memory-project-abc",
		},
		{
			name:     "branch with spaces",
			wtBase:   "/tmp/wt",
			branch:   "feature branch",
			contains: "feature_branch",
		},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := scopeDir(tc.wtBase, tc.branch)
			if !strings.Contains(got, tc.contains) {
				t.Errorf("expected path to contain %q, got %q", tc.contains, got)
			}
			if !strings.HasPrefix(got, tc.wtBase) {
				t.Errorf("expected path to start with %q, got %q", tc.wtBase, got)
			}
		})
	}
}

// parseBranch
