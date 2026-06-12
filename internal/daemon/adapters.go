package daemon

import (
	"context"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/dream"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type EmbeddingModule struct {
	rootPath string
	interval time.Duration
	cacheDir string
}

func NewEmbeddingModule(rootPath string, interval time.Duration, cacheDir string) *EmbeddingModule {
	if interval <= 0 {
		interval = 2 * time.Minute
	}
	return &EmbeddingModule{rootPath: rootPath, interval: interval, cacheDir: cacheDir}
}

func (m *EmbeddingModule) Name() string { return "embedding" }

func (m *EmbeddingModule) Start(ctx context.Context) error {
	return ast.RunEmbeddingLoop(ctx, m.interval, m.cacheDir, nil)
}

type DreamModule struct {
	projectDir string
	ide        string
}

func NewDreamModule(projectDir, ide string) *DreamModule {
	return &DreamModule{projectDir: projectDir, ide: ide}
}

func (m *DreamModule) Name() string { return "dream" }

func (m *DreamModule) Start(ctx context.Context) error {
	runner := dream.NewRunner(m.projectDir, m.ide, func() config.ConfigMap {
		return loadProjectConfigFromDir(m.projectDir)
	})
	return runner.Run(ctx)
}

// WikiEmbeddingModule generates vector embeddings for wiki chunks in the background.
type WikiEmbeddingModule struct {
	projectDir string
	interval   time.Duration
}

func NewWikiEmbeddingModule(projectDir string, interval time.Duration) *WikiEmbeddingModule {
	if interval <= 0 {
		interval = 2 * time.Minute
	}
	return &WikiEmbeddingModule{projectDir: projectDir, interval: interval}
}

func (m *WikiEmbeddingModule) Name() string { return "wiki_embedding" }

func (m *WikiEmbeddingModule) Start(ctx context.Context) error {
	return wiki.RunWikiEmbeddingLoop(ctx, m.interval, m.projectDir, nil)
}

func loadProjectConfigFromDir(projectDir string) config.ConfigMap {
	lp := filepath.Join(projectDir, brand.LockFileName())
	if lf, err := hub.LoadLockfile(lp); err == nil && lf != nil {
		return lf.Config
	}
	return nil
}
