//go:build lancedb

package commands

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// The daemon reconciles a project's documentation by running `knowledge index` in the
// project, so these are the documentation-scope rules that command must keep.

func knowledgeIndexProject(t *testing.T, files map[string]string) map[string]bool {
	t.Helper()
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	root := t.TempDir()
	t.Chdir(root)
	if _, err := store.EnsureProjectID(root); err != nil {
		t.Fatal(err)
	}
	for name, body := range files {
		path := filepath.Join(root, name)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if err := runKnowledgeIndex(root, knowledge.ScopeFor(root, nil, loadProjectConfig()), 0, false, false); err != nil {
		t.Fatalf("knowledge index: %v", err)
	}
	chunks, err := wiki.IndexedChunks(context.Background(), store.KnowledgeProjectDir(root))
	if err != nil {
		t.Fatalf("reading indexed chunks: %v", err)
	}
	out := map[string]bool{}
	for path := range knowledge.ManifestFromChunks(chunks).SourceHashes {
		out[path] = true
	}
	return out
}

func TestKnowledgeIndexIndexesDocsAndReadmeOnly(t *testing.T) {
	got := knowledgeIndexProject(t, map[string]string{
		"docs/test.md": "# Test\n\nCorpo.\n",
		"README.md":    "# Projeto\n\nPorta.\n",
		"NOTAS.md":     "# Notas\n\nSoltas.\n",
	})
	for _, want := range []string{filepath.Join("docs", "test.md"), "README.md"} {
		if !got[want] {
			t.Errorf("%s is not in the wiki; indexed: %v", want, got)
		}
	}
	if got["NOTAS.md"] {
		t.Error("a root document that is not the README was indexed")
	}
}

// A project that has not created its docs tree yet still has a front page.
func TestKnowledgeIndexIndexesTheReadmeWithoutADocsTree(t *testing.T) {
	if got := knowledgeIndexProject(t, map[string]string{"README.md": "# Projeto\n\nSem docs.\n"}); !got["README.md"] {
		t.Errorf("README.md is not in the wiki; indexed: %v", got)
	}
}
