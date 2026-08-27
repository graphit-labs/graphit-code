package daemon

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"sync"
	"testing"
	"time"
)

// moduleEntry — construction and initial state

type fakeModule struct {
	name    string
	startFn func(ctx context.Context) error
}

func (m *fakeModule) Name() string                    { return m.name }
func (m *fakeModule) Start(ctx context.Context) error { return m.startFn(ctx) }

func TestNewModuleEntry(t *testing.T) {
	mod := &fakeModule{name: "test-mod"}
	entry := newModuleEntry(mod)
	if entry.mod != mod {
		t.Error("mod field mismatch")
	}
	if entry.state != ModuleStopped {
		t.Errorf("initial state: expected stopped, got %v", entry.state)
	}
	if entry.restarts != 0 {
		t.Errorf("initial restarts should be 0, got %d", entry.restarts)
	}
}

func TestModuleEntry_SetState(t *testing.T) {
	mod := &fakeModule{name: "test"}
	entry := newModuleEntry(mod)

	entry.setState(ModuleRunning)
	if entry.state != ModuleRunning {
		t.Errorf("expected Running, got %v", entry.state)
	}

	entry.setState(ModuleCrashed)
	if entry.state != ModuleCrashed {
		t.Errorf("expected Crashed, got %v", entry.state)
	}
}

func TestModuleEntry_SetError(t *testing.T) {
	mod := &fakeModule{name: "test"}
	entry := newModuleEntry(mod)

	entry.setError(errors.New("something went wrong"))
	if entry.lastError != "something went wrong" {
		t.Errorf("expected 'something went wrong', got %q", entry.lastError)
	}

	// nil error should not overwrite
	entry.setError(nil)
	if entry.lastError != "something went wrong" {
		t.Errorf("nil error should not clear LastError, got %q", entry.lastError)
	}
}

func TestModuleEntry_SetStarted(t *testing.T) {
	mod := &fakeModule{name: "test"}
	entry := newModuleEntry(mod)

	before := time.Now()
	entry.setStarted()
	after := time.Now()

	if entry.state != ModuleRunning {
		t.Errorf("expected Running after setStarted, got %v", entry.state)
	}
	if entry.startedAt.Before(before) || entry.startedAt.After(after) {
		t.Errorf("startedAt %v not in range [%v, %v]", entry.startedAt, before, after)
	}
}

func TestModuleEntry_Restarts(t *testing.T) {
	mod := &fakeModule{name: "test"}
	entry := newModuleEntry(mod)

	for i := 1; i <= 5; i++ {
		got := entry.incRestarts()
		if got != i {
			t.Errorf("incRestarts[%d]: expected %d, got %d", i, i, got)
		}
	}

	if entry.restarts != 5 {
		t.Errorf("expected 5 restarts, got %d", entry.restarts)
	}

	entry.resetRestarts()
	if entry.restarts != 0 {
		t.Errorf("expected 0 restarts after reset, got %d", entry.restarts)
	}
}

// moduleEntry — thread safety

func TestModuleEntry_ConcurrentAccess(t *testing.T) {
	mod := &fakeModule{name: "concurrent"}
	entry := newModuleEntry(mod)

	var wg sync.WaitGroup
	for i := 0; i < 100; i++ {
		wg.Add(3)
		go func() {
			defer wg.Done()
			entry.setState(ModuleRunning)
		}()
		go func() {
			defer wg.Done()
			entry.setStarted()
		}()
		go func() {
			defer wg.Done()
			entry.incRestarts()
		}()
	}
	wg.Wait()
}

// runProtected — catches panics

func TestRunProtected_NormalReturn(t *testing.T) {
	mod := &fakeModule{
		name:    "ok",
		startFn: func(ctx context.Context) error { return nil },
	}
	entry := newModuleEntry(mod)
	err := runProtected(context.Background(), entry)
	if err != nil {
		t.Errorf("expected nil, got %v", err)
	}
}

func TestRunProtected_ErrorReturn(t *testing.T) {
	mod := &fakeModule{
		name:    "err",
		startFn: func(ctx context.Context) error { return errors.New("module error") },
	}
	entry := newModuleEntry(mod)
	err := runProtected(context.Background(), entry)
	if err == nil || err.Error() != "module error" {
		t.Errorf("expected 'module error', got %v", err)
	}
}

func TestRunProtected_Panic(t *testing.T) {
	mod := &fakeModule{
		name: "panic",
		startFn: func(ctx context.Context) error {
			panic("boom!")
		},
	}
	entry := newModuleEntry(mod)
	err := runProtected(context.Background(), entry)
	if err == nil {
		t.Fatal("expected error from panic")
	}
	got := err.Error()
	if !strings.HasPrefix(got, "panic: boom!") {
		t.Errorf("expected the error to lead with 'panic: boom!', got %q", got)
	}
	// The stack is the point: without it a crash-looping module writes one line per
	// restart naming no file and no function.
	if !strings.Contains(got, "runProtected") {
		t.Errorf("expected the panic error to carry a stack trace, got %q", got)
	}
}

func TestRunProtected_PanicWithError(t *testing.T) {
	mod := &fakeModule{
		name: "panic-err",
		startFn: func(ctx context.Context) error {
			panic(fmt.Errorf("formatted panic"))
		},
	}
	entry := newModuleEntry(mod)
	err := runProtected(context.Background(), entry)
	if err == nil {
		t.Fatal("expected error from panic")
	}
	got := err.Error()
	if !strings.HasPrefix(got, "panic: formatted panic") {
		t.Errorf("expected the error to lead with 'panic: formatted panic', got %q", got)
	}
	if !strings.Contains(got, "runProtected") {
		t.Errorf("expected the panic error to carry a stack trace, got %q", got)
	}
}
