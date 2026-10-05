package daemon

import (
	"context"
	"errors"
	"fmt"
	"log/slog"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/storelifecycle"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// These services receive their scope explicitly. They never use the process CWD,
// mutate flags/environment, or keep a native store open between scheduled runs.
func indexProjectAST(ctx context.Context, dir string, changes *astChangeSet, logger *slog.Logger) error {
	release, err := sysutil.AcquireHeavy(ctx)
	if err != nil {
		return err
	}
	defer release()
	if _, err := store.EnsureASTProjectDir(dir); err != nil {
		return err
	}
	cfg := ast.LadybugConfigForContextIn(dir, "")
	cfg.RequestContext = ctx
	db := ast.NewLadybugDB(cfg)
	defer func() { _ = db.Close() }()
	projectCfg := loadProjectConfigFromDir(dir)
	reverse := config.ResolveHubIcebugReverseEdges(nil, projectCfg)
	opts := ast.PipelineOptions{
		Workers: ast.SafeWorkers(0), CacheDir: cfg.StoreDir,
		IndexSource:      config.ResolveIndexSource(nil, projectCfg),
		GrammarOverrides: config.ResolveGrammarOverrides(nil, projectCfg),
		ClusterPathMap:   config.ResolveClusterPathMap(nil, projectCfg),
		ReverseEdges:     &reverse, Logger: logger,
	}
	if changes != nil {
		_, err = ast.RunPipelineForPaths(ctx, db, dir, changes.Changed, changes.Removed, opts)
	} else {
		_, err = ast.RunPipeline(ctx, db, dir, opts)
	}
	return err
}

type astChangeSet struct{ Changed, Removed []string }

func indexProjectKnowledge(ctx context.Context, dir string) error {
	release, err := sysutil.AcquireHeavy(ctx)
	if err != nil {
		return err
	}
	defer release()
	wikiDir, err := store.EnsureKnowledgeProjectDir(dir)
	if err != nil {
		return err
	}
	projectCfg := loadProjectConfigFromDir(dir)
	_, err = knowledge.RunIndexPipeline(ctx, dir, wikiDir, knowledge.IndexConfig{
		Workers: 0, BatchSize: 100, UseLouvain: false,
		Scope: knowledge.ScopeFor(dir, nil, projectCfg), ProjectCfg: projectCfg,
	})
	return err
}

func embedProjectAST(ctx context.Context, dir string, logger *slog.Logger) error {
	release, err := sysutil.AcquireHeavy(ctx)
	if err != nil {
		return err
	}
	defer release()
	cache := ast.LadybugConfigForContextIn(dir, "").StoreDir
	deferred, err := rebuildProjectSearchIndex(ctx, cache, dir)
	if err != nil || deferred {
		return err
	}
	_, _, err = ast.RunEmbeddingCycleIfIdle(ctx, cache, dir, ai.NewEmbeddingClientFromConfig, logger)
	return err
}

func rebuildProjectSearchIndex(ctx context.Context, cache, dir string) (bool, error) {
	lockedCtx, guard, err := storelifecycle.TryAcquire(ctx, cache)
	if errors.Is(err, storelifecycle.ErrLocked) {
		return true, nil
	}
	if err != nil {
		return false, err
	}
	defer guard.Release()
	if ast.SearchIndexBuilt(lockedCtx, cache) {
		return false, nil
	}
	parse, err := ast.NewShardCache(cache)
	if err != nil {
		return false, err
	}
	defer func() { _ = parse.Close() }()
	parse.SetRoot(dir)
	emb, err := ast.NewShardEmbCache(cache, parse)
	if err != nil {
		return false, err
	}
	defer func() { _ = emb.Close() }()
	index, err := ast.OpenSearchIndex(lockedCtx, cache)
	if err != nil {
		return false, err
	}
	defer func() { _ = index.Close() }()
	return false, index.RebuildFromCache(lockedCtx, parse, ast.BuildEmbLookup(parse, emb))
}

func embedProjectWiki(ctx context.Context, dir string) error {
	release, err := sysutil.AcquireHeavy(ctx)
	if err != nil {
		return err
	}
	defer release()
	client, err := ai.NewEmbeddingClientFromConfig()
	if err != nil {
		return fmt.Errorf("wiki embedding client: %w", err)
	}
	embedder := wiki.NewWikiEmbedder(client, wiki.DefaultWikiEmbedConfig())
	for _, target := range WikiEmbedTargets(dir, nil) {
		if _, err := embedder.RunCycle(ctx, target.Dir); err != nil {
			return err
		}
	}
	return nil
}
