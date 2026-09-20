//go:build lancedb

package lancequery_test

import (
	"context"
	"fmt"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/lancequery"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	page "github.com/graphit-labs/graphit-code/internal/pagination"
)

// The fixture mirrors the shapes this package actually meets in the four real stores: string
// keys, an int64, a bool, a redacted secret, a heavy body and a vector.
func fixtureSchema() lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "id", Type: lancestore.FieldString},
		{Name: "status", Type: lancestore.FieldString},
		{Name: "priority", Type: lancestore.FieldInt64},
		{Name: "flagged", Type: lancestore.FieldBool},
		{Name: "secret", Type: lancestore.FieldString, Nullable: true},
		{Name: "secret_hash", Type: lancestore.FieldString, Nullable: true},
		{Name: "body", Type: lancestore.FieldString, Nullable: true},
		{Name: "embedding", Type: lancestore.FieldVector, Dim: 4, Nullable: true},
	}}
}

func policy() lancequery.Policy {
	return lancequery.Policy{
		Tables:   []string{"tasks"},
		Redacted: map[string][]string{"tasks": {"secret"}},
		Heavy:    map[string][]string{"tasks": {"body"}},
	}
}

func openFixture(t *testing.T, rows int) *lancestore.Store {
	t.Helper()
	ctx := context.Background()
	st, err := lancestore.Open(ctx, lancestore.Config{URI: t.TempDir()})
	if err != nil {
		t.Fatalf("open store: %v", err)
	}
	t.Cleanup(func() { _ = st.Close() })

	tbl, err := st.CreateTable(ctx, "tasks", fixtureSchema())
	if err != nil {
		t.Fatalf("create table: %v", err)
	}
	batch := make([]lancestore.Row, 0, rows)
	for i := range rows {
		status := "open"
		if i%2 == 0 {
			status = "completed"
		}
		batch = append(batch, lancestore.Row{
			"id": fmt.Sprintf("tsk-%02d", i), "status": status,
			"priority": int64(i % 5), "flagged": i%3 == 0,
			"secret": fmt.Sprintf("token-%02d", i), "secret_hash": fmt.Sprintf("hash-%02d", i),
			"body": strings.Repeat("x", 64), "embedding": []float32{float32(i), 0, 0, 1},
		})
	}
	if err := tbl.Append(ctx, batch); err != nil {
		t.Fatalf("append: %v", err)
	}
	// A second table proves the policy hides what it does not list.
	if _, err := st.CreateTable(ctx, "internal_only", lancestore.Schema{
		Fields: []lancestore.Field{{Name: "k", Type: lancestore.FieldString}},
	}); err != nil {
		t.Fatalf("create second table: %v", err)
	}
	return st
}

// B-T1: Describe reads the real table, not a hand-written list.
func TestDescribeReportsRealColumnsAndCounts(t *testing.T) {
	ctx := context.Background()
	st := openFixture(t, 7)

	schema, err := lancequery.Describe(ctx, st, policy(), nil)
	if err != nil {
		t.Fatalf("describe: %v", err)
	}
	if len(schema.Tables) != 1 || schema.Tables[0].Name != "tasks" {
		t.Fatalf("policy did not hide the unlisted table: %+v", tableNames(schema))
	}
	tbl := schema.Tables[0]
	if tbl.Rows != 7 {
		t.Fatalf("row count = %d, want 7", tbl.Rows)
	}

	want := map[string]lancequery.Column{
		"id":          {Name: "id", Type: "string"},
		"priority":    {Name: "priority", Type: "int64"},
		"flagged":     {Name: "flagged", Type: "bool"},
		"secret":      {Name: "secret", Type: "string", Nullable: true, Redacted: true},
		"body":        {Name: "body", Type: "string", Nullable: true, Heavy: true},
		"embedding":   {Name: "embedding", Type: "vector", Nullable: true, Dim: 4},
		"secret_hash": {Name: "secret_hash", Type: "string", Nullable: true},
	}
	got := map[string]lancequery.Column{}
	for _, c := range tbl.Columns {
		got[c.Name] = c
	}
	if len(got) != len(fixtureSchema().Fields) {
		t.Fatalf("described %d columns, table has %d", len(got), len(fixtureSchema().Fields))
	}
	for name, expected := range want {
		if got[name] != expected {
			t.Errorf("column %s = %+v, want %+v", name, got[name], expected)
		}
	}
}

// B-T1 (cont.): naming a table that the policy hides must not silently describe nothing.
func TestDescribeRefusesATableThePolicyHides(t *testing.T) {
	st := openFixture(t, 1)
	_, err := lancequery.Describe(context.Background(), st, policy(), []string{"internal_only"})
	if err == nil {
		t.Fatal("describing a hidden table succeeded")
	}
	if !strings.Contains(err.Error(), "internal_only") || !strings.Contains(err.Error(), "tasks") {
		t.Fatalf("error must name the request and the alternatives, got: %v", err)
	}
}

// B-T2: an empty filter is a query, and a predicate selects.
func TestRunWithoutFilterReturnsRowsAndWithFilterSelects(t *testing.T) {
	ctx := context.Background()
	st := openFixture(t, 6)

	all, err := lancequery.Run(ctx, st, policy(), lancequery.Request{Table: "tasks"})
	if err != nil {
		t.Fatalf("unfiltered run: %v", err)
	}
	if len(all.Rows) != 6 {
		t.Fatalf("unfiltered returned %d rows, want 6", len(all.Rows))
	}
	if all.Mode != "filter" {
		t.Fatalf("mode = %q, want filter", all.Mode)
	}

	// The question that started all of this: the status of a known set, in one call.
	picked, err := lancequery.Run(ctx, st, policy(), lancequery.Request{
		Table:   "tasks",
		Filter:  "id IN ('tsk-01','tsk-03','tsk-04')",
		Columns: []string{"id", "status"},
	})
	if err != nil {
		t.Fatalf("predicate run: %v", err)
	}
	got := map[string]string{}
	for _, row := range picked.Rows {
		if len(row) != 2 {
			t.Fatalf("projection leaked extra columns: %v", row)
		}
		got[fmt.Sprint(row["id"])] = fmt.Sprint(row["status"])
	}
	want := map[string]string{"tsk-01": "open", "tsk-03": "open", "tsk-04": "completed"}
	if fmt.Sprint(got) != fmt.Sprint(want) {
		t.Fatalf("got %v, want %v", got, want)
	}
}

// B-T3: a redacted column is refused in the projection AND in the filter. The substring case
// fixes the boundary: `secret_hash` is a different column and stays usable.
func TestRedactedColumnIsRefusedInProjectionAndFilter(t *testing.T) {
	ctx := context.Background()
	st := openFixture(t, 4)

	for _, tc := range []struct {
		name string
		req  lancequery.Request
	}{
		{"projection", lancequery.Request{Table: "tasks", Columns: []string{"id", "secret"}}},
		{"filter", lancequery.Request{Table: "tasks", Filter: "secret = 'token-01'"}},
		{"filter mixed case", lancequery.Request{Table: "tasks", Filter: "SECRET IS NOT NULL"}},
		{"text column", lancequery.Request{Table: "tasks", Text: "token", TextColumn: "secret"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			_, err := lancequery.Run(ctx, st, policy(), tc.req)
			if err == nil {
				t.Fatal("a redacted column was accepted")
			}
			if !strings.Contains(err.Error(), "secret") {
				t.Fatalf("error must name the column, got: %v", err)
			}
		})
	}

	// The boundary: a distinct column whose name contains the redacted one as a prefix is not
	// the redacted column, and Go's \b keeps them apart because `_` is a word character.
	t.Run("distinct column sharing a prefix stays usable", func(t *testing.T) {
		res, err := lancequery.Run(ctx, st, policy(), lancequery.Request{
			Table: "tasks", Filter: "secret_hash = 'hash-01'", Columns: []string{"id", "secret_hash"},
		})
		if err != nil {
			t.Fatalf("secret_hash was wrongly treated as secret: %v", err)
		}
		if len(res.Rows) != 1 || fmt.Sprint(res.Rows[0]["id"]) != "tsk-01" {
			t.Fatalf("unexpected rows: %v", res.Rows)
		}
	})

	// The known imprecision, pinned so a later reader sees it was a choice. A string literal
	// spelling the column name is refused although it leaks nothing; over-refusal is the safe
	// direction and costs one rephrasing.
	t.Run("a literal spelling the column name is refused, by design", func(t *testing.T) {
		_, err := lancequery.Run(ctx, st, policy(), lancequery.Request{
			Table: "tasks", Filter: "status = 'secret'",
		})
		if err == nil {
			t.Fatal("expected the documented over-refusal; the doc comment must be updated if this changed")
		}
	})
}

// B-T4: vectors never come back as numbers.
func TestVectorColumnIsNeverReturnedAsValues(t *testing.T) {
	ctx := context.Background()
	st := openFixture(t, 3)

	def, err := lancequery.Run(ctx, st, policy(), lancequery.Request{Table: "tasks"})
	if err != nil {
		t.Fatalf("default projection: %v", err)
	}
	for _, row := range def.Rows {
		if _, ok := row["embedding"]; ok {
			t.Fatal("the default projection included the vector column")
		}
		if _, ok := row["body"]; ok {
			t.Fatal("the default projection included the heavy column")
		}
		if _, ok := row["secret"]; ok {
			t.Fatal("the default projection included the redacted column")
		}
	}

	asked, err := lancequery.Run(ctx, st, policy(), lancequery.Request{
		Table: "tasks", Columns: []string{"id", "embedding"},
	})
	if err != nil {
		t.Fatalf("explicit vector projection: %v", err)
	}
	for _, row := range asked.Rows {
		v, ok := row["embedding"]
		if !ok {
			t.Fatal("an explicitly requested vector column vanished from the row")
		}
		switch v.(type) {
		case []float32, []float64, []any:
			t.Fatalf("vector values reached the caller: %T", v)
		}
		if s, _ := v.(string); !strings.Contains(s, "dim=4") {
			t.Fatalf("marker must carry the dimension, got %v", v)
		}
	}

	// A heavy column is different: asked for, it is returned in full.
	heavy, err := lancequery.Run(ctx, st, policy(), lancequery.Request{
		Table: "tasks", Columns: []string{"id", "body"},
	})
	if err != nil {
		t.Fatalf("explicit heavy projection: %v", err)
	}
	if got := fmt.Sprint(heavy.Rows[0]["body"]); len(got) != 64 {
		t.Fatalf("heavy column came back truncated or missing: %d chars", len(got))
	}
}

// B-T5: Limit and Offset reach the engine verbatim. This layer does not trim, does not cap and
// does not look ahead — that is pagination's job, and doing it twice is how the last page ends
// up claiming to be the last when it is not.
func TestLimitAndOffsetReachTheEngineVerbatim(t *testing.T) {
	ctx := context.Background()
	st := openFixture(t, 25)

	first, err := lancequery.Run(ctx, st, policy(), lancequery.Request{Table: "tasks", Columns: []string{"id"}})
	if err != nil {
		t.Fatalf("default limit: %v", err)
	}
	if len(first.Rows) != lancequery.DefaultLimit {
		t.Fatalf("got %d rows, want the default %d", len(first.Rows), lancequery.DefaultLimit)
	}
	if lancequery.DefaultLimit != page.DefaultPageSize {
		t.Fatalf("the default drifted from pagination's: %d vs %d", lancequery.DefaultLimit, page.DefaultPageSize)
	}

	// A request above pagination's page ceiling is honoured, not clipped. A full page asks for
	// MaxPageSize+1 rows so the caller can see there is another page; clamping here would eat
	// exactly that row.
	big := openFixture(t, page.MaxPageSize+30)
	over, err := lancequery.Run(ctx, big, policy(), lancequery.Request{
		Table: "tasks", Columns: []string{"id"}, Limit: page.MaxPageSize + 1,
	})
	if err != nil {
		t.Fatalf("look-ahead sized limit: %v", err)
	}
	if len(over.Rows) != page.MaxPageSize+1 {
		t.Fatalf("got %d rows, want %d; a ceiling here would break the look-ahead",
			len(over.Rows), page.MaxPageSize+1)
	}

	past, err := lancequery.Run(ctx, st, policy(), lancequery.Request{
		Table: "tasks", Columns: []string{"id"}, Offset: 100,
	})
	if err != nil {
		t.Fatalf("offset past the end: %v", err)
	}
	if len(past.Rows) != 0 {
		t.Fatalf("offset past the end returned %d rows", len(past.Rows))
	}

	exact, err := lancequery.Run(ctx, st, policy(), lancequery.Request{
		Table: "tasks", Columns: []string{"id"}, Offset: 20, Limit: 5,
	})
	if err != nil {
		t.Fatalf("last page: %v", err)
	}
	if len(exact.Rows) != 5 {
		t.Fatalf("offset 20 limit 5 over 25 rows returned %d, want 5", len(exact.Rows))
	}
}

// The look-ahead contract as pagination actually drives it: Window.FetchLimit in, one row past
// the page out, and FinishFetched turning that into a cursor.
func TestRunFeedsPaginationWithoutDoubleCounting(t *testing.T) {
	ctx := context.Background()
	st := openFixture(t, 25)

	bind := struct{ Tool, Table string }{"test", "tasks"}
	window, err := page.Open(page.Spec{PageSize: 10, DefaultPageSize: lancequery.DefaultLimit, Bind: bind})
	if err != nil {
		t.Fatalf("open page: %v", err)
	}
	res, err := lancequery.Run(ctx, st, policy(), lancequery.Request{
		Table: "tasks", Columns: []string{"id"}, Limit: window.FetchLimit, Offset: window.Offset,
	})
	if err != nil {
		t.Fatalf("first page: %v", err)
	}
	if len(res.Rows) != 11 {
		t.Fatalf("a 10-row page must fetch 11 to see the next one; got %d", len(res.Rows))
	}
	first := page.FinishFetched(window, res.Rows)
	if len(first.Results) != 10 || first.NextCursor == "" {
		t.Fatalf("first page: %d results, cursor %q", len(first.Results), first.NextCursor)
	}

	// The cursor continues where the page stopped, with no repeat and no gap.
	next, err := page.Open(page.Spec{
		PageSize: 10, Cursor: first.NextCursor, DefaultPageSize: lancequery.DefaultLimit, Bind: bind,
	})
	if err != nil {
		t.Fatalf("reopen with cursor: %v", err)
	}
	res2, err := lancequery.Run(ctx, st, policy(), lancequery.Request{
		Table: "tasks", Columns: []string{"id"}, Limit: next.FetchLimit, Offset: next.Offset,
	})
	if err != nil {
		t.Fatalf("second page: %v", err)
	}
	second := page.FinishFetched(next, res2.Rows)
	if len(second.Results) != 10 {
		t.Fatalf("second page returned %d results", len(second.Results))
	}
	seen := map[string]bool{}
	for _, row := range first.Results {
		seen[fmt.Sprint(row["id"])] = true
	}
	for _, row := range second.Results {
		if seen[fmt.Sprint(row["id"])] {
			t.Fatalf("row %v appeared on both pages", row["id"])
		}
	}

	// A cursor minted for one query must not be accepted by another.
	_, err = page.Open(page.Spec{
		PageSize: 10, Cursor: first.NextCursor, DefaultPageSize: lancequery.DefaultLimit,
		Bind: struct{ Tool, Table string }{"test", "task_checks"},
	})
	if err == nil {
		t.Fatal("a cursor from another query was accepted")
	}
}

// B-A6: a wrong name is answered with the right ones.
func TestUnknownTableAndColumnNameTheAlternatives(t *testing.T) {
	ctx := context.Background()
	st := openFixture(t, 2)

	_, err := lancequery.Run(ctx, st, policy(), lancequery.Request{Table: "taskz"})
	if err == nil || !strings.Contains(err.Error(), "tasks") {
		t.Fatalf("unknown table must list what exists, got: %v", err)
	}

	_, err = lancequery.Run(ctx, st, policy(), lancequery.Request{Table: "tasks", Columns: []string{"statuz"}})
	if err == nil {
		t.Fatal("unknown column accepted")
	}
	for _, want := range []string{"statuz", "status", "priority"} {
		if !strings.Contains(err.Error(), want) {
			t.Fatalf("error must name %q; got: %v", want, err)
		}
	}

	_, err = lancequery.Run(ctx, st, policy(), lancequery.Request{})
	if err == nil || !strings.Contains(err.Error(), "tasks") {
		t.Fatalf("a missing table must list the options, got: %v", err)
	}
}

// A projection of nothing but vectors would read every column, which is the opposite of the
// request. It is refused rather than silently widened.
func TestVectorOnlyProjectionIsRefused(t *testing.T) {
	st := openFixture(t, 2)
	_, err := lancequery.Run(context.Background(), st, policy(), lancequery.Request{
		Table: "tasks", Columns: []string{"embedding"},
	})
	if err == nil {
		t.Fatal("a vector-only projection was accepted")
	}
	if !strings.Contains(err.Error(), "scalar") {
		t.Fatalf("error should explain the fix, got: %v", err)
	}
}

// An empty policy is the common case for a store with nothing to hide.
func TestZeroPolicyExposesEverythingOrdinarily(t *testing.T) {
	ctx := context.Background()
	st := openFixture(t, 2)

	schema, err := lancequery.Describe(ctx, st, lancequery.Policy{}, nil)
	if err != nil {
		t.Fatalf("describe: %v", err)
	}
	if len(schema.Tables) != 2 {
		t.Fatalf("zero policy described %v, want both tables", tableNames(schema))
	}
	res, err := lancequery.Run(ctx, st, lancequery.Policy{}, lancequery.Request{Table: "tasks"})
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if _, ok := res.Rows[0]["secret"]; !ok {
		t.Fatal("a zero policy redacts nothing, so secret should be present")
	}
	if _, ok := res.Rows[0]["embedding"]; ok {
		t.Fatal("vector exclusion is structural, not a policy opt-in")
	}
}

func tableNames(s lancequery.Schema) []string {
	out := make([]string, 0, len(s.Tables))
	for _, t := range s.Tables {
		out = append(out, t.Name)
	}
	return out
}
