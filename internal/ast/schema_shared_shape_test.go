package ast

import (
	"context"
	"path/filepath"
	"strings"
	"testing"
)

// The schema is read at the start of every session, and nearly every label is an
// ENTITY label carrying the SAME row of 16 properties. One line per label is ~25
// repetitions of the same list, paid for in context every time and learned from once.
//
// Grouping must lose no information: the UNIQUE shapes — File, Directory, Module —
// still print one per line, because `File.path` versus an entity's `path` is exactly
// the distinction that makes a query crash with "Cannot find property".
func TestSchemaTextGroupsLabelsThatShareAPropertySet(t *testing.T) {
	t.Parallel()

	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(filepath.Join(t.TempDir(), "ladybugdb")), IcebugDir: filepath.Join(filepath.Dir(filepath.Join(t.TempDir(), "ladybugdb")), "graph.icebug")})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer func() { _ = db.Close() }()

	if err := db.initSchemaForLabels(SchemaInfo{
		Labels: []string{"Function", "Method", "Class", "Comment"},
	}); err != nil {
		t.Fatalf("schema: %v", err)
	}

	text, err := SchemaText(context.Background(), db)
	if err != nil {
		t.Fatalf("SchemaText: %v", err)
	}

	if !strings.Contains(text, "all carry the SAME properties") {
		t.Fatalf("entity labels were not grouped; schema still repeats the property list:\n%s", text)
	}

	// The proof grouping hid nothing: the entity properties and File's own shape
	// stay readable in the text.
	for _, want := range []string{"line_number", "cyclomatic_complexity", "is_stub", "relative_path"} {
		if !strings.Contains(text, want) {
			t.Errorf("property %q disappeared from the schema output", want)
		}
	}

	// File has a shape of its own and must not have been swept into the group.
	if !strings.Contains(text, "- File(") {
		t.Errorf("File lost its own line; its property set differs and that difference is load-bearing:\n%s", text)
	}

	// And the repetition really is gone: the entity property list appears ONCE.
	if n := strings.Count(text, "cyclomatic_complexity, context, context_type"); n != 1 {
		t.Errorf("the shared entity property list appears %d times, want exactly 1", n)
	}
}
