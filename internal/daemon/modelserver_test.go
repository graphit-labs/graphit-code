package daemon

import (
	"context"
	"reflect"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/clirun"
)

// The supervisor restarts a module whose Start returns an error, so a model server
// that exits must surface as an error — and a daemon shutting down must not.
func TestModelServerModuleReportsAnExitForRestartAndStopsCleanly(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	runner := &recordingRunner{}
	useRunner(t, runner)

	if err := NewModelServerModule("rerank").Start(context.Background()); err == nil {
		t.Fatal("an exited model server returned nil; the supervisor would not restart it")
	}
	specs, _ := runner.recorded()
	if len(specs) != 1 {
		t.Fatalf("children = %+v", specs)
	}
	spec := specs[0]
	if !reflect.DeepEqual(spec.Args, []string{"model", "serve", "rerank"}) || spec.Dir != brand.GlobalDir() {
		t.Fatalf("model server child = %+v", spec)
	}
	if !spec.ExitWithParent || !spec.Background {
		t.Fatal("a model server must stop with the daemon and run as background work")
	}

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	useRunner(t, clirun.RunnerFunc(func(ctx context.Context, _ clirun.Spec) error { return ctx.Err() }))
	if err := NewModelServerModule("embedding").Start(ctx); err != nil {
		t.Fatalf("shutdown reported %v", err)
	}
}
