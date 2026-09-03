package daemon

import (
	"context"
	"testing"
)

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
	var _ WatchModule = NewEmbedServer(nil)
	if NewEmbedServer(nil).Name() != "embed-server" {
		t.Error("the embedding server must name itself for the log")
	}
}
