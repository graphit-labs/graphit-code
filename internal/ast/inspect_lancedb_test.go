//go:build lancedb

package ast

import (
	"context"
	"fmt"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/lancequery"
	page "github.com/graphit-labs/graphit-code/internal/pagination"
)

// inspectIndex seeds two files with known entities, one of them a dependency, so the tests can
// tell project code from vendored code the way a real caller would.
func inspectIndex(t *testing.T) *SearchIndex {
	t.Helper()
	idx := newLanceIndexForTest(t)
	cache := newShardCacheForTest(t,
		entryWith("internal/task/service.go", "package task // service source",
			cachedEntity{Name: "Open", Docstring: "Opens the task service.", Line: 51},
			cachedEntity{Name: "withTables", Docstring: "Runs a read against the open tables.", Line: 73}),
		entryWith("internal/memory/table.go", "package memory // table source",
			cachedEntity{Name: "OpenMemoryTable", Docstring: "Opens one scope's store.", Line: 149}),
	)
	if err := idx.RebuildFromCache(context.Background(), cache, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}
	return idx
}

// AS-T1: both tables are described from the index, with the vector marked.
func TestDescribeStoreReportsBothFTSTables(t *testing.T) {
	idx := inspectIndex(t)

	schema, err := idx.DescribeStore(context.Background(), nil)
	if err != nil {
		t.Fatalf("describe: %v", err)
	}
	got := map[string]lancequery.TableInfo{}
	for _, tbl := range schema.Tables {
		got[tbl.Name] = tbl
	}
	for _, want := range StoreTables() {
		if _, ok := got[want]; !ok {
			t.Fatalf("table %s missing; described %d tables", want, len(got))
		}
	}
	if _, ok := got["search"]; ok {
		t.Fatal("the store was opened one directory too high: `search` is the index itself, not a table")
	}
	if n := got[lanceEntitiesTable].Rows; n != 3 {
		t.Fatalf("entities row count = %d, want 3", n)
	}
	if n := got[lanceFilesTable].Rows; n != 2 {
		t.Fatalf("files row count = %d, want 2", n)
	}

	cols := map[string]lancequery.Column{}
	for _, c := range got[lanceEntitiesTable].Columns {
		cols[c.Name] = c
	}
	if c := cols["embedding"]; c.Type != "vector" || c.Dim == 0 {
		t.Errorf("entities.embedding described as %+v, want a vector with a dimension", c)
	}
	// entities.body is a synthesised BM25 document — name variants, split forms and n-grams,
	// not source — so it is refused outright rather than merely held back for size.
	if !cols["body"].Redacted {
		t.Error("entities.body must be redacted: it is a search document, not code")
	}
	// AS-A5: docstring is deliberately NOT heavy. It explains an entity without being it,
	// which is exactly what a listing wants.
	if cols["docstring"].Heavy {
		t.Error("entities.docstring must stay in the default projection")
	}
	fileCols := map[string]lancequery.Column{}
	for _, c := range got[lanceFilesTable].Columns {
		fileCols[c.Name] = c
	}
	if !fileCols["source"].Redacted {
		t.Error("files.source must be redacted: it appends path n-grams behind a NUL separator")
	}
}

// AS-T2: the predicate separates project code from dependencies and scopes to a path.
func TestQueryStoreFiltersEntitiesByPathAndDependency(t *testing.T) {
	ctx := context.Background()
	idx := inspectIndex(t)

	res, err := idx.QueryStore(ctx, lancequery.Request{
		Table:   lanceEntitiesTable,
		Filter:  "path = 'internal/task/service.go' AND is_dep = false",
		Columns: []string{"name", "path", "line"},
	})
	if err != nil {
		t.Fatalf("path query: %v", err)
	}
	if len(res.Rows) != 2 {
		t.Fatalf("expected the 2 entities of service.go, got %d: %v", len(res.Rows), res.Rows)
	}
	names := map[string]bool{}
	for _, row := range res.Rows {
		if len(row) != 3 {
			t.Fatalf("projection leaked columns: %v", row)
		}
		if got := fmt.Sprint(row["path"]); got != "internal/task/service.go" {
			t.Errorf("row from the wrong file: %s", got)
		}
		names[fmt.Sprint(row["name"])] = true
	}
	for _, want := range []string{"Open", "withTables"} {
		if !names[want] {
			t.Errorf("entity %s missing from the answer", want)
		}
	}

	// The files table answers a different question from the same call shape.
	files, err := idx.QueryStore(ctx, lancequery.Request{
		Table: lanceFilesTable, Columns: []string{"path"},
	})
	if err != nil {
		t.Fatalf("files query: %v", err)
	}
	if len(files.Rows) != 2 {
		t.Fatalf("files returned %d rows, want 2", len(files.Rows))
	}
}

// AS-A5 in behaviour: whole-source columns stay out unless named, and the vector never comes
// back as numbers. On this store that is the difference between a listing and a code dump.
func TestQueryStoreWithholdsSourceTextAndVectors(t *testing.T) {
	ctx := context.Background()
	idx := inspectIndex(t)

	res, err := idx.QueryStore(ctx, lancequery.Request{Table: lanceEntitiesTable})
	if err != nil {
		t.Fatalf("default projection: %v", err)
	}
	for _, row := range res.Rows {
		if _, ok := row["body"]; ok {
			t.Fatal("entities.body reached a default projection")
		}
		if _, ok := row["embedding"]; ok {
			t.Fatal("the embedding reached a default projection")
		}
		if _, ok := row["docstring"]; !ok {
			t.Fatal("docstring must be in a default projection")
		}
	}

	files, err := idx.QueryStore(ctx, lancequery.Request{Table: lanceFilesTable})
	if err != nil {
		t.Fatalf("files default projection: %v", err)
	}
	for _, row := range files.Rows {
		if _, ok := row["source"]; ok {
			t.Fatal("files.source reached a default projection")
		}
	}

	// Naming either search document is refused, in the projection and in the filter. This is
	// the case that corrected the original plan: both columns were specified as heavy, on the
	// assumption that they held source. They do not.
	for _, tc := range []struct {
		name string
		req  lancequery.Request
	}{
		{"file document projected", lancequery.Request{
			Table: lanceFilesTable, Columns: []string{"path", "source"}}},
		{"file document filtered", lancequery.Request{
			Table: lanceFilesTable, Filter: "source LIKE '%package%'"}},
		{"entity document projected", lancequery.Request{
			Table: lanceEntitiesTable, Columns: []string{"name", "body"}}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if _, err := idx.QueryStore(ctx, tc.req); err == nil {
				t.Fatal("a synthesised search document was handed to the caller")
			}
		})
	}
}

// AS-T3: a query with no limit is bounded, and the caller can tell there is more. The ceiling
// lives in pagination, so the test drives it the way a tool does.
func TestFTSQueryPagesRatherThanDumpingTheTable(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)
	entities := make([]cachedEntity, 0, 25)
	for i := range 25 {
		entities = append(entities, cachedEntity{
			Name: fmt.Sprintf("Entity%02d", i), Docstring: "seeded", Line: i + 1,
		})
	}
	cache := newShardCacheForTest(t, entryWith("fixture.go", "package fixture", entities...))
	if err := idx.RebuildFromCache(ctx, cache, nil); err != nil {
		t.Fatal(err)
	}

	// No limit at all: the layer applies its default rather than returning all 25.
	def, err := idx.QueryStore(ctx, lancequery.Request{Table: lanceEntitiesTable, Columns: []string{"name"}})
	if err != nil {
		t.Fatalf("default: %v", err)
	}
	if len(def.Rows) != lancequery.DefaultLimit {
		t.Fatalf("an unbounded query returned %d rows, want the default %d", len(def.Rows), lancequery.DefaultLimit)
	}

	// Driven through pagination, the caller learns there is a next page.
	bind := struct{ Tool, Table string }{"ast_fts_query", lanceEntitiesTable}
	window, err := page.Open(page.Spec{PageSize: 10, DefaultPageSize: lancequery.DefaultLimit, Bind: bind})
	if err != nil {
		t.Fatal(err)
	}
	res, err := idx.QueryStore(ctx, lancequery.Request{
		Table: lanceEntitiesTable, Columns: []string{"name"},
		Limit: window.FetchLimit - window.Offset, Offset: window.Offset,
	})
	if err != nil {
		t.Fatal(err)
	}
	first := page.FinishFetched(window, res.Rows)
	if len(first.Results) != 10 || first.NextCursor == "" {
		t.Fatalf("first page: %d rows, cursor %q", len(first.Results), first.NextCursor)
	}
}

// AS-A3 guard at the package level: the FTS policy must not reach the graph. The Cypher store
// is graph.icebug, a sibling of search.lance, and nothing here should list it.
func TestFTSPolicyExposesOnlyTheTwoSearchTables(t *testing.T) {
	idx := inspectIndex(t)
	_, err := idx.QueryStore(context.Background(), lancequery.Request{Table: "graph"})
	if err == nil {
		t.Fatal("a name outside the FTS store was accepted")
	}
	for _, want := range []string{lanceEntitiesTable, lanceFilesTable} {
		if !strings.Contains(err.Error(), want) {
			t.Fatalf("error must list %s; got: %v", want, err)
		}
	}
	if len(StoreTables()) != 2 {
		t.Fatalf("the FTS store owns %d tables, want exactly entities and files", len(StoreTables()))
	}
}
