//go:build lancedb

package wiki

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

// publishWiki writes what a producer publishes: shards and sidecars, no sources and
// no database. Returns the directory a consumer would receive.
func publishWiki(t *testing.T, chunks map[string]CachedChunk, slugs map[string]string) string {
	t.Helper()
	dir := t.TempDir()
	c, err := NewWikiProcessCache(dir)
	if err != nil {
		t.Fatal(err)
	}
	for relPath, ch := range chunks {
		c.Store(relPath, ch.ContentHash, []CachedChunk{ch})
		c.StoreSlug(relPath, slugs[relPath])
	}
	if err := c.Save(); err != nil {
		t.Fatal(err)
	}
	return dir
}

// The property the whole publishing change rests on: a wiki's search index can be
// built from its shards with no source document anywhere on the machine.
//
// A consumer of a published knowledge context is in exactly that position — the
// producer's docs tree does not travel — so if this cannot be done, an installed
// context is unsearchable.
func TestBuildDBFromCacheNeedsNoSourceDocuments(t *testing.T) {
	body := "Acme ships widgets to three continents."
	dir := publishWiki(t,
		map[string]CachedChunk{
			"docs/overview.md": {
				Title:       "Overview",
				Body:        body,
				Summary:     "what Acme does",
				DocType:     "architecture",
				Breadcrumb:  "docs/overview.md",
				ContentHash: ContentHash([]byte(body)),
				ClusterID:   4,
				ClusterName: "platform",
				Confidence:  0.75,
				Updated:     "2026-08-14",
				Important:   true,
				CrossRefs:   []string{"Billing"},
			},
		},
		map[string]string{"docs/overview.md": "Overview"},
	)

	// Nothing but the cache is present: no sources, no index. Named by the constant — this guard
	// exists to stop the test proving nothing, so it must not itself pass for the wrong reason.
	if _, err := os.Stat(filepath.Join(dir, WikiIndexDirName)); err == nil {
		t.Fatal("the fixture already has an index; this test would prove nothing")
	}

	n, err := BuildDBFromCache(context.Background(), dir)
	if err != nil {
		t.Fatalf("BuildDBFromCache: %v", err)
	}
	if n != 1 {
		t.Fatalf("indexed %d chunks, want 1", n)
	}

	db, err := OpenWikiDB(context.Background(), dir)
	if err != nil {
		t.Fatalf("OpenWikiDB: %v", err)
	}
	defer db.Close()
	if !db.HasContent(context.Background()) {
		t.Fatal("the index reports no content after being built from the shards")
	}

	results := BM25Search(context.Background(), dir, "widgets continents", 5)
	if len(results) == 0 {
		t.Fatal("a term from the published body did not match")
	}
	if results[0].Title != "Overview" {
		t.Errorf("Title = %q, want Overview", results[0].Title)
	}
}

// Every field that reaches the database has to survive publication, because a
// consumer has no source to recover it from. This is what makes CachedChunk's
// completeness a requirement rather than a convenience.
func TestPublishedChunkKeepsItsCorpusLevelFields(t *testing.T) {
	body := "Billing handles invoices."
	dir := publishWiki(t,
		map[string]CachedChunk{
			"docs/billing.md": {
				Title:       "Billing",
				Body:        body,
				DocType:     "specification",
				ContentHash: ContentHash([]byte(body)),
				ClusterID:   9,
				ClusterName: "money",
				Confidence:  0.5,
				Updated:     "2026-01-02",
				Important:   true,
			},
		},
		map[string]string{"docs/billing.md": "Billing"},
	)
	if _, err := BuildDBFromCache(context.Background(), dir); err != nil {
		t.Fatal(err)
	}

	db, err := OpenWikiDB(context.Background(), dir)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()

	docs, err := db.Browse(context.Background(), BrowseFilter{ClusterID: -1, Limit: 10})
	if err != nil {
		t.Fatalf("Browse: %v", err)
	}
	if len(docs) != 1 {
		t.Fatalf("Browse returned %d docs, want 1", len(docs))
	}
	got := docs[0]
	if got.Slug != "Billing" || got.Title != "Billing" {
		t.Errorf("slug/title = %q/%q", got.Slug, got.Title)
	}
	if got.DocType != "specification" {
		t.Errorf("DocType = %q, want specification — lost in publication", got.DocType)
	}
	// The corpus-level fields are the ones a consumer cannot recover: they were
	// computed over the whole publisher's corpus, not over this document.
	if got.ClusterName != "money" {
		t.Errorf("ClusterName = %q, want money — lost in publication", got.ClusterName)
	}
	if got.Confidence != 0.5 {
		t.Errorf("Confidence = %v, want 0.5 — lost in publication", got.Confidence)
	}
	if !got.Important {
		t.Error("Important = false — lost in publication")
	}
	if got.WordCount == 0 {
		t.Error("WordCount = 0; it is counted from the body on load")
	}
}

// A file with no recorded slug is skipped rather than guessed: a chunk with no page is
// unreachable, and inventing a slug would collide with a real one.
func TestChunksWithNoSlugAreSkipped(t *testing.T) {
	dir := t.TempDir()
	c, err := NewWikiProcessCache(dir)
	if err != nil {
		t.Fatal(err)
	}
	c.Store("orphan.md", "h", []CachedChunk{chunk("Orphan", "no slug was ever assigned")})
	c.Store("named.md", "h2", []CachedChunk{chunk("Named", "this one has a page")})
	c.StoreSlug("named.md", "Named")
	if err := c.Save(); err != nil {
		t.Fatal(err)
	}

	chunks, _ := c.LoadAllChunks()
	if len(chunks) != 1 || chunks[0].Slug != "Named" {
		t.Fatalf("LoadAllChunks = %+v, want only the slugged entry", chunks)
	}
}

// An artifact can be published empty, and zero indexed chunks has to be reported as
// such rather than as success — an empty index answers every query with "no results"
// for a reason that has nothing to do with the query.
func TestBuildDBFromCacheReportsAnEmptyPublication(t *testing.T) {
	n, err := BuildDBFromCache(context.Background(), t.TempDir())
	if err != nil {
		t.Fatalf("an empty publication is not an error: %v", err)
	}
	if n != 0 {
		t.Errorf("indexed %d chunks from nothing, want 0", n)
	}
}

// ResetDir has to clear, not merge: a page its publisher deleted must not survive in
// the consumer, answering searches with documentation that no longer exists upstream.
func TestResetDirClearsWhatWasThere(t *testing.T) {
	dir := t.TempDir()
	stale := filepath.Join(dir, "Deleted_Upstream.md")
	if err := os.WriteFile(stale, []byte("# Gone\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	got, err := ResetDir(dir)
	if err != nil {
		t.Fatalf("ResetDir: %v", err)
	}
	if got != dir {
		t.Errorf("ResetDir = %q, want %q", got, dir)
	}
	if _, err := os.Stat(stale); !os.IsNotExist(err) {
		t.Error("a page the publisher deleted survived the reset")
	}
	if _, err := os.Stat(dir); err != nil {
		t.Errorf("the directory itself must remain, ready to receive: %v", err)
	}

	if _, err := ResetDir(""); err == nil {
		t.Error("ResetDir(\"\") succeeded; an empty path must be refused rather than clearing a guess")
	}
}
