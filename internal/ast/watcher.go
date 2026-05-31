package ast

import (
	"context"
	"crypto/sha256"
	"fmt"
	"os"
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
	cfg      WatcherConfig
	ic       *ignorer.IgnoreChecker
	g        git.Git
}

func NewWatcher(db GraphDB, rootPath string, cfg WatcherConfig) (*Watcher, error) {
	if cfg.PollInterval <= 0 {
		cfg.PollInterval = 2 * time.Second
	}
	return &Watcher{
		db:       db,
		rootPath: rootPath,
		cfg:      cfg,
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

			pipeOpts := PipelineOptions{
				Workers:     SafeWorkers(w.cfg.Workers),
				IndexSource: w.cfg.IndexSource,
				Cluster:     w.cfg.Cluster,
			}
			_, _ = RunPipeline(ctx, w.db, w.rootPath, pipeOpts)
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
	status, _ := w.g.RunOutput(w.rootPath, "status", "--porcelain", "-uall")
	head, _ := w.g.RunOutput(w.rootPath, "rev-parse", "HEAD")

	status = w.filterIgnored(status)
	mtimes := dirtyFileMtimes(status, w.rootPath)
	combined := head + "\n" + status + "\n" + mtimes
	h := sha256.Sum256([]byte(combined))
	return fmt.Sprintf("%x", h[:8])
}

func dirtyFileMtimes(porcelain, rootDir string) string {
	var b strings.Builder
	for _, line := range strings.Split(porcelain, "\n") {
		if len(line) < 4 {
			continue
		}
		rel := strings.TrimSpace(line[3:])
		if rel == "" || strings.HasSuffix(rel, "/") {
			continue
		}
		if info, err := os.Stat(filepath.Join(rootDir, rel)); err == nil {
			fmt.Fprintf(&b, "%s:%d\n", rel, info.ModTime().UnixNano())
		}
	}
	return b.String()
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

