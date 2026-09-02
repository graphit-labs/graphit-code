package wiki

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

// A consumer can receive a STALE index alongside fresh shards: a publish that stopped carrying the
// index does not delete the copy an earlier, buggy publish already put on the remote. The rebuild
// must answer from the SHARDS regardless, or the fix to IsDerivedFile would trade published bytes
// for served staleness.
func TestStaleIndexIsOverriddenByTheShards(t *testing.T) {
	dir := t.TempDir()

	write := func(hash, title, body string) {
		pc, err := NewWikiProcessCache(dir)
		if err != nil {
			t.Fatal(err)
		}
		pc.Store("docs/a.md", hash, []CachedChunk{{
			Title: title, Body: body, ContentHash: ContentHash([]byte(body)),
		}})
		pc.StoreSlug("docs/a.md", "A_Page")
		if err := pc.Save(); err != nil {
			t.Fatal(err)
		}
		_ = pc.Close()
	}

	write("h-old", "STALE TITLE", "stale body")
	if _, err := BuildDBFromCache(context.Background(), dir); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(dir, WikiIndexDirName)); err != nil {
		t.Fatalf("no index was built, so the fixture proves nothing: %v", err)
	}

	// New shards, same slug, index deliberately left in place.
	write("h-new", "FRESH TITLE", "fresh body")
	if _, err := BuildDBFromCache(context.Background(), dir); err != nil {
		t.Fatal(err)
	}

	db, err := OpenWikiDB(context.Background(), dir)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = db.Close() }()
	c, err := db.Chunk(context.Background(), "A_Page")
	if err != nil {
		t.Fatalf("page missing after rebuild: %v", err)
	}
	if c == nil {
		t.Fatal("page missing after rebuild")
	}
	if c.Title != "FRESH TITLE" {
		t.Errorf("title = %q, want FRESH TITLE — the stale index survived the rebuild", c.Title)
	}
}
