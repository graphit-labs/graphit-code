package daemon

import (
	"context"
	"fmt"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/fswatch"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

const (
	syncDebounce    = 1 * time.Second
	syncMaxDebounce = 5 * time.Second
)

type SyncModule struct {
	projectDir string
	cacheDir   string
	onActivity func()
}

func NewSyncModule(projectDir, cacheDir string) *SyncModule {
	return &SyncModule{projectDir: projectDir, cacheDir: cacheDir}
}

func (m *SyncModule) Name() string { return "sync" }

// SetActivityCallback implements ActivityReporter. cb is invoked whenever a
// batch of filesystem events arrives, even if nothing in it is reindexable —
// any change under the watched tree counts as the project being worked on.
func (m *SyncModule) SetActivityCallback(cb func()) { m.onActivity = cb }

func (m *SyncModule) Start(ctx context.Context) error {
	astIgnore := ast.NewAstIgnoreChecker(m.projectDir)
	wikiIgnore := knowledge.NewKnowledgeIgnoreChecker(m.projectDir)

	w, err := fswatch.New(fswatch.Config{
		Root:        m.projectDir,
		Ignore:      ignoreUnion{astIgnore, wikiIgnore},
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
			if m.onActivity != nil {
				m.onActivity()
			}
			m.handleBatch(ctx, batch, astIgnore, wikiIgnore)
		}
	}
}

type ignoreUnion []fswatch.Ignorer

func (u ignoreUnion) IsIgnored(relPath string, isDir bool) bool {
	for _, ig := range u {
		if ig == nil {
			continue
		}
		if !ig.IsIgnored(relPath, isDir) {
			return false
		}
	}
	return len(u) > 0
}

func (u ignoreUnion) ShouldDescend(dirRelPath string) bool {
	for _, ig := range u {
		if ig != nil && ig.ShouldDescend(dirRelPath) {
			return true
		}
	}
	return false
}

// At deepens each member into the directory, keeping the union's semantics: a
// path is skipped only when EVERY member says to skip it, with each member
// carrying now the ignore files of the directories it crossed.
func (u ignoreUnion) At(dirRelPath string) fswatch.Ignorer {
	next := make(ignoreUnion, 0, len(u))
	for _, ig := range u {
		if ig != nil {
			next = append(next, ig.At(dirRelPath))
		}
	}
	return next
}

type batchTargets struct {
	astChanged []string
	astRemoved []string
	knowledge  bool
}

func classifyBatch(batch fswatch.Batch, projectDir, docsPath string, knowledgeExts map[string]bool,
	astIgnore, wikiIgnore fswatch.Ignorer, extraDocs []string) batchTargets {
	var t batchTargets

	isExtraDoc := func(slashRel string) bool {
		for _, extra := range extraDocs {
			if filepath.ToSlash(extra) == slashRel {
				return true
			}
		}
		return false
	}

	classify := func(paths []string, removed bool) {
		for _, p := range paths {
			rel, err := filepath.Rel(projectDir, p)
			if err != nil || insideDotDir(rel) {
				continue
			}
			ext := strings.ToLower(filepath.Ext(p))
			slashRel := filepath.ToSlash(rel)

			if knowledgeExts[ext] && (isUnder(p, docsPath) || isExtraDoc(slashRel)) &&
				!ignores(wikiIgnore, slashRel) {
				t.knowledge = true
			}

			if ignores(astIgnore, slashRel) {
				continue
			}
			if !ast.HasParserForExtensionIn(projectDir, ext) {
				continue
			}
			if removed {
				t.astRemoved = append(t.astRemoved, filepath.ToSlash(rel))
				continue
			}
			t.astChanged = append(t.astChanged, filepath.ToSlash(rel))
		}
	}
	classify(batch.Changed, false)
	classify(batch.Removed, true)

	return t
}

func (m *SyncModule) handleBatch(ctx context.Context, batch fswatch.Batch,
	astIgnore, wikiIgnore fswatch.Ignorer) {
	projectCfg := loadProjectConfigFromDir(m.projectDir)

	scope := knowledge.ScopeFor(m.projectDir, nil, projectCfg)
	docsPath := filepath.Join(m.projectDir, scope.Subdir)
	targets := classifyBatch(batch, m.projectDir, docsPath,
		config.ResolveKnowledgeExtensions(nil, projectCfg), astIgnore, wikiIgnore, scope.ExtraFiles)

	astWork := !config.IsModuleDisabled("ast", nil, projectCfg) &&
		(batch.Rescan || len(targets.astChanged) > 0 || len(targets.astRemoved) > 0)
	knowledgeWork := (targets.knowledge || batch.Rescan) &&
		!config.IsModuleDisabled("knowledge", nil, projectCfg)
	if !astWork && !knowledgeWork {
		return
	}

	askedAt := time.Now()
	release, err := sysutil.AcquireHeavy(ctx)
	if err != nil {
		return
	}
	defer release()

	if waited := time.Since(askedAt); waited > time.Second {
		slog.Info("daemon: waited for the indexing slot",
			"project", m.projectDir, "waited", waited.Round(time.Millisecond))
	}

	if astWork {
		if batch.Rescan {
			slog.Warn("daemon: watcher lost events, running a full AST scan", "project", m.projectDir)
			m.reindexAST(ctx, projectCfg, nil, nil)
		} else {
			m.reindexAST(ctx, projectCfg, targets.astChanged, targets.astRemoved)
		}
	}

	if knowledgeWork {
		m.reindexKnowledge(ctx, projectCfg)
	}
}

func ignores(ig fswatch.Ignorer, relPath string) bool {
	return ig != nil && ig.IsIgnored(relPath, false)
}

func insideDotDir(rel string) bool {
	dir := filepath.ToSlash(filepath.Dir(rel))
	if dir == "." || dir == "" {
		return false
	}
	for _, seg := range strings.Split(dir, "/") {
		if strings.HasPrefix(seg, ".") {
			return true
		}
	}
	return false
}

func isUnder(path, dir string) bool {
	rel, err := filepath.Rel(dir, path)
	if err != nil {
		return false
	}
	return rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}

func (m *SyncModule) reindexAST(ctx context.Context, projectCfg config.ConfigMap, changed, removed []string) {
	cfg := ast.LadybugConfigFor(m.projectDir)
	db := ast.NewLadybugDB(cfg)
	defer func() { _ = db.Close() }()

	rebuildLog, closeLog := projectRebuildLogger(m.projectDir)
	defer closeLog()

	revEdges := config.ResolveHubIcebugReverseEdges(nil, projectCfg)
	pipeOpts := ast.PipelineOptions{
		Workers:          ast.SafeWorkers(0),
		IndexSource:      config.ResolveIndexSource(nil, projectCfg),
		CacheDir:         cfg.StoreDir,
		ReverseEdges:     &revEdges,
		GrammarOverrides: config.ResolveGrammarOverrides(nil, projectCfg),
		Logger:           rebuildLog,
	}
	var perr error
	if len(changed) > 0 || len(removed) > 0 {
		_, perr = ast.RunPipelineForPaths(ctx, db, m.projectDir, changed, removed, pipeOpts)
	} else {
		_, perr = ast.RunPipeline(ctx, db, m.projectDir, pipeOpts)
	}
	if perr != nil {
		slog.Error("daemon: AST pipeline failed", "store", cfg.StoreDir, "error", perr)
	}
}

func (m *SyncModule) reindexKnowledge(ctx context.Context, projectCfg config.ConfigMap) {
	scope := knowledge.ScopeFor(m.projectDir, nil, projectCfg)

	if _, err := os.Stat(filepath.Join(m.projectDir, scope.Subdir)); err != nil && len(scope.ExtraFiles) == 0 {
		return
	}

	wikiDir := store.KnowledgeProjectDir(m.projectDir)
	kCfg := knowledge.IndexConfig{UseLouvain: false, ProjectCfg: projectCfg, Scope: scope}
	_, _ = knowledge.RunIndexPipeline(ctx, m.projectDir, wikiDir, kCfg)
}

func projectRebuildLogger(projectDir string) (*slog.Logger, func()) {
	dir := brand.ProjectRuntimePath(projectDir, "daemon")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return slog.New(slog.NewTextHandler(io.Discard, nil)), func() {}
	}
	f, err := os.OpenFile(filepath.Join(dir, "daemon.log"),
		os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o644)
	if err != nil {
		return slog.New(slog.NewTextHandler(io.Discard, nil)), func() {}
	}
	logger := slog.New(slog.NewTextHandler(f, &slog.HandlerOptions{Level: slog.LevelInfo})).
		With("module", "ast")
	return logger, func() { _ = f.Close() }
}
