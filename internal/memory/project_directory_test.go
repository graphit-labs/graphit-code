//go:build lancedb

package memory

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/projectversion"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func TestMemoryExplicitProjectDirectoryKeepsVersionOutsideCWD(t *testing.T) {
	dir := t.TempDir()
	id := "01M00000000000000000000001"
	if err := os.WriteFile(filepath.Join(dir, brand.LockFileName()), []byte(`{"project":{"id":"`+id+`"}}`), 0600); err != nil {
		t.Fatal(err)
	}
	uri := store.MemoryTableDir("project", id)
	ctx := WithProjectDirectory(context.Background(), dir)
	release, err := projectversion.Acquire(dir, "selected")
	if err != nil {
		t.Fatal(err)
	}
	defer release()
	table, err := OpenMemoryTable(ctx, uri)
	if err != nil {
		t.Fatal(err)
	}
	defer table.Close()
	if err := table.Put(ctx, MemoryRecord{ID: "branch-only", Title: "Branch", Body: "Scoped fixture", Scope: "project", ScopeID: id}); err != nil {
		t.Fatal(err)
	}
	main, err := OpenMemoryTable(context.Background(), uri)
	if err != nil {
		t.Fatal(err)
	}
	defer main.Close()
	if _, ok, err := main.Get(context.Background(), "branch-only"); err != nil || ok {
		t.Fatalf("branch write reached default lineage: %v, %v", ok, err)
	}
	if _, ok, err := table.Get(ctx, "branch-only"); err != nil || !ok {
		t.Fatalf("selected lineage missing write: %v, %v", ok, err)
	}
}
