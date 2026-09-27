//go:build lancedb

package wiki

import (
	"context"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/relations"
)

func relationRowsForTest(t *testing.T, db *WikiDB) []lancestore.Row {
	t.Helper()
	ctx := context.Background()
	table, err := db.store.OpenTable(ctx, relations.TableName)
	if err != nil {
		t.Fatal(err)
	}
	defer table.Close()
	rows, err := table.Rows(ctx)
	if err != nil {
		t.Fatal(err)
	}
	return rows
}

func countSourceRows(rows []lancestore.Row, id string) int {
	n := 0
	for _, row := range rows {
		if row["source_id"] == id {
			n++
		}
	}
	return n
}

func metaValueForTest(t *testing.T, dir, key string) string {
	t.Helper()
	ctx := context.Background()
	db, err := OpenWikiDB(ctx, dir)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	if err := db.ensureTables(ctx); err != nil {
		t.Fatal(err)
	}
	hits, err := db.meta.Search(ctx, lancestore.Query{Filter: "key = " + lanceQuote(key), Limit: 1})
	if err != nil {
		t.Fatal(err)
	}
	if len(hits) == 0 {
		return ""
	}
	return str(hits[0].Row["value"])
}

func TestWikiRelationSyncSkipsUnchangedSourcesAndPrunesHistory(t *testing.T) {
	ctx := context.Background()
	db := newWikiForTest(t)
	chunks := []WikiChunk{
		lanceChunk("a", "A", "", "A links to B."),
		lanceChunk("b", "B", "", "B has no links."),
	}
	xrefs := map[string][]string{"a": {"b"}}
	if err := db.Sync(ctx, chunks, xrefs, nil); err != nil {
		t.Fatal(err)
	}
	first := relationRowsForTest(t, db)
	if len(first) != 3 || countSourceRows(first, "b") != 1 {
		t.Fatalf("initial relation rows: total=%d b=%d", len(first), countSourceRows(first, "b"))
	}
	if err := db.Sync(ctx, chunks, xrefs, nil); err != nil {
		t.Fatal(err)
	}
	if got := relationRowsForTest(t, db); len(got) != len(first) {
		t.Fatalf("unchanged sync added relation generations: %d -> %d", len(first), len(got))
	}
	chunks[0] = lanceChunk("a", "A", "", "A still links to B, with new text.")
	chunks[0].ContentHash = "h-a-v2"
	if err := db.Sync(ctx, chunks, xrefs, nil); err != nil {
		t.Fatal(err)
	}
	beforePrune := relationRowsForTest(t, db)
	if countSourceRows(beforePrune, "a") != 4 || countSourceRows(beforePrune, "b") != 1 {
		t.Fatalf("changed source rows: a=%d b=%d", countSourceRows(beforePrune, "a"), countSourceRows(beforePrune, "b"))
	}
	// Simulate an existing store whose old maintenance timestamp is recent but
	// whose relation migration marker is missing.
	if err := db.meta.Upsert(ctx, "key", []lancestore.Row{{"key": "last_maintenance", "value": time.Now().UTC().Format(time.RFC3339Nano)}}); err != nil {
		t.Fatal(err)
	}
	if err := db.meta.DeleteByKey(ctx, "key", []string{relationMaintenanceKey}); err != nil {
		t.Fatal(err)
	}
	dir := filepath.Dir(db.DBPath())
	if err := EnsureReferences(ctx, dir); err != nil {
		t.Fatal(err)
	}
	marker := metaValueForTest(t, dir, relationMaintenanceKey)
	if marker == "" {
		t.Fatal("first no-change index did not record relation maintenance")
	}
	lastMaintenance := metaValueForTest(t, dir, "last_maintenance")
	if err := EnsureReferences(ctx, dir); err != nil {
		t.Fatal(err)
	}
	if got := metaValueForTest(t, dir, "last_maintenance"); got != lastMaintenance {
		t.Fatalf("immediate repeat ran maintenance again: %q -> %q", lastMaintenance, got)
	}
	pruned := relationRowsForTest(t, db)
	if len(pruned) != 3 || countSourceRows(pruned, "a") != 2 || countSourceRows(pruned, "b") != 1 {
		t.Fatalf("maintenance did not retain only current generations: total=%d a=%d b=%d", len(pruned), countSourceRows(pruned, "a"), countSourceRows(pruned, "b"))
	}
	storedXRefs, err := db.AllXRefs(ctx)
	if err != nil || len(storedXRefs["a"]) != 1 || storedXRefs["a"][0] != "b" {
		t.Fatalf("cross-references after maintenance: xrefs=%v err=%v", storedXRefs, err)
	}
	edges, complete, err := db.ReferenceEdges(ctx)
	if err != nil || !complete || len(edges) != 1 || edges[0].Source.ID != "a" || edges[0].Target.ID != "b" {
		t.Fatalf("references after maintenance: edges=%+v complete=%v err=%v", edges, complete, err)
	}
}

func TestWikiRelationMaintenanceRetriesAfterFailure(t *testing.T) {
	db := newWikiForTest(t)
	ctx := context.Background()
	if err := db.ensureTables(ctx); err != nil {
		t.Fatal(err)
	}
	if err := db.meta.Upsert(ctx, "key", []lancestore.Row{{"key": "last_maintenance", "value": time.Now().UTC().Format(time.RFC3339Nano)}}); err != nil {
		t.Fatal(err)
	}
	cancelled, cancel := context.WithCancel(ctx)
	cancel()
	db.maintainIfDue(cancelled)
	if got := metaValueForTest(t, filepath.Dir(db.DBPath()), relationMaintenanceKey); got != "" {
		t.Fatalf("failed maintenance was marked complete: %q", got)
	}
	db.maintainIfDue(ctx)
	if got := metaValueForTest(t, filepath.Dir(db.DBPath()), relationMaintenanceKey); got == "" {
		t.Fatal("successful retry did not record relation maintenance")
	}
}

func TestWikiMaintenanceKeepsReferencesWhenNewHeadWasNotCommitted(t *testing.T) {
	ctx := context.Background()
	db := newWikiForTest(t)
	chunks := []WikiChunk{
		lanceChunk("a", "A", "", "A links to B."),
		lanceChunk("b", "B", "", "B has no links."),
	}
	if err := db.Sync(ctx, chunks, map[string][]string{"a": {"b"}}, nil); err != nil {
		t.Fatal(err)
	}
	source := relations.Entity{Type: "knowledge", ID: "a"}
	if err := relations.Replace(ctx, db.store, source, time.Now().Add(time.Hour).UnixNano(), "record", nil, "uncommitted"); err != nil {
		t.Fatal(err)
	}
	if !db.Maintain(ctx) {
		t.Fatal("wiki maintenance failed")
	}
	edges, complete, err := db.ReferenceEdges(ctx)
	if err != nil || !complete || len(edges) != 1 || edges[0].Source.ID != "a" || edges[0].Target.ID != "b" {
		t.Fatalf("committed page references lost after incomplete write: edges=%+v complete=%v err=%v", edges, complete, err)
	}
}

func TestWikiMaintenanceAcceptsLegacyStoreWithoutRelationsTable(t *testing.T) {
	ctx := context.Background()
	db := newWikiForTest(t)
	if err := db.ensureTables(ctx); err != nil {
		t.Fatal(err)
	}
	db.Maintain(ctx)
	names, err := db.store.TableNames(ctx)
	if err != nil {
		t.Fatal(err)
	}
	for _, name := range names {
		if name == relations.TableName {
			t.Fatal("maintenance created an absent relations table")
		}
	}
}
