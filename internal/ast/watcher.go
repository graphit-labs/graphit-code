package ast

import (
	"context"
	"io/fs"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/fsnotify/fsnotify"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/ignorer"
)

type WatcherConfig struct {
	Debounce time.Duration

	Workers int

	IndexSource bool

	Cluster string
}

func DefaultWatcherConfig() WatcherConfig {
	return WatcherConfig{
		Debounce:    500 * time.Millisecond,
		Workers:     2,
		IndexSource: true,
	}
}

type Watcher struct {
	db       GraphDB
	rootPath string
	repoPath string
	cfg      WatcherConfig
	jobs     *JobManager
	ic       *ignorer.IgnoreChecker

	fsw     *fsnotify.Watcher
	pending map[string]time.Time
	mu      sync.Mutex
}

func NewWatcher(db GraphDB, rootPath, repoPath string, cfg WatcherConfig, jobs *JobManager) (*Watcher, error) {
	fsw, err := fsnotify.NewWatcher()
	if err != nil {
		return nil, err
	}
	return &Watcher{
		db:       db,
		rootPath: rootPath,
		repoPath: repoPath,
		cfg:      cfg,
		jobs:     jobs,
		ic:       NewAstIgnoreChecker(rootPath),
		fsw:      fsw,
		pending:  make(map[string]time.Time),
	}, nil
}

func (w *Watcher) Start(ctx context.Context) error {

	if err := w.addDirsRecursive(w.rootPath); err != nil {
		return err
	}

	ticker := time.NewTicker(w.cfg.Debounce / 2)
	defer ticker.Stop()
	defer w.fsw.Close()

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()

		case event, ok := <-w.fsw.Events:
			if !ok {
				return nil
			}
			w.handleEvent(event)

		case <-w.fsw.Errors:

		case <-ticker.C:
			w.flushPending(ctx)
		}
	}
}

func (w *Watcher) handleEvent(event fsnotify.Event) {
	path := event.Name

	if rel, err := filepath.Rel(w.rootPath, path); err == nil && rel != "." {
		top := strings.SplitN(rel, string(filepath.Separator), 2)[0]
		if top == ".git" || top == brand.DotDir() {
			return
		}
	}

	ext := strings.ToLower(filepath.Ext(path))
	if !HasTreeSitterForExtension(ext) {

		if event.Has(fsnotify.Create) {
			_ = w.fsw.Add(path)
		}
		return
	}

	if rel, err := filepath.Rel(w.rootPath, path); err == nil && rel != "." {
		if w.ic.IsIgnored(rel, false) {
			return
		}
	}

	w.mu.Lock()
	w.pending[path] = time.Now()
	w.mu.Unlock()
}

func (w *Watcher) flushPending(ctx context.Context) {
	now := time.Now()
	w.mu.Lock()

	ready := make([]string, 0)
	for path, lastEvent := range w.pending {
		if now.Sub(lastEvent) >= w.cfg.Debounce {
			ready = append(ready, path)
			delete(w.pending, path)
		}
	}
	w.mu.Unlock()

	if len(ready) == 0 {
		return
	}

	writer := NewGraphWriter(w.db, w.rootPath, w.cfg.IndexSource)
	writer.cluster = w.cfg.Cluster
	sem := make(chan struct{}, w.cfg.Workers)
	opts := ParseOptions{}

	for _, path := range ready {
		sem <- struct{}{}
		go func(p string) {
			defer func() { <-sem }()
			w.reindexFile(ctx, writer, p, opts)
		}(path)
	}

	for i := 0; i < w.cfg.Workers; i++ {
		sem <- struct{}{}
	}

	if r, ok := w.db.(Releaser); ok {
		r.Release()
	}
}

func (w *Watcher) reindexFile(ctx context.Context, writer *GraphWriter, path string, opts ParseOptions) {
	tsParser := &TreeSitterParser{}
	pf, err := tsParser.Parse(path, false, opts)
	if pf != nil {
		pf.RepoPath = w.repoPath
	}

	if err != nil {
		abs, _ := filepath.Abs(path)
		rel := writer.rel(abs)
		_ = writer.db.ExecuteBatch(ctx, writer.getDeleteQueries(rel))
		return
	}

	_ = writer.WriteFileIncremental(ctx, pf, w.repoPath)
}

func (w *Watcher) addDirsRecursive(root string) error {
	return filepath.WalkDir(root, func(path string, d fs.DirEntry, err error) error {
		if err != nil || !d.IsDir() {
			return nil
		}

		base := filepath.Base(path)
		if base == ".git" || base == brand.DotDir() {
			return filepath.SkipDir
		}

		if rel, relErr := filepath.Rel(w.rootPath, path); relErr == nil && rel != "." {
			if w.ic.IsIgnored(rel, true) {
				return filepath.SkipDir
			}
		}
		return w.fsw.Add(path)
	})
}
