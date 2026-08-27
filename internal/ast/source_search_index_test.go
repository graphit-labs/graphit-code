package ast

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// countingEmptyDB answers every query with no rows, and records that it was asked.
type countingEmptyDB struct {
	emptyGraphDB
	queries int
}

func (d *countingEmptyDB) Query(ctx context.Context, q string, p map[string]any) (*QueryResult, error) {
	d.queries++
	return d.emptyGraphDB.Query(ctx, q, p)
}

// writeSearchIndexSource builds a real store holding one file, so the test reads the
// same SearchFile rows the search tool does.
//
// The store is closed before returning. Readers open it read-only, which the engine
// allows alongside a live writer, but leaving the write handle open would make every
// test in this file depend on that rather than state it.
func writeSearchIndexSource(t *testing.T, relPath, src string) string {
	t.Helper()
	dbPath := filepath.Join(t.TempDir(), "ladybugdb")

	idx, err := OpenSearchIndex(context.Background(), dbPath)
	if err != nil {
		t.Fatalf("open search index: %v", err)
	}
	putFileRow(t, idx, relPath, src)
	// No index build step: source reads go through a predicate on the path, not through the
	// inverted index, so a file's text is readable the moment the row lands. Under SQLite the FTS
	// tables had to be rebuilt here or the row was invisible.
	if err := idx.Close(); err != nil {
		t.Fatalf("close search index: %v", err)
	}
	return dbPath
}

// TestSourceComesFromTheSearchIndexAlone pins the store that owns file text.
//
// It used to live in two places: File.source in the graph and file_fts in the search
// index. That cost a COPY of the whole repository on every rebuild — 2.4 GB on a
// 36k-file export — and when that COPY failed the graph published File nodes with no
// text while answering "source not found", which read as missing code rather than as a
// failed load. Only the index copy was ever queryable, so it is now the only copy.
func TestSourceComesFromTheSearchIndexAlone(t *testing.T) {
	const rel = "schema/packages/PCK_X.sql"
	const body = "CREATE PACKAGE PCK_X AS\n PROCEDURE P;\nEND;"
	dbPath := writeSearchIndexSource(t, rel, body)

	db := &countingEmptyDB{}
	svc := NewSourceService(db).WithStore(dbPath)

	res, err := svc.GetSource(context.Background(), SourceRequest{Path: rel})
	if err != nil {
		t.Fatalf("GetSource: %v", err)
	}
	if res.Source != body {
		t.Errorf("source mismatch:\n got %q\nwant %q", res.Source, body)
	}
	if db.queries != 0 {
		t.Error("the graph must not be queried for file text — it no longer stores any")
	}
}

// TestFileRowsCarryNoSource is the other half of the same decision: nothing writes
// file text into the graph, so nothing can drift out of sync with the index and no
// rebuild has to push a repository-sized column through one COPY.
func TestFileRowsCarryNoSource(t *testing.T) {
	ri := newRebuildIndexForTest(map[string]*parseCacheEntry{
		"a/b.go": {Language: "go", Source: "package main\n\nfunc main() {}\n"},
	})

	rows := ri.fileNodeJSON("")
	if len(rows) != 1 {
		t.Fatalf("got %d File rows, want 1", len(rows))
	}
	if _, present := rows[0]["source"]; present {
		t.Error("a File row carries source again — the graph is not where file text lives")
	}
	if rows[0]["path"] != "a/b.go" {
		t.Errorf("path = %v, want a/b.go", rows[0]["path"])
	}
}

// SAFETY regression: reading a source must not mutate the store it reads from.
//
// The hazard is that OpenSearchIndex runs the schema migration, which DROPs and recreates
// every table on a version mismatch — so a read that opened read-write could destroy the
// index it came to read. FileSourceAt opens read-only, which cannot migrate.
//
// It stats the INDEX rather than the graph store: this test seeds a search index and no
// graph at all, which is also the arrangement that catches the mistake of stat-ing the
// wrong one — the graph path would simply not exist.
func TestFileSourceAtDoesNotMutateTheStore(t *testing.T) {
	const rel = "keep/me.sql"
	dbPath := writeSearchIndexSource(t, rel, "SELECT 1 FROM DUAL;")
	idxPath := LanceIndexPath(dbPath)

	before, err := os.Stat(idxPath)
	if err != nil {
		t.Fatalf("stat before: %v", err)
	}

	if _, ok := FileSourceAt(context.Background(), dbPath, rel); !ok {
		t.Fatal("the seeded file must be readable")
	}
	// Twice: a read that destroyed what it read would still answer the first call.
	if _, ok := FileSourceAt(context.Background(), dbPath, rel); !ok {
		t.Error("the second read failed — reading the store damaged it")
	}

	after, err := os.Stat(idxPath)
	if err != nil {
		t.Fatalf("stat after: %v", err)
	}
	if before.Size() != after.Size() || !before.ModTime().Equal(after.ModTime()) {
		t.Errorf("reading a source rewrote the index: size %d->%d, mtime %v->%v",
			before.Size(), after.Size(), before.ModTime(), after.ModTime())
	}
}

// A read must not take the write handle either, or a source lookup would lock the daemon
// out of its own store for as long as the lookup runs.
func TestFileSourceAtLeavesTheWriteHandleFree(t *testing.T) {
	const rel = "a/b.sql"
	dbPath := writeSearchIndexSource(t, rel, "SELECT 1;")

	if _, ok := FileSourceAt(context.Background(), dbPath, rel); !ok {
		t.Fatal("the seeded file must be readable")
	}

	idx, err := OpenSearchIndex(context.Background(), dbPath)
	if err != nil {
		t.Fatalf("the write handle was not free after a read: %v", err)
	}
	_ = idx.Close()
}

// A service with no store cannot reach the index, and the error has to say that
// rather than imply the file does not exist.
func TestSourceErrorNamesTheMissingStore(t *testing.T) {
	svc := NewSourceService(&countingEmptyDB{})

	_, err := svc.GetSource(context.Background(), SourceRequest{Path: "a/b.sql"})
	if err == nil {
		t.Fatal("expected an error when the service has no store")
	}
	if !strings.Contains(err.Error(), "without a") || !strings.Contains(err.Error(), "store") {
		t.Errorf("error should name the missing store, got: %v", err)
	}
}

// A path the index does not have must point at the two reasons it can happen.
func TestSourceErrorNamesIndexSourceWhenPathIsAbsent(t *testing.T) {
	dbPath := writeSearchIndexSource(t, "a/b.sql", "SELECT 1;")
	svc := NewSourceService(&countingEmptyDB{}).WithStore(dbPath)

	_, err := svc.GetSource(context.Background(), SourceRequest{Path: "not/indexed.sql"})
	if err == nil {
		t.Fatal("expected an error for a path that is not indexed")
	}
	if !strings.Contains(err.Error(), "ast.index_source") {
		t.Errorf("error should mention ast.index_source as one of the causes, got: %v", err)
	}
}

func TestFileSourceAtEdgeCases(t *testing.T) {
	const rel = "a/b.sql"
	dbPath := writeSearchIndexSource(t, rel, "SELECT 1;")
	idxPath := dbPath

	if _, ok := FileSourceAt(context.Background(), idxPath, "not/indexed.sql"); ok {
		t.Error("a path that is not in the index must not resolve")
	}
	if _, ok := FileSourceAt(context.Background(), "", rel); ok {
		t.Error("an empty index path must not resolve")
	}
	if _, ok := FileSourceAt(context.Background(), idxPath, ""); ok {
		t.Error("an empty file path must not resolve")
	}
	if _, ok := FileSourceAt(context.Background(), filepath.Join(t.TempDir(), "absent-store"), rel); ok {
		t.Error("a missing index file must not resolve")
	}
}

// An empty source is not an answer: the caller must still learn it is unavailable.
func TestFileSourceAtDeclinesEmptySource(t *testing.T) {
	const rel = "empty.sql"
	dbPath := writeSearchIndexSource(t, rel, "")

	if _, ok := FileSourceAt(context.Background(), dbPath, rel); ok {
		t.Error("an empty source must not be reported as found")
	}
}

// TestBuildSearchIndexForMakesAContextSearchable covers the gap that left a
// Hub-installed AST context traversable but neither searchable nor readable: that path
// built the graph and never opened a search index, and the shards it built from stay in
// the Hub clone rather than beside the store.
func TestBuildSearchIndexForMakesAContextSearchable(t *testing.T) {
	cacheDir := t.TempDir()
	cache, err := NewShardCache(cacheDir)
	if err != nil {
		t.Fatalf("shard cache: %v", err)
	}
	defer func() { _ = cache.Close() }()

	const rel = "svc/handler.go"
	const body = "package svc\n\nfunc HandlePayment() {}\n"
	if err := cache.Store(rel, "h1", &parseCacheEntry{Language: "go", Source: body}); err != nil {
		t.Fatalf("store shard: %v", err)
	}

	dbPath := filepath.Join(t.TempDir(), "ladybugdb")
	if err := BuildSearchIndexFor(context.Background(), dbPath, cache, nil); err != nil {
		t.Fatalf("BuildSearchIndexFor: %v", err)
	}

	got, ok := FileSourceAt(context.Background(), dbPath, rel)
	if !ok {
		t.Fatal("the built index does not serve the file it was built from")
	}
	if got != body {
		t.Errorf("source mismatch:\n got %q\nwant %q", got, body)
	}

	idx, err := OpenSearchIndex(context.Background(), dbPath)
	if err != nil {
		t.Fatalf("open index: %v", err)
	}
	defer func() { _ = idx.Close() }()

	results, err := idx.Search(context.Background(), "HandlePayment", 5)
	if err != nil {
		t.Fatalf("search: %v", err)
	}
	if len(results) == 0 {
		t.Error("the context is not searchable — building the graph alone is not enough")
	}
}
