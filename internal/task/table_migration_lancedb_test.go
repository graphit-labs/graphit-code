//go:build lancedb

package task

import (
	"context"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

// schemaWithout stands in for a store written before a column existed.
func schemaWithout(schema lancestore.Schema, name string) lancestore.Schema {
	var out lancestore.Schema
	for _, field := range schema.Fields {
		if field.Name != name {
			out.Fields = append(out.Fields, field)
		}
	}
	return out
}

func schemaRetyping(schema lancestore.Schema, name string, to lancestore.FieldType) lancestore.Schema {
	var out lancestore.Schema
	for _, field := range schema.Fields {
		if field.Name == name {
			field.Type = to
		}
		out.Fields = append(out.Fields, field)
	}
	return out
}

func seedRow(schema lancestore.Schema, overrides lancestore.Row) lancestore.Row {
	row := lancestore.Row{}
	for _, field := range schema.Fields {
		switch field.Type {
		case lancestore.FieldInt64:
			row[field.Name] = int64(0)
		case lancestore.FieldFloat64:
			row[field.Name] = float64(0)
		case lancestore.FieldBool:
			row[field.Name] = false
		default:
			row[field.Name] = ""
		}
	}
	for name, value := range overrides {
		row[name] = value
	}
	return row
}

func writeTasksTable(t *testing.T, dir string, schema lancestore.Schema, rows []lancestore.Row) {
	t.Helper()
	ctx := context.Background()
	st, err := lancestore.Open(ctx, lancestore.Config{URI: dir, Writable: true, StrongReadConsistency: true})
	if err != nil {
		t.Fatalf("opening the seed store: %v", err)
	}
	defer func() { _ = st.Close() }()
	tbl, err := st.CreateTable(ctx, tasksTableName, schema)
	if err != nil {
		t.Fatalf("creating the seed tasks table: %v", err)
	}
	defer func() { _ = tbl.Close() }()
	if len(rows) > 0 {
		if err := tbl.Append(ctx, rows); err != nil {
			t.Fatalf("seeding %d row(s): %v", len(rows), err)
		}
	}
}

func rowsByID(t *testing.T, rows []lancestore.Row) map[string]lancestore.Row {
	t.Helper()
	out := make(map[string]lancestore.Row, len(rows))
	for _, row := range rows {
		id, _ := row["id"].(string)
		out[id] = row
	}
	return out
}

func legacyTaskRows() (lancestore.Schema, []lancestore.Row) {
	legacy := schemaWithout(taskSchema(), "session_id")
	rows := []lancestore.Row{
		seedRow(legacy, lancestore.Row{
			"id": "tsk-1111", "project_id": "project-legacy", "title": "Carried over work",
			"description": "Written before durable sessions existed.", "type": "task", "status": "open",
			"priority": int64(1), "revision": int64(4), "depends_on_json": "[]", "checks_json": "[]",
			"created_at": "2026-09-01T10:00:00Z", "updated_at": "2026-09-01T11:00:00Z",
			"search_text": "carried over work quixotic",
		}),
		seedRow(legacy, lancestore.Row{
			"id": "tsk-2222", "project_id": "project-legacy", "title": "Finished earlier",
			"description": "Also predates sessions.", "type": "bug", "status": "completed",
			"priority": int64(3), "revision": int64(9), "depends_on_json": "[]", "checks_json": "[]",
			"flagged": true, "owner": "someone", "completed_by": "someone",
			"created_at": "2026-08-20T08:00:00Z", "updated_at": "2026-08-21T08:00:00Z",
			"search_text": "finished earlier",
		}),
	}
	return legacy, rows
}

// sameStoredValue compares what was written with what came back. The generic row reader hands an
// Int64 column back as float64 whether or not anything migrated, so numbers are compared by value.
// Whether those columns still decode as integers is checked separately, through the module's own
// typed read, which is where a botched round trip would actually hurt.
func sameStoredValue(got, want any) bool {
	gotNum, gotOK := asNumber(got)
	wantNum, wantOK := asNumber(want)
	if gotOK && wantOK {
		return gotNum == wantNum
	}
	return got == want
}

func asNumber(value any) (float64, bool) {
	switch v := value.(type) {
	case int:
		return float64(v), true
	case int32:
		return float64(v), true
	case int64:
		return float64(v), true
	case float32:
		return float64(v), true
	case float64:
		return v, true
	default:
		return 0, false
	}
}

func TestAStoreWrittenBeforeSessionsMigratesAndKeepsItsTasks(t *testing.T) {
	ctx := context.Background()
	dir := t.TempDir()
	legacy, seeded := legacyTaskRows()
	writeTasksTable(t, dir, legacy, seeded)

	tables, err := openTables(ctx, dir, config.S3Config{})
	if err != nil {
		t.Fatalf("opening a store written before session_id existed: %v", err)
	}
	defer tables.close()

	if !tables.tasks.Schema().Equal(taskSchema()) {
		t.Fatalf("migrated schema = %#v, want the current task schema", tables.tasks.Schema())
	}

	got, err := tables.tasks.Rows(ctx)
	if err != nil {
		t.Fatalf("reading migrated rows: %v", err)
	}
	if len(got) != len(seeded) {
		t.Fatalf("migrated row count = %d, want %d", len(got), len(seeded))
	}

	migrated := rowsByID(t, got)
	for _, original := range seeded {
		id := original["id"].(string)
		after, ok := migrated[id]
		if !ok {
			t.Fatalf("task %s did not survive the migration", id)
		}
		for name, want := range original {
			if !sameStoredValue(after[name], want) {
				t.Errorf("task %s field %s = %#v (%T), want %#v (%T)", id, name, after[name], after[name], want, want)
			}
		}
		if after["session_id"] != "" {
			t.Errorf("task %s session_id = %#v, want empty for a task that predates sessions", id, after["session_id"])
		}
	}
}

func TestAMigratedTableIsSearchableAgain(t *testing.T) {
	ctx := context.Background()
	dir := t.TempDir()
	legacy, seeded := legacyTaskRows()
	writeTasksTable(t, dir, legacy, seeded)

	tables, err := openTables(ctx, dir, config.S3Config{})
	if err != nil {
		t.Fatalf("opening a store written before session_id existed: %v", err)
	}
	defer tables.close()

	// The inverted index lives on search_text and a recreated table starts without it, so a hit
	// here is what proves the migration left the table usable and not just readable.
	hits, err := tables.tasks.Search(ctx, lancestore.Query{Text: "quixotic", TextColumn: "search_text", Limit: 5})
	if err != nil {
		t.Fatalf("searching the migrated table: %v", err)
	}
	if len(hits) == 0 {
		t.Fatal("searching the migrated table returned no hits, want the carried-over task")
	}
}

func TestARetypedColumnIsRefusedInsteadOfMigrated(t *testing.T) {
	ctx := context.Background()
	dir := t.TempDir()
	// priority as text is not something this build can read as a number, and guessing would
	// throw away whatever the column actually holds.
	broken := schemaRetyping(taskSchema(), "priority", lancestore.FieldString)
	seeded := []lancestore.Row{seedRow(broken, lancestore.Row{"id": "tsk-3333", "title": "Do not lose me"})}
	writeTasksTable(t, dir, broken, seeded)

	if _, err := openTables(ctx, dir, config.S3Config{}); err == nil {
		t.Fatal("opening a retyped store succeeded, want it refused")
	} else if !strings.Contains(err.Error(), "incompatible schema") {
		t.Fatalf("error = %v, want it to report an incompatible schema", err)
	}

	st, err := lancestore.Open(ctx, lancestore.Config{URI: dir, Writable: false, StrongReadConsistency: true})
	if err != nil {
		t.Fatalf("reopening the refused store: %v", err)
	}
	defer func() { _ = st.Close() }()
	tbl, err := st.OpenTable(ctx, tasksTableName)
	if err != nil {
		t.Fatalf("the refused table is gone: %v", err)
	}
	defer func() { _ = tbl.Close() }()
	rows, err := tbl.Rows(ctx)
	if err != nil {
		t.Fatalf("reading the refused table: %v", err)
	}
	if len(rows) != 1 {
		t.Fatalf("refused table row count = %d, want 1 — a refusal must not drop anything", len(rows))
	}
}

func TestAStoreAlreadyOnTheCurrentSchemaIsNotRewritten(t *testing.T) {
	ctx := context.Background()
	dir := t.TempDir()
	current := taskSchema()
	seeded := []lancestore.Row{seedRow(current, lancestore.Row{
		"id": "tsk-4444", "project_id": "project-current", "title": "Already current",
		"status": "open", "priority": int64(2), "revision": int64(1), "session_id": "ses-9999",
		"depends_on_json": "[]", "checks_json": "[]",
	})}
	writeTasksTable(t, dir, current, seeded)

	first := openAndReadTasks(t, ctx, dir)
	second := openAndReadTasks(t, ctx, dir)

	if len(first) != 1 || len(second) != 1 {
		t.Fatalf("row counts = %d and %d, want 1 on both opens", len(first), len(second))
	}
	if first[0]["session_id"] != "ses-9999" || second[0]["session_id"] != "ses-9999" {
		t.Fatalf("session_id = %#v then %#v, want it untouched", first[0]["session_id"], second[0]["session_id"])
	}
	for name, want := range first[0] {
		if !sameStoredValue(second[0][name], want) {
			t.Errorf("field %s = %#v on reopen, want %#v", name, second[0][name], want)
		}
	}
}

func openAndReadTasks(t *testing.T, ctx context.Context, dir string) []lancestore.Row {
	t.Helper()
	tables, err := openTables(ctx, dir, config.S3Config{})
	if err != nil {
		t.Fatalf("opening the store: %v", err)
	}
	defer tables.close()
	rows, err := tables.tasks.Rows(ctx)
	if err != nil {
		t.Fatalf("reading tasks: %v", err)
	}
	return rows
}

func TestAMigratedTaskStillDecodesThroughTheModule(t *testing.T) {
	ctx := context.Background()
	dir := t.TempDir()
	legacy, _ := legacyTaskRows()
	_, seeded := legacyTaskRows()
	writeTasksTable(t, dir, legacy, seeded)

	// Migration moves rows through an untyped map, so the numbers have to come back as numbers
	// on the path the module actually uses, not only as raw cells.
	svc := OpenAt("project-legacy", dir)
	detail, err := svc.Get(ctx, "tsk-1111")
	if err != nil {
		t.Fatalf("reading a carried-over task: %v", err)
	}
	if detail.Task.Title != "Carried over work" {
		t.Errorf("title = %q, want the seeded title", detail.Task.Title)
	}
	if detail.Task.Priority != 1 {
		t.Errorf("priority = %d, want 1", detail.Task.Priority)
	}
	if detail.Task.Revision != 4 {
		t.Errorf("revision = %d, want 4", detail.Task.Revision)
	}
	if detail.Task.SessionID != "" {
		t.Errorf("session_id = %q, want empty for a task that predates sessions", detail.Task.SessionID)
	}

	completed, err := svc.Get(ctx, "tsk-2222")
	if err != nil {
		t.Fatalf("reading the second carried-over task: %v", err)
	}
	if completed.Task.Priority != 3 || completed.Task.Revision != 9 || !completed.Task.Flagged {
		t.Errorf("second task = priority %d, revision %d, flagged %v; want 3, 9, true",
			completed.Task.Priority, completed.Task.Revision, completed.Task.Flagged)
	}
}
