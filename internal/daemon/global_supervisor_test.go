package daemon

import (
	"context"
	"errors"
	"sync"
	"testing"
	"time"
)

type flakyModule struct {
	mu     sync.Mutex
	name   string
	starts int
	fail   int // fail this many times before blocking on ctx
	panics bool
}

func (m *flakyModule) Name() string { return m.name }

func (m *flakyModule) Start(ctx context.Context) error {
	m.mu.Lock()
	m.starts++
	n := m.starts
	m.mu.Unlock()

	if m.panics && n == 1 {
		panic("module blew up")
	}
	if n <= m.fail {
		return errors.New("transient failure")
	}
	<-ctx.Done()
	return ctx.Err()
}

func (m *flakyModule) startCount() int {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.starts
}

// A global module that dies has to be restarted, and its death has to be visible. Both used to
// be false: memory-sync and the embedding server ran as `go func() { _ = mod.Start(ctx) }()`,
// which discards the error and never tries again — a watcher that died stopped recompiling
// memory with nothing in any log to say so.
func TestSuperviseGlobalRestartsAndLogs(t *testing.T) {
	mod := &flakyModule{name: "flaky", fail: 2}

	var mu sync.Mutex
	var lines []string
	logf := func(format string, args ...any) {
		mu.Lock()
		defer mu.Unlock()
		lines = append(lines, format)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 8*time.Second)
	defer cancel()

	done := make(chan struct{})
	go func() {
		SuperviseGlobal(ctx, mod, logf)
		close(done)
	}()

	deadline := time.After(7 * time.Second)
	for mod.startCount() < 3 {
		select {
		case <-deadline:
			t.Fatalf("the module was started %d time(s); a crash was not retried", mod.startCount())
		case <-time.After(20 * time.Millisecond):
		}
	}

	cancel()
	select {
	case <-done:
	case <-time.After(3 * time.Second):
		t.Fatal("SuperviseGlobal did not return after its context was cancelled")
	}

	mu.Lock()
	defer mu.Unlock()
	var started, crashed bool
	for _, l := range lines {
		if l == "%s: starting" {
			started = true
		}
		if l == "%s: crashed (attempt=%d, error=%v)" {
			crashed = true
		}
	}
	if !started {
		t.Error("nothing logged the module starting, so an operator cannot tell it is alive")
	}
	if !crashed {
		t.Error("nothing logged the crash, which is the failure that used to be silent")
	}
}

// A panic in a global module must not take the daemon down with it.
func TestSuperviseGlobalSurvivesAPanic(t *testing.T) {
	mod := &flakyModule{name: "panicky", panics: true}

	ctx, cancel := context.WithTimeout(context.Background(), 8*time.Second)
	defer cancel()

	done := make(chan struct{})
	go func() {
		SuperviseGlobal(ctx, mod, func(string, ...any) {})
		close(done)
	}()

	deadline := time.After(7 * time.Second)
	for mod.startCount() < 2 {
		select {
		case <-deadline:
			t.Fatal("the module never restarted after panicking")
		case <-time.After(20 * time.Millisecond):
		}
	}
	cancel()
	<-done
}

// The daemon has to actually supervise what it is given, or the wiring is decoration.
func TestAddGlobalModuleRegisters(t *testing.T) {
	d := New(Config{}, nil)
	if len(d.globalModules) != 0 {
		t.Fatalf("a fresh daemon has %d global module(s), want 0", len(d.globalModules))
	}
	d.AddGlobalModule(&flakyModule{name: "one"})
	d.AddGlobalModule(nil)
	d.AddGlobalModule(&flakyModule{name: "two"})
	if len(d.globalModules) != 2 {
		t.Errorf("registered %d module(s), want 2 — and nil must be ignored", len(d.globalModules))
	}
}

// The two modules this exists for must satisfy the contract, or they cannot be registered.
func TestGlobalModulesImplementWatchModule(t *testing.T) {
	var _ WatchModule = NewMemorySyncModule()
	var _ WatchModule = NewEmbedServer(nil)
	if NewMemorySyncModule().Name() != "memory-sync" {
		t.Error("the memory watcher must name itself for the log")
	}
	if NewEmbedServer(nil).Name() != "embed-server" {
		t.Error("the embedding server must name itself for the log")
	}
}
