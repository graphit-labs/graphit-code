package daemon

import (
	"context"
	"os"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

type drainingGlobalModule struct{ started, stopping, release chan struct{} }

func (m *drainingGlobalModule) Name() string { return "draining-global" }
func (m *drainingGlobalModule) Start(ctx context.Context) error {
	close(m.started)
	<-ctx.Done()
	close(m.stopping)
	<-m.release
	return ctx.Err()
}

func TestDaemonWaitsForGlobalModuleShutdown(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	cwd, err := os.Getwd()
	if err != nil {
		cwd = os.TempDir()
	}
	if err := os.Chdir(cwd); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chdir(cwd) })
	m := &drainingGlobalModule{make(chan struct{}), make(chan struct{}), make(chan struct{})}
	d := New(DefaultConfig(), nil)
	d.AddGlobalModule(m)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	done := make(chan error, 1)
	go func() { done <- d.Start(ctx, func() ([]ProjectInfo, error) { return nil, nil }) }()
	select {
	case <-m.started:
	case <-time.After(5 * time.Second):
		t.Fatal("global module did not start")
	}
	cancel()
	select {
	case <-m.stopping:
	case <-time.After(time.Second):
		t.Fatal("global module did not cancel")
	}
	select {
	case <-done:
		t.Fatal("daemon returned before global resources closed")
	case <-time.After(20 * time.Millisecond):
	}
	if d.pid.IsAlive() == nil {
		t.Error("daemon released PID before global cleanup")
	}
	close(m.release)
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(time.Second):
		t.Fatal("daemon did not finish cleanup")
	}
}

type globalTestModule struct{ name string }

func (m *globalTestModule) Name() string { return m.name }

func (m *globalTestModule) Start(ctx context.Context) error {
	<-ctx.Done()
	return ctx.Err()
}

func TestAddGlobalModuleRegisters(t *testing.T) {
	d := New(Config{}, nil)
	if len(d.globalModules) != 0 {
		t.Fatalf("a fresh daemon has %d global module(s), want 0", len(d.globalModules))
	}
	d.AddGlobalModule(&globalTestModule{name: "one"})
	d.AddGlobalModule(nil)
	d.AddGlobalModule(&globalTestModule{name: "two"})
	if len(d.globalModules) != 2 {
		t.Errorf("registered %d module(s), want 2 — and nil must be ignored", len(d.globalModules))
	}
}

func TestGlobalModulesImplementWatchModule(t *testing.T) {
	var _ WatchModule = NewModelServerModule("embedding")
	if NewModelServerModule("rerank").Name() != "model-server-rerank" {
		t.Error("a model server must name itself and its task for the log")
	}
}
