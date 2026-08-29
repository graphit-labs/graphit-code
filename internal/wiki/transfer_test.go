//go:build lancedb

package wiki

import (
	"context"
	"path/filepath"
	"testing"
)

// TestWikiParquetRoundTrip exports a built wiki and loads it into an empty one, then checks
// that what a consumer actually does with a wiki still works: counting, full-text search and
// reading a chunk back whole.
//
// Counting alone would not catch the failure worth catching. COPY maps Parquet columns by
// POSITION, so a mismatch preserves every count and moves the values one column over —
// which is why the assertions here read fields rather than totals.
func TestWikiParquetRoundTrip(t *testing.T) {
	tmp := t.TempDir()

	srcDir := filepath.Join(tmp, "src")
	ctx := context.Background()
	src, err := OpenWikiDB(ctx, srcDir)
	if err != nil {
		t.Fatalf("open src: %v", err)
	}
	chunks := testChunks()
	xrefs := map[string][]string{"autenticacao": {"indexacao"}}
	if err := src.Rebuild(ctx, chunks, xrefs, &SyncLogEntry{
		Timestamp: "2026-08-18T00:00:00Z", TotalDocs: len(chunks), ArticlesWritten: len(chunks),
	}, nil); err != nil {
		_ = src.Close()
		t.Fatalf("build src: %v", err)
	}
	_ = src.Close()

	out := filepath.Join(tmp, "artifact", BundleDir)
	written, err := ExportToParquet(ctx, srcDir, out)
	if err != nil {
		t.Fatalf("export: %v", err)
	}
	if written == 0 {
		t.Fatal("export copied no bytes")
	}
	if !HasBundle(filepath.Join(tmp, "artifact")) {
		t.Fatal("finished export is not detected as a bundle")
	}

	dstDir := filepath.Join(tmp, "dst")
	n, err := ImportFromParquet(ctx, dstDir, out)
	if err != nil {
		t.Fatalf("import: %v", err)
	}
	if n != len(chunks) {
		t.Fatalf("imported %d chunks, wrote %d", n, len(chunks))
	}

	dst, err := OpenWikiDB(ctx, dstDir)
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
		t.Error("cross-references did not survive the round trip")
	}
	if gotLogs == 0 {
		t.Error("the sync log did not survive the round trip")
	}

	// The index has to ANSWER, not merely exist. What changed is why that is a real test: it is no
	// longer rebuilt on this side. The inverted and vector indexes travelled inside the directory,
	// so a search working here proves the copied structure is usable — which a rebuild would have
	// masked by regenerating whatever arrived broken.
	results, err := dst.Search(ctx, "credenciais", 5)
	if err != nil {
		t.Fatalf("search: %v", err)
	}
	if len(results) == 0 {
		t.Fatal("full-text search returned nothing after the round trip")
	}

	// And a whole row, field by field, against what went in.
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
