package wiki

import (
	"context"
	"fmt"
	"log/slog"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// EmbedTarget is a wiki to keep embedded.
//
// It carried an OnEmbedded callback until every wiki became read where it is
// written: the fresh vectors used to have to be pushed into each project's copy or
// those copies stayed lexical. With one copy there is nothing to push.
type EmbedTarget struct {
	Dir string
}

// RunWikiEmbeddingLoop continuously generates embeddings for the given wiki
// targets in the background.
//
// The directories are a PARAMETER, and that is the whole point of this signature.
// This function used to derive them itself as
// "<project>/.<brand>/knowledge/project/wiki" — a path that does not exist, since
// the index lives one level above it. And because OpenWikiDB creates whatever it
// opens, every cycle built an empty database at the wrong path, found nothing
// pending there, and returned success. Three separate call sites had copied the
// same wrong layout.
//
// Only the caller can know where a wiki really is: the memory wiki is addressed by
// scope and exists in more than one place. So the layout is not this package's
// business — it is a leaf, and it takes what it is told to embed.
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
