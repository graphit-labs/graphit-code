package wiki

import (
	"context"
	"fmt"
	"log/slog"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

type EmbedTarget struct {
	Dir string
}

func RunWikiEmbeddingLoop(ctx context.Context, interval time.Duration, targets []EmbedTarget, logger *slog.Logger) error {
	log := slogutil.Resolve(logger)

	if len(targets) == 0 {
		return fmt.Errorf("wiki embedding: no wiki directories given")
	}

	client, err := ai.NewEmbeddingClientFromConfig()
	if err != nil {
		log.Error("failed to create embedding client for wiki", "error", err)
		return fmt.Errorf("wiki embedding client: %w", err)
	}

	cfg := DefaultWikiEmbedConfig()
	embedder := NewWikiEmbedder(client, cfg)
	embedder.Logger = logger

	runAll := func() {
		for _, target := range targets {
			if ctx.Err() != nil {
				return
			}
			if target.Dir == "" {
				continue
			}
			n, err := embedder.RunCycle(ctx, target.Dir)
			switch {
			case err != nil:
				log.Warn("wiki embedding cycle error", "dir", target.Dir, "error", err)
			case n > 0:
				log.Info("wiki embedding cycle", "dir", target.Dir, "embedded", n)
			}
		}
	}

	runAll()

	ticker := time.NewTicker(interval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return nil
		case <-ticker.C:
			runAll()
		}
	}
}
