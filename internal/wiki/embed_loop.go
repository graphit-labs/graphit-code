package wiki

import (
	"context"
	"fmt"
	"log/slog"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// RunWikiEmbeddingLoop continuously generates wiki embeddings in the background.
// It processes both knowledge and memory wikis.
func RunWikiEmbeddingLoop(ctx context.Context, interval time.Duration, projectDir string, logger *slog.Logger) error {
	log := slogutil.Resolve(logger)

	client, err := ai.NewEmbeddingClientFromConfig()
	if err != nil {
		log.Error("failed to create embedding client for wiki", "error", err)
		return fmt.Errorf("wiki embedding client: %w", err)
	}

	cfg := DefaultWikiEmbedConfig()
	embedder := NewWikiEmbedder(client, cfg)
	embedder.Logger = logger

	// Resolve wiki directories.
	wikiDirs := resolveWikiDirs(projectDir)

	// Initial cycle.
	for _, dir := range wikiDirs {
		if n, err := embedder.RunCycle(ctx, dir); err != nil {
			log.Warn("wiki embedding initial cycle error", "dir", dir, "error", err)
		} else if n > 0 {
			log.Info("wiki embedding initial cycle", "dir", dir, "embedded", n)
		}
	}

	ticker := time.NewTicker(interval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return nil
		case <-ticker.C:
			for _, dir := range wikiDirs {
				if ctx.Err() != nil {
					return nil
				}
				if n, err := embedder.RunCycle(ctx, dir); err != nil {
					log.Warn("wiki embedding cycle error", "dir", dir, "error", err)
				} else if n > 0 {
					log.Info("wiki embedding cycle", "dir", dir, "embedded", n)
				}
			}
		}
	}
}

// resolveWikiDirs returns the wiki directories for the project (knowledge + memory).
func resolveWikiDirs(projectDir string) []string {
	var dirs []string

	// Knowledge wiki.
	knowledgeWiki := filepath.Join(projectDir, brand.DotDir(), "knowledge", "project", "wiki")
	dirs = append(dirs, knowledgeWiki)

	// Memory wiki.
	memoryWiki := filepath.Join(projectDir, brand.DotDir(), "memory", "project", "wiki")
	dirs = append(dirs, memoryWiki)

	return dirs
}
