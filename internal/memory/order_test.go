package memory

import (
	"reflect"
	"testing"
)

func TestSortMemoryEntriesUsesPriorityThenNewestDate(t *testing.T) {
	entries := []MemoryEntry{
		{ID: "normal-new", UpdatedAt: "2026-09-06T12:00:00Z"},
		{ID: "important-old", Important: true, UpdatedAt: "2026-09-04T12:00:00Z"},
		{ID: "mandatory-old", Mandatory: true, UpdatedAt: "2026-09-03T12:00:00Z"},
		{ID: "important-new", Important: true, UpdatedAt: "2026-09-05T12:00:00Z"},
		{ID: "mandatory-new", Mandatory: true, UpdatedAt: "2026-09-06T12:00:00Z"},
		{ID: "normal-old", CreatedAt: "2026-09-01T12:00:00Z"},
	}

	SortMemoryEntries(entries)
	got := make([]string, len(entries))
	for i, entry := range entries {
		got[i] = entry.ID
	}
	want := []string{"mandatory-new", "mandatory-old", "important-new", "important-old", "normal-new", "normal-old"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("order = %v, want %v", got, want)
	}
}

func TestSortChainResultsIgnoresScoreAsAnOrderingKey(t *testing.T) {
	results := []ChainResult{
		{Path: "normal", Score: 100, UpdatedAt: "2026-09-06T12:00:00Z"},
		{Path: "important-old", Important: true, Score: 50, UpdatedAt: "2026-09-04T12:00:00Z"},
		{Path: "mandatory", Mandatory: true, Score: 1, UpdatedAt: "2026-09-01T12:00:00Z"},
		{Path: "important-new", Important: true, Score: 2, UpdatedAt: "2026-09-05T12:00:00Z"},
	}

	SortChainResults(results)
	got := []string{results[0].Path, results[1].Path, results[2].Path, results[3].Path}
	want := []string{"mandatory", "important-new", "important-old", "normal"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("order = %v, want %v", got, want)
	}
}

func TestPriorityLabelMakesMandatoryExclusiveOfImportant(t *testing.T) {
	if got := PriorityLabel(true, true); got != "mandatory" {
		t.Fatalf("PriorityLabel(true, true) = %q, want mandatory", got)
	}
}
