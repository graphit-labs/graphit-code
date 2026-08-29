//go:build lancedb

package ast

import (
	"context"
	"fmt"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

// recordingEmbClient captures the shape of each batch it is given.
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

// The model receives a batchSize x maxLen tensor, where maxLen is the longest text in the
// batch — so a batch costs its worst member for every one of its rows. Batching texts of
// similar length is what stops one long entity from being paid for 127 times.
//
// MEASURED against the real model on this repository's own texts: 2560 texts in batches of 128
// took 2m46s in arrival order and 1m45s sorted by length, 63%. The structural prediction from
// tensor cells alone was 59%; the gap is per-batch overhead that does not scale with tokens.
func TestProcessBatchGroupsTextsOfSimilarLength(t *testing.T) {
	lang := embedLabelsTestLang
	projectDir := stageEmbedLabelsGrammar(t, "Function")

	dir := t.TempDir()
	pc, err := NewShardCache(dir)
	if err != nil {
		t.Fatal(err)
	}
	// Docstrings of wildly different lengths, interleaved so arrival order is the worst case:
	// every batch would otherwise contain both a very long and many very short texts.
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

	// Every batch must be internally uniform: the four long texts together, the four short
	// ones together. A batch holding both is one where the short texts are padded to the long.
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
