package daemon

import (
	"context"
	"crypto/sha256"
	"fmt"
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
	g := git.Default()
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
			lastHash = hash

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

	_ = ast.CreateGraphSchema(ctx, db)

	pipeOpts := ast.PipelineOptions{
		Workers:     ast.SafeWorkers(0),
		IndexSource: config.ResolveIndexSource(nil, projectCfg),
		CacheDir:    filepath.Dir(cfg.DBPath),
	}
	_, _ = ast.RunPipeline(ctx, db, m.projectDir, pipeOpts)
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
	status, _ := g.RunOutput(dir, "status", "--porcelain", "-unormal")
	head, _ := g.RunOutput(dir, "rev-parse", "HEAD")

	if ic != nil {
		status = filterIgnored(status, ic)
	}
	combined := head + "\n" + status
	h := sha256.Sum256([]byte(combined))
	return fmt.Sprintf("%x", h[:8])
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
