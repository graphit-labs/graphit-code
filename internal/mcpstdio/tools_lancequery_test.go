package mcpstdio

import (
	"context"
	"fmt"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/lancequery"
)

// fakeStoreQuerier stands in for a real LanceDB store and, crucially, honours Limit and Offset
// the way the engine does: it slices. That is what lets the test prove the look-ahead row is
// requested once and consumed once.
type fakeStoreQuerier struct {
	rows   []map[string]any
	limits []int
}

func (f *fakeStoreQuerier) QueryStore(_ context.Context, req lancequery.Request) (lancequery.Result, error) {
	f.limits = append(f.limits, req.Limit)
	start := req.Offset
	if start > len(f.rows) {
		start = len(f.rows)
	}
	end := start + req.Limit
	if end > len(f.rows) {
		end = len(f.rows)
	}
	return lancequery.Result{
		Table: req.Table, Mode: "filter",
		Rows: append([]map[string]any(nil), f.rows[start:end]...),
	}, nil
}

func queryRows(n int) []map[string]any {
	out := make([]map[string]any, 0, n)
	for i := range n {
		out = append(out, map[string]any{"id": fmt.Sprintf("tsk-%04d", i), "status": "open"})
	}
	return out
}

// PG-T1: paging walks the whole set once, with no repeat and no gap, and the last page says so.
func TestTaskQueryPaginationWalksEveryRowExactlyOnce(t *testing.T) {
	store := &fakeStoreQuerier{rows: queryRows(7)}
	in := taskQueryInput{ProjectDir: "/project", Table: "tasks", Filter: "status = 'open'", PageSize: 3}

	var got []string
	pages := 0
	for {
		result, err := paginateTaskQuery(context.Background(), store, "/project", in)
		if err != nil {
			t.Fatal(err)
		}
		pages++
		for _, row := range result.Results {
			got = append(got, fmt.Sprint(row["id"]))
		}
		if result.NextCursor == "" {
			break
		}
		in.Cursor = result.NextCursor
		if pages > 10 {
			t.Fatal("pagination did not terminate")
		}
	}

	if pages != 3 {
		t.Fatalf("7 rows at page size 3 took %d pages, want 3", pages)
	}
	if len(got) != 7 {
		t.Fatalf("walked %d rows, want 7: %v", len(got), got)
	}
	seen := map[string]bool{}
	for i, id := range got {
		if seen[id] {
			t.Fatalf("row %s came back twice", id)
		}
		seen[id] = true
		if want := fmt.Sprintf("tsk-%04d", i); id != want {
			t.Fatalf("row %d is %s, want %s: a gap or a reorder", i, id, want)
		}
	}

	// Each request asks for one row beyond the page, and only one. Asking for PageSize would
	// make the last full page claim to be final; asking for PageSize+2 would waste a read.
	for i, limit := range store.limits {
		if limit != 4 {
			t.Fatalf("request %d asked for limit %d, want page size 3 plus one look-ahead", i, limit)
		}
	}
}

// PG-T2: a cursor belongs to the exact question that produced it.
func TestTaskQueryCursorIsRefusedByADifferentQuery(t *testing.T) {
	store := &fakeStoreQuerier{rows: queryRows(10)}
	base := taskQueryInput{ProjectDir: "/project", Table: "tasks", Filter: "status = 'open'", PageSize: 2}

	first, err := paginateTaskQuery(context.Background(), store, "/project", base)
	if err != nil {
		t.Fatal(err)
	}
	if first.NextCursor == "" {
		t.Fatal("expected a cursor")
	}

	for _, tc := range []struct {
		name  string
		alter func(in *taskQueryInput)
	}{
		{"different filter", func(in *taskQueryInput) { in.Filter = "status = 'completed'" }},
		{"different projection", func(in *taskQueryInput) { in.Columns = []string{"id"} }},
		{"different table", func(in *taskQueryInput) { in.Table = "task_checks" }},
		{"different top_k", func(in *taskQueryInput) { in.TopK = 5 }},
	} {
		t.Run(tc.name, func(t *testing.T) {
			altered := base
			altered.Cursor = first.NextCursor
			tc.alter(&altered)
			_, err := paginateTaskQuery(context.Background(), store, "/project", altered)
			if err == nil {
				t.Fatal("a cursor from another query was accepted")
			}
			if !strings.Contains(err.Error(), "cursor") {
				t.Fatalf("error should name the cursor, got: %v", err)
			}
		})
	}

	// The same question with the same cursor still works, so the binding is not simply
	// rejecting everything.
	same := base
	same.Cursor = first.NextCursor
	if _, err := paginateTaskQuery(context.Background(), store, "/project", same); err != nil {
		t.Fatalf("the cursor was refused by its own query: %v", err)
	}
}

// top_k caps the walk across pages, the way it does for task_search.
func TestTaskQueryTopKCapsTheTotalWalk(t *testing.T) {
	store := &fakeStoreQuerier{rows: queryRows(20)}
	in := taskQueryInput{ProjectDir: "/project", Table: "tasks", PageSize: 2, TopK: 5}

	total := 0
	for {
		result, err := paginateTaskQuery(context.Background(), store, "/project", in)
		if err != nil {
			t.Fatal(err)
		}
		total += len(result.Results)
		if result.NextCursor == "" {
			break
		}
		in.Cursor = result.NextCursor
		if total > 20 {
			t.Fatal("top_k did not cap the walk")
		}
	}
	if total != 5 {
		t.Fatalf("walked %d rows with top_k 5", total)
	}
}
