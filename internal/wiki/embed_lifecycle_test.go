package wiki

import (
	"context"
	"errors"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/storelifecycle"
)

func TestEmbeddingCycleWaitsForWikiLifecycle(t *testing.T) {
	wikiDir := filepath.Join(t.TempDir(), "wiki")
	_, held, err := storelifecycle.TryAcquire(context.Background(), wikiDir)
	if err != nil {
		t.Fatalf("acquire wiki lifecycle: %v", err)
	}
	defer held.Release()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	embedder := NewWikiEmbedder(nil, DefaultWikiEmbedConfig())
	_, err = embedder.RunCycle(ctx, wikiDir)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("wiki embedding while lifecycle locked = %v, want context canceled", err)
	}
}
