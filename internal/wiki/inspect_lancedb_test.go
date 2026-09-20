//go:build lancedb

package wiki

import (
	"context"
	"fmt"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/lancequery"
)

// KN-T1 and KN-T3: the four tables are described from the index itself, and the URI trap is
// pinned. Opening the PARENT of index.lance makes TableNames answer `[index]`, because LanceDB
// treats any `.lance` subdirectory as a table; the schema answer would then be one fictional
// table with no columns instead of four real ones.
func TestDescribeStoreReportsTheFourIndexTables(t *testing.T) {
	db := rebuiltTestDB(t)

	schema, err := db.DescribeStore(context.Background(), nil)
	if err != nil {
		t.Fatalf("describe: %v", err)
	}
	got := map[string]lancequery.TableInfo{}
	for _, tbl := range schema.Tables {
		got[tbl.Name] = tbl
	}
	for _, want := range StoreTables() {
		if _, ok := got[want]; !ok {
			t.Errorf("table %s missing; described %v", want, names(schema))
		}
	}
	if _, ok := got["index"]; ok {
		t.Fatal("the store was opened one directory too high: `index` is the index itself, not a table")
	}
	if n := got[lanceChunksTable].Rows; n != int64(len(testChunks())) {
		t.Fatalf("chunks row count = %d, want %d", n, len(testChunks()))
	}

	cols := map[string]lancequery.Column{}
	for _, c := range got[lanceChunksTable].Columns {
		cols[c.Name] = c
	}
	if c := cols["embedding"]; c.Type != "vector" {
		t.Errorf("chunks.embedding described as %q, want vector", c.Type)
	}
	for _, heavy := range []string{"body", "summary", "search_terms"} {
		if !cols[heavy].Heavy {
			t.Errorf("chunks.%s must be marked heavy", heavy)
		}
	}
	for _, c := range got[lanceChunksTable].Columns {
		if c.Redacted {
			t.Errorf("a wiki page holds no secret, yet %s came back redacted", c.Name)
		}
	}
}

// KN-T2: xrefs is where the query earns its place. "What links here" is a question BM25 cannot
// answer and a page read cannot answer either.
func TestQueryStoreAnswersWhatLinksToAPage(t *testing.T) {
	ctx := context.Background()
	db := rebuiltTestDB(t)

	res, err := db.QueryStore(ctx, lancequery.Request{
		Table:   lanceXRefsTable,
		Filter:  "target_slug = 'indexacao'",
		Columns: []string{"source_slug", "target_slug"},
	})
	if err != nil {
		t.Fatalf("xrefs query: %v", err)
	}
	if len(res.Rows) != 1 {
		t.Fatalf("expected one page linking to indexacao, got %d: %v", len(res.Rows), res.Rows)
	}
	if got := fmt.Sprint(res.Rows[0]["source_slug"]); got != "autenticacao" {
		t.Fatalf("source_slug = %q, want autenticacao", got)
	}

	// And the reverse direction, from the same table.
	out, err := db.QueryStore(ctx, lancequery.Request{
		Table:   lanceXRefsTable,
		Filter:  "source_slug = 'indexacao'",
		Columns: []string{"target_slug"},
	})
	if err != nil {
		t.Fatalf("outbound query: %v", err)
	}
	if len(out.Rows) != 1 || fmt.Sprint(out.Rows[0]["target_slug"]) != "implantacao" {
		t.Fatalf("outbound links wrong: %v", out.Rows)
	}
}

// KN-A5 and the heavy tier: a page's prose stays out unless asked for, and the vector never
// comes back as numbers.
func TestQueryStoreWithholdsPageProseAndVectors(t *testing.T) {
	ctx := context.Background()
	db := rebuiltTestDB(t)

	res, err := db.QueryStore(ctx, lancequery.Request{Table: lanceChunksTable})
	if err != nil {
		t.Fatalf("default projection: %v", err)
	}
	if len(res.Rows) != len(testChunks()) {
		t.Fatalf("got %d rows, want %d", len(res.Rows), len(testChunks()))
	}
	for _, row := range res.Rows {
		for _, absent := range []string{"embedding", "body", "summary", "search_terms"} {
			if _, ok := row[absent]; ok {
				t.Fatalf("%s reached a default projection", absent)
			}
		}
		if _, ok := row["slug"]; !ok {
			t.Fatal("slug must be in a default projection")
		}
	}

	asked, err := db.QueryStore(ctx, lancequery.Request{
		Table: lanceChunksTable, Filter: "slug = 'autenticacao'", Columns: []string{"slug", "body", "embedding"},
	})
	if err != nil {
		t.Fatalf("explicit projection: %v", err)
	}
	row := asked.Rows[0]
	if body := fmt.Sprint(row["body"]); !strings.Contains(body, "fluxo de login") {
		t.Fatalf("heavy body came back empty: %q", body)
	}
	switch v := row["embedding"].(type) {
	case []float32, []float64, []any:
		t.Fatalf("embedding values reached the caller: %T", v)
	case string:
		if !strings.Contains(v, "vector") {
			t.Fatalf("marker should describe the column, got %q", v)
		}
	default:
		t.Fatalf("unexpected embedding value %T", v)
	}
}

// A table outside the index is refused with the real ones named.
func TestQueryStoreRefusesATableFromAnotherModule(t *testing.T) {
	db := rebuiltTestDB(t)
	_, err := db.QueryStore(context.Background(), lancequery.Request{Table: "tasks"})
	if err == nil {
		t.Fatal("a foreign table was accepted")
	}
	for _, want := range []string{lanceChunksTable, lanceXRefsTable} {
		if !strings.Contains(err.Error(), want) {
			t.Fatalf("error must list %s; got: %v", want, err)
		}
	}
}

func names(s lancequery.Schema) []string {
	out := make([]string, 0, len(s.Tables))
	for _, t := range s.Tables {
		out = append(out, t.Name)
	}
	return out
}
