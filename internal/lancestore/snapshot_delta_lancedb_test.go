//go:build lancedb

package lancestore

import (
	"context"
	"path/filepath"
	"reflect"
	"testing"
)

func TestApplySnapshotDeltaWritesOnlyChangedKeysAndPreservesTaggedBase(t *testing.T) {
	ctx := context.Background()
	root := t.TempDir()
	store, err := Open(ctx, Config{URI: root, Writable: true})
	if err != nil {
		t.Fatal(err)
	}
	table, err := store.CreateTable(ctx, "items", Schema{Fields: []Field{
		{Name: "section", Type: FieldString}, {Name: "key", Type: FieldString}, {Name: "value", Type: FieldString},
	}})
	if err != nil {
		t.Fatal(err)
	}
	base := []Row{
		{"section": "one", "key": "same", "value": "before"},
		{"section": "one", "key": "edit", "value": "old"},
		{"section": "two", "key": "delete", "value": "old"},
	}
	if err := table.Append(ctx, base); err != nil {
		t.Fatal(err)
	}
	baseVersion, err := table.CurrentVersion(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if err := table.PutTag(ctx, "git-base", baseVersion); err != nil {
		t.Fatal(err)
	}
	desired := []Row{
		{"section": "one", "key": "same", "value": "before"},
		{"section": "one", "key": "edit", "value": "new"},
		{"section": "two", "key": "add", "value": "new"},
	}
	changed, err := table.ApplySnapshotDelta(ctx, []string{"section", "key"}, desired)
	if err != nil || changed != 3 {
		t.Fatalf("delta changed=%d err=%v, want 3", changed, err)
	}
	rows, err := table.Rows(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if !sameSnapshotRows(rows, desired) {
		t.Fatalf("current rows = %#v, want %#v", rows, desired)
	}
	version, err := table.CurrentVersion(ctx)
	if err != nil {
		t.Fatal(err)
	}
	changed, err = table.ApplySnapshotDelta(ctx, []string{"section", "key"}, desired)
	if err != nil || changed != 0 {
		t.Fatalf("repeat delta changed=%d err=%v, want 0", changed, err)
	}
	if current, err := table.CurrentVersion(ctx); err != nil || current != version {
		t.Fatalf("repeat delta version=%d err=%v, want %d", current, err, version)
	}
	if err := store.Close(); err != nil {
		t.Fatal(err)
	}
	clone, err := Open(ctx, Config{URI: t.TempDir(), Writable: true})
	if err != nil {
		t.Fatal(err)
	}
	old, err := clone.CloneTable(ctx, "items", filepath.Join(root, "items.lance"), CloneOptions{SourceTag: "git-base"})
	if err != nil {
		t.Fatal(err)
	}
	oldRows, err := old.Rows(ctx)
	if err != nil || !sameSnapshotRows(oldRows, base) {
		t.Fatalf("tagged base rows=%#v err=%v", oldRows, err)
	}
	if err := clone.Close(); err != nil {
		t.Fatal(err)
	}
}

func sameSnapshotRows(left, right []Row) bool {
	if len(left) != len(right) {
		return false
	}
	byKey := make(map[string]Row, len(left))
	for _, row := range left {
		byKey[row["section"].(string)+"/"+row["key"].(string)] = row
	}
	for _, row := range right {
		if !reflect.DeepEqual(byKey[row["section"].(string)+"/"+row["key"].(string)], row) {
			return false
		}
	}
	return true
}
