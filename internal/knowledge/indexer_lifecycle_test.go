package knowledge

import (
	"context"
	"errors"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/storelifecycle"
)

func TestIndexPipelineWaitsForWikiLifecycle(t *testing.T) {
	wikiDir := filepath.Join(t.TempDir(), "wiki")
	_, held, err := storelifecycle.TryAcquire(context.Background(), wikiDir)
	if err != nil {
		t.Fatalf("acquire wiki lifecycle: %v", err)
	}
	defer held.Release()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	_, err = RunIndexPipeline(ctx, t.TempDir(), wikiDir, IndexConfig{})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("knowledge pipeline while lifecycle locked = %v, want context canceled", err)
	}
}
