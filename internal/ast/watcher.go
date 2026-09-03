package ast

import (
	"context"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/fswatch"
	"github.com/graphit-labs/graphit-code/internal/ignorer"
)

type WatcherConfig struct {
	Debounce time.Duration

	Workers int

	IndexSource bool

	Cluster string

	ClusterPathMap map[string]string

	// MaxDebounce caps how long a busy tree may defer a reindex.
	MaxDebounce time.Duration

	GrammarOverrides map[string]string
}

func DefaultWatcherConfig() WatcherConfig {
	return WatcherConfig{
		Debounce:    500 * time.Millisecond,
		Workers:     2,
		IndexSource: true,
		MaxDebounce: 5 * time.Second,
	}
}

type Watcher struct {
	db       GraphDB
	rootPath string
	cfg      WatcherConfig
	ic       *ignorer.IgnoreChecker
}

func NewWatcher(db GraphDB, rootPath string, cfg WatcherConfig) (*Watcher, error) {
	return &Watcher{
		db:       db,
		rootPath: rootPath,
		cfg:      cfg,
		ic:       NewAstIgnoreChecker(rootPath),
	}, nil
}

func (w *Watcher) Start(ctx context.Context) error {
	fw, err := fswatch.New(fswatch.Config{
		Root:        w.rootPath,
		Ignore:      w.ic,
		Debounce:    w.cfg.Debounce,
		MaxDebounce: w.cfg.MaxDebounce,
		Accept: func(p string) bool {
			return HasParserForExtensionIn(w.rootPath, strings.ToLower(filepath.Ext(p)))
		},
	})
	if err != nil {
		return err
	}
	defer func() { _ = fw.Close() }()

	batches, err := fw.Start(ctx)
	if err != nil {
		return err
	}

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case batch, ok := <-batches:
			if !ok {
				return ctx.Err()
			}
			w.reindex(ctx, batch)
		}
	}
}

func (w *Watcher) reindex(ctx context.Context, batch fswatch.Batch) {
	pipeOpts := PipelineOptions{
		Workers:          SafeWorkers(w.cfg.Workers),
		IndexSource:      w.cfg.IndexSource,
		Cluster:          w.cfg.Cluster,
		ClusterPathMap:   w.cfg.ClusterPathMap,
		GrammarOverrides: w.cfg.GrammarOverrides,
	}

	if batch.Rescan {
		_, _ = RunPipeline(ctx, w.db, w.rootPath, pipeOpts)
		return
	}

	removed := make([]string, 0, len(batch.Removed))
	for _, p := range batch.Removed {
		if rel, err := filepath.Rel(w.rootPath, p); err == nil {
			removed = append(removed, filepath.ToSlash(rel))
		}
	}
	changed := repoRelativePaths(w.rootPath, batch.Changed)
	if len(changed) == 0 && len(removed) == 0 {
		return
	}
	_, _ = RunPipelineForPaths(ctx, w.db, w.rootPath, changed, removed, pipeOpts)
}
