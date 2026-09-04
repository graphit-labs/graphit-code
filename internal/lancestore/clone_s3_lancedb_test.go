//go:build lancedb

package lancestore

import (
	"context"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/testsupport"
)

func TestS3ShallowCloneReadsInheritedRowsAndWritesLocally(t *testing.T) {
	ctx := context.Background()
	_, endpoint := testsupport.StartFakeS3(t, "graphit-lance")
	s3Config := config.S3Config{
		Bucket:          "graphit-lance",
		Region:          "us-east-1",
		Endpoint:        endpoint,
		AccessKeyID:     "test-key",
		SecretAccessKey: "test-secret",
	}
	sourceURI := "s3://graphit-lance/branches/main"
	source, err := Open(ctx, Config{URI: sourceURI, S3: s3Config, Writable: true})
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
	if err := table.Append(ctx, []Row{{"id": "base", "value": "s3"}}); err != nil {
		t.Fatal(err)
	}
	version, err := table.CurrentVersion(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if err := table.PutTag(ctx, "git-base", version); err != nil {
		t.Fatal(err)
	}
	if err := source.Close(); err != nil {
		t.Fatal(err)
	}

	targetDir := t.TempDir()
	target, err := Open(ctx, Config{URI: targetDir, S3: s3Config, Writable: true})
	if err != nil {
		t.Fatal(err)
	}
	cloned, err := target.CloneTable(ctx, "items", sourceURI+"/items.lance", CloneOptions{SourceTag: "git-base"})
	if err != nil {
		t.Fatal(err)
	}
	if files := regularFilesUnder(filepath.Join(targetDir, "items.lance", "data")); files != 0 {
		t.Fatalf("shallow clone copied %d inherited data files", files)
	}
	if err := cloned.Append(ctx, []Row{{"id": "local", "value": "overlay"}}); err != nil {
		t.Fatal(err)
	}
	if err := target.Close(); err != nil {
		t.Fatal(err)
	}

	target, err = Open(ctx, Config{URI: targetDir, S3: s3Config, Writable: true})
	if err != nil {
		t.Fatal(err)
	}
	defer target.Close()
	cloned, err = target.OpenTable(ctx, "items")
	if err != nil {
		t.Fatal(err)
	}
	if count, err := cloned.Count(ctx); err != nil || count != 2 {
		t.Fatalf("clone count = %d, %v", count, err)
	}

	source, err = Open(ctx, Config{URI: sourceURI, S3: s3Config})
	if err != nil {
		t.Fatal(err)
	}
	defer source.Close()
	table, err = source.OpenTable(ctx, "items")
	if err != nil {
		t.Fatal(err)
	}
	if count, err := table.Count(ctx); err != nil || count != 1 {
		t.Fatalf("source count = %d, %v", count, err)
	}
}
