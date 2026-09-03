package daemon

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/fswatch"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func TestSyncModuleDoesNotTriggerItself(t *testing.T) {
	if testing.Short() {
		t.Skip("starts a filesystem watcher and runs the indexing pipeline")
	}
	requireParsers(t)

	projectDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(projectDir, "a.sql"),
		[]byte(plsqlFunction("FUNCAO_A")), 0o644); err != nil {
		t.Fatal(err)
	}

	ic := ast.NewAstIgnoreChecker(projectDir)

	w, err := fswatch.New(fswatch.Config{
		Root: projectDir, Ignore: ic,
		Debounce: syncDebounce, MaxDebounce: syncMaxDebounce,
	})
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = w.Close() }()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	batches, err := w.Start(ctx)
	if err != nil {
		t.Fatal(err)
	}
	time.Sleep(500 * time.Millisecond)

	mod := NewSyncModule(projectDir, store.ASTProjectDir(projectDir))

	if err := os.WriteFile(filepath.Join(projectDir, "b.sql"),
		[]byte(plsqlFunction("FUNCAO_B")), 0o644); err != nil {
		t.Fatal(err)
	}

	const settle = 15 * time.Second
	n := 0
	deadline := time.After(settle)
	for {
		select {
		case b := <-batches:
			n++
			if n > 1 {
				t.Fatalf("batch %d arrived after a single external write "+
					"(%d changed, %d removed, first: %s) — the daemon's own index "+
					"writes are being fed back into it",
					n, len(b.Changed), len(b.Removed), sampleOf(b.Changed))
			}
			mod.handleBatch(ctx, b, ic, nil)
		case <-deadline:
			if n != 1 {
				t.Fatalf("expected exactly 1 batch from 1 external write, got %d", n)
			}
			if !searchFinds(t, projectDir, "FUNCAO_B", "FUNCAO_B") {
				t.Error("no feedback loop, but the change never reached the index either")
			}
			shards, _ := filepath.Glob(filepath.Join(store.ASTProjectDir(projectDir),
				"shards", "*.json.nodes.json"))
			for _, s := range shards {
				t.Errorf("the indexer indexed its own output: %s", filepath.Base(s))
			}
			return
		}
	}
}

func sampleOf(s []string) string {
	if len(s) == 0 {
		return "<none>"
	}
	return filepath.Base(s[0])
}
