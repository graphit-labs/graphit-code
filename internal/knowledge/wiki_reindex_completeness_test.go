//go:build lancedb

package knowledge

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// GenerateKnowledgeWiki shares StatPreCheck with the memory wiki, where a file
// the cache had never seen was invisible to the pre-check. This pins whether a
// doc added without touching any existing doc reaches the index.
func TestGenerateKnowledgeWiki_IndexesDocAddedAfterFirstRun(t *testing.T) {
	root := t.TempDir()
	wikiDir := t.TempDir()

	docsDir := filepath.Join(root, "docs")
	if err := os.MkdirAll(docsDir, 0o755); err != nil {
		t.Fatalf("creating docs dir: %v", err)
	}
	writeDoc := func(name, body string) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(docsDir, name), []byte(body), 0o644); err != nil {
			t.Fatalf("writing %s: %v", name, err)
		}
	}

	writeDoc("first.md", "# First Doc\n\nThe first document mentions alphaterm.\n")

	ctx := context.Background()
	if _, err := GenerateKnowledgeWiki(ctx, root, wikiDir, nil, WikiScope{}); err != nil {
		t.Fatalf("first GenerateKnowledgeWiki: %v", err)
	}

	// first.md is left untouched, so every cached stat still matches.
	writeDoc("second.md", "# Second Doc\n\nThe second document mentions zarquonterm.\n")

	if _, err := GenerateKnowledgeWiki(ctx, root, wikiDir, nil, WikiScope{}); err != nil {
		t.Fatalf("second GenerateKnowledgeWiki: %v", err)
	}

	db, err := wiki.OpenWikiDB(context.Background(), wikiDir)
	if err != nil {
		t.Fatalf("OpenWikiDB: %v", err)
	}
	defer func() { _ = db.Close() }()

	results, err := db.Search(context.Background(), "zarquonterm", 5)
	if err != nil {
		t.Fatalf("searching wiki db: %v", err)
	}
	if len(results) == 0 {
		t.Error("the doc added after the first run is not searchable")
	}
}
