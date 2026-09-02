package pagination

import (
	"reflect"
	"testing"
)

func TestPagesRespectTotalCap(t *testing.T) {
	all := []int{0, 1, 2, 3, 4, 5, 6}
	var cursor string
	var got []int
	for {
		window, err := Open(Spec{PageSize: 2, Cursor: cursor, Total: 5, Bind: map[string]any{"q": "x"}})
		if err != nil {
			t.Fatal(err)
		}
		prefix := all
		if len(prefix) > window.FetchLimit {
			prefix = prefix[:window.FetchLimit]
		}
		page := Finish(window, prefix)
		got = append(got, page.Results...)
		cursor = page.NextCursor
		if cursor == "" {
			break
		}
	}
	if want := []int{0, 1, 2, 3, 4}; !reflect.DeepEqual(got, want) {
		t.Fatalf("got %v, want %v", got, want)
	}
}

func TestUnlimitedPagesReachEndWithoutGaps(t *testing.T) {
	all := []string{"a", "b", "c", "d", "e"}
	var cursor string
	var got []string
	for {
		window, err := Open(Spec{PageSize: 2, Cursor: cursor, Bind: "same"})
		if err != nil {
			t.Fatal(err)
		}
		prefix := all
		if len(prefix) > window.FetchLimit {
			prefix = prefix[:window.FetchLimit]
		}
		page := Finish(window, prefix)
		got = append(got, page.Results...)
		cursor = page.NextCursor
		if cursor == "" {
			break
		}
	}
	if !reflect.DeepEqual(got, all) {
		t.Fatalf("got %v, want %v", got, all)
	}
}

func TestCursorBindsRequestAndPageSize(t *testing.T) {
	w, err := Open(Spec{PageSize: 2, Total: 5, Bind: "one"})
	if err != nil {
		t.Fatal(err)
	}
	cursor := Finish(w, []int{1, 2, 3}).NextCursor
	for _, spec := range []Spec{
		{PageSize: 2, Cursor: cursor, Total: 5, Bind: "two"},
		{PageSize: 3, Cursor: cursor, Total: 5, Bind: "one"},
		{PageSize: 2, Cursor: cursor, Total: 6, Bind: "one"},
	} {
		if _, err := Open(spec); err == nil {
			t.Fatalf("Open(%+v) accepted a cursor for another result set", spec)
		}
	}
}

func TestFinishFetchedUsesIteratorWindow(t *testing.T) {
	w, err := Open(Spec{PageSize: 2, Bind: "query"})
	if err != nil {
		t.Fatal(err)
	}
	first := FinishFetched(w, []int{10, 11, 12})
	if !reflect.DeepEqual(first.Results, []int{10, 11}) || first.NextCursor == "" {
		t.Fatalf("unexpected first page: %+v", first)
	}
	w, err = Open(Spec{PageSize: 2, Cursor: first.NextCursor, Bind: "query"})
	if err != nil {
		t.Fatal(err)
	}
	last := FinishFetched(w, []int{12})
	if !reflect.DeepEqual(last.Results, []int{12}) || last.NextCursor != "" {
		t.Fatalf("unexpected last page: %+v", last)
	}
}

func TestFinishFetchedEmptyResultsMarshalAsArray(t *testing.T) {
	w, err := Open(Spec{PageSize: 2, Bind: "empty"})
	if err != nil {
		t.Fatal(err)
	}
	page := FinishFetched[int](w, nil)
	if page.Results == nil {
		t.Fatal("empty results must be an empty array, not null")
	}
}

func TestValidation(t *testing.T) {
	for _, spec := range []Spec{
		{PageSize: -1},
		{PageSize: MaxPageSize + 1},
		{Total: -1},
		{Cursor: "not-a-cursor"},
	} {
		if _, err := Open(spec); err == nil {
			t.Fatalf("Open(%+v) unexpectedly succeeded", spec)
		}
	}
}

func TestLargeTotalDefaultsToMaximumPageSize(t *testing.T) {
	w, err := Open(Spec{Total: 250, DefaultPageSize: 250, Bind: "large"})
	if err != nil {
		t.Fatal(err)
	}
	if w.PageSize != MaxPageSize || w.FetchLimit != MaxPageSize+1 {
		t.Fatalf("window = %+v, want page=%d fetch=%d", w, MaxPageSize, MaxPageSize+1)
	}
}
