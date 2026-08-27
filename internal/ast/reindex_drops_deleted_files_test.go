package ast

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

// TestForceRebuildDropsAFileThatLeftTheDisk covers the mode `ast index --reindex`
// selects: ForceRebuild, which is the one mode whose entire purpose is to distrust
// what is cached and rebuild from the tree as it is now.
//
// It regressed on 2026-08-05 (a6dd378) and nothing caught it, because the graph
// delete that ran before the pipeline hid it for eight days — and that delete's
// output has been discarded by the atomic swap ever since. The deletion detection
// that actually matters lives in the pipeline and used to sit inside the
// `!ForceRebuild` branch, so the one mode that promises a clean rebuild was the one
// mode that never pruned the cache it rebuilds from.
//
// Both stores are asserted on purpose: the graph and the search index are built from
// the same shard cache, so a stale shard is republished into both, and the index is
// the copy that answers `ast source`.
func TestForceRebuildDropsAFileThatLeftTheDisk(t *testing.T) {
	work := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	tmp := t.TempDir()

	// Below shrinkFloor (20), so the catastrophic-shrink guard cannot be what makes
	// this pass or fail — the subject here is the prune, not the publish gate.
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

	db := NewLadybugDB(LadybugConfig{DBPath: dbPath})
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

	// This is `--reindex`.
	forced := opts
	forced.ForceRebuild = true

	db2 := NewLadybugDB(LadybugConfig{DBPath: dbPath})
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

	// The survivors are the other half of the assertion: a prune that took the whole
	// corpus with it would satisfy every check above.
	if n := countGraphEntities(t, dbPath, "Fn06"); n == 0 {
		t.Error("Fn06 disappeared too — the prune removed more than the deleted file")
	}
}

func countGraphEntities(t *testing.T, dbPath, name string) int {
	t.Helper()

	db := NewLadybugDB(LadybugConfig{DBPath: dbPath})
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

	idx, err := OpenSearchIndex(context.Background(), dbPath)
	if err != nil {
		t.Fatalf("open search index: %v", err)
	}
	defer func() { _ = idx.Close() }()

	n, _ := idx.CountForPath(context.Background(), relPath)
	return n
}

func countIndexedFiles(t *testing.T, dbPath, relPath string) int {
	t.Helper()

	idx, err := OpenSearchIndex(context.Background(), dbPath)
	if err != nil {
		t.Fatalf("open search index: %v", err)
	}
	defer func() { _ = idx.Close() }()

	_, n := idx.CountForPath(context.Background(), relPath)
	return n
}
