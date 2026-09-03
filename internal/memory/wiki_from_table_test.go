//go:build lancedb

package memory

import (
	"context"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

func tableWith(t *testing.T, records ...MemoryRecord) *MemoryTable {
	t.Helper()
	t.Setenv("GRAPHIT_HUB_BUCKET", "")
	tbl, err := OpenMemoryTable(context.Background(), filepath.Join(t.TempDir(), "table"))
	if err != nil {
		t.Fatalf("OpenMemoryTable: %v", err)
	}
	t.Cleanup(func() { _ = tbl.Close() })
	if err := tbl.Put(context.Background(), records...); err != nil {
		t.Fatalf("Put: %v", err)
	}
	return tbl
}

// A wiki compiled FROM THE TABLE must be indistinguishable from one compiled from files: same
// chunks, same chain columns. This is the seam T2.3 turns on, so it is proved before anything is
// wired to it.
func TestGenerateMemoryWikiFromTableCompilesTheChain(t *testing.T) {
	ctx := context.Background()
	const id = "01ABCDEFGHIJKLMNOPQRSTUVWX"
	const revID = "01ZZZZZZZZZZZZZZZZZZZZZZZZ"

	tbl := tableWith(t,
		MemoryRecord{
			ID: id, Title: "Current wording", Body: "the body as it stands now",
			Type: "decision", Important: true, CreatedAt: "2026-09-01T00:00:00Z",
			Revision: 2, Previous: HistoryPath(id, revID), ContentHash: "hash-live",
		},
		MemoryRecord{
			ID: id, RevisionID: revID, Superseded: true,
			Title: "Current wording", Body: "what it said before",
			Type: "decision", CreatedAt: "2026-08-01T00:00:00Z",
			Revision: 1, Next: MemoryFileName(id), ContentHash: "hash-rev",
		},
	)

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	res, err := GenerateMemoryWikiFromTable(ctx, tbl, wikiDir)
	if err != nil {
		t.Fatalf("GenerateMemoryWikiFromTable: %v", err)
	}
	if res.ArticlesWritten != 2 {
		t.Fatalf("ArticlesWritten = %d, want 2 — the live memory and its superseded revision", res.ArticlesWritten)
	}

	db, err := wiki.OpenWikiDB(ctx, wikiDir)
	if err != nil {
		t.Fatalf("OpenWikiDB: %v", err)
	}
	defer func() { _ = db.Close() }()

	chunks, err := db.Chunks(ctx)
	if err != nil {
		t.Fatalf("Chunks: %v", err)
	}
	if len(chunks) != 2 {
		t.Fatalf("indexed %d chunks, want 2", len(chunks))
	}

	var live, superseded *wiki.WikiChunk
	for i := range chunks {
		if chunks[i].Superseded {
			superseded = &chunks[i]
		} else {
			live = &chunks[i]
		}
	}
	if live == nil || superseded == nil {
		t.Fatal("the index does not hold one live chunk and one superseded chunk")
	}

	if live.EntityID != id || superseded.EntityID != id {
		t.Errorf("entity ids = %q and %q, want both %q", live.EntityID, superseded.EntityID, id)
	}
	if superseded.RevisionID != revID {
		t.Errorf("superseded revision_id = %q, want %q", superseded.RevisionID, revID)
	}
	if live.Previous != HistoryPath(id, revID) {
		t.Errorf("live previous = %q, want the archive path", live.Previous)
	}
	if superseded.Next != MemoryFileName(id) {
		t.Errorf("superseded next = %q, want the live file name", superseded.Next)
	}
	if superseded.CurrentID != id {
		t.Errorf("superseded current_id = %q, want %q", superseded.CurrentID, id)
	}
	if !live.Important {
		t.Error("the live memory lost its important flag")
	}

	if live.Slug == superseded.Slug {
		t.Fatalf("both revisions compiled to the same slug %q", live.Slug)
	}
}

// The incremental gate must work from rows: a second compile over an unchanged table changes
// nothing. It is FastPathCheck comparing content hashes against the index, which is why the
// stat-based gate the markdown path uses has no analogue here and is simply absent.
func TestGenerateMemoryWikiFromTableIsIncremental(t *testing.T) {
	ctx := context.Background()
	tbl := tableWith(t, MemoryRecord{
		ID: "01ABCDEFGHIJKLMNOPQRSTUVWX", Title: "Only one", Body: "a body",
		Type: "fact", CreatedAt: "2026-09-01T00:00:00Z", Revision: 1, ContentHash: "hash-a",
	})

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	first, err := GenerateMemoryWikiFromTable(ctx, tbl, wikiDir)
	if err != nil {
		t.Fatalf("first compile: %v", err)
	}
	if first.ArticlesWritten != 1 {
		t.Fatalf("first compile wrote %d, want 1", first.ArticlesWritten)
	}

	second, err := GenerateMemoryWikiFromTable(ctx, tbl, wikiDir)
	if err != nil {
		t.Fatalf("second compile: %v", err)
	}
	if second.ArticlesWritten != 0 {
		t.Errorf("second compile wrote %d, want 0 — nothing changed, so the gate must skip", second.ArticlesWritten)
	}
}

func TestGenerateMemoryWikiFromTableRecompilesAChangedRecord(t *testing.T) {
	ctx := context.Background()
	const id = "01ABCDEFGHIJKLMNOPQRSTUVWX"
	tbl := tableWith(t, MemoryRecord{
		ID: id, Title: "First wording", Body: "first body",
		Type: "fact", CreatedAt: "2026-09-01T00:00:00Z", Revision: 1, ContentHash: "hash-a",
	})

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	if _, err := GenerateMemoryWikiFromTable(ctx, tbl, wikiDir); err != nil {
		t.Fatalf("first compile: %v", err)
	}

	if err := tbl.Put(ctx, MemoryRecord{
		ID: id, Title: "Second wording", Body: "second body",
		Type: "fact", CreatedAt: "2026-09-01T00:00:00Z", Revision: 2, ContentHash: "hash-b",
	}); err != nil {
		t.Fatalf("Put: %v", err)
	}

	res, err := GenerateMemoryWikiFromTable(ctx, tbl, wikiDir)
	if err != nil {
		t.Fatalf("second compile: %v", err)
	}
	if res.ArticlesWritten != 1 {
		t.Fatalf("recompile wrote %d, want 1", res.ArticlesWritten)
	}

	db, err := wiki.OpenWikiDB(ctx, wikiDir)
	if err != nil {
		t.Fatalf("OpenWikiDB: %v", err)
	}
	defer func() { _ = db.Close() }()
	chunks, err := db.Chunks(ctx)
	if err != nil {
		t.Fatalf("Chunks: %v", err)
	}
	if len(chunks) != 1 {
		t.Fatalf("indexed %d chunks, want 1", len(chunks))
	}
	if chunks[0].Title != "Second wording" {
		t.Errorf("title = %q, want Second wording — the stale chunk survived", chunks[0].Title)
	}
}
