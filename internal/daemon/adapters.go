package daemon

import (
	"context"
	"log/slog"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/dream"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/task"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type EmbeddingModule struct {
	rootPath string
	interval time.Duration
	cacheDir string
}

const (
	defaultMemoryMaintenanceInterval = 15 * time.Minute
	defaultTaskMaintenanceInterval   = 15 * time.Minute
)

// MemoryMaintenanceModule is the single maintenance owner of one authoritative memory table.
// Writes stay independent and cheap; this daemon loop folds fragments into indexes, compacts them,
// and prunes physical table versions according to the memory retention policy.
type MemoryMaintenanceModule struct {
	uri      string
	interval time.Duration
	maintain func(context.Context, string) error
}

func NewMemoryMaintenanceModule(uri string, interval time.Duration) *MemoryMaintenanceModule {
	if interval <= 0 {
		interval = defaultMemoryMaintenanceInterval
	}
	return &MemoryMaintenanceModule{uri: uri, interval: interval, maintain: maintainMemoryTable}
}

func (m *MemoryMaintenanceModule) Name() string { return "memory_maintenance" }

func (m *MemoryMaintenanceModule) Start(ctx context.Context) error {
	if m.uri == "" {
		return nil
	}
	if err := m.maintain(ctx, m.uri); err != nil {
		return err
	}
	ticker := time.NewTicker(m.interval)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return nil
		case <-ticker.C:
			if err := m.maintain(ctx, m.uri); err != nil {
				return err
			}
		}
	}
}

func maintainMemoryTable(ctx context.Context, uri string) error {
	table, err := memory.OpenMemoryTable(ctx, uri)
	if err != nil {
		return err
	}
	defer func() { _ = table.Close() }()
	count, err := table.Count(ctx)
	if err != nil || count == 0 {
		return err
	}
	if err := table.EnsureIndexes(ctx); err != nil {
		return err
	}
	return table.Maintain(ctx)
}

type TaskMaintenanceModule struct {
	projectDir string
	interval   time.Duration
	maintain   func(context.Context, string) error
}

func NewTaskMaintenanceModule(projectDir string, interval time.Duration) *TaskMaintenanceModule {
	if interval <= 0 {
		interval = defaultTaskMaintenanceInterval
	}
	return &TaskMaintenanceModule{projectDir: projectDir, interval: interval, maintain: maintainTaskTables}
}

func (m *TaskMaintenanceModule) Name() string { return "task_maintenance" }

func (m *TaskMaintenanceModule) Start(ctx context.Context) error {
	if m.projectDir == "" {
		return nil
	}
	if err := m.maintain(ctx, m.projectDir); err != nil {
		return err
	}
	ticker := time.NewTicker(m.interval)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return nil
		case <-ticker.C:
			if err := m.maintain(ctx, m.projectDir); err != nil {
				return err
			}
		}
	}
}

func maintainTaskTables(ctx context.Context, projectDir string) error {
	service, err := task.Open(projectDir)
	if err != nil {
		return err
	}
	return service.Maintain(ctx)
}

func NewEmbeddingModule(rootPath string, interval time.Duration, cacheDir string) *EmbeddingModule {
	if interval <= 0 {
		interval = 2 * time.Minute
	}
	return &EmbeddingModule{rootPath: rootPath, interval: interval, cacheDir: cacheDir}
}

func (m *EmbeddingModule) Name() string { return "embedding" }

func (m *EmbeddingModule) Start(ctx context.Context) error {
	logger, closeLog := projectRebuildLogger(m.rootPath)
	defer closeLog()
	return ast.RunEmbeddingLoop(ctx, m.interval, m.cacheDir, m.rootPath, logger)
}

type DreamModule struct {
	projectDir string
	agent      string
}

func NewDreamModule(projectDir, agent string) *DreamModule {
	return &DreamModule{projectDir: projectDir, agent: agent}
}

func (m *DreamModule) Name() string { return "dream" }

func (m *DreamModule) Start(ctx context.Context) error {
	runner := dream.NewRunner(m.projectDir, m.agent, func() config.ConfigMap {
		return loadProjectConfigFromDir(m.projectDir)
	})
	return runner.Run(ctx)
}

// WikiEmbeddingModule generates vector embeddings for wiki chunks in the background.
//
// Registered in cmd/graphit/commands/daemon.go under the same gate as the AST
// embedding module. It existed here, fully written, and was never instantiated —
// which is why no wiki chunk in any project had ever been embedded, and why hybrid
// search silently answered from the lexical index alone.
type WikiEmbeddingModule struct {
	projectDir string
	targets    []wiki.EmbedTarget
	interval   time.Duration
}

// MemoryEmbeddingModule fills the vector column of one authoritative memory table.
type MemoryEmbeddingModule struct {
	uri      string
	interval time.Duration
	embedder *memory.Embedder
}

func NewMemoryEmbeddingModule(uri string, client ai.EmbeddingClient, interval time.Duration) *MemoryEmbeddingModule {
	if interval <= 0 {
		interval = 2 * time.Minute
	}
	return &MemoryEmbeddingModule{uri: uri, interval: interval, embedder: memory.NewEmbedder(client, memory.DefaultEmbedConfig())}
}

func (m *MemoryEmbeddingModule) Name() string { return "memory_embedding" }

func (m *MemoryEmbeddingModule) Start(ctx context.Context) error {
	if m.uri == "" || m.embedder == nil {
		return nil
	}
	run := func() {
		if _, err := m.embedder.RunCycle(ctx, m.uri); err != nil && ctx.Err() == nil {
			slog.Warn("memory embedding cycle error", "uri", m.uri, "error", err)
		}
	}
	run()
	ticker := time.NewTicker(m.interval)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return nil
		case <-ticker.C:
			run()
		}
	}
}

func NewWikiEmbeddingModule(projectDir string, targets []wiki.EmbedTarget, interval time.Duration) *WikiEmbeddingModule {
	if interval <= 0 {
		interval = 2 * time.Minute
	}
	return &WikiEmbeddingModule{projectDir: projectDir, targets: targets, interval: interval}
}

func (m *WikiEmbeddingModule) Name() string { return "wiki_embedding" }

func (m *WikiEmbeddingModule) Start(ctx context.Context) error {
	logger, closeLog := projectRebuildLogger(m.projectDir)
	defer closeLog()
	return wiki.RunWikiEmbeddingLoop(ctx, m.interval, m.targets, logger)
}

func WikiEmbedTargets(projectDir string, _ *slog.Logger) []wiki.EmbedTarget {
	return []wiki.EmbedTarget{
		{Dir: store.KnowledgeProjectDir(projectDir)},
	}
}

func loadProjectConfigFromDir(projectDir string) config.ConfigMap {
	lp := filepath.Join(projectDir, brand.LockFileName())
	if lf, err := hub.LoadLockfile(lp); err == nil && lf != nil {
		return lf.Config
	}
	return nil
}
