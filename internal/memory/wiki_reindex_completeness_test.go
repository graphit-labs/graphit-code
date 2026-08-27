package memory

import (
	"context"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// The symptom these two tests exist for is memory_search returning zero results
// for every query while the store itself is intact: the pages were never
// generated and wiki.db held no chunks. Asserting on ArticlesWritten alone does
// not catch it, so both assert on what search actually reads.

func assertIndexedChunks(t *testing.T, wikiDir string, want int) {
	t.Helper()
	db, err := wiki.OpenWikiDB(context.Background(), wikiDir)
	if err != nil {
		t.Fatalf("OpenWikiDB: %v", err)
	}
	defer func() { _ = db.Close() }()

	got, _, _, _, err := db.Stats(context.Background())
	if err != nil {
		t.Fatalf("wiki db stats: %v", err)
	}
	if got != want {
		t.Errorf("indexed chunks = %d; want %d", got, want)
	}
}

// Nothing stops two inserts from sharing a title — the ULID is the only truly
// unique field — so the page slug and the DB chunk slug must agree on how the
// collision is resolved.
func TestGenerateMemoryWiki_MemoriesSharingATitleGetDistinctSlugs(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()

	writeMemFile(t, rawDir, "MEM1.md", "---\ntitle: Shared Title\ntype: fact\n---\n\n# Shared Title\n\nThe first one mentions alphaterm.")
	writeMemFile(t, rawDir, "MEM2.md", "---\ntitle: Shared Title\ntype: fact\n---\n\n# Shared Title\n\nThe second one mentions zarquonterm.")

	if _, err := GenerateMemoryWiki(context.Background(), rawDir, wikiDir); err != nil {
		t.Fatalf("GenerateMemoryWiki: %v", err)
	}

	db, err := wiki.OpenWikiDB(context.Background(), wikiDir)
	if err != nil {
		t.Fatalf("OpenWikiDB: %v", err)
	}
	defer func() { _ = db.Close() }()

	chunks, slugs, _, _, err := db.Stats(context.Background())
	if err != nil {
		t.Fatalf("wiki db stats: %v", err)
	}
	if chunks != 2 {
		t.Errorf("indexed chunks = %d; want 2", chunks)
	}
	if slugs != 2 {
		t.Errorf("distinct slugs = %d; want 2 — the two memories collided in the DB", slugs)
	}

	for _, term := range []string{"alphaterm", "zarquonterm"} {
		results, searchErr := db.Search(context.Background(), term, 5)
		if searchErr != nil {
			t.Fatalf("searching %q: %v", term, searchErr)
		}
		if len(results) == 0 {
			t.Errorf("memory containing %q is not searchable", term)
		}
	}
}

func TestGenerateMemoryWiki_ColdStartWithEmptyDBAlreadyOnDisk(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()

	writeMemFile(t, rawDir, "MEM1.md", "---\ntitle: Alpha Rule\ntype: convention\n---\n\n# Alpha Rule\n\nAlways prefer alpha.")

	// memory_search opens the wiki DB before anything has been indexed, which
	// leaves an empty wiki.db behind. Generation must not read that as "done".
	db, err := wiki.OpenWikiDB(context.Background(), wikiDir)
	if err != nil {
		t.Fatalf("OpenWikiDB: %v", err)
	}
	if err := db.Close(); err != nil {
		t.Fatalf("closing wiki db: %v", err)
	}

	result, err := GenerateMemoryWiki(context.Background(), rawDir, wikiDir)
	if err != nil {
		t.Fatalf("GenerateMemoryWiki: %v", err)
	}
	if result.ArticlesWritten != 1 {
		t.Errorf("ArticlesWritten = %d; want 1", result.ArticlesWritten)
	}
	assertIndexedChunks(t, wikiDir, 1)
}

func TestGenerateMemoryWiki_IndexesMemoryAddedAfterFirstRun(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()

	writeMemFile(t, rawDir, "MEM1.md", "---\ntitle: Alpha Rule\ntype: convention\n---\n\n# Alpha Rule\n\nAlways prefer alpha.")

	if _, err := GenerateMemoryWiki(context.Background(), rawDir, wikiDir); err != nil {
		t.Fatalf("first GenerateMemoryWiki: %v", err)
	}
	assertIndexedChunks(t, wikiDir, 1)

	// The existing memory is untouched, so every cached stat still matches.
	writeMemFile(t, rawDir, "MEM2.md", "---\ntitle: Beta Fact\ntype: fact\n---\n\n# Beta Fact\n\nBeta was discovered later.")

	if _, err := GenerateMemoryWiki(context.Background(), rawDir, wikiDir); err != nil {
		t.Fatalf("second GenerateMemoryWiki: %v", err)
	}
	assertIndexedChunks(t, wikiDir, 2)

	db, err := wiki.OpenWikiDB(context.Background(), wikiDir)
	if err != nil {
		t.Fatalf("OpenWikiDB: %v", err)
	}
	defer func() { _ = db.Close() }()

	results, err := db.Search(context.Background(), "Beta", 5)
	if err != nil {
		t.Fatalf("searching wiki db: %v", err)
	}
	if len(results) == 0 {
		t.Error("the memory added after the first run is not searchable")
	}
}
