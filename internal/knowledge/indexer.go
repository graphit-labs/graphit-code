package knowledge

import (
	"context"
	"fmt"
	"os"
	"time"
)

type IndexConfig struct {
	Workers    int
	Reset      bool
	BatchSize  int
	UseLouvain bool
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

	".yaml": true, ".yml": true, ".json": true,
	".proto": true, ".graphql": true, ".gql": true,
	".wsdl": true, ".xml": true,
}

func RunIndexPipeline(ctx context.Context, rootPath, wikiDir string, cfg IndexConfig) (*IndexResult, error) {
	start := time.Now()

	if wikiDir == "" {
		wikiDir = WikiDir()
	}

	if cfg.Reset {
		_ = os.RemoveAll(wikiDir)
	}

	wikiResult, err := GenerateKnowledgeWiki(ctx, rootPath, wikiDir)
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
