package knowledge

import (
	"context"
	"fmt"
	"os"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
)

type IndexConfig struct {
	Workers    int
	Reset      bool
	BatchSize  int
	UseLouvain bool
	InlineCfg  config.ConfigMap
	ProjectCfg config.ConfigMap

	// Scope narrows the build to part of rootPath — normally the documentation
	// tree plus the root README, which ScopeFor assembles from configuration. The
	// zero value indexes everything under rootPath, which is what an imported
	// context needs: its extracted docs tree is the root.
	Scope WikiScope
}

type IndexResult struct {
	TotalFiles   int
	IndexedFiles int
	SkippedFiles int
	PrunedFiles  int
	NodesCreated int
	EdgesCreated int
	TotalTime    time.Duration
	ExtractTime  time.Duration
	WriteTime    time.Duration
	Communities  int
	StalePages   int
	LintFindings int
}

var supportedKnowledgeExts = map[string]bool{

	".md": true, ".markdown": true, ".mdx": true,
	".txt": true, ".adoc": true, ".rst": true,
	".puml": true, ".plantuml": true,

	".proto": true, ".graphql": true, ".gql": true,
}

func RunIndexPipeline(ctx context.Context, rootPath, wikiDir string, cfg IndexConfig) (*IndexResult, error) {
	start := time.Now()

	if wikiDir == "" {
		wikiDir = WikiDir()
	}

	if cfg.Reset {
		_ = os.RemoveAll(wikiDir)
	}

	allowedExts := config.ResolveKnowledgeExtensions(cfg.InlineCfg, cfg.ProjectCfg)

	wikiResult, err := GenerateKnowledgeWiki(ctx, rootPath, wikiDir, allowedExts, cfg.Scope)
	if err != nil {
		return nil, fmt.Errorf("knowledge wiki generation: %w", err)
	}

	return &IndexResult{
		IndexedFiles: wikiResult.ArticlesWritten,
		TotalTime:    time.Since(start),
		Communities:  wikiResult.Communities,
		StalePages:   wikiResult.StalePages,
		LintFindings: wikiResult.LintFindings,
	}, nil
}
