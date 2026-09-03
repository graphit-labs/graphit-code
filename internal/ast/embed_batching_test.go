//go:build lancedb

package ast

import (
	"context"
	"fmt"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

type recordingEmbClient struct{ batches [][]string }

func (r *recordingEmbClient) Embed(ctx context.Context, text string) ([]float32, error) {
	v, err := r.EmbedBatch(ctx, []string{text})
	if err != nil {
		return nil, err
	}
	return v[0], nil
}

func (r *recordingEmbClient) EmbedBatch(_ context.Context, texts []string) ([][]float32, error) {
	r.batches = append(r.batches, append([]string(nil), texts...))
	out := make([][]float32, len(texts))
	for i := range texts {
		out[i] = make([]float32, ai.EmbeddingDimensions)
	}
	return out, nil
}

func (r *recordingEmbClient) ModelName() string { return "recording" }

func (r *recordingEmbClient) Dimensions() int { return ai.EmbeddingDimensions }

func TestProcessBatchGroupsTextsOfSimilarLength(t *testing.T) {
	lang := embedLabelsTestLang
	projectDir := stageEmbedLabelsGrammar(t, "Function")

	dir := t.TempDir()
	pc, err := NewShardCache(dir)
	if err != nil {
		t.Fatal(err)
	}
	ents := make([]cachedEntity, 0, 8)
	for i := 0; i < 8; i++ {
		doc := "short"
		if i%2 == 0 {
			doc = ""
			for j := 0; j < 400; j++ {
				doc += "x"
			}
		}
		ents = append(ents, cachedEntity{
			Label: "Function", Lang: lang, UID: fmt.Sprintf("a.go::Fn%d", i),
			Name: fmt.Sprintf("Fn%d", i), Path: "a.go", Line: i + 1, EndLine: i + 1,
			Docstring: doc,
		})
	}
	if err := pc.Store("a.go", "h", &parseCacheEntry{RelPath: "a.go", Language: lang, Entities: ents}); err != nil {
		t.Fatal(err)
	}
	if err := pc.FlushDirty(); err != nil {
		t.Fatal(err)
	}
	ec, err := NewShardEmbCache(dir, pc)
	if err != nil {
		t.Fatal(err)
	}
	idx := newLanceIndexForTest(t)

	fc := &recordingEmbClient{}
	cfg := DefaultEmbeddingConfig()
	cfg.BatchSize = 4
	cfg.ParseCache = pc
	cfg.EmbCache = ec
	cfg.Index = idx
	cfg.ProjectDir = projectDir
	e := NewEmbedder(fc, cfg)

	if _, err := e.RunCycle(context.Background()); err != nil {
		t.Fatalf("RunCycle: %v", err)
	}
	if len(fc.batches) == 0 {
		t.Fatal("the fixture produced no batch")
	}

	for i, b := range fc.batches {
		shortest, longest := len(b[0]), len(b[0])
		for _, txt := range b {
			if len(txt) < shortest {
				shortest = len(txt)
			}
			if len(txt) > longest {
				longest = len(txt)
			}
		}
		if longest > shortest*2 {
			t.Errorf("batch %d mixes lengths %d..%d: the short rows are padded to the long one",
				i, shortest, longest)
		}
	}
}
