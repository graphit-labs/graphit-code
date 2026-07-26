package daemon

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/fswatch"
	"github.com/graphit-labs/graphit-code/internal/ignorer"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
)

const (
	// syncDebounce is the quiet period before a burst of filesystem events is
	// turned into one reindex; syncMaxDebounce caps how long a continuously
	// busy tree may defer that reindex.
	syncDebounce    = 1 * time.Second
	syncMaxDebounce = 5 * time.Second
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
	// Filesystem notifications replace the previous `git status` poll: the poll
	// walked the whole worktree every 5s per project and reported a change up to
	// ~6s late, while the watcher is idle until something happens and names the
	// exact paths — which lets the AST reindex skip discovery entirely
	// (~350ms of a ~1.07s incremental on a 35k-file repository).
	ic := ignorer.New(m.projectDir, m.projectDir, ast.AstIgnoreFile, nil)

	w, err := fswatch.New(fswatch.Config{
		Root:        m.projectDir,
		Ignore:      ic,
		Debounce:    syncDebounce,
		MaxDebounce: syncMaxDebounce,
	})
	if err != nil {
		return fmt.Errorf("sync module: start watcher: %w", err)
	}
	defer func() { _ = w.Close() }()

	batches, err := w.Start(ctx)
	if err != nil {
		return fmt.Errorf("sync module: watch %s: %w", m.projectDir, err)
	}

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case batch, ok := <-batches:
			if !ok {
				return ctx.Err()
			}
			m.handleBatch(ctx, batch)
		}
	}
}

// handleBatch reindexes whatever the batch touched. AST and knowledge are
// dispatched independently so a docs-only edit does not reparse code, and vice
// versa.
func (m *SyncModule) handleBatch(ctx context.Context, batch fswatch.Batch) {
	projectCfg := loadProjectConfigFromDir(m.projectDir)

	docsPath := filepath.Join(m.projectDir, config.ResolveDocsDir(nil, projectCfg))

	var astChanged, astRemoved []string
	knowledgeTouched := false

	classify := func(paths []string, removed bool) {
		for _, p := range paths {
			if isUnder(p, docsPath) {
				knowledgeTouched = true
				continue
			}
			if !ast.HasParserForExtension(strings.ToLower(filepath.Ext(p))) {
				continue
			}
			if removed {
				// The parse cache is keyed by repo-relative path.
				if rel, err := filepath.Rel(m.projectDir, p); err == nil {
					astRemoved = append(astRemoved, filepath.ToSlash(rel))
				}
				continue
			}
			astChanged = append(astChanged, p)
		}
	}
	classify(batch.Changed, false)
	classify(batch.Removed, true)

	if !config.IsModuleDisabled("ast", nil, projectCfg) {
		switch {
		case batch.Rescan:
			// Events were dropped by the kernel; only a full scan restores a
			// consistent picture.
			slog.Warn("daemon: watcher lost events, running a full AST scan", "project", m.projectDir)
			m.reindexAST(ctx, projectCfg, nil, nil)
		case len(astChanged) > 0 || len(astRemoved) > 0:
			m.reindexAST(ctx, projectCfg, astChanged, astRemoved)
		}
	}

	if (knowledgeTouched || batch.Rescan) && !config.IsModuleDisabled("knowledge", nil, projectCfg) {
		m.reindexKnowledge(ctx, projectCfg)
	}
}

// isUnder reports whether path is inside dir.
func isUnder(path, dir string) bool {
	rel, err := filepath.Rel(dir, path)
	if err != nil {
		return false
	}
	return rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}

// reindexAST reindexes the project. When changed/removed are supplied the
// pipeline skips discovery and touches only those paths; passing nil for both
// runs a full scan.
func (m *SyncModule) reindexAST(ctx context.Context, projectCfg config.ConfigMap, changed, removed []string) {
	cfg := ast.LadybugConfig{
		DBPath: filepath.Join(m.projectDir, brand.DotDir(), "ast", "project", "ladybugdb"),
	}
	db := ast.NewLadybugDB(cfg)
	defer func() { _ = db.Close() }()

	if err := ast.CreateGraphSchema(ctx, db); err != nil {
		slog.Error("daemon: failed to create graph schema", "path", cfg.DBPath, "error", err)
	}

	pipeOpts := ast.PipelineOptions{
		Workers:          ast.SafeWorkers(0),
		IndexSource:      config.ResolveIndexSource(nil, projectCfg),
		CacheDir:         filepath.Dir(cfg.DBPath),
		GrammarOverrides: config.ResolveGrammarOverrides(nil, projectCfg),
	}
	var perr error
	if len(changed) > 0 || len(removed) > 0 {
		_, perr = ast.RunPipelineForPaths(ctx, db, m.projectDir, changed, removed, pipeOpts)
	} else {
		_, perr = ast.RunPipeline(ctx, db, m.projectDir, pipeOpts)
	}
	if perr != nil {
		slog.Error("daemon: AST pipeline failed", "path", cfg.DBPath, "error", perr)
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
