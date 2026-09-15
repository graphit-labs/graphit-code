//go:build lancedb

package wiki

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/storelifecycle"
)

type pausedWikiClient struct {
	entered chan struct{}
	resume  chan struct{}
}

func (c *pausedWikiClient) Embed(ctx context.Context, text string) ([]float32, error) {
	return nil, nil
}

func (c *pausedWikiClient) EmbedBatch(ctx context.Context, texts []string) ([][]float32, error) {
	close(c.entered)
	select {
	case <-c.resume:
		vectors := make([][]float32, len(texts))
		for i := range vectors {
			vectors[i] = make([]float32, c.Dimensions())
			vectors[i][0] = 1
		}
		return vectors, nil
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

func (c *pausedWikiClient) ModelName() string { return "paused-test" }
func (c *pausedWikiClient) Dimensions() int   { return ai.ResolveConfiguredEmbeddingDimensions() }

func seedWikiEmbeddingTest(t *testing.T, dir, title string) {
	t.Helper()
	db, err := OpenWikiDB(context.Background(), dir)
	if err != nil {
		t.Fatal(err)
	}
	chunk := WikiChunk{Slug: "same-slug", Title: title, Body: title + " body", ContentHash: title}
	if err := db.Sync(context.Background(), []WikiChunk{chunk}, nil, nil); err != nil {
		_ = db.Close()
		t.Fatal(err)
	}
	if err := db.Close(); err != nil {
		t.Fatal(err)
	}
}

func TestWikiInferenceReleasesLifecycleLock(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "wiki")
	seedWikiEmbeddingTest(t, dir, "before")
	client := &pausedWikiClient{entered: make(chan struct{}), resume: make(chan struct{})}
	type result struct {
		n   int
		err error
	}
	done := make(chan result, 1)
	go func() {
		n, err := NewWikiEmbedder(client, DefaultWikiEmbedConfig()).RunCycle(context.Background(), dir)
		done <- result{n, err}
	}()
	select {
	case <-client.entered:
	case <-time.After(10 * time.Second):
		t.Fatal("wiki inference did not start")
	}
	_, guard, err := storelifecycle.TryAcquire(context.Background(), dir)
	if err != nil {
		close(client.resume)
		t.Fatalf("wiki lifecycle remained locked during inference: %v", err)
	}
	guard.Release()
	close(client.resume)
	select {
	case r := <-done:
		if r.err != nil || r.n != 1 {
			t.Fatalf("embedding result = %+v, want one vector", r)
		}
	case <-time.After(10 * time.Second):
		t.Fatal("wiki embedding did not finish")
	}
}

func TestWikiInferenceDiscardsVectorAfterReset(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "wiki")
	seedWikiEmbeddingTest(t, dir, "before")
	client := &pausedWikiClient{entered: make(chan struct{}), resume: make(chan struct{})}
	type result struct {
		n   int
		err error
	}
	done := make(chan result, 1)
	go func() {
		n, err := NewWikiEmbedder(client, DefaultWikiEmbedConfig()).RunCycle(context.Background(), dir)
		done <- result{n, err}
	}()
	select {
	case <-client.entered:
	case <-time.After(10 * time.Second):
		t.Fatal("wiki inference did not start")
	}
	_, guard, err := storelifecycle.Acquire(context.Background(), dir)
	if err != nil {
		close(client.resume)
		t.Fatal(err)
	}
	if err := os.RemoveAll(dir); err != nil {
		guard.Release()
		close(client.resume)
		t.Fatal(err)
	}
	seedWikiEmbeddingTest(t, dir, "after")
	guard.Release()
	close(client.resume)
	select {
	case r := <-done:
		if r.err != nil || r.n != 0 {
			t.Fatalf("stale embedding result = %+v, want no vector", r)
		}
	case <-time.After(10 * time.Second):
		t.Fatal("wiki embedding did not finish after reset")
	}
	db, err := OpenWikiDB(context.Background(), dir)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = db.Close() }()
	if embedded, total := db.EmbeddingStats(context.Background()); embedded != 0 || total != 1 {
		t.Fatalf("wiki vectors after reset = %d/%d, want 0/1", embedded, total)
	}
}
