package daemon

import (
	"context"
	"log/slog"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/dream"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

const (
	defaultEmbeddingInterval = 2 * time.Minute
	// defaultIndexEmbeddingInterval is the safety net for AST and wiki embedding, which
	// normally run right after the index they depend on changes.
	defaultIndexEmbeddingInterval    = 30 * time.Minute
	defaultMemoryMaintenanceInterval = 15 * time.Minute
	defaultTaskMaintenanceInterval   = 15 * time.Minute
)

// periodicCLIModule dispatches one job now, then on every tick and whenever its
// trigger fires, internally for core jobs or to a dedicated binary. A failed run is logged and retried on
// the next tick: the next run starts from whatever the failed one left, so there is
// nothing to restart.
type periodicCLIModule struct {
	name     string
	dir      string
	logPath  string
	args     []string
	interval time.Duration
	trigger  *Trigger
}

func (m *periodicCLIModule) Name() string { return m.name }

func (m *periodicCLIModule) Start(ctx context.Context) error {
	run := func() {
		if err := runChild(ctx, m.dir, m.logPath, nil, m.args...); err != nil && ctx.Err() == nil {
			slog.Warn("daemon: scheduled run failed", "module", m.name, "dir", m.dir, "error", err)
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
		case <-m.trigger.c():
			run()
			ticker.Reset(m.interval)
		}
	}
}

func orDefault(interval, fallback time.Duration) time.Duration {
	if interval <= 0 {
		return fallback
	}
	return interval
}

// memoryScope resolves where a Memory child runs and logs: the project for a project
// table, the global directory for the user table (projectDir == "").
func memoryScope(projectDir, name string) (dir, logPath string, args []string) {
	if projectDir == "" {
		return brand.GlobalDir(), globalLogPath(name + "-user"), []string{"--user"}
	}
	return projectDir, projectLogPath(projectDir), nil
}

// NewEmbeddingModule runs `graphit ast embed --if-idle` for a project when trigger fires
// — after the AST index changed — and otherwise only every interval (default 30
// minutes) as a safety net.
func NewEmbeddingModule(projectDir string, interval time.Duration, trigger *Trigger) WatchModule {
	return &periodicCLIModule{name: "embedding", dir: projectDir, logPath: projectLogPath(projectDir),
		args: []string{"ast", "embed", "--if-idle"}, interval: orDefault(interval, defaultIndexEmbeddingInterval), trigger: trigger}
}

// NewWikiEmbeddingModule runs `graphit wiki embed` for a project when trigger fires —
// after the Knowledge index changed — and otherwise only every interval (default 30
// minutes).
func NewWikiEmbeddingModule(projectDir string, interval time.Duration, trigger *Trigger) WatchModule {
	return &periodicCLIModule{name: "wiki_embedding", dir: projectDir, logPath: projectLogPath(projectDir),
		args: []string{"wiki", "embed"}, interval: orDefault(interval, defaultIndexEmbeddingInterval), trigger: trigger}
}

// NewMemoryEmbeddingModule schedules `graphit memory embed` for a project's table, or
// for the user table when projectDir is empty.
func NewMemoryEmbeddingModule(projectDir string, interval time.Duration) WatchModule {
	dir, logPath, scope := memoryScope(projectDir, "memory-embedding")
	return &periodicCLIModule{name: "memory_embedding", dir: dir, logPath: logPath,
		args: append([]string{"memory", "embed"}, scope...), interval: orDefault(interval, defaultEmbeddingInterval)}
}

// NewMemoryMaintenanceModule schedules `graphit memory maintain` — the single
// maintenance owner of one authoritative memory table — for a project, or for the user
// table when projectDir is empty.
func NewMemoryMaintenanceModule(projectDir string, interval time.Duration) WatchModule {
	dir, logPath, scope := memoryScope(projectDir, "memory-maintenance")
	return &periodicCLIModule{name: "memory_maintenance", dir: dir, logPath: logPath,
		args: append([]string{"memory", "maintain"}, scope...), interval: orDefault(interval, defaultMemoryMaintenanceInterval)}
}

// NewTaskMaintenanceModule schedules `graphit task maintain` for a project.
func NewTaskMaintenanceModule(projectDir string, interval time.Duration) WatchModule {
	return &periodicCLIModule{name: "task_maintenance", dir: projectDir, logPath: projectLogPath(projectDir),
		args: []string{"task", "maintain"}, interval: orDefault(interval, defaultTaskMaintenanceInterval)}
}

// DreamModule keeps the Dream scheduling — idle detection, session rotation, deep
// sleep — in the daemon, and executes each due session internally.
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
	runner.SetExecutor(m.runSession)
	return runner.Run(ctx)
}

func (m *DreamModule) runSession(ctx context.Context, sessionID string) error {
	args := []string{"dream", "run", "--session", sessionID}
	if m.agent != "" {
		args = append(args, "--agent", m.agent)
	}
	return runProjectChild(ctx, m.projectDir, nil, args...)
}

// WikiEmbedTargets lists the wiki stores `graphit wiki embed` fills for a project.
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
