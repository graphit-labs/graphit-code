//go:build lancedb

package lancestore

import (
	"context"
	"testing"
)

func TestCompactLatestSnapshotKeepsCurrentRowsAndOneVersion(t *testing.T) {
	ctx := context.Background()
	dir := t.TempDir()
	st, err := Open(ctx, Config{URI: dir})
	if err != nil {
		t.Fatal(err)
	}
	table, err := st.CreateTable(ctx, "items", Schema{Fields: []Field{
		{Name: "id", Type: FieldString},
		{Name: "value", Type: FieldString},
	}})
	if err != nil {
		t.Fatal(err)
	}
	if err := table.Append(ctx, []Row{{"id": "one", "value": "old"}}); err != nil {
		t.Fatal(err)
	}
	if err := table.Upsert(ctx, "id", []Row{{"id": "one", "value": "current"}}); err != nil {
		t.Fatal(err)
	}
	if err := table.Append(ctx, []Row{{"id": "two", "value": "kept"}}); err != nil {
		t.Fatal(err)
	}
	if versions, err := table.Versions(ctx); err != nil || len(versions) < 3 {
		t.Fatalf("versions before snapshot = %d, %v", len(versions), err)
	}
	if err := st.Close(); err != nil {
		t.Fatal(err)
	}

	result, err := CompactLatestSnapshot(ctx, Config{URI: dir})
	if err != nil {
		t.Fatal(err)
	}
	if result.Tables != 1 || result.OldVersions == 0 {
		t.Fatalf("snapshot result = %+v", result)
	}

	st, err = Open(ctx, Config{URI: dir})
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	table, err = st.OpenTable(ctx, "items")
	if err != nil {
		t.Fatal(err)
	}
	versions, err := table.Versions(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if len(versions) != 1 {
		t.Fatalf("versions after snapshot = %d, want 1", len(versions))
	}
	hits, err := table.Search(ctx, Query{Filter: "id = 'one'", Limit: 2})
	if err != nil {
		t.Fatal(err)
	}
	if len(hits) != 1 || hits[0].Row["value"] != "current" {
		t.Fatalf("current row after snapshot = %#v", hits)
	}
	count, err := table.Count(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if count != 2 {
		t.Fatalf("row count after snapshot = %d, want 2", count)
	}
}
