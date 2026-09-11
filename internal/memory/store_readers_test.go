//go:build lancedb

package memory

import (
	"context"
	"path/filepath"
	"reflect"
	"testing"
)

func svcWithTable(t *testing.T, records ...MemoryRecord) *MemoryService {
	t.Helper()
	svc := &MemoryService{
		scope:    MemoryScopeProject,
		scopeID:  "test-id",
		tableURI: filepath.Join(t.TempDir(), "table"),
	}
	if len(records) > 0 {
		ctx := context.Background()
		tbl, err := svc.openTable(ctx)
		if err != nil {
			t.Fatalf("openTable: %v", err)
		}
		defer func() { _ = tbl.Close() }()
		if err := tbl.Put(ctx, records...); err != nil {
			t.Fatalf("Put: %v", err)
		}
	}
	return svc
}

func TestListMemoriesReturnsLiveMemoriesWithTheirClassification(t *testing.T) {
	svc := svcWithTable(t,
		MemoryRecord{
			ID: "MEM1", Title: "Normal Memory", Body: "a body", Type: "fact",
			CreatedAt: "2026-09-01T00:00:00Z", Revision: 1, ContentHash: "h1",
		},
		MemoryRecord{
			ID: "MEM2", Title: "Important Memory", Body: "another body", Type: "convention",
			Important: true, Tags: []string{"auth", "security"},
			CreatedAt: "2026-09-02T00:00:00Z", Revision: 1, ContentHash: "h2",
		},
		MemoryRecord{
			ID: "MEM1", RevisionID: "01REVISIONREVISIONREVISION", Superseded: true,
			Title: "Normal Memory", Body: "what it said before", Type: "fact",
			CreatedAt: "2026-08-01T00:00:00Z", Revision: 1, ContentHash: "h3",
		},
	)

	memories, err := svc.ListMemories()
	if err != nil {
		t.Fatalf("ListMemories: %v", err)
	}
	if len(memories) != 2 {
		t.Fatalf("got %d memories, want 2 — the superseded revision must not be catalogued", len(memories))
	}

	byID := map[string]MemoryEntry{}
	for _, m := range memories {
		byID[m.ID] = m
	}
	one, ok := byID["MEM1"]
	if !ok {
		t.Fatal("MEM1 is missing")
	}
	if one.Important {
		t.Error("MEM1 should not be important")
	}
	if one.Title != "Normal Memory" {
		t.Errorf("MEM1 title = %q", one.Title)
	}
	two, ok := byID["MEM2"]
	if !ok {
		t.Fatal("MEM2 is missing")
	}
	if !two.Important {
		t.Error("MEM2 should be important")
	}
	if two.Type != MemoryTypeConvention {
		t.Errorf("MEM2 type = %q, want convention — the type used to be unreadable from a listing", two.Type)
	}
	if len(two.Tags) != 2 {
		t.Errorf("MEM2 tags = %v, want two — tags used to be absent from a listing entirely", two.Tags)
	}
}

func TestListAndSearchMemoriesUsePriorityThenNewestDate(t *testing.T) {
	records := []MemoryRecord{
		{ID: "normal-new", Title: "Shared marker normal repeated repeated", Body: "shared-order-marker repeated repeated", UpdatedAt: "2026-09-06T12:00:00Z", CreatedAt: "2026-09-06T12:00:00Z"},
		{ID: "important-old", Title: "Shared marker important", Body: "shared-order-marker", Important: true, UpdatedAt: "2026-09-04T12:00:00Z", CreatedAt: "2026-09-04T12:00:00Z"},
		{ID: "mandatory-old", Title: "Shared marker mandatory old", Body: "shared-order-marker", Mandatory: true, UpdatedAt: "2026-09-03T12:00:00Z", CreatedAt: "2026-09-03T12:00:00Z"},
		{ID: "mandatory-new", Title: "Shared marker mandatory new", Body: "shared-order-marker", Mandatory: true, UpdatedAt: "2026-09-05T12:00:00Z", CreatedAt: "2026-09-05T12:00:00Z"},
	}
	svc := svcWithTable(t, records...)

	listed, err := svc.ListMemories()
	if err != nil {
		t.Fatal(err)
	}
	listedIDs := make([]string, len(listed))
	for i, entry := range listed {
		listedIDs[i] = entry.ID
	}
	wantList := []string{"mandatory-new", "mandatory-old", "important-old", "normal-new"}
	if !reflect.DeepEqual(listedIDs, wantList) {
		t.Fatalf("list order = %v, want %v", listedIDs, wantList)
	}

	searched, err := svc.SearchMemories(context.Background(), "shared-order-marker", 3, SearchOptions{})
	if err != nil {
		t.Fatal(err)
	}
	searchedIDs := make([]string, len(searched))
	for i, result := range searched {
		searchedIDs[i] = result.MemoryID
	}
	wantSearch := []string{"mandatory-new", "mandatory-old", "important-old"}
	if !reflect.DeepEqual(searchedIDs, wantSearch) {
		t.Fatalf("search order = %v, want %v", searchedIDs, wantSearch)
	}
}

func TestListMemoriesOnAnEmptyScope(t *testing.T) {
	svc := svcWithTable(t)
	memories, err := svc.ListMemories()
	if err != nil {
		t.Fatalf("ListMemories: %v", err)
	}
	if len(memories) != 0 {
		t.Errorf("got %d memories, want 0", len(memories))
	}
}

// IndexMemories maintains the scope's authoritative table in place.
func TestIndexMemoriesMaintainsTheAuthoritativeTable(t *testing.T) {
	svc := svcWithTable(t, MemoryRecord{
		ID: "01ABCDEFGHIJKLMNOPQRSTUVWX", Title: "Indexed", Body: "a body",
		Type: "fact", CreatedAt: "2026-09-01T00:00:00Z", Revision: 1, ContentHash: "h1",
	})
	if err := svc.IndexMemories(context.Background()); err != nil {
		t.Fatalf("IndexMemories: %v", err)
	}
	results, err := svc.SearchMemories(context.Background(), "Indexed", 10, SearchOptions{})
	if err != nil {
		t.Fatalf("SearchMemories: %v", err)
	}
	if len(results) != 1 || results[0].MemoryID != "01ABCDEFGHIJKLMNOPQRSTUVWX" {
		t.Fatalf("direct search = %+v, want the stored record", results)
	}
}
