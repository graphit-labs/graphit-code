//go:build lancedb

package memory

import (
	"context"
	"path/filepath"
	"testing"
)

func tableWith(t *testing.T, records ...MemoryRecord) *MemoryTable {
	t.Helper()
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

func TestAuthoritativeTableSearchesCurrentAndHistoricalRecords(t *testing.T) {
	ctx := context.Background()
	const id = "01ABCDEFGHIJKLMNOPQRSTUVWX"
	const revID = "01ZZZZZZZZZZZZZZZZZZZZZZZZ"
	tbl := tableWith(t,
		MemoryRecord{ID: id, Title: "Current wording", Body: "neotericum marker", Type: "decision", Revision: 2},
		MemoryRecord{ID: id, RevisionID: revID, Superseded: true, Title: "Previous wording", Body: "paleolithicum marker", Type: "decision", Revision: 1},
	)

	current, err := tbl.SearchChains(ctx, "neotericum", 10, SearchOptions{})
	if err != nil {
		t.Fatalf("searching current record: %v", err)
	}
	if len(current) != 1 || current[0].Path != id || current[0].Superseded {
		t.Fatalf("current search = %+v", current)
	}

	history, err := tbl.SearchChains(ctx, "paleolithicum", 10, SearchOptions{})
	if err != nil {
		t.Fatalf("searching historical record: %v", err)
	}
	if len(history) != 1 || history[0].Path != id+"/"+revID || !history[0].Superseded || history[0].Current != id {
		t.Fatalf("history search = %+v", history)
	}
}

func TestAuthoritativeTableSearchCollapsesARevisionChain(t *testing.T) {
	ctx := context.Background()
	const id = "01ABCDEFGHIJKLMNOPQRSTUVWX"
	tbl := tableWith(t,
		MemoryRecord{ID: id, Title: "Current wording", Body: "shared-chain-marker now", Revision: 2},
		MemoryRecord{ID: id, RevisionID: "old", Superseded: true, Title: "Previous wording", Body: "shared-chain-marker before", Revision: 1},
	)

	results, err := tbl.SearchChains(ctx, "shared-chain-marker", 10, SearchOptions{})
	if err != nil {
		t.Fatalf("SearchChains: %v", err)
	}
	if len(results) != 1 || results[0].MemoryID != id || results[0].Superseded {
		t.Fatalf("collapsed search = %+v, want the current head only", results)
	}
}
