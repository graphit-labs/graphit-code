//go:build lancedb

package wiki

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

func TestStagePublishedIndexCopiesAQueryableWiki(t *testing.T) {
	tmp := t.TempDir()

	srcDir := filepath.Join(tmp, "src")
	ctx := context.Background()
	src, err := OpenWikiDB(ctx, srcDir)
	if err != nil {
		t.Fatalf("open src: %v", err)
	}
	chunks := testChunks()
	xrefs := map[string][]string{"autenticacao": {"indexacao"}}
	if err := src.Sync(ctx, chunks, xrefs, &SyncLogEntry{
		Timestamp: "2026-08-18T00:00:00Z", TotalDocs: len(chunks), ArticlesWritten: len(chunks),
	}); err != nil {
		_ = src.Close()
		t.Fatalf("build src: %v", err)
	}
	_ = src.Close()

	out := filepath.Join(tmp, "artifact")
	written, err := StagePublishedIndex(ctx, srcDir, out)
	if err != nil {
		t.Fatalf("export: %v", err)
	}
	if written == 0 {
		t.Fatal("export copied no bytes")
	}
	if _, err := os.Stat(WikiIndexPath(out)); err != nil {
		t.Fatalf("published index is missing: %v", err)
	}

	dst, err := OpenWikiDB(ctx, out)
	if err != nil {
		t.Fatalf("open dst: %v", err)
	}
	defer func() { _ = dst.Close() }()

	gotChunks, gotSlugs, gotXRefs, gotLogs, err := dst.Stats(ctx)
	if err != nil {
		t.Fatalf("stats: %v", err)
	}
	if gotChunks != len(chunks) || gotSlugs != len(chunks) {
		t.Errorf("chunks=%d slugs=%d, want %d of each", gotChunks, gotSlugs, len(chunks))
	}
	if gotXRefs == 0 {
		t.Error("cross-references did not survive publication")
	}
	if gotLogs == 0 {
		t.Error("the sync log did not survive publication")
	}

	results, err := dst.Search(ctx, "credenciais", 5)
	if err != nil {
		t.Fatalf("search: %v", err)
	}
	if len(results) == 0 {
		t.Fatal("full-text search returned nothing after the round trip")
	}

	var want WikiChunk
	for _, c := range chunks {
		if c.Slug == "autenticacao" {
			want = c
		}
	}
	for _, got := range results {
		if got.Slug != want.Slug {
			continue
		}
		if got.Title != want.Title {
			t.Errorf("title = %q, want %q", got.Title, want.Title)
		}
		if got.Source != want.Source {
			t.Errorf("source = %q, want %q — columns may have shifted", got.Source, want.Source)
		}
		if got.Breadcrumb != want.Breadcrumb {
			t.Errorf("breadcrumb = %q, want %q", got.Breadcrumb, want.Breadcrumb)
		}
		if got.DocType != want.DocType {
			t.Errorf("doc_type = %q, want %q", got.DocType, want.DocType)
		}
		return
	}
	t.Errorf("the chunk searched for is not among the results: %v", results)
}
