package memory

import (
	"context"
	"fmt"
	"log/slog"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

type EmbedConfig struct {
	BatchSize      int
	MaxSourceChars int
	OnProgress     func(done, total int)
}

func DefaultEmbedConfig() EmbedConfig {
	return EmbedConfig{BatchSize: 64, MaxSourceChars: 1600}
}

// Embedder writes vectors into the authoritative memory table itself.
type Embedder struct {
	client ai.EmbeddingClient
	cfg    EmbedConfig
	Logger *slog.Logger
}

func NewEmbedder(client ai.EmbeddingClient, cfg EmbedConfig) *Embedder {
	if cfg.BatchSize <= 0 {
		cfg.BatchSize = 64
	}
	if cfg.MaxSourceChars <= 0 {
		cfg.MaxSourceChars = 1600
	}
	return &Embedder{client: client, cfg: cfg}
}

func (e *Embedder) RunCycle(ctx context.Context, tableURI string) (int, error) {
	if e == nil || e.client == nil {
		return 0, fmt.Errorf("memory embedding client is not configured")
	}
	tbl, err := OpenMemoryTable(ctx, tableURI)
	if err != nil {
		return 0, err
	}
	defer func() { _ = tbl.Close() }()
	pending, err := tbl.PendingEmbeddings(ctx)
	if err != nil {
		return 0, err
	}
	if e.cfg.OnProgress != nil {
		e.cfg.OnProgress(0, len(pending))
	}
	done := 0
	for start := 0; start < len(pending); start += e.cfg.BatchSize {
		if err := ctx.Err(); err != nil {
			return done, err
		}
		end := start + e.cfg.BatchSize
		if end > len(pending) {
			end = len(pending)
		}
		batch := pending[start:end]
		texts := make([]string, len(batch))
		for i, rec := range batch {
			texts[i] = e.embeddingText(rec)
		}
		vectors, err := e.client.EmbedBatch(ctx, texts)
		if err != nil {
			return done, fmt.Errorf("embed memory batch: %w", err)
		}
		if len(vectors) != len(batch) {
			return done, fmt.Errorf("expected %d memory vectors, got %d", len(batch), len(vectors))
		}
		for i, rec := range batch {
			vec := fitMemoryVector(vectors[i], ai.ResolveConfiguredEmbeddingDimensions())
			if len(vec) == 0 {
				continue
			}
			if err := tbl.SetEmbedding(ctx, rec.Key(), vec); err != nil {
				slogutil.Resolve(e.Logger).Warn("attach memory vector", "key", rec.Key(), "error", err)
				continue
			}
			done++
		}
		if e.cfg.OnProgress != nil {
			e.cfg.OnProgress(done, len(pending))
		}
	}
	if done > 0 {
		if err := tbl.RefreshIndexes(ctx); err != nil {
			return done, fmt.Errorf("refresh memory indexes after embedding: %w", err)
		}
	}
	return done, nil
}

func (e *Embedder) embeddingText(rec MemoryRecord) string {
	parts := []string{rec.Title}
	if rec.Type != "" {
		parts = append([]string{"[" + rec.Type + "]"}, parts...)
	}
	if len(rec.Tags) > 0 {
		parts = append(parts, strings.Join(rec.Tags, " "))
	}
	body := rec.Body
	if len(body) > e.cfg.MaxSourceChars {
		body = body[:e.cfg.MaxSourceChars]
	}
	if body != "" {
		parts = append(parts, body)
	}
	return strings.Join(parts, "\n")
}

func fitMemoryVector(vec []float32, dim int) []float32 {
	if len(vec) == 0 || dim <= 0 {
		return nil
	}
	if len(vec) == dim {
		return vec
	}
	if len(vec) > dim {
		return vec[:dim]
	}
	out := make([]float32, dim)
	copy(out, vec)
	return out
}
