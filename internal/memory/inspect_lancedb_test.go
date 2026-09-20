//go:build lancedb

package memory

import (
	"context"
	"fmt"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/lancequery"
)

func inspectFixture(t *testing.T) *MemoryService {
	t.Helper()
	t.Setenv("HOME", t.TempDir())
	svc := newLocalService(t)
	for _, m := range []struct {
		title, body string
		kind        MemoryType
		mandatory   bool
	}{
		{"Broker revocation", "Revoking a refresh token ends the grant.", "lesson", true},
		{"Adapter hook order", "SessionStart runs before the first prompt.", "lesson", false},
		{"Storage root", "Global state lives under the resolved global directory.", "reference", false},
	} {
		if _, err := svc.AddMemory(m.title, m.body, MemoryOpts{Type: m.kind, Mandatory: m.mandatory}); err != nil {
			t.Fatalf("seed %q: %v", m.title, err)
		}
	}
	return svc
}

// ME-T1: the schema is read from the table. The decisive column is content_hash — the
// hand-written list this replaced had already lost it, which is what made the old answer wrong
// rather than merely redundant.
func TestDescribeStoreReadsTheRealMemorySchema(t *testing.T) {
	svc := inspectFixture(t)

	schema, err := svc.DescribeStore(context.Background(), nil)
	if err != nil {
		t.Fatalf("describe: %v", err)
	}
	if len(schema.Tables) != 1 || schema.Tables[0].Name != memoryTableName {
		t.Fatalf("expected the single memories table, got %d", len(schema.Tables))
	}
	tbl := schema.Tables[0]

	cols := map[string]lancequery.Column{}
	for _, c := range tbl.Columns {
		cols[c.Name] = c
	}
	expected := memoryTableSchema(cols["embedding"].Dim)
	if len(cols) != len(expected.Fields) {
		t.Fatalf("described %d columns, memoryTableSchema declares %d", len(cols), len(expected.Fields))
	}
	for _, f := range expected.Fields {
		if _, ok := cols[f.Name]; !ok {
			t.Errorf("column %s missing from the description", f.Name)
		}
	}
	if _, ok := cols["content_hash"]; !ok {
		t.Error("content_hash is absent; this is exactly the column the hand-written list lost")
	}
	if c := cols["embedding"]; c.Type != "vector" || c.Dim == 0 {
		t.Errorf("embedding described as %+v, want a vector with a dimension", c)
	}
	if !cols["body"].Heavy {
		t.Error("body must be marked heavy")
	}
	for _, c := range tbl.Columns {
		if c.Redacted {
			t.Errorf("memory declares no secret, yet %s came back redacted", c.Name)
		}
	}
	if tbl.Rows != 3 {
		t.Fatalf("row count = %d, want 3", tbl.Rows)
	}
}

// ME-T2: the predicate selects, and the vector never comes back as numbers.
func TestQueryStoreFiltersAndWithholdsTheEmbedding(t *testing.T) {
	ctx := context.Background()
	svc := inspectFixture(t)

	lessons, err := svc.QueryStore(ctx, lancequery.Request{
		Filter: "type = 'lesson'", Columns: []string{"title", "type"},
	})
	if err != nil {
		t.Fatalf("filtered query: %v", err)
	}
	if len(lessons.Rows) != 2 {
		t.Fatalf("type = 'lesson' returned %d rows, want 2", len(lessons.Rows))
	}
	for _, row := range lessons.Rows {
		if got := fmt.Sprint(row["type"]); got != "lesson" {
			t.Errorf("row type = %q, want lesson", got)
		}
	}

	mandatory, err := svc.QueryStore(ctx, lancequery.Request{
		Filter: "mandatory = true", Columns: []string{"title"},
	})
	if err != nil {
		t.Fatalf("boolean predicate: %v", err)
	}
	if len(mandatory.Rows) != 1 {
		t.Fatalf("mandatory = true returned %d rows, want 1", len(mandatory.Rows))
	}

	all, err := svc.QueryStore(ctx, lancequery.Request{})
	if err != nil {
		t.Fatalf("default query: %v", err)
	}
	if len(all.Rows) != 3 {
		t.Fatalf("an empty filter returned %d rows, want every one of the 3", len(all.Rows))
	}
	for _, row := range all.Rows {
		if _, ok := row["embedding"]; ok {
			t.Fatal("the embedding reached a default projection")
		}
		if _, ok := row["body"]; ok {
			t.Fatal("the heavy body column reached a default projection")
		}
	}

	asked, err := svc.QueryStore(ctx, lancequery.Request{Columns: []string{"id", "embedding"}, Limit: 1})
	if err != nil {
		t.Fatalf("explicit embedding: %v", err)
	}
	v := asked.Rows[0]["embedding"]
	switch v.(type) {
	case []float32, []float64, []any:
		t.Fatalf("embedding values reached the caller: %T", v)
	}
	if s, _ := v.(string); !strings.Contains(s, "vector") {
		t.Fatalf("expected a marker describing the column, got %v", v)
	}

	// The body is available on request; heavy is a cost default, not a prohibition.
	body, err := svc.QueryStore(ctx, lancequery.Request{
		Filter: "title = 'Broker revocation'", Columns: []string{"title", "body"},
	})
	if err != nil {
		t.Fatalf("heavy column: %v", err)
	}
	if got := fmt.Sprint(body.Rows[0]["body"]); !strings.Contains(got, "refresh token") {
		t.Fatalf("body came back empty: %q", got)
	}
}

// Naming the one table explicitly is allowed; naming a different one is not.
func TestQueryStoreAcceptsTheTableNameAndRefusesAnother(t *testing.T) {
	ctx := context.Background()
	svc := inspectFixture(t)

	if _, err := svc.QueryStore(ctx, lancequery.Request{Table: memoryTableName, Columns: []string{"id"}}); err != nil {
		t.Fatalf("naming the memories table explicitly failed: %v", err)
	}
	_, err := svc.QueryStore(ctx, lancequery.Request{Table: "tasks", Columns: []string{"id"}})
	if err == nil {
		t.Fatal("a table from another module was accepted")
	}
	if !strings.Contains(err.Error(), memoryTableName) {
		t.Fatalf("the error must name the one real table, got: %v", err)
	}
}
