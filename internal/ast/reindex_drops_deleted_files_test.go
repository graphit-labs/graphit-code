//go:build lancedb

package ast

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

func TestForceRebuildDropsAFileThatLeftTheDisk(t *testing.T) {
	work := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	tmp := t.TempDir()

	const total = 12
	for i := 0; i < total; i++ {
		name := fmt.Sprintf("f%02d.go", i)
		src := fmt.Sprintf("package p\n\nfunc Fn%02d() {}\n", i)
		if err := os.WriteFile(filepath.Join(work, name), []byte(src), 0o644); err != nil {
			t.Fatalf("write fixture: %v", err)
		}
	}

	dbPath := filepath.Join(tmp, "ladybugdb")
	cacheDir := filepath.Join(tmp, "cache")
	opts := PipelineOptions{CacheDir: cacheDir}

	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	if _, err := RunPipeline(context.Background(), db, work, opts); err != nil {
		_ = db.Close()
		t.Fatalf("first index: %v", err)
	}
	_ = db.Close()

	const doomed = "f07.go"
	const doomedFn = "Fn07"

	if n := countGraphEntities(t, dbPath, doomedFn); n == 0 {
		t.Fatalf("fixture did not land: %s is absent from the graph before the deletion", doomedFn)
	}
	if n := countIndexedEntities(t, dbPath, doomed); n == 0 {
		t.Fatalf("fixture did not land: %s is absent from the search index before the deletion", doomed)
	}

	if err := os.Remove(filepath.Join(work, doomed)); err != nil {
		t.Fatalf("remove fixture: %v", err)
	}

	forced := opts
	forced.ForceRebuild = true

	db2 := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	if _, err := RunPipeline(context.Background(), db2, work, forced); err != nil {
		_ = db2.Close()
		t.Fatalf("reindex: %v", err)
	}
	_ = db2.Close()

	if n := countGraphEntities(t, dbPath, doomedFn); n != 0 {
		t.Errorf("the graph still holds %d node(s) for %s after a reindex that followed its deletion", n, doomedFn)
	}
	if n := countIndexedEntities(t, dbPath, doomed); n != 0 {
		t.Errorf("the search index still holds %d row(s) for %s after a reindex that followed its deletion", n, doomed)
	}
	if n := countIndexedFiles(t, dbPath, doomed); n != 0 {
		t.Errorf("the search index still holds the file row for %s, so ast source would still serve its text", doomed)
	}

	if n := countGraphEntities(t, dbPath, "Fn06"); n == 0 {
		t.Error("Fn06 disappeared too — the prune removed more than the deleted file")
	}
}

func countGraphEntities(t *testing.T, dbPath, name string) int {
	t.Helper()
	storeDir := filepath.Dir(dbPath)

	db := NewLadybugDB(LadybugConfig{StoreDir: storeDir, IcebugDir: filepath.Join(storeDir, "graph.icebug")})
	defer func() { _ = db.Close() }()

	res, err := db.Query(context.Background(),
		"MATCH (n:Function) WHERE n.name = $name AND n.is_stub = false RETURN count(n) AS n",
		map[string]any{"name": name})
	if err != nil {
		t.Fatalf("count graph nodes for %s: %v", name, err)
	}
	return toInt(res.Records[0]["n"])
}

func countIndexedEntities(t *testing.T, dbPath, relPath string) int {
	t.Helper()

	idx, err := OpenSearchIndex(context.Background(), filepath.Dir(dbPath))
	if err != nil {
		t.Fatalf("open search index: %v", err)
	}
	defer func() { _ = idx.Close() }()

	n, _ := idx.CountForPath(context.Background(), relPath)
	return n
}

func countIndexedFiles(t *testing.T, dbPath, relPath string) int {
	t.Helper()

	idx, err := OpenSearchIndex(context.Background(), filepath.Dir(dbPath))
	if err != nil {
		t.Fatalf("open search index: %v", err)
	}
	defer func() { _ = idx.Close() }()

	_, n := idx.CountForPath(context.Background(), relPath)
	return n
}
