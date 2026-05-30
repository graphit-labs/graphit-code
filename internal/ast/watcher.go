package ast

import (
	"context"
	"crypto/sha256"
	"fmt"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/ignorer"
)

type WatcherConfig struct {
	Debounce time.Duration

	Workers int

	IndexSource bool

	Cluster string

	PollInterval time.Duration
}

func DefaultWatcherConfig() WatcherConfig {
	return WatcherConfig{
		Debounce:     500 * time.Millisecond,
		Workers:      2,
		IndexSource:  true,
		PollInterval: 2 * time.Second,
	}
}

type Watcher struct {
	db       GraphDB
	rootPath string
	repoPath string
	cfg      WatcherConfig
	jobs     *JobManager
	ic       *ignorer.IgnoreChecker
	g        git.Git
}

func NewWatcher(db GraphDB, rootPath, repoPath string, cfg WatcherConfig, jobs *JobManager) (*Watcher, error) {
	if cfg.PollInterval <= 0 {
		cfg.PollInterval = 2 * time.Second
	}
	return &Watcher{
		db:       db,
		rootPath: rootPath,
		repoPath: repoPath,
		cfg:      cfg,
		jobs:     jobs,
		ic:       NewAstIgnoreChecker(rootPath),
		g:        git.Default(),
	}, nil
}

func (w *Watcher) Start(ctx context.Context) error {
	var lastHash string

	ticker := time.NewTicker(w.cfg.PollInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C:
			hash := w.statusHash()
			if hash == lastHash || lastHash == "" {
				lastHash = hash
				continue
			}

			if !w.waitDebounce(ctx, hash) {
				return ctx.Err()
			}

			lastHash = w.statusHash()

			changed := w.changedFiles()
			if len(changed) > 0 {
				w.reindexFiles(ctx, changed)
			}
		}
	}
}

func (w *Watcher) waitDebounce(ctx context.Context, hash string) bool {
	timer := time.NewTimer(w.cfg.Debounce)
	defer timer.Stop()

	poll := time.NewTicker(250 * time.Millisecond)
	defer poll.Stop()

	for {
		select {
		case <-ctx.Done():
			return false
		case <-timer.C:
			return true
		case <-poll.C:
			current := w.statusHash()
			if current != hash {
				hash = current
				timer.Reset(w.cfg.Debounce)
			}
		}
	}
}

func (w *Watcher) statusHash() string {
	status, _ := w.g.RunOutput(w.rootPath, "status", "--porcelain", "-unormal")
	head, _ := w.g.RunOutput(w.rootPath, "rev-parse", "HEAD")

	status = w.filterIgnored(status)
	combined := head + "\n" + status
	h := sha256.Sum256([]byte(combined))
	return fmt.Sprintf("%x", h[:8])
}

func (w *Watcher) changedFiles() []string {
	out, err := w.g.RunOutput(w.rootPath, "status", "--porcelain", "-unormal")
	if err != nil {
		return nil
	}
	var files []string
	for _, line := range strings.Split(out, "\n") {
		if len(line) < 4 {
			continue
		}
		rel := strings.TrimSpace(line[3:])
		if rel == "" {
			continue
		}
		rel = strings.TrimSuffix(rel, "/")

		if w.ic.IsIgnored(rel, false) {
			continue
		}

		ext := strings.ToLower(filepath.Ext(rel))
		if !HasTreeSitterForExtension(ext) {
			continue
		}

		abs := filepath.Join(w.rootPath, rel)
		files = append(files, abs)
	}
	return files
}

func (w *Watcher) filterIgnored(porcelain string) string {
	var kept []string
	for _, line := range strings.Split(porcelain, "\n") {
		if len(line) < 4 {
			continue
		}
		path := strings.TrimSpace(line[3:])
		if path == "" {
			continue
		}
		path = strings.TrimSuffix(path, "/")
		if w.ic.IsIgnored(path, false) {
			continue
		}
		kept = append(kept, line)
	}
	return strings.Join(kept, "\n")
}

func (w *Watcher) reindexFiles(ctx context.Context, files []string) {
	writer := NewGraphWriter(w.db, w.rootPath, w.cfg.IndexSource)
	writer.cluster = w.cfg.Cluster
	sem := make(chan struct{}, w.cfg.Workers)
	opts := ParseOptions{}

	for _, path := range files {
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
