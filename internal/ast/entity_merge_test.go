package ast

import (
	"strconv"
	"testing"
	"time"
)

// The merge exists because two queries legitimately match the same node: one for
// the declaration, one to reach the value it declares. Indexing must not change
// that verdict.
func TestAddOrMergeEntityMergesTheSameNode(t *testing.T) {
	pf := &ParsedFile{}
	pf.AddOrMergeEntity("constants", Entity{
		GraphLabel: "Constant", Name: "Endpoint", Context: "cfg", Line: 7,
		Docstring: "the base url",
	})
	pf.AddOrMergeEntity("constants", Entity{
		GraphLabel: "Constant", Name: "Endpoint", Context: "cfg", Line: 7,
		ContextType: "File",
		Args:        []string{"a"},
		Properties:  map[string]string{"value": "https://acme"},
	})

	got := pf.GetEntities("constants")
	if len(got) != 1 {
		t.Fatalf("the same node matched twice must stay one entity, got %d", len(got))
	}
	if got[0].Docstring != "the base url" {
		t.Errorf("the first match's docstring was lost: %q", got[0].Docstring)
	}
	if got[0].ContextType != "File" || len(got[0].Args) != 1 {
		t.Errorf("the second match did not fill what the first lacked: %+v", got[0])
	}
	if got[0].Properties["value"] != "https://acme" {
		t.Errorf("properties were not merged: %v", got[0].Properties)
	}
}

// Name and line alone would merge the two 1s of {"a": 1, "b": 1}; context is what
// keeps them apart.
func TestAddOrMergeEntityKeepsEqualValuesOfDifferentKeys(t *testing.T) {
	pf := &ParsedFile{}
	pf.AddOrMergeEntity("values", Entity{GraphLabel: "Value", Name: "1", Context: "a", Line: 3})
	pf.AddOrMergeEntity("values", Entity{GraphLabel: "Value", Name: "1", Context: "b", Line: 3})

	if got := pf.GetEntities("values"); len(got) != 2 {
		t.Errorf("values of different keys must stay apart, got %d entities", len(got))
	}
}

// AddEntity appends without merging, and a later merge has to see what it added.
func TestAddOrMergeEntitySeesWhatAddEntityAdded(t *testing.T) {
	pf := &ParsedFile{}
	pf.AddOrMergeEntity("functions", Entity{GraphLabel: "Function", Name: "A", Line: 1})
	pf.AddEntity("functions", Entity{GraphLabel: "Function", Name: "B", Line: 2})
	pf.AddOrMergeEntity("functions", Entity{GraphLabel: "Function", Name: "B", Line: 2, Docstring: "second match"})

	got := pf.GetEntities("functions")
	if len(got) != 2 {
		t.Fatalf("expected 2 entities, got %d: %+v", len(got), got)
	}
	if got[1].Docstring != "second match" {
		t.Errorf("the merge did not reach the entity AddEntity had appended: %+v", got[1])
	}
}

// A ParsedFile can arrive with Entities already populated; the index is built on
// demand and must find what was there before it existed.
func TestAddOrMergeEntityWithPrePopulatedEntities(t *testing.T) {
	pf := &ParsedFile{Entities: map[string][]Entity{
		"functions": {{GraphLabel: "Function", Name: "A", Line: 1}},
	}}
	pf.AddOrMergeEntity("functions", Entity{GraphLabel: "Function", Name: "A", Line: 1, Docstring: "filled in"})

	got := pf.GetEntities("functions")
	if len(got) != 1 {
		t.Fatalf("expected the pre-existing entity to be merged, got %d", len(got))
	}
	if got[0].Docstring != "filled in" {
		t.Errorf("merge missed the pre-populated entity: %+v", got[0])
	}
}

// The regression that froze a 36k-file index. The scan this replaced was O(n) per
// insert, so a file with this many entities cost ~5e9 comparisons and one worker
// span for minutes with no I/O and no subprocess -- indistinguishable from a hang.
// Linear behaviour finishes in milliseconds; the ceiling is generous on purpose so
// a slow shared runner cannot make it flaky, while quadratic cannot fit under it.
func TestAddOrMergeEntityIsNotQuadratic(t *testing.T) {
	const n = 100_000
	pf := &ParsedFile{}

	start := time.Now()
	for i := 0; i < n; i++ {
		pf.AddOrMergeEntity("values", Entity{
			GraphLabel: "Value",
			Name:       "v" + strconv.Itoa(i),
			Context:    "root",
			Line:       i,
		})
	}
	elapsed := time.Since(start)

	if got := len(pf.GetEntities("values")); got != n {
		t.Fatalf("expected %d distinct entities, got %d", n, got)
	}
	if elapsed > 15*time.Second {
		t.Errorf("%d merges took %v: the identity lookup is not O(1)", n, elapsed)
	}
	t.Logf("%d merges in %v", n, elapsed)
}
