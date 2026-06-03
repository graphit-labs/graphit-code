package daemon

import (
	"context"
	"crypto/sha256"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/ignorer"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
)

const (
	syncPollInterval = 5 * time.Second
	syncDebounce     = 1 * time.Second
)

type SyncModule struct {
	projectDir string
	cacheDir   string
}

func NewSyncModule(projectDir, cacheDir string) *SyncModule {
	return &SyncModule{projectDir: projectDir, cacheDir: cacheDir}
}

func (m *SyncModule) Name() string { return "sync" }

func (m *SyncModule) Start(ctx context.Context) error {
	g, err := git.DefaultErr()
	if err != nil {
		return fmt.Errorf("sync module requires git: %w", err)
	}
	ic := ignorer.New(m.projectDir, m.projectDir, ast.AstIgnoreFile, nil)

	var lastHash string

	ticker := time.NewTicker(syncPollInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C:
			hash := gitStateHash(g, m.projectDir, ic)
			if hash == lastHash || lastHash == "" {
				lastHash = hash
				continue
			}

			if !waitDebounce(ctx, g, m.projectDir, ic, hash) {
				return ctx.Err()
			}

			lastHash = gitStateHash(g, m.projectDir, ic)
			m.reindex(ctx)
		}
	}
}

func waitDebounce(ctx context.Context, g git.Git, dir string, ic *ignorer.IgnoreChecker, hash string) bool {
	timer := time.NewTimer(syncDebounce)
	defer timer.Stop()

	poll := time.NewTicker(500 * time.Millisecond)
	defer poll.Stop()

	for {
		select {
		case <-ctx.Done():
			return false
		case <-timer.C:
			return true
		case <-poll.C:
			current := gitStateHash(g, dir, ic)
			if current != hash {
				hash = current
				timer.Reset(syncDebounce)
			}
		}
	}
}

func (m *SyncModule) reindex(ctx context.Context) {
	projectCfg := loadProjectConfigFromDir(m.projectDir)

	if !config.IsModuleDisabled("ast", nil, projectCfg) {
		m.reindexAST(ctx, projectCfg)
	}

	if !config.IsModuleDisabled("knowledge", nil, projectCfg) {
		m.reindexKnowledge(ctx, projectCfg)
	}
}

func (m *SyncModule) reindexAST(ctx context.Context, projectCfg config.ConfigMap) {
	cfg := ast.LadybugConfig{
		DBPath: filepath.Join(m.projectDir, brand.DotDir(), "ast", "project", "ladybugdb"),
	}
	db := ast.NewLadybugDB(cfg)
	defer func() { _ = db.Close() }()

	if err := ast.CreateGraphSchema(ctx, db); err != nil {
		slog.Error("daemon: failed to create graph schema", "path", cfg.DBPath, "error", err)
	}

	pipeOpts := ast.PipelineOptions{
		Workers:     ast.SafeWorkers(0),
		IndexSource: config.ResolveIndexSource(nil, projectCfg),
		CacheDir:    filepath.Dir(cfg.DBPath),
	}
	if _, err := ast.RunPipeline(ctx, db, m.projectDir, pipeOpts); err != nil {
		slog.Error("daemon: AST pipeline failed", "path", cfg.DBPath, "error", err)
	}
}

func (m *SyncModule) reindexKnowledge(ctx context.Context, projectCfg config.ConfigMap) {
	docsDir := config.ResolveDocsDir(nil, projectCfg)
	docsPath := filepath.Join(m.projectDir, docsDir)

	if _, err := os.Stat(docsPath); err != nil {
		return
	}

	wikiDir := filepath.Join(m.projectDir, brand.DotDir(), "knowledge", "project")
	kCfg := knowledge.IndexConfig{UseLouvain: false}
	_, _ = knowledge.RunIndexPipeline(ctx, docsPath, wikiDir, kCfg)
}

func gitStateHash(g git.Git, dir string, ic *ignorer.IgnoreChecker) string {
	status, _ := g.RunOutput(dir, "status", "--porcelain", "-uall")
	head, _ := g.RunOutput(dir, "rev-parse", "HEAD")

	if ic != nil {
		status = filterIgnored(status, ic)
	}
	mtimes := dirtyFileMtimes(status, dir)
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

func filterIgnored(porcelain string, ic *ignorer.IgnoreChecker) string {
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
		if ic.IsIgnored(path, false) {
			continue
		}
		kept = append(kept, line)
	}
	return strings.Join(kept, "\n")
}
