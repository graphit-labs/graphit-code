//go:build lancedb

package memory

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/projectversion"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func withTableDimensions(t *testing.T, dims int) {
	t.Helper()
	previous := tableEmbeddingDimensions
	tableEmbeddingDimensions = func() int { return dims }
	t.Cleanup(func() { tableEmbeddingDimensions = previous })
}

func vectorOf(width int) []float32 {
	v := make([]float32, width)
	for i := range v {
		v[i] = float32(i + 1)
	}
	return v
}

// An embedding width that is momentarily unknown — the model not configured or not
// reachable while a daemon restarts — once made every open drop the memories table.
// It must leave an existing table exactly as it is and never create a missing one.
func TestOpenMemoryTableNeverDropsWhileTheWidthIsUnknown(t *testing.T) {
	ctx := context.Background()
	dir := t.TempDir()
	withTableDimensions(t, 8)
	tbl, err := OpenMemoryTable(ctx, dir)
	if err != nil {
		t.Fatal(err)
	}
	if err := tbl.Put(ctx, MemoryRecord{ID: "kept", Title: "kept", Body: "kept", Type: "fact", Revision: 1, Embedding: vectorOf(8)}); err != nil {
		t.Fatal(err)
	}
	_ = tbl.Close()

	withTableDimensions(t, 0)
	tbl, err = OpenMemoryTable(ctx, dir)
	if err != nil {
		t.Fatalf("opening while the width is unknown: %v", err)
	}
	if !tbl.table.Schema().Equal(memoryTableSchema(8)) {
		t.Fatalf("the schema changed while the width was unknown: %+v", tbl.table.Schema())
	}
	rec, ok, err := tbl.Get(ctx, "kept")
	_ = tbl.Close()
	if err != nil || !ok || len(rec.Embedding) != 8 {
		t.Fatalf("the memory or its vector was lost: ok=%v err=%v vector=%d", ok, err, len(rec.Embedding))
	}

	empty := t.TempDir()
	if _, err := OpenMemoryTable(ctx, empty); err == nil {
		t.Fatal("a table was opened without a width to create it with")
	}
	st, err := lancestore.Open(ctx, lancestore.Config{URI: empty})
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = st.Close() }()
	if names, _ := st.TableNames(ctx); len(names) != 0 {
		t.Fatalf("tables were created without a width: %v", names)
	}
}

// A new embedding width rebuilds the table, and every memory survives it — on the main
// lineage and on each project version branch — with only the old vectors dropped.
func TestANewEmbeddingWidthKeepsEveryVersionsMemories(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}
	ctx := context.Background()
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv(projectversion.EnvVersion(), "")
	mainDir := t.TempDir()
	gitIn(t, mainDir, "init", "-q", "-b", "main")
	if err := os.WriteFile(filepath.Join(mainDir, brand.LockFileName()),
		[]byte(`{"project":{"id":"`+versionTestProject+`"}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	gitIn(t, mainDir, "add", ".")
	gitIn(t, mainDir, "commit", "-q", "-m", "init")
	featureDir := filepath.Join(t.TempDir(), "feature")
	gitIn(t, mainDir, "worktree", "add", "-q", "-b", "feature/m", featureDir)

	withTableDimensions(t, 8)
	uri := store.MemoryTableDir("project", versionTestProject)
	put := func(dir, id string) {
		tbl, err := OpenMemoryTableIn(ctx, uri, dir)
		if err != nil {
			t.Fatal(err)
		}
		defer func() { _ = tbl.Close() }()
		if err := tbl.Put(ctx, MemoryRecord{ID: id, Title: id, Body: id, Type: "fact", Revision: 1, Embedding: vectorOf(8)}); err != nil {
			t.Fatal(err)
		}
	}
	put(mainDir, "shared-fact")
	put(featureDir, "feature-fact")
	put(mainDir, "main-fact")

	withTableDimensions(t, 16)
	if got := memoryIDs(t, uri, mainDir); len(got) != 2 || !got["shared-fact"] || !got["main-fact"] {
		t.Fatalf("main memory after the rebuild = %v", got)
	}
	if got := memoryIDs(t, uri, featureDir); len(got) != 2 || !got["shared-fact"] || !got["feature-fact"] {
		t.Fatalf("feature memory after the rebuild = %v", got)
	}
	for _, dir := range []string{mainDir, featureDir} {
		tbl, err := OpenMemoryTableIn(ctx, uri, dir)
		if err != nil {
			t.Fatal(err)
		}
		if !tbl.table.Schema().Equal(memoryTableSchema(16)) {
			t.Fatalf("%s kept the old schema: %+v", dir, tbl.table.Schema())
		}
		pending, err := tbl.PendingEmbeddings(ctx)
		_ = tbl.Close()
		if err != nil || len(pending) != 2 {
			t.Fatalf("the old vectors were not cleared for re-embedding: %d pending, %v", len(pending), err)
		}
	}
	snapshots, _ := filepath.Glob(filepath.Join(lancestore.RebuildBackupDir(store.Root(), uri), "*.pending.json"))
	if len(snapshots) != 0 {
		t.Fatalf("the rebuild left pending snapshots: %v", snapshots)
	}
}
