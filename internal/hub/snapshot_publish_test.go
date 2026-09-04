//go:build lancedb

package hub

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

func TestTagKnowledgePublicationContainsOnlyCurrentTableVersions(t *testing.T) {
	ctx := context.Background()
	srcDir := filepath.Join(t.TempDir(), "wiki")
	db, err := wiki.OpenWikiDB(ctx, srcDir)
	if err != nil {
		t.Fatal(err)
	}
	chunks := []wiki.WikiChunk{{Slug: "current", Title: "Old", Body: "old body"}}
	if err := db.Sync(ctx, chunks, nil, &wiki.SyncLogEntry{Timestamp: "2026-09-04T00:00:00Z", TotalDocs: 1, ArticlesWritten: 1}); err != nil {
		t.Fatal(err)
	}
	chunks[0].Title = "Current"
	chunks[0].Body = "current body"
	if err := db.Sync(ctx, chunks, nil, &wiki.SyncLogEntry{Timestamp: "2026-09-04T00:01:00Z", TotalDocs: 1, ArticlesWritten: 1}); err != nil {
		t.Fatal(err)
	}
	if err := db.Close(); err != nil {
		t.Fatal(err)
	}

	staged, err := prepareKnowledgePublishVersion(ctx, srcDir, true, config.S3Config{})
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(staged)

	store, err := lancestore.Open(ctx, lancestore.Config{URI: wiki.WikiIndexPath(staged)})
	if err != nil {
		t.Fatal(err)
	}
	names, err := store.TableNames(ctx)
	if err != nil {
		t.Fatal(err)
	}
	for _, name := range names {
		table, err := store.OpenTable(ctx, name)
		if err != nil {
			t.Fatal(err)
		}
		versions, err := table.Versions(ctx)
		if err != nil {
			t.Fatal(err)
		}
		if len(versions) != 1 {
			t.Errorf("%s retained %d table versions, want 1", name, len(versions))
		}
	}
	if err := store.Close(); err != nil {
		t.Fatal(err)
	}

	published, err := wiki.OpenWikiDB(ctx, staged)
	if err != nil {
		t.Fatal(err)
	}
	defer published.Close()
	results, err := published.Search(ctx, "current body", 5)
	if err != nil {
		t.Fatal(err)
	}
	if len(results) != 1 || results[0].Title != "Current" {
		t.Fatalf("published current state = %#v", results)
	}
}
