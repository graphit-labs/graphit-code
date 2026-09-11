package ast

import (
	"context"
	"errors"
	"fmt"
	"path/filepath"
	"strings"
	"testing"
)

// `MATCH (n:Import)` against a project where nothing produced an Import failed with
// "Binder exception: Table Import does not exist." — which reads like the graph is
// broken. Import IS a label the grammars declare; it just has no table here, because
// tables are created per label actually indexed. Neo4j would answer zero rows.
func TestQueryExplainsALabelThatHasNoTable(t *testing.T) {
	t.Parallel()

	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(filepath.Join(t.TempDir(), "ladybugdb")), IcebugDir: filepath.Join(filepath.Dir(filepath.Join(t.TempDir(), "ladybugdb")), "graph.icebug")})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer func() { _ = db.Close() }()

	ctx := context.Background()
	if err := db.initSchemaForLabels(SchemaInfo{Labels: []string{"Function", "Method"}}); err != nil {
		t.Fatalf("schema: %v", err)
	}

	_, err := db.Query(ctx, "MATCH (n:Import) RETURN count(n) AS n", nil)
	if err == nil {
		t.Fatal("querying a label with no table should still be an error — an empty result would make a typo look like an answer")
	}

	msg := err.Error()
	for _, want := range []string{
		`"Import" is not a label or relationship type in this project's graph`,
		"nothing indexed here produced it",
		"Present:",
		"Function",
		"Method",
	} {
		if !strings.Contains(msg, want) {
			t.Errorf("the error does not say %q:\n  %s", want, msg)
		}
	}

	if !strings.Contains(msg, "Table Import does not exist") {
		t.Errorf("the original engine message was dropped:\n  %s", msg)
	}
}

// A mistyped relationship type produces the same shape, and the message has to fit it
// — the reader cannot tell from "Table CALS" whether CALS was meant to be a label.
func TestQueryExplainsAMistypedRelationshipType(t *testing.T) {
	t.Parallel()

	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(filepath.Join(t.TempDir(), "ladybugdb")), IcebugDir: filepath.Join(filepath.Dir(filepath.Join(t.TempDir(), "ladybugdb")), "graph.icebug")})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer func() { _ = db.Close() }()

	if err := db.initSchemaForLabels(SchemaInfo{Labels: []string{"Function"}}); err != nil {
		t.Fatalf("schema: %v", err)
	}

	_, err := db.Query(context.Background(), "MATCH ()-[:CALS]->() RETURN count(*) AS n", nil)
	if err == nil {
		t.Fatal("a mistyped relationship type should be an error")
	}
	if !strings.Contains(err.Error(), `"CALS" is not a label or relationship type`) {
		t.Errorf("the message does not name the mistyped type:\n  %s", err)
	}
}

// A label that IS present must keep answering zero rows rather than erroring — the
// point is to explain absence, not to turn emptiness into a failure.
func TestQueryOnAPresentLabelWithNoRowsIsNotAnError(t *testing.T) {
	t.Parallel()

	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(filepath.Join(t.TempDir(), "ladybugdb")), IcebugDir: filepath.Join(filepath.Dir(filepath.Join(t.TempDir(), "ladybugdb")), "graph.icebug")})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer func() { _ = db.Close() }()

	if err := db.initSchemaForLabels(SchemaInfo{Labels: []string{"Function"}}); err != nil {
		t.Fatalf("schema: %v", err)
	}

	res, err := db.Query(context.Background(), "MATCH (n:Function) RETURN count(n) AS n", nil)
	if err != nil {
		t.Fatalf("a present label must not error: %v", err)
	}
	if len(res.Records) != 1 {
		t.Fatalf("expected one count row, got %d", len(res.Records))
	}
}

func TestMissingTableMessage(t *testing.T) {
	t.Parallel()

	t.Run("other errors are left alone", func(t *testing.T) {
		for _, e := range []string{
			"Binder exception: Cannot find property line for n",
			"Parser exception: Invalid input",
			"",
		} {
			if got := missingTableMessage(errors.New(e), []string{"Function"}); got != "" {
				t.Errorf("%q should not be explained as a missing table, got: %s", e, got)
			}
		}
	})

	t.Run("an empty graph says so", func(t *testing.T) {
		got := missingTableMessage(errors.New("Table Function does not exist."), nil)
		if !strings.Contains(got, "no tables at all") || !strings.Contains(got, "mid-rebuild") {
			t.Errorf("an empty graph deserves its own explanation, got: %s", got)
		}
	})

	t.Run("a long label list is capped", func(t *testing.T) {
		present := make([]string, 0, 60)
		for i := 0; i < 60; i++ {
			present = append(present, fmt.Sprintf("Label%02d", i))
		}
		got := missingTableMessage(errors.New("Table Nope does not exist."), present)
		if !strings.Contains(got, "and 20 more") {
			t.Errorf("the list should be capped with a count of the rest, got: %s", got)
		}
		if strings.Contains(got, "Label59") {
			t.Errorf("the list was not capped, got: %s", got)
		}
	})
}

// An agent ran a correct query — `MATCH (f:File)-[:CONTAINS]->(e) ... RETURN
// e.line_number` — and got "Cannot find property line_number for e". The query was fine;
// the graph was mid-rebuild, holding a partial schema in which no table carried the
// property yet. The raw message sends the reader off to fix what was never broken, which
// is worse than a failure: it is a failure that accuses the wrong thing.
func TestQueryDistinguishesAWrongPropertyFromAPartialSchema(t *testing.T) {
	t.Parallel()

	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(filepath.Join(t.TempDir(), "ladybugdb")), IcebugDir: filepath.Join(filepath.Dir(filepath.Join(t.TempDir(), "ladybugdb")), "graph.icebug")})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer func() { _ = db.Close() }()

	if err := db.initSchema(); err != nil {
		t.Fatalf("schema: %v", err)
	}

	_, err := db.Query(context.Background(), "MATCH (n) RETURN n.line_number LIMIT 1", nil)
	if err == nil {
		t.Fatal("expected the partial schema to reject line_number")
	}
	for _, want := range []string{
		"no label in this graph has a property",
		"mid-rebuild",
		"File",
	} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("the error does not point at the rebuild, missing %q:\n  %s", want, err)
		}
	}

	if err := db.initSchemaForLabels(SchemaInfo{Labels: []string{"Function", "Method"}}); err != nil {
		t.Fatalf("schema for labels: %v", err)
	}
	if _, err := db.Query(context.Background(), "MATCH (n) RETURN n.line_number LIMIT 1", nil); err != nil {
		t.Errorf("with entity labels present, line_number must bind: %v", err)
	}

	_, err = db.Query(context.Background(), "MATCH (n) RETURN n.line LIMIT 1", nil)
	if err == nil {
		t.Fatal("`line` does not exist and must still fail")
	}
	if !strings.Contains(err.Error(), "`line` is `line_number`") {
		t.Errorf("the message does not offer the real name:\n  %s", err)
	}
}

// The other half: a property that exists on some labels and not on the ones this pattern
// can reach. Pinning the label is the fix, and `WHERE label(n) IN [...]` is not, because
// binding happens before filtering.
func TestQueryExplainsAPropertyMissingFromSomeLabels(t *testing.T) {
	t.Parallel()

	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(filepath.Join(t.TempDir(), "ladybugdb")), IcebugDir: filepath.Join(filepath.Dir(filepath.Join(t.TempDir(), "ladybugdb")), "graph.icebug")})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer func() { _ = db.Close() }()

	if err := db.initSchemaForLabels(SchemaInfo{Labels: []string{"Function"}}); err != nil {
		t.Fatalf("schema: %v", err)
	}

	_, err := db.Query(context.Background(), "MATCH (n:Function) RETURN n.relative_path LIMIT 1", nil)
	if err == nil {
		t.Skip("relative_path bound on Function; this engine build does not constrain it")
	}
	for _, want := range []string{"not on every label", "File", "Pin the label"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("the error does not explain the per-label binding, missing %q:\n  %s", want, err)
		}
	}
}
