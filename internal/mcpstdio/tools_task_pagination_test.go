package mcpstdio

import (
	"context"
	"fmt"
	"strings"
	"testing"

	graphtask "github.com/graphit-labs/graphit-code/internal/task"
)

type fakeTaskSearcher struct {
	results []graphtask.SearchResult
	limits  []int
}

func (f *fakeTaskSearcher) Search(_ context.Context, _ string, limit int) ([]graphtask.SearchResult, error) {
	f.limits = append(f.limits, limit)
	if limit > len(f.results) {
		limit = len(f.results)
	}
	return append([]graphtask.SearchResult(nil), f.results[:limit]...), nil
}

func TestTaskSearchPaginationTraversesStableTopK(t *testing.T) {
	searcher := &fakeTaskSearcher{results: []graphtask.SearchResult{
		{ID: "tsk-0001", Title: "one"},
		{ID: "tsk-0002", Title: "two"},
		{ID: "tsk-0003", Title: "three"},
		{ID: "tsk-0004", Title: "four"},
		{ID: "tsk-0005", Title: "five"},
		{ID: "tsk-0006", Title: "outside top_k"},
	}}
	in := taskSearchInput{ProjectDir: "/project", Query: " shared work ", TopK: 5, PageSize: 2}

	var got []string
	for {
		page, err := paginateTaskSearch(context.Background(), searcher, in)
		if err != nil {
			t.Fatal(err)
		}
		for _, result := range page.Results {
			got = append(got, result.ID)
		}
		if page.NextCursor == "" {
			break
		}
		in.Cursor = page.NextCursor
	}
	want := []string{"tsk-0001", "tsk-0002", "tsk-0003", "tsk-0004", "tsk-0005"}
	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Fatalf("paginated IDs = %v; want %v", got, want)
	}
	if strings.Join(intStrings(searcher.limits), ",") != "3,5,5" {
		t.Fatalf("search limits = %v; want look-ahead prefixes [3 5 5]", searcher.limits)
	}
}

func TestTaskSearchPaginationRejectsInvalidOrMismatchedCursor(t *testing.T) {
	searcher := &fakeTaskSearcher{results: []graphtask.SearchResult{{ID: "tsk-0001"}, {ID: "tsk-0002"}}}
	first, err := paginateTaskSearch(context.Background(), searcher, taskSearchInput{ProjectDir: "/project", Query: "alpha", TopK: 2, PageSize: 1})
	if err != nil {
		t.Fatal(err)
	}
	if first.NextCursor == "" {
		t.Fatal("first page did not return a cursor")
	}
	for name, in := range map[string]taskSearchInput{
		"malformed": {ProjectDir: "/project", Query: "alpha", TopK: 2, PageSize: 1, Cursor: "not-a-cursor"},
		"query":     {ProjectDir: "/project", Query: "beta", TopK: 2, PageSize: 1, Cursor: first.NextCursor},
		"page size": {ProjectDir: "/project", Query: "alpha", TopK: 2, PageSize: 2, Cursor: first.NextCursor},
		"top k":     {ProjectDir: "/project", Query: "alpha", TopK: 3, PageSize: 1, Cursor: first.NextCursor},
	} {
		t.Run(name, func(t *testing.T) {
			if _, err := paginateTaskSearch(context.Background(), searcher, in); err == nil {
				t.Fatal("cursor mismatch was accepted")
			}
		})
	}
}

func intStrings(values []int) []string {
	out := make([]string, len(values))
	for i, value := range values {
		out[i] = fmt.Sprint(value)
	}
	return out
}
