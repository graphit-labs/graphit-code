package ai

import (
	"context"
	"fmt"
	"sort"
	"strings"
)

// The adapter between this package's cross-encoder and the search layer's Reranker interface.
//
// It lives HERE rather than in internal/lancestore so the search package keeps no dependency on a
// model, a tokenizer or the ONNX runtime: lancestore declares the two-method interface and this
// package satisfies it. A caller that never enables reranking never links a model path at all.

// RerankHit is one candidate to score. It mirrors what the search layer holds without importing
// it, which is what keeps the dependency pointing one way.
type RerankHit struct {
	// Text is what the model reads alongside the query. It is the caller's choice of what
	// represents the candidate — see BuildRerankText for the shape this project uses.
	Text string
	// Score is filled in on return: the cross-encoder's relevance for this pair.
	Score float64
	// Index is the candidate's position in the input, so a caller can map the reordered result
	// back to whatever it was holding.
	Index int
}

// Scorer is the part of CrossEncoderReranker the adapter needs, narrowed so a test can supply a
// deterministic stand-in instead of loading a gigabyte of model.
type Scorer interface {
	Score(ctx context.Context, query string, candidates []string) ([]float64, error)
	Name() string
}

// RerankAdapter turns a Scorer into an ordering.
type RerankAdapter struct {
	Scorer Scorer
}

// Name identifies the underlying model.
func (a RerankAdapter) Name() string {
	if a.Scorer == nil {
		return "none"
	}
	return a.Scorer.Name()
}

// Rank scores every candidate and returns them ordered, highest relevance first.
//
// It returns the SAME SET it was given, reordered — never a subset. The search layer refuses a
// reranker that changes the set, because a shortened list served as a ranked one silently drops
// results the user asked for.
func (a RerankAdapter) Rank(ctx context.Context, query string, hits []RerankHit) ([]RerankHit, error) {
	if a.Scorer == nil {
		return hits, fmt.Errorf("rerank: no scorer configured")
	}
	if len(hits) == 0 {
		return hits, nil
	}

	texts := make([]string, len(hits))
	for i, h := range hits {
		texts[i] = h.Text
	}
	scores, err := a.Scorer.Score(ctx, query, texts)
	if err != nil {
		return hits, err
	}
	if len(scores) != len(hits) {
		return hits, fmt.Errorf("rerank: %d scores for %d candidates", len(scores), len(hits))
	}

	out := make([]RerankHit, len(hits))
	copy(out, hits)
	for i := range out {
		out[i].Score = scores[i]
	}
	// SliceStable and a deterministic tiebreak: two runs of one query must agree, and a ranker
	// that ties two candidates and then orders them by whatever the sort happened to do makes the
	// search flap between runs, which reads as a bug in whatever consumes it.
	sort.SliceStable(out, func(i, j int) bool {
		if out[i].Score != out[j].Score {
			return out[i].Score > out[j].Score
		}
		return out[i].Index < out[j].Index
	})
	return out, nil
}

// BuildRerankText renders a candidate for the model.
//
// WHAT GOES IN AND WHY. The cross-encoder was trained on natural language, so it is fed the parts
// of an entity that read as language — the identifier, split into words, and the docstring — and
// NOT the gram bag. The bag exists so BM25 can match a truncation; to a transformer it is a few
// hundred meaningless three-letter tokens that crowd out the sentence and consume the sequence
// budget. Feeding the indexed column straight in was the obvious thing and would have been wrong.
func BuildRerankText(name, splitName, docstring, entityType, path string) string {
	parts := make([]string, 0, 5)
	add := func(s string) {
		if s = strings.TrimSpace(s); s != "" {
			parts = append(parts, s)
		}
	}
	add(name)
	if splitName != name {
		add(splitName)
	}
	add(entityType)
	add(docstring)
	add(path)
	return strings.Join(parts, " — ")
}
