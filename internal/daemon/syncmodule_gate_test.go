package daemon

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/fswatch"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

// A batch with nothing to reindex must not queue for the indexing slot. One
// supervisor's empty batch waiting behind another project's full rebuild would turn
// the gate into a latency amplifier for work that was never going to happen.
func TestHandleBatchSkipsTheGateWhenThereIsNoWork(t *testing.T) {
	projectDir := t.TempDir()
	mod := NewSyncModule(projectDir, store.ASTProjectDir(projectDir))

	// Hold the only slot, so anything that tries to acquire one blocks.
	release, err := sysutil.AcquireHeavy(context.Background())
	if err != nil {
		t.Fatalf("AcquireHeavy: %v", err)
	}
	defer release()

	done := make(chan struct{})
	go func() {
		defer close(done)
		mod.handleBatch(context.Background(), fswatch.Batch{}, ast.NewAstIgnoreChecker(projectDir), nil)
	}()

	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("handleBatch queued for the indexing slot with an empty batch")
	}
}

// A parked or shutting-down supervisor must abandon the queue rather than reindex on
// the way out — and must not strand the slot it never got.
func TestHandleBatchAbandonsTheQueueOnCancel(t *testing.T) {
	projectDir := t.TempDir()
	mod := NewSyncModule(projectDir, store.ASTProjectDir(projectDir))

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	// Rescan is work for both indexers no matter what the batch names, so this
	// reaches the gate and nothing else.
	mod.handleBatch(ctx, fswatch.Batch{Rescan: true}, ast.NewAstIgnoreChecker(projectDir), nil)

	if _, err := os.Stat(filepath.Join(projectDir, brand.DotDir(), "ast")); !os.IsNotExist(err) {
		t.Errorf("a cancelled handleBatch still opened the AST database (stat err = %v)", err)
	}

	// The slot must still be free: a gate that leaks on the cancel path deadlocks
	// the daemon after enough parked supervisors.
	acquired := make(chan struct{})
	go func() {
		release, err := sysutil.AcquireHeavy(context.Background())
		if err == nil {
			release()
		}
		close(acquired)
	}()

	select {
	case <-acquired:
	case <-time.After(5 * time.Second):
		t.Fatal("the indexing slot was never returned after a cancelled handleBatch")
	}
}
