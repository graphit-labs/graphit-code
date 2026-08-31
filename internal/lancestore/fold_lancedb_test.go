//go:build lancedb

package lancestore

import (
	"context"
	"testing"
)

// MEASURED: a row appended AFTER the inverted index was built is found by full-text search
// WITHOUT any fold. The engine scans the unindexed fragments alongside the index.
//
// This test exists because the opposite is the intuitive belief, and acting on it would have
// made FoldNewRowsIntoIndexes a correctness requirement on the incremental path — a fold that
// must never be skipped, ordered before every read. It is not: it is a latency measure. Keeping
// the probe means the day the engine changes this, the design is corrected by a failing test
// rather than by an outage.
func TestFoldIsAboutLatencyNotVisibility(t *testing.T) {
	ctx := context.Background()
	st := openLocal(t)

	schema := Schema{Fields: []Field{
		{Name: "uid", Type: FieldString},
		{Name: "body", Type: FieldString},
	}}
	tbl, err := st.CreateTable(ctx, "fold", schema)
	if err != nil {
		t.Fatalf("create: %v", err)
	}

	// Enough rows that the engine builds a real index rather than declining.
	seed := make([]Row, 0, 64)
	for i := 0; i < 64; i++ {
		seed = append(seed, Row{"uid": string(rune('a'+i%26)) + itoa(i), "body": "filler unrelated text"})
	}
	if err := tbl.Append(ctx, seed); err != nil {
		t.Fatalf("seed: %v", err)
	}
	if err := tbl.EnsureIndexes(ctx, Index{Column: "body", Kind: IndexInvertedText}); err != nil {
		t.Fatalf("index: %v", err)
	}

	find := func(what string) int {
		hits, err := tbl.Search(ctx, Query{Text: what, TextColumn: "body", Limit: 10})
		if err != nil {
			t.Fatalf("search %q: %v", what, err)
		}
		return len(hits)
	}

	if n := find("filler"); n == 0 {
		t.Fatal("the indexed rows are not findable at all — the index did not build")
	}

	// The new row uses a term that appears nowhere in the seed.
	if err := tbl.Append(ctx, []Row{{"uid": "new-1", "body": "quokka appears exactly once"}}); err != nil {
		t.Fatalf("append: %v", err)
	}

	before := find("quokka")
	if err := tbl.FoldNewRowsIntoIndexes(ctx); err != nil {
		t.Fatalf("fold: %v", err)
	}
	after := find("quokka")

	t.Logf("full-text hits for the appended row: before fold = %d, after fold = %d", before, after)
	if after == 0 {
		t.Error("after folding, the appended row is not found by full-text search — " +
			"folding must never LOSE a row")
	}
	if before == 0 {
		t.Error("the appended row was invisible before the fold. That is the belief this test " +
			"was written to refute, so the incremental path's assumptions must be revisited: " +
			"the fold becomes mandatory before any read, not a latency measure.")
	}
}

func itoa(i int) string {
	if i == 0 {
		return "0"
	}
	var b []byte
	for i > 0 {
		b = append([]byte{byte('0' + i%10)}, b...)
		i /= 10
	}
	return string(b)
}

// A vector written as []float32 must READ BACK as []float32.
//
// It does not, natively: the Arrow-to-Go bridge hands a fixed-size list back as []interface{} of
// float64, so `v.([]float32)` fails — and a two-value type assertion does not error, it yields
// nil. That produced a real symptom: the wiki's StoredEmbeddings returned an empty list while
// EmbeddingStats counted the very same rows as embedded, because one asked the engine and the
// other asked Go. Table.normalizeRead closes it, and this test is what keeps it closed.
func TestVectorColumnRoundTripsAsFloat32(t *testing.T) {
	ctx := context.Background()
	st := openLocal(t)

	const dim = 8
	tbl, err := st.CreateTable(ctx, "vecs", Schema{Fields: []Field{
		{Name: "uid", Type: FieldString},
		{Name: "v", Type: FieldVector, Dim: dim},
	}})
	if err != nil {
		t.Fatalf("create: %v", err)
	}

	want := make([]float32, dim)
	for i := range want {
		want[i] = float32(i) / 4
	}
	if err := tbl.Append(ctx, []Row{{"uid": "a", "v": want}}); err != nil {
		t.Fatalf("append: %v", err)
	}

	hits, err := tbl.Search(ctx, Query{Filter: "uid = 'a'", Limit: 1})
	if err != nil {
		t.Fatalf("read back: %v", err)
	}
	if len(hits) != 1 {
		t.Fatalf("got %d rows, want 1", len(hits))
	}

	got, ok := hits[0].Row["v"].([]float32)
	if !ok {
		t.Fatalf("the vector came back as %T, not []float32 — every caller asserting the "+
			"written type gets nil, silently", hits[0].Row["v"])
	}
	if len(got) != dim {
		t.Fatalf("vector width %d, want %d", len(got), dim)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("element %d = %v, want %v", i, got[i], want[i])
		}
	}
}

func TestNullableVectorPreservesNullAndValue(t *testing.T) {
	ctx := context.Background()
	st := openLocal(t)

	const dim = 8
	tbl, err := st.CreateTable(ctx, "nullable_vecs", Schema{Fields: []Field{
		{Name: "uid", Type: FieldString},
		{Name: "v", Type: FieldVector, Dim: dim, Nullable: true},
	}})
	if err != nil {
		t.Fatalf("create: %v", err)
	}
	want := make([]float32, dim)
	for i := range want {
		want[i] = float32(i) / 8
	}
	if err := tbl.Append(ctx, []Row{
		{"uid": "missing", "v": nil},
		{"uid": "present", "v": want},
	}); err != nil {
		t.Fatalf("append: %v", err)
	}

	missing, err := tbl.Search(ctx, Query{Filter: "uid = 'missing'", Limit: 1})
	if err != nil || len(missing) != 1 {
		t.Fatalf("read null: hits=%d err=%v", len(missing), err)
	}
	if got := missing[0].Row["v"]; got != nil {
		t.Fatalf("null vector came back as %#v", got)
	}
	present, err := tbl.Search(ctx, Query{Filter: "uid = 'present'", Limit: 1})
	if err != nil || len(present) != 1 {
		t.Fatalf("read value: hits=%d err=%v", len(present), err)
	}
	got, ok := present[0].Row["v"].([]float32)
	if !ok || len(got) != dim {
		t.Fatalf("vector came back as %T with width %d", present[0].Row["v"], len(got))
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("element %d = %v, want %v", i, got[i], want[i])
		}
	}
}

func BenchmarkRecordOfAllNullVectors(b *testing.B) {
	const (
		dim  = 768
		rows = 2_000
	)
	schema := Schema{Fields: []Field{
		{Name: "uid", Type: FieldString},
		{Name: "v", Type: FieldVector, Dim: dim, Nullable: true},
	}}
	batch := make([]Row, rows)
	for i := range batch {
		batch[i] = Row{"uid": itoa(i), "v": nil}
	}

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		rec, err := recordOf(schema, batch)
		if err != nil {
			b.Fatal(err)
		}
		rec.Release()
	}
}
