//go:build lancedb

package wiki

import (
	"context"
	"testing"
)

// FastPathCheck trusts only the compiled table's slug/hash projection. An empty table, a changed
// hash, an addition, and a deletion must all force a sync.
func TestFastPathCheckRequiresAnExactTableProjection(t *testing.T) {
	wikiDir := t.TempDir()
	ctx := context.Background()

	if FastPathCheck(ctx, wikiDir, nil) {
		t.Fatal("an empty index satisfied the fast path")
	}

	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		t.Fatalf("OpenWikiDB: %v", err)
	}
	if err := db.Sync(ctx, []WikiChunk{{
		Slug: "mem", Title: "Mem", Body: "indexed body", ContentHash: "h1",
	}}, nil, nil); err != nil {
		t.Fatalf("populating the index: %v", err)
	}
	if err := db.Close(); err != nil {
		t.Fatalf("closing wiki db: %v", err)
	}

	exact := []DocHashEntry{{ContentHash: "h1", Slug: "mem"}}
	if !FastPathCheck(ctx, wikiDir, exact) {
		t.Error("the exact slug/hash projection did not satisfy the fast path")
	}
	if FastPathCheck(ctx, wikiDir, []DocHashEntry{{ContentHash: "h2", Slug: "mem"}}) {
		t.Error("a changed hash satisfied the fast path")
	}
	if FastPathCheck(ctx, wikiDir, append(exact,
		DocHashEntry{ContentHash: "h2", Slug: "new"})) {
		t.Error("an added document satisfied the fast path")
	}
	if FastPathCheck(ctx, wikiDir, nil) {
		t.Error("a deleted document satisfied the fast path")
	}
}
