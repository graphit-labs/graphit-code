package daemon

import (
	"context"
	"fmt"
	"os"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/clirun"
)

// ModelServerModule keeps one local-model server process running: `graphit model serve
// embedding` or `graphit model serve rerank`. The model lives in that process, so its
// CPU and memory are visible on their own, and every caller — the daemon's MCP search,
// project children, interactive commands — reaches it through its socket. When the
// process exits the module returns an error and the supervisor restarts it with backoff.
type ModelServerModule struct {
	task string
}

func NewModelServerModule(task string) *ModelServerModule { return &ModelServerModule{task: task} }

func (m *ModelServerModule) Name() string { return "model-server-" + m.task }

func (m *ModelServerModule) Start(ctx context.Context) error {
	dir := brand.GlobalDir()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return fmt.Errorf("model server %s: %w", m.task, err)
	}
	spec := clirun.Spec{
		Dir:            dir,
		Args:           []string{"model", "serve", m.task},
		Background:     true,
		ExitWithParent: true,
	}
	if f, err := openAppendLog(globalLogPath("model-" + m.task)); err == nil {
		defer func() { _ = f.Close() }()
		spec.Stdout, spec.Stderr = f, f
	}
	err := cliRunner.Run(ctx, spec)
	if ctx.Err() != nil {
		return nil
	}
	if err == nil {
		return fmt.Errorf("model server %s exited", m.task)
	}
	return fmt.Errorf("model server %s: %w", m.task, err)
}
