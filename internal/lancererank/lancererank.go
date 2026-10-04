// Package lancererank adapts the configured reranker to the search layer's Reranker
// interface. It lives outside internal/ai so the model packages — and the model-server
// binary built from them — link no LanceDB.
package lancererank

import (
	"context"
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

// Configured resolves the optional second-stage ranker used by AST and knowledge
// searches. The network/model work is reached only when search.rerank is explicitly enabled.
func Configured(ctx context.Context) (lancestore.RerankConfig, error) {
	if !config.SearchRerank() {
		return lancestore.RerankConfig{}, nil
	}
	adapter, err := ai.ConfiguredReranker(ctx)
	if err != nil {
		return lancestore.RerankConfig{}, fmt.Errorf("configure search reranker: %w", err)
	}
	return lancestore.RerankConfig{Reranker: lanceReranker{adapter: adapter}}, nil
}

type lanceReranker struct{ adapter *ai.RerankAdapter }

func (r lanceReranker) Name() string { return r.adapter.Name() }

func (r lanceReranker) Rerank(ctx context.Context, query string, hits []lancestore.Hit) ([]lancestore.Hit, error) {
	candidates := make([]ai.RerankHit, len(hits))
	for i := range hits {
		candidates[i] = ai.RerankHit{Text: lanceRerankText(hits[i].Row), Index: i}
	}
	ranked, err := r.adapter.Rank(ctx, query, candidates)
	if err != nil {
		return hits, err
	}
	out := make([]lancestore.Hit, len(ranked))
	for i, candidate := range ranked {
		if candidate.Index < 0 || candidate.Index >= len(hits) {
			return hits, fmt.Errorf("rerank returned invalid original index %d", candidate.Index)
		}
		out[i] = hits[candidate.Index]
		out[i].Score = candidate.Score
	}
	return out, nil
}

func lanceRerankText(row lancestore.Row) string {
	value := func(key string) string {
		text, _ := row[key].(string)
		return strings.TrimSpace(text)
	}
	if name := value("name"); name != "" {
		return ai.BuildRerankText(name, splitCodeLikeIdentifier(name), value("docstring"), value("etype"), value("path"))
	}
	parts := []string{value("title"), value("summary"), value("breadcrumb"), value("doc_type"), value("body"), value("source"), value("slug"), value("path")}
	out := parts[:0]
	for _, part := range parts {
		if part != "" {
			out = append(out, part)
		}
	}
	return strings.Join(out, " — ")
}

func splitCodeLikeIdentifier(value string) string {
	var out strings.Builder
	for i, r := range value {
		if i > 0 && r >= 'A' && r <= 'Z' {
			out.WriteByte(' ')
		}
		if r == '_' || r == '-' {
			out.WriteByte(' ')
			continue
		}
		out.WriteRune(r)
	}
	return strings.Join(strings.Fields(out.String()), " ")
}
