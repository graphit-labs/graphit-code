package memory

import (
	"context"
	"path/filepath"
	"testing"
)

// svcWithTable is a service whose store is a temporary local table.
//
// It sets `tableURI` explicitly, which is what that field is FOR: without it the service derives the
// machine's real store location, and a test would write into it.
func svcWithTable(t *testing.T, records ...MemoryRecord) *MemoryService {
	t.Helper()
	t.Setenv("GRAPHIT_HUB_BUCKET", "")
	svc := &MemoryService{
		scope:    MemoryScopeProject,
		scopeID:  "test-id",
		wikiDir:  filepath.Join(t.TempDir(), "wiki"),
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

// ListMemories is the catalogue of LIVE memories, with their classification.
//
// This replaces three directory-shaped tests. What they actually proved survives — the flag, the
// title and the id come back per memory, and an archived revision is not catalogued — while their
// subjects did not: there is no directory to be missing, and no `.md` extension or subdirectory to
// skip. What DID need proving instead is that a superseded row is excluded, which the directory
// version could not check because history lived in a subdirectory it skipped for a different reason.
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

// An empty scope lists nothing and is not an error. A scope with no memories is the normal state of
// a fresh project, and it used to be expressed as a missing directory.
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

// IndexMemories compiles the scope's wiki from its store.
func TestIndexMemoriesCompilesFromTheStore(t *testing.T) {
	svc := svcWithTable(t, MemoryRecord{
		ID: "01ABCDEFGHIJKLMNOPQRSTUVWX", Title: "Indexed", Body: "a body",
		Type: "fact", CreatedAt: "2026-09-01T00:00:00Z", Revision: 1, ContentHash: "h1",
	})
	if err := svc.IndexMemories(context.Background()); err != nil {
		t.Fatalf("IndexMemories: %v", err)
	}
	live, superseded, _ := indexedMemoryPages(t, svc.WikiDir())
	if live != 1 || superseded != 0 {
		t.Errorf("indexed %d live and %d superseded, want 1 and 0", live, superseded)
	}
}

// RunCycle compiles a scope named by its table URI.
func TestRunCycleCompilesFromTheTableURI(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "")
	ctx := context.Background()
	uri := filepath.Join(t.TempDir(), "table")

	tbl, err := OpenMemoryTable(ctx, uri)
	if err != nil {
		t.Fatalf("OpenMemoryTable: %v", err)
	}
	if err := tbl.Put(ctx, MemoryRecord{
		ID: "01ABCDEFGHIJKLMNOPQRSTUVWX", Title: "Cycled", Body: "a body",
		Type: "fact", CreatedAt: "2026-09-01T00:00:00Z", Revision: 1, ContentHash: "h1",
	}); err != nil {
		t.Fatalf("Put: %v", err)
	}
	_ = tbl.Close()

	res := RunCycle(ctx, "test", uri, filepath.Join(t.TempDir(), "wiki"))
	if res.Err != nil {
		t.Fatalf("RunCycle: %v", res.Err)
	}
	if res.WikiFiles != 1 {
		t.Errorf("WikiFiles = %d, want 1", res.WikiFiles)
	}
}

// An empty URI is "this scope has no identity yet", and it compiles nothing without erroring —
// which is what TableURIForScope returns for a scope whose id cannot be resolved.
func TestRunCycleWithNoURICompilesNothing(t *testing.T) {
	res := RunCycle(context.Background(), "test", "", filepath.Join(t.TempDir(), "wiki"))
	if res.Err != nil {
		t.Errorf("an empty URI must not error: %v", res.Err)
	}
	if res.WikiFiles != 0 {
		t.Errorf("WikiFiles = %d, want 0", res.WikiFiles)
	}
}

// A URI that cannot be opened IS an error, and this is a deliberate change of behaviour.
//
// The raw-store version treated a missing directory as "nothing to compile" and returned no error.
// Opening a table CREATES it, so a path that cannot be opened is not an empty scope — it is a broken
// location, and reporting it as success would hide the case where a whole scope silently stops
// compiling.
func TestRunCycleReportsAnUnopenableURI(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "")
	res := RunCycle(context.Background(), "test", "/proc/cannot-create-here", filepath.Join(t.TempDir(), "wiki"))
	if res.Err == nil {
		t.Error("an unopenable store location must be reported, not treated as an empty scope")
	}
}
