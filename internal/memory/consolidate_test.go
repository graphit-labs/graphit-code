package memory

import (
	"strings"
	"testing"
)

// A corpus that fits one prompt was fully compared, and must say nothing.
func TestCoverageNoteIsSilentWhenTheCorpusFitsOneBatch(t *testing.T) {
	r := &ConsolidationReport{Batches: 1}
	if note := r.CoverageNote(); note != "" {
		t.Errorf("a single-batch pass volunteered a caveat: %q", note)
	}
}

// A split corpus was NOT fully compared, and the report has to say so.
//
// This is the silent half of batching: two duplicates that land in different batches
// are never put in front of the model together, so it cannot notice them — and the
// report then says "nothing to do" about a pair it never looked at. That reads as
// completeness the pass did not earn.
func TestCoverageNoteStatesWhatASplitPassCouldNotCompare(t *testing.T) {
	r := &ConsolidationReport{Batches: 4}
	note := r.CoverageNote()
	if note == "" {
		t.Fatal("a 4-batch pass reported nothing about what it could not see")
	}
	for _, want := range []string{"4 batches", "never compared"} {
		if !strings.Contains(note, want) {
			t.Errorf("note does not mention %q: %s", want, note)
		}
	}
	// Worded as a limit on the pass, not a finding about the corpus.
	if strings.Contains(note, "no duplicates") {
		t.Errorf("the note claims something about the corpus it did not check: %s", note)
	}
}

// And it has to survive into the audit the developer actually reads.
func TestCoverageNoteReachesTheOutcomeMarkdown(t *testing.T) {
	o := &ConsolidationOutcome{
		Scope: "project", Analysed: 500,
		CoverageNote: "Analysed in 4 batches: ... never compared ...",
	}
	if !strings.Contains(o.Markdown(), "never compared") {
		t.Errorf("the coverage limit never reached the report:\n%s", o.Markdown())
	}
}

// mkVec builds a unit-ish vector pointing mostly along one axis, so "similar" is
// something the test states rather than hopes for.
func mkVec(axis int, dims int) []float32 {
	v := make([]float32, dims)
	v[axis] = 1
	return v
}

// The point of ordering: two memories about the same thing must end up ADJACENT, so
// that batching puts them in the same prompt. In arrival order they can sit at opposite
// ends of the corpus, never be compared, and their duplication cannot be noticed.
func TestOrderBySimilarityPutsNearDuplicatesNextToEachOther(t *testing.T) {
	mems := []memorySnapshot{
		{ID: "a1", Title: "indexing throughput"},
		{ID: "b1", Title: "unrelated: git remotes"},
		{ID: "c1", Title: "unrelated: terminal colours"},
		{ID: "d1", Title: "indexing throughput, again"}, // the duplicate of a1
	}
	vecs := map[string][]float32{
		"a1": mkVec(0, 8),
		"d1": mkVec(0, 8), // identical direction: the near-duplicate
		"b1": mkVec(3, 8),
		"c1": mkVec(6, 8),
	}

	ordered := orderBySimilarity(mems, vecs)
	if len(ordered) != len(mems) {
		t.Fatalf("ordering lost memories: got %d, want %d", len(ordered), len(mems))
	}

	pos := map[string]int{}
	for i, m := range ordered {
		pos[m.ID] = i
	}
	if gap := pos["d1"] - pos["a1"]; gap != 1 && gap != -1 {
		var ids []string
		for _, m := range ordered {
			ids = append(ids, m.ID)
		}
		t.Errorf("the duplicate pair is %d apart, want adjacent: %v", gap, ids)
	}
}

// Same input, same order, every time. A consolidation whose batching shifts between
// runs has coverage nobody can reason about.
func TestOrderBySimilarityIsDeterministic(t *testing.T) {
	mems := []memorySnapshot{
		{ID: "m3"}, {ID: "m1"}, {ID: "m4"}, {ID: "m2"},
	}
	vecs := map[string][]float32{
		"m1": mkVec(0, 8), "m2": mkVec(1, 8), "m3": mkVec(2, 8), "m4": mkVec(3, 8),
	}

	first := orderBySimilarity(mems, vecs)
	for i := 0; i < 5; i++ {
		again := orderBySimilarity(mems, vecs)
		for j := range first {
			if first[j].ID != again[j].ID {
				t.Fatalf("run %d ordered differently at %d: %s vs %s", i, j, first[j].ID, again[j].ID)
			}
		}
	}
}

// A scope with no embeddings yet is a real state — freshly written, not embedded — and
// it has to degrade to the previous behaviour instead of losing memories.
func TestOrderBySimilarityWithoutVectorsKeepsEverythingInOrder(t *testing.T) {
	mems := []memorySnapshot{{ID: "a"}, {ID: "b"}, {ID: "c"}, {ID: "d"}}

	got := orderBySimilarity(mems, nil)
	if len(got) != len(mems) {
		t.Fatalf("lost memories with no vectors: %d vs %d", len(got), len(mems))
	}
	for i := range mems {
		if got[i].ID != mems[i].ID {
			t.Errorf("order changed with nothing to order by: %v", got)
			break
		}
	}
}

// A partially embedded corpus must not drop the unembedded half.
func TestOrderBySimilarityKeepsMemoriesThatHaveNoVector(t *testing.T) {
	mems := []memorySnapshot{{ID: "a"}, {ID: "b"}, {ID: "c"}, {ID: "d"}}
	vecs := map[string][]float32{"a": mkVec(0, 8), "b": mkVec(1, 8), "c": mkVec(2, 8)}

	got := orderBySimilarity(mems, vecs)
	seen := map[string]bool{}
	for _, m := range got {
		seen[m.ID] = true
	}
	for _, id := range []string{"a", "b", "c", "d"} {
		if !seen[id] {
			t.Errorf("memory %q disappeared from the ordering", id)
		}
	}
}

// The wiki names a memory `<ID>_…​.md`; the ID is what joins a vector to a snapshot.
func TestMemoryIDIsRecoveredFromTheWikiSourceName(t *testing.T) {
	cases := map[string]string{
		"01M04E0YB97A79ZV42QSNKDNWF_important_.md": "01M04E0YB97A79ZV42QSNKDNWF",
		"/abs/path/01ABC_plain_.md":                "01ABC",
		"01XYZ.md":                                 "01XYZ",
	}
	for source, want := range cases {
		if got := memoryIDFromSource(source); got != want {
			t.Errorf("memoryIDFromSource(%q) = %q, want %q", source, got, want)
		}
	}
}
