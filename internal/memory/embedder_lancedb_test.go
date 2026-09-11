//go:build lancedb

package memory

import (
	"context"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

type fixedMemoryEmbedder struct{ dim int }

func (f fixedMemoryEmbedder) Embed(context.Context, string) ([]float32, error) {
	return f.vector(), nil
}

func (f fixedMemoryEmbedder) EmbedBatch(_ context.Context, texts []string) ([][]float32, error) {
	out := make([][]float32, len(texts))
	for i := range texts {
		out[i] = f.vector()
	}
	return out, nil
}

func (f fixedMemoryEmbedder) ModelName() string { return "fixed-memory-test" }
func (f fixedMemoryEmbedder) Dimensions() int   { return f.dim }

func (f fixedMemoryEmbedder) vector() []float32 {
	v := make([]float32, f.dim)
	v[0] = 1
	return v
}

func TestEmbedderWritesAndSearchesTheAuthoritativeTable(t *testing.T) {
	ctx := context.Background()
	uri := filepath.Join(t.TempDir(), "table")
	tbl, err := OpenMemoryTable(ctx, uri)
	if err != nil {
		t.Fatal(err)
	}
	if err := tbl.Put(ctx, MemoryRecord{ID: "embedded-memory", Title: "Embedded", Body: "vector body"}); err != nil {
		t.Fatal(err)
	}
	if err := tbl.Close(); err != nil {
		t.Fatal(err)
	}

	dim := ai.ResolveConfiguredEmbeddingDimensions()
	client := fixedMemoryEmbedder{dim: dim}
	done, err := NewEmbedder(client, DefaultEmbedConfig()).RunCycle(ctx, uri)
	if err != nil || done != 1 {
		t.Fatalf("RunCycle = %d, %v", done, err)
	}

	reopened, err := OpenMemoryTable(ctx, uri)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = reopened.Close() }()
	record, found, err := reopened.Get(ctx, "embedded-memory")
	if err != nil || !found || len(record.Embedding) != dim {
		t.Fatalf("stored embedding found=%v width=%d err=%v", found, len(record.Embedding), err)
	}
	results, err := reopened.SearchChains(ctx, "", 5, SearchOptions{Mode: "semantic", Vector: client.vector()})
	if err != nil || len(results) != 1 || results[0].MemoryID != "embedded-memory" {
		t.Fatalf("semantic search = %+v, %v", results, err)
	}
}
