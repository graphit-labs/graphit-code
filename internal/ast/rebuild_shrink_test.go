package ast

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

// The first layer, upstream of the guard: a scoped run — the shape the daemon uses,
// naming the files a watcher saw change — must not treat an empty cache as "the project
// is these files". Discovery costs one slow pass; the alternative cost the graph.
func TestScopedRunWithAnEmptyCacheRediscoversTheProject(t *testing.T) {
	work := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	tmp := t.TempDir()

	const total = 30
	for i := 0; i < total; i++ {
		name := filepath.Join(work, "f"+string(rune('a'+i%26))+string(rune('a'+i/26))+".go")
		src := "package p\n\nfunc F" + string(rune('A'+i%26)) + string(rune('a'+i/26)) + "() {}\n"
		if err := os.WriteFile(name, []byte(src), 0o644); err != nil {
			t.Fatalf("write fixture: %v", err)
		}
	}

	dbPath := filepath.Join(tmp, "ladybugdb")
	cacheDir := filepath.Join(tmp, "cache")

	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	if _, err := RunPipeline(context.Background(), db, work, PipelineOptions{CacheDir: cacheDir}); err != nil {
		_ = db.Close()
		t.Fatalf("first index: %v", err)
	}
	_ = db.Close()

	// The cache is discarded — which is exactly what a shardCacheVersion bump does.
	if err := os.RemoveAll(cacheDir); err != nil {
		t.Fatalf("drop cache: %v", err)
	}

	// The watcher reports one file. Before the fallback, this published a one-file graph.
	touched := filepath.Join(work, "faa.go")
	if err := os.WriteFile(touched, []byte("package p\n\nfunc FAa() { _ = 1 }\n"), 0o644); err != nil {
		t.Fatalf("touch: %v", err)
	}

	db2 := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	res, err := RunPipelineForPaths(context.Background(), db2, work, []string{touched}, nil,
		PipelineOptions{CacheDir: cacheDir})
	if err != nil {
		_ = db2.Close()
		t.Fatalf("scoped run: %v", err)
	}
	_ = db2.Close()

	if res.TotalFiles < total {
		t.Errorf("the scoped run saw %d files; it should have rediscovered all %d", res.TotalFiles, total)
	}

	graph := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	defer func() { _ = graph.Close() }()
	count, err := graph.Query(context.Background(), "MATCH (f:File) RETURN count(f) AS n", nil)
	if err != nil {
		t.Fatalf("count: %v", err)
	}
	if got := toInt(count.Records[0]["n"]); got < total {
		t.Errorf("the graph holds %d files after a scoped run with an empty cache, want %d", got, total)
	}
}
