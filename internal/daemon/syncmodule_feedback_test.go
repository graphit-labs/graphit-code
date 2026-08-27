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

// The daemon writes its index into .graphit, which sits inside the directory it
// watches. Those writes must not come back as events.
//
// A project set up by `graphit init` has .graphit/ in its .gitignore and the
// watcher honours that, so this is the second line of defence rather than the
// first — but the first is best-effort (the injection's error is downgraded to a
// warning in one caller and discarded in the other), and what it guards against
// does not degrade gracefully. The loop amplifies: a shard is a .json file and
// .json has a parser, so indexing a shard emits a shard for the shard —
// a.sql.nodes.json becomes a.sql.nodes.json.nodes.json — and each round produces
// more files than the last (measured: 1, 5, 14, 25, 51, 99 … still climbing when
// the probe hit its two-minute timeout).
//
// Hence no .gitignore here: the guard has to hold on its own. A single external
// write must produce exactly one batch.
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

	// The same checker Start builds, so this exercises the production guard
	// rather than a copy of it.
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

	// One external write, then the tree is left alone. Everything that happens
	// from here on is the daemon reacting to itself.
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
			// The write must still have been indexed — a watcher that reports
			// nothing would also pass a "no feedback" check.
			if !searchFinds(t, projectDir, "FUNCAO_B", "FUNCAO_B") {
				t.Error("no feedback loop, but the change never reached the index either")
			}
			// And no shard-of-a-shard may exist.
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
