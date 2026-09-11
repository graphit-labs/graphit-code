//go:build lancedb

package lancestore

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

func TestShallowCloneKeepsInheritedDataAtSourceAndLocalWritesLocal(t *testing.T) {
	ctx := context.Background()
	sourceDir := t.TempDir()
	source, err := Open(ctx, Config{URI: sourceDir})
	if err != nil {
		t.Fatal(err)
	}
	table, err := source.CreateTable(ctx, "items", Schema{Fields: []Field{
		{Name: "id", Type: FieldString},
		{Name: "value", Type: FieldString},
	}})
	if err != nil {
		t.Fatal(err)
	}
	if err := table.Append(ctx, []Row{{"id": "base", "value": "remote"}}); err != nil {
		t.Fatal(err)
	}
	baseVersion, err := table.CurrentVersion(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if err := table.PutTag(ctx, "git-base", baseVersion); err != nil {
		t.Fatal(err)
	}
	if err := source.Close(); err != nil {
		t.Fatal(err)
	}

	targetDir := t.TempDir()
	target, err := Open(ctx, Config{URI: targetDir})
	if err != nil {
		t.Fatal(err)
	}
	cloned, err := target.CloneTable(ctx, "items", filepath.Join(sourceDir, "items.lance"), CloneOptions{SourceTag: "git-base"})
	if err != nil {
		t.Fatal(err)
	}
	if files := regularFilesUnder(filepath.Join(targetDir, "items.lance", "data")); files != 0 {
		t.Fatalf("shallow clone copied %d inherited data files", files)
	}
	if err := cloned.Append(ctx, []Row{{"id": "local", "value": "dirty"}}); err != nil {
		t.Fatal(err)
	}
	if err := target.Close(); err != nil {
		t.Fatal(err)
	}

	target, err = Open(ctx, Config{URI: targetDir})
	if err != nil {
		t.Fatal(err)
	}
	cloned, err = target.OpenTable(ctx, "items")
	if err != nil {
		t.Fatal(err)
	}
	if count, err := cloned.Count(ctx); err != nil || count != 2 {
		t.Fatalf("reopened clone count = %d, %v", count, err)
	}
	if err := target.Close(); err != nil {
		t.Fatal(err)
	}

	source, err = Open(ctx, Config{URI: sourceDir})
	if err != nil {
		t.Fatal(err)
	}
	table, err = source.OpenTable(ctx, "items")
	if err != nil {
		t.Fatal(err)
	}
	if count, err := table.Count(ctx); err != nil || count != 1 {
		t.Fatalf("source count = %d, %v", count, err)
	}
	if err := source.Close(); err != nil {
		t.Fatal(err)
	}

	if _, err := CompactLatestSnapshot(ctx, Config{URI: targetDir}); err != nil {
		t.Fatal(err)
	}
	if err := os.RemoveAll(sourceDir); err != nil {
		t.Fatal(err)
	}
	target, err = Open(ctx, Config{URI: targetDir})
	if err != nil {
		t.Fatal(err)
	}
	defer target.Close()
	cloned, err = target.OpenTable(ctx, "items")
	if err != nil {
		t.Fatal(err)
	}
	if count, err := cloned.Count(ctx); err != nil || count != 2 {
		t.Fatalf("materialized snapshot count = %d, %v", count, err)
	}
	if versions, err := cloned.Versions(ctx); err != nil || len(versions) != 1 {
		t.Fatalf("materialized snapshot versions = %d, %v", len(versions), err)
	}
}

func regularFilesUnder(root string) int {
	count := 0
	_ = filepath.Walk(root, func(_ string, info os.FileInfo, err error) error {
		if err == nil && info.Mode().IsRegular() {
			count++
		}
		return nil
	})
	return count
}
