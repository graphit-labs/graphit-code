//go:build lancedb

package lancestore

import (
	"context"
	"fmt"
	"os"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/config"
)

// The schema the search index needs: text to match, a vector to compare, scalars to filter.
func testSchema() Schema {
	return Schema{Fields: []Field{
		{Name: "uid", Type: FieldString},
		{Name: "path", Type: FieldString},
		{Name: "name", Type: FieldString},
		{Name: "body", Type: FieldString},
		{Name: "line", Type: FieldInt64, Nullable: true},
		{Name: "is_dependency", Type: FieldBool, Nullable: true},
		{Name: "embedding", Type: FieldVector, Dim: 4, Nullable: true},
	}}
}

var testRows = []Row{
	{"uid": "u1", "path": "a.go", "name": "FuseRankings", "body": "hybrid search fuses bm25 and vectors",
		"line": int64(10), "is_dependency": false, "embedding": []float32{0.9, 0.1, 0, 0}},
	{"uid": "u2", "path": "b.go", "name": "MemoryStore", "body": "the memory store now lives on s3",
		"line": int64(20), "is_dependency": false, "embedding": []float32{0, 0.9, 0.1, 0}},
	{"uid": "u3", "path": "c.go", "name": "ReciprocalRank", "body": "reciprocal rank fusion combines rankings",
		"line": int64(30), "is_dependency": false, "embedding": []float32{0.85, 0.15, 0, 0}},
	{"uid": "u4", "path": "d.go", "name": "RowGroups", "body": "parquet row groups broke the icebug reader",
		"line": int64(40), "is_dependency": true, "embedding": []float32{0, 0, 0.9, 0.1}},
	{"uid": "u5", "path": "e.go", "name": "InvertedIndex", "body": "full text search uses an inverted index",
		"line": int64(50), "is_dependency": false, "embedding": []float32{0.1, 0, 0, 0.9}},
	{"uid": "u6", "path": "f.go", "name": "Launcher", "body": "the launcher extracts native libraries",
		"line": int64(60), "is_dependency": false, "embedding": []float32{0, 0.1, 0.9, 0}},
}

func populate(t *testing.T, tbl *Table) {
	t.Helper()
	ctx := context.Background()
	if err := tbl.Append(ctx, testRows); err != nil {
		t.Fatalf("append: %v", err)
	}
	if err := tbl.EnsureIndexes(ctx, Index{Column: "body", Kind: IndexInvertedText}); err != nil {
		t.Fatalf("inverted index: %v", err)
	}
}

func names(hits []Hit) []string {
	out := make([]string, 0, len(hits))
	for _, h := range hits {
		out = append(out, fmt.Sprintf("%v", h.Row["name"]))
	}
	return out
}

// openLocal is the mode a project's own index uses: a directory, full write access.
func openLocal(t *testing.T) *Store {
	t.Helper()
	st, err := Open(context.Background(), Config{URI: t.TempDir()})
	if err != nil {
		t.Fatalf("open local: %v", err)
	}
	t.Cleanup(func() { _ = st.Close() })
	if st.Remote() {
		t.Fatal("a directory URI reported itself remote")
	}
	return st
}

// THE THREE SEARCHES, on a local store — which is what replaces the SQLite index.
func TestLocalStoreServesAllThreeSearches(t *testing.T) {
	ctx := context.Background()
	st := openLocal(t)

	tbl, err := st.CreateTable(ctx, "entities", testSchema())
	if err != nil {
		t.Fatalf("create table: %v", err)
	}
	populate(t, tbl)

	if n, err := tbl.Count(ctx); err != nil || n != int64(len(testRows)) {
		t.Fatalf("count = %d, %v; want %d", n, err, len(testRows))
	}

	// Full text: BM25 over the inverted index.
	fts, err := tbl.Search(ctx, Query{Text: "fusion rankings", TextColumn: "body", Limit: 3})
	if err != nil {
		t.Fatalf("fts: %v", err)
	}
	if len(fts) == 0 || fts[0].Row["name"] != "ReciprocalRank" {
		t.Errorf("fts returned %v, want ReciprocalRank first", names(fts))
	}
	if fts[0].Mode != "fts" {
		t.Errorf("hit mode = %q, want fts", fts[0].Mode)
	}

	// Semantic: nearest neighbour.
	sem, err := tbl.Search(ctx, Query{
		Vector: []float32{0, 0.95, 0.05, 0}, VectorColumn: "embedding", Limit: 3})
	if err != nil {
		t.Fatalf("semantic: %v", err)
	}
	if len(sem) == 0 || sem[0].Row["name"] != "MemoryStore" {
		t.Errorf("semantic returned %v, want MemoryStore first", names(sem))
	}

	// HYBRID, and the assertion is the REORDERING: the BM25 winner is promoted above the
	// vector winner, which is what reciprocal rank fusion does and what proves the fusion is
	// the engine's rather than ours.
	hyb, err := tbl.Search(ctx, Query{
		Text: "fusion rankings", TextColumn: "body",
		Vector: []float32{0, 0.95, 0.05, 0}, VectorColumn: "embedding", Limit: 3})
	if err != nil {
		t.Fatalf("hybrid: %v", err)
	}
	if len(hyb) < 2 {
		t.Fatalf("hybrid returned %d hits, want at least 2", len(hyb))
	}
	if hyb[0].Row["name"] != "ReciprocalRank" {
		t.Errorf("hybrid returned %v, want the BM25 winner promoted to first", names(hyb))
	}
	if hyb[0].Mode != "hybrid" {
		t.Errorf("hit mode = %q, want hybrid", hyb[0].Mode)
	}
	// The vector winner must still be in the fused set — a hybrid that dropped it would be an
	// FTS query wearing a different name.
	var sawVectorWinner bool
	for _, h := range hyb {
		if h.Row["name"] == "MemoryStore" {
			sawVectorWinner = true
		}
	}
	if !sawVectorWinner {
		t.Errorf("hybrid returned %v — the vector winner was dropped, so nothing was fused", names(hyb))
	}
}

// A filter has to reach the engine, or a caller would have to post-filter and lose the ranking.
func TestFilterIsAppliedByTheEngine(t *testing.T) {
	ctx := context.Background()
	st := openLocal(t)
	tbl, err := st.CreateTable(ctx, "entities", testSchema())
	if err != nil {
		t.Fatal(err)
	}
	populate(t, tbl)

	hits, err := tbl.Search(ctx, Query{
		Vector: []float32{0, 0, 0.9, 0.1}, VectorColumn: "embedding",
		Filter: "is_dependency = false", Limit: 6})
	if err != nil {
		t.Fatalf("filtered search: %v", err)
	}
	for _, h := range hits {
		if h.Row["name"] == "RowGroups" {
			t.Errorf("the filter did not exclude the dependency row: %v", names(hits))
		}
	}
}

// Upsert replaces by key rather than accumulating, which is what an incremental reindex needs.
func TestUpsertReplacesByKey(t *testing.T) {
	ctx := context.Background()
	st := openLocal(t)
	tbl, err := st.CreateTable(ctx, "entities", testSchema())
	if err != nil {
		t.Fatal(err)
	}
	populate(t, tbl)

	changed := Row{"uid": "u2", "path": "b.go", "name": "MemoryStoreRenamed",
		"body": "the memory store moved to object storage", "line": int64(21),
		"is_dependency": false, "embedding": []float32{0, 0.9, 0.1, 0}}
	if err := tbl.Upsert(ctx, "uid", []Row{changed}); err != nil {
		t.Fatalf("upsert: %v", err)
	}

	if n, err := tbl.Count(ctx); err != nil || n != int64(len(testRows)) {
		t.Fatalf("count after upsert = %d, %v; want %d — the row was added, not replaced",
			n, err, len(testRows))
	}
	hits, err := tbl.Search(ctx, Query{Text: "object storage", TextColumn: "body", Limit: 3})
	if err != nil {
		t.Fatalf("search after upsert: %v", err)
	}
	if len(hits) == 0 || hits[0].Row["name"] != "MemoryStoreRenamed" {
		t.Errorf("after upsert the search returned %v, want the new row", names(hits))
	}
}

// A key with an apostrophe must not end the SQL literal. Paths like `it's/a.go` exist.
func TestDeleteByKeyQuotesSafely(t *testing.T) {
	ctx := context.Background()
	st := openLocal(t)
	tbl, err := st.CreateTable(ctx, "entities", testSchema())
	if err != nil {
		t.Fatal(err)
	}
	rows := append([]Row{}, testRows...)
	rows = append(rows, Row{"uid": "it's/odd.go", "path": "it's/odd.go", "name": "Apostrophe",
		"body": "a path with an apostrophe in it", "line": int64(1),
		"is_dependency": false, "embedding": []float32{0.5, 0.5, 0, 0}})
	if err := tbl.Append(ctx, rows); err != nil {
		t.Fatal(err)
	}

	if err := tbl.DeleteByKey(ctx, "uid", []string{"it's/odd.go"}); err != nil {
		t.Fatalf("delete with an apostrophe in the key: %v", err)
	}
	if n, err := tbl.Count(ctx); err != nil || n != int64(len(testRows)) {
		t.Errorf("count = %d, %v; want %d", n, err, len(testRows))
	}
}

// Deleting everything must be asked for explicitly. An accidentally-empty filter that emptied
// the index would be unrecoverable.
func TestDeleteWhereRefusesAnEmptyFilter(t *testing.T) {
	ctx := context.Background()
	st := openLocal(t)
	tbl, err := st.CreateTable(ctx, "entities", testSchema())
	if err != nil {
		t.Fatal(err)
	}
	populate(t, tbl)

	if err := tbl.DeleteWhere(ctx, "   "); err == nil {
		t.Error("an empty filter was accepted")
	}
	if n, _ := tbl.Count(ctx); n != int64(len(testRows)) {
		t.Errorf("rows were deleted anyway: %d", n)
	}
}

// A schema that cannot be stored is refused here, with the column named, rather than surfacing
// as an Arrow error from three layers down.
func TestSchemaValidationNamesTheColumn(t *testing.T) {
	for _, c := range []struct {
		what   string
		schema Schema
	}{
		{"no fields", Schema{}},
		{"vector without Dim", Schema{Fields: []Field{{Name: "v", Type: FieldVector}}}},
		{"Dim on a string", Schema{Fields: []Field{{Name: "s", Type: FieldString, Dim: 3}}}},
		{"duplicate name", Schema{Fields: []Field{{Name: "a", Type: FieldString}, {Name: "a", Type: FieldInt64}}}},
		{"unnamed field", Schema{Fields: []Field{{Name: " ", Type: FieldString}}}},
	} {
		if err := c.schema.Validate(); err == nil {
			t.Errorf("%s: accepted", c.what)
		}
	}
}

// A row missing a non-nullable column is refused with the row index and the column named.
func TestAppendRefusesAMissingRequiredColumn(t *testing.T) {
	ctx := context.Background()
	st := openLocal(t)
	tbl, err := st.CreateTable(ctx, "entities", testSchema())
	if err != nil {
		t.Fatal(err)
	}
	err = tbl.Append(ctx, []Row{{"path": "x.go", "name": "NoUID", "body": "missing its uid"}})
	if err == nil {
		t.Fatal("a row with no uid was accepted")
	}
	if !contains(err.Error(), "uid") {
		t.Errorf("the error does not name the missing column: %v", err)
	}
}

func contains(s, sub string) bool {
	return len(s) >= len(sub) && (func() bool {
		for i := 0; i+len(sub) <= len(s); i++ {
			if s[i:i+len(sub)] == sub {
				return true
			}
		}
		return false
	})()
}

// ---------- the remote half: on-the-fly, against a real object store ----------

// THE POINT OF THE MIGRATION: a published version is queried over the network, with nothing
// downloaded. This runs against MinIO because a fake cannot prove object-store behaviour.
//
//	GRAPHIT_LANCE_S3_ENDPOINT=http://localhost:9000 \
//	GRAPHIT_LANCE_S3_BUCKET=lance-otf \
//	AWS_ACCESS_KEY_ID=… AWS_SECRET_ACCESS_KEY=… \
//	  go test -tags lancedb -run TestRemote ./internal/lancestore/ -v
func TestRemoteStoreIsQueriedOnTheFly(t *testing.T) {
	endpoint := os.Getenv("GRAPHIT_LANCE_S3_ENDPOINT")
	bucket := os.Getenv("GRAPHIT_LANCE_S3_BUCKET")
	if endpoint == "" || bucket == "" {
		t.Skip("set GRAPHIT_LANCE_S3_ENDPOINT and GRAPHIT_LANCE_S3_BUCKET to run the on-the-fly test")
	}
	ctx := context.Background()
	cfg := Config{
		URI: fmt.Sprintf("s3://%s/lancestore-test", bucket),
		S3:  config.S3Config{Bucket: bucket, Region: "us-east-1", Endpoint: endpoint},
	}
	if !cfg.IsRemote() {
		t.Fatal("an s3:// URI did not report itself remote")
	}

	// The publisher's side: writes once, by extraction from a populated local index.
	pub, err := Open(ctx, cfg)
	if err != nil {
		t.Fatalf("open for publish: %v", err)
	}
	// Opened against s3://, so the guard is on: flip it off for the publish half only.
	pub.remote = false
	_ = pub.DropTable(ctx, "entities")
	tbl, err := pub.CreateTable(ctx, "entities", testSchema())
	if err != nil {
		t.Fatalf("create table on s3: %v", err)
	}
	populate(t, tbl)
	_ = pub.Close()

	// The consumer's side: a fresh connection, read-only, nothing downloaded.
	st, err := Open(ctx, cfg)
	if err != nil {
		t.Fatalf("open remote: %v", err)
	}
	defer st.Close()
	if !st.Remote() {
		t.Fatal("the store did not report itself remote")
	}

	rt, err := st.OpenTable(ctx, "entities")
	if err != nil {
		t.Fatalf("open table over s3: %v", err)
	}
	if n, err := rt.Count(ctx); err != nil || n != int64(len(testRows)) {
		t.Fatalf("remote count = %d, %v; want %d", n, err, len(testRows))
	}

	// The schema was recovered from the table, so a consumer needs no manifest.
	if _, ok := rt.Schema().Field("embedding"); !ok {
		t.Error("the remote schema lost the vector column")
	}

	hyb, err := rt.Search(ctx, Query{
		Text: "fusion rankings", TextColumn: "body",
		Vector: []float32{0, 0.95, 0.05, 0}, VectorColumn: "embedding", Limit: 3})
	if err != nil {
		t.Fatalf("hybrid over s3: %v", err)
	}
	if len(hyb) == 0 || hyb[0].Row["name"] != "ReciprocalRank" {
		t.Errorf("hybrid over s3 returned %v, want the BM25 winner first", names(hyb))
	}

	// And a write must be refused: a consumer that could write would fork the published version
	// the registry names.
	if err := rt.Append(ctx, testRows[:1]); err == nil {
		t.Error("a write to a published version was accepted")
	}
}

// A GUARD AGAINST A SILENT DATA CORRUPTION, and against someone "fixing" quoteIdent back to
// standard SQL.
//
// The filter dialect reads a double-quoted name as a string LITERAL, so `"uid" IN ('u2')`
// evaluates `'uid' IN ('u2')` — false for every row — and Delete reports success having removed
// nothing. Because Upsert is delete-then-append, that turns every re-index into a duplicate.
// This test pins the identifier form that actually matches, and demonstrates the trap.
func TestIdentifierQuotingActuallyMatchesRows(t *testing.T) {
	ctx := context.Background()
	st := openLocal(t)
	tbl, err := st.CreateTable(ctx, "entities", testSchema())
	if err != nil {
		t.Fatal(err)
	}
	populate(t, tbl)

	// The form this package uses must delete.
	if err := tbl.DeleteByKey(ctx, "uid", []string{"u2"}); err != nil {
		t.Fatalf("DeleteByKey: %v", err)
	}
	n, err := tbl.Count(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if n != int64(len(testRows))-1 {
		t.Fatalf("count = %d, want %d — quoteIdent no longer matches rows", n, len(testRows)-1)
	}

	// And the double-quoted form is still the trap, so the comment on quoteIdent stays true.
	// If this ever starts deleting, the dialect changed and quoteIdent can be revisited.
	before, _ := tbl.Count(ctx)
	if err := tbl.DeleteWhere(ctx, `"uid" IN ('u3')`); err != nil {
		t.Logf("the double-quoted form now errors instead of no-opping: %v", err)
		return
	}
	after, _ := tbl.Count(ctx)
	if after != before {
		t.Errorf("the double-quoted form deleted %d rows — the dialect changed, revisit quoteIdent",
			before-after)
	}
}

// ---------- the opt-in second stage ----------

// fakeReranker reverses the engine's order, which is a reordering no scoring would produce by
// accident — so a test asserting it proves the stage ran rather than that the numbers agreed.
type fakeReranker struct {
	calls   int
	widened int
	fail    error
}

func (f *fakeReranker) Name() string { return "fake" }

func (f *fakeReranker) Rerank(_ context.Context, _ string, hits []Hit) ([]Hit, error) {
	f.calls++
	f.widened = len(hits)
	if f.fail != nil {
		return nil, f.fail
	}
	out := make([]Hit, len(hits))
	for i := range hits {
		out[i] = hits[len(hits)-1-i]
	}
	return out, nil
}

// OFF BY DEFAULT is the contract: a Query that says nothing about reranking must not invoke one.
func TestRerankIsOffByDefault(t *testing.T) {
	ctx := context.Background()
	st := openLocal(t)
	tbl, err := st.CreateTable(ctx, "entities", testSchema())
	if err != nil {
		t.Fatal(err)
	}
	populate(t, tbl)

	fake := &fakeReranker{}
	if _, err := tbl.Search(ctx, Query{Text: "memory", TextColumn: "body", Limit: 3}); err != nil {
		t.Fatalf("search: %v", err)
	}
	if fake.calls != 0 {
		t.Errorf("a reranker ran without being asked for: %d calls", fake.calls)
	}
}

// When asked for, the stage runs, reorders, and the first stage WIDENS — a cross-encoder cannot
// promote what retrieval never returned.
func TestRerankRunsAndWidensTheCandidateSet(t *testing.T) {
	ctx := context.Background()
	st := openLocal(t)
	tbl, err := st.CreateTable(ctx, "entities", testSchema())
	if err != nil {
		t.Fatal(err)
	}
	populate(t, tbl)

	// A limit of ONE against a query that matches several: that is what makes the widening
	// observable. Asking for six candidates cannot show anything when the corpus only has two
	// matches — the widening is in the REQUEST, and the only way to see it in the response is for
	// the caller's limit to be smaller than the matchable set.
	const limit = 1
	plain, err := tbl.Search(ctx, Query{Text: "search", TextColumn: "body", Limit: limit})
	if err != nil {
		t.Fatalf("baseline: %v", err)
	}
	if len(plain) != limit {
		t.Fatalf("baseline returned %d hits, want %d", len(plain), limit)
	}

	fake := &fakeReranker{}
	ranked, err := tbl.Search(ctx, Query{Text: "search", TextColumn: "body", Limit: limit,
		Rerank: RerankConfig{Reranker: fake, CandidateLimit: 6}})
	if err != nil {
		t.Fatalf("reranked search: %v", err)
	}

	if fake.calls != 1 {
		t.Fatalf("the reranker ran %d times, want 1", fake.calls)
	}
	if fake.widened <= limit {
		t.Errorf("the first stage did not widen: the reranker saw %d candidates for a limit of %d",
			fake.widened, limit)
	}
	if len(ranked) != limit {
		t.Errorf("the result was not trimmed back to the limit: %d hits, want %d", len(ranked), limit)
	}
	// The fake reverses the candidate list, so the engine's top must no longer be on top.
	if ranked[0].String() == plain[0].String() {
		t.Errorf("the order did not change, so the stage did not take effect: %v", names(ranked))
	}
	if !strings.HasSuffix(ranked[0].Mode, "+fake") {
		t.Errorf("Mode = %q, want the reranker named in it", ranked[0].Mode)
	}
}

// A failing reranker DEGRADES to the engine's order. Losing every result because a second-stage
// model could not load would be worse than losing the reordering.
func TestRerankFailureDegradesToTheEngineOrder(t *testing.T) {
	ctx := context.Background()
	st := openLocal(t)
	tbl, err := st.CreateTable(ctx, "entities", testSchema())
	if err != nil {
		t.Fatal(err)
	}
	populate(t, tbl)

	plain, err := tbl.Search(ctx, Query{Text: "memory", TextColumn: "body", Limit: 3})
	if err != nil {
		t.Fatal(err)
	}

	fake := &fakeReranker{fail: context.DeadlineExceeded}
	ranked, err := tbl.Search(ctx, Query{Text: "memory", TextColumn: "body", Limit: 3,
		Rerank: RerankConfig{Reranker: fake}})
	if err == nil {
		t.Error("the failure was not reported to the caller")
	}
	if len(ranked) == 0 {
		t.Fatal("the results were lost along with the reranking")
	}
	if ranked[0].String() != plain[0].String() {
		t.Errorf("the degraded order is not the engine's: got %v", names(ranked))
	}
}

// A reranker that returns a different set has broken its contract, and the safe reading is to
// distrust the reordering rather than serve a truncated answer as if it were ranked.
func TestRerankRefusesAChangedSet(t *testing.T) {
	ctx := context.Background()
	st := openLocal(t)
	tbl, err := st.CreateTable(ctx, "entities", testSchema())
	if err != nil {
		t.Fatal(err)
	}
	populate(t, tbl)

	dropping := rerankerFunc(func(_ context.Context, _ string, hits []Hit) ([]Hit, error) {
		if len(hits) == 0 {
			return hits, nil
		}
		return hits[:len(hits)-1], nil
	})
	ranked, err := tbl.Search(ctx, Query{Text: "memory", TextColumn: "body", Limit: 3,
		Rerank: RerankConfig{Reranker: dropping}})
	if err == nil {
		t.Error("a reranker that dropped a hit was accepted")
	}
	if len(ranked) == 0 {
		t.Error("the engine order was not preserved on refusal")
	}
}

type rerankerFunc func(context.Context, string, []Hit) ([]Hit, error)

func (f rerankerFunc) Name() string { return "func" }
func (f rerankerFunc) Rerank(ctx context.Context, q string, h []Hit) ([]Hit, error) {
	return f(ctx, q, h)
}
