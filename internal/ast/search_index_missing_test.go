package ast

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

// The sibling of TestMissingGraphIsRebuiltFromCacheWithoutReparsing, for the OTHER half of the
// store — and it was found the way that one was: by running the installed binary against a real
// project rather than by a test.
//
// A store indexed by a build that predates the current search engine has a search directory that
// exists and holds nothing. `ast index` compares file hashes, finds no change, and took the
// shortcut — reporting "N files up to date (no changes detected)" over a search that answered
// nothing at all. Silence, reported as success.
//
// os.Stat cannot detect it: OpenSearchIndex CREATES what it opens, so the directory exists in
// exactly the broken case. The check has to count rows, which is what SearchIndexBuilt does.
func TestEmptySearchIndexIsRebuiltFromCacheWithoutReparsing(t *testing.T) {
	work := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	if err := os.WriteFile(filepath.Join(work, "a.xml"),
		[]byte(`<alpha><beta>gamma</beta></alpha>`), 0o644); err != nil {
		t.Fatal(err)
	}

	store := t.TempDir()
	dbPath := filepath.Join(store, "ladybugdb")
	opts := PipelineOptions{CacheDir: filepath.Join(store, "cache")}

	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	first, err := RunPipeline(context.Background(), db, work, opts)
	if err != nil {
		t.Fatalf("first pipeline: %v", err)
	}
	_ = db.Close()
	if first.ParsedFiles == 0 {
		t.Fatal("the first run parsed nothing; the fixture is wrong")
	}

	ctx := context.Background()
	if !SearchIndexBuilt(ctx, dbPath) {
		t.Skip("this build has no search engine linked (needs -tags lancedb), so there is no index to empty")
	}

	// The search index goes; the graph and the parse cache stay. This is the shape of a store
	// carried across an engine change.
	if err := os.RemoveAll(LanceIndexPath(dbPath)); err != nil {
		t.Fatal(err)
	}
	if SearchIndexBuilt(ctx, dbPath) {
		t.Fatal("the index still reports as built after being removed")
	}

	db2 := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	second, err := RunPipeline(ctx, db2, work, opts)
	if err != nil {
		t.Fatalf("second pipeline: %v", err)
	}
	_ = db2.Close()

	if second.ParsedFiles != 0 {
		t.Errorf("reparsed %d file(s); the parse cache was current and should have been replayed",
			second.ParsedFiles)
	}
	if !second.SearchIndexRebuilt {
		t.Error("the run did not report rebuilding the search index — it took the up-to-date shortcut")
	}
	if !SearchIndexBuilt(ctx, dbPath) {
		t.Fatal("the search index is still empty after a run that reported success")
	}

	// The point of the rebuild is that search answers, so assert that rather than the row count.
	idx, err := OpenSearchIndex(ctx, dbPath)
	if err != nil {
		t.Fatalf("open rebuilt index: %v", err)
	}
	defer func() { _ = idx.Close() }()
	results, err := idx.Search(ctx, "beta", 5)
	if err != nil {
		t.Fatalf("search the rebuilt index: %v", err)
	}
	if len(results) == 0 {
		t.Error("the rebuilt index returned nothing for a term that is in the fixture")
	}
}

// Removing the index directory ENTIRELY and leaving one that exists but is empty are different
// states on disk, and only the second one defeats os.Stat. This pins the second.
func TestSearchIndexThatExistsButIsEmptyIsStillRebuilt(t *testing.T) {
	work := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	if err := os.WriteFile(filepath.Join(work, "a.xml"),
		[]byte(`<alpha><beta>gamma</beta></alpha>`), 0o644); err != nil {
		t.Fatal(err)
	}

	store := t.TempDir()
	dbPath := filepath.Join(store, "ladybugdb")
	opts := PipelineOptions{CacheDir: filepath.Join(store, "cache")}

	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	if _, err := RunPipeline(context.Background(), db, work, opts); err != nil {
		t.Fatalf("first pipeline: %v", err)
	}
	_ = db.Close()

	ctx := context.Background()
	if !SearchIndexBuilt(ctx, dbPath) {
		t.Skip("this build has no search engine linked (needs -tags lancedb)")
	}

	// Empty the index but leave the directory, which is what a store created and never
	// populated looks like — and what os.Stat cannot tell from a healthy one.
	idxPath := LanceIndexPath(dbPath)
	if err := os.RemoveAll(idxPath); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(idxPath, 0o755); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(idxPath); err != nil {
		t.Fatalf("the directory is meant to exist for this test: %v", err)
	}

	db2 := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	second, err := RunPipeline(ctx, db2, work, opts)
	if err != nil {
		t.Fatalf("second pipeline: %v", err)
	}
	_ = db2.Close()

	if !second.SearchIndexRebuilt {
		t.Error("an existing-but-empty index was treated as up to date")
	}
	if !SearchIndexBuilt(ctx, dbPath) {
		t.Error("the search index is still empty")
	}
}
