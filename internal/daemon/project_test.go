package daemon

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestNewProjectSupervisor(t *testing.T) {
	mods := []WatchModule{
		&fakeModule{name: "mod1"},
		&fakeModule{name: "mod2"},
		&fakeModule{name: "mod3"},
	}
	ps := newProjectSupervisor("proj-123", "/tmp/project", mods)
	if ps.projectID != "proj-123" {
		t.Errorf("projectID: expected 'proj-123', got %q", ps.projectID)
	}
	if ps.projectDir != "/tmp/project" {
		t.Errorf("projectDir: expected '/tmp/project', got %q", ps.projectDir)
	}
	if len(ps.modules) != 3 {
		t.Fatalf("modules: expected 3, got %d", len(ps.modules))
	}
	for i, entry := range ps.modules {
		if entry.mod.Name() != mods[i].Name() {
			t.Errorf("module[%d]: expected %q, got %q", i, mods[i].Name(), entry.mod.Name())
		}
		if entry.state != ModuleStopped {
			t.Errorf("module[%d]: expected initial state Stopped, got %d", i, entry.state)
		}
	}
}

func TestNewProjectSupervisor_Empty(t *testing.T) {
	ps := newProjectSupervisor("empty", "/tmp/empty", nil)
	if len(ps.modules) != 0 {
		t.Errorf("expected 0 modules, got %d", len(ps.modules))
	}
}

func TestProjectSupervisor_AddCloser(t *testing.T) {
	ps := newProjectSupervisor("test", "/tmp", nil)
	closed := false
	ps.AddCloser(closerFunc(func() error {
		closed = true
		return nil
	}))
	if len(ps.closers) != 1 {
		t.Errorf("expected 1 closer, got %d", len(ps.closers))
	}
	_ = ps.closers[0].Close()
	if !closed {
		t.Error("closer was not called")
	}
}

func TestProjectSupervisor_Stop(t *testing.T) {
	ps := newProjectSupervisor("test", "/tmp", nil)
	ps.Stop()
	if !ps.stopped {
		t.Error("expected stopped to be true after Stop()")
	}
}

func TestProjectSupervisor_Stop_WithCancel(t *testing.T) {
	ps := newProjectSupervisor("test", "/tmp", nil)
	_, cancel := context.WithCancel(context.Background())
	ps.cancel = cancel
	ps.Stop()
	if !ps.stopped {
		t.Error("expected stopped to be true")
	}
}

func TestProjectSupervisor_ProjectLog_NoFile(t *testing.T) {
	ps := newProjectSupervisor("test", "/tmp", nil)
	ps.projectLog("test %s", "message")
}

func TestProjectSupervisor_ProjectLog_WithGlobalFn(t *testing.T) {
	var gotMsg string
	ps := newProjectSupervisor("proj1", "/tmp", nil)
	ps.globalLogFn = func(format string, args ...any) {
		gotMsg = format
	}
	ps.projectLog("hello %s", "world")
	if gotMsg == "" {
		t.Error("globalLogFn should have been called")
	}
}

func TestProjectSupervisor_ProjectLog_WithFile(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "project.log")
	f, _ := os.Create(logPath)
	defer f.Close()

	ps := newProjectSupervisor("proj1", "/tmp", nil)
	ps.projectLogFile = f
	ps.projectLog("test %s", "msg")

	data, _ := os.ReadFile(logPath)
	if !strings.Contains(string(data), "test msg") {
		t.Errorf("expected 'test msg' in log, got %q", string(data))
	}
}

func TestProjectSupervisor_Start_ModulesRunAndStop(t *testing.T) {
	projectDir := t.TempDir()
	startedCh := make(chan string, 2)

	mods := []WatchModule{
		&fakeModule{
			name: "mod-a",
			startFn: func(ctx context.Context) error {
				startedCh <- "mod-a"
				<-ctx.Done()
				return ctx.Err()
			},
		},
		&fakeModule{
			name: "mod-b",
			startFn: func(ctx context.Context) error {
				startedCh <- "mod-b"
				<-ctx.Done()
				return ctx.Err()
			},
		},
	}

	ps := newProjectSupervisor("test-proj", projectDir, mods)

	ctx, cancel := context.WithCancel(context.Background())
	var logLines []string

	doneCh := make(chan struct{})
	go func() {
		ps.Start(ctx, func(format string, args ...any) {
			logLines = append(logLines, format)
		})
		close(doneCh)
	}()

	for i := 0; i < 2; i++ {
		select {
		case <-startedCh:
		case <-time.After(2 * time.Second):
			t.Fatal("module did not start in time")
		}
	}

	cancel()

	select {
	case <-doneCh:
	case <-time.After(5 * time.Second):
		t.Fatal("Start did not return after cancel")
	}
}

func TestProjectSupervisor_Start_WithCloserError(t *testing.T) {
	projectDir := t.TempDir()

	mods := []WatchModule{
		&fakeModule{
			name: "mod",
			startFn: func(ctx context.Context) error {
				<-ctx.Done()
				return ctx.Err()
			},
		},
	}

	ps := newProjectSupervisor("test", projectDir, mods)
	ps.AddCloser(closerFunc(func() error {
		return errors.New("closer error")
	}))

	ctx, cancel := context.WithCancel(context.Background())

	doneCh := make(chan struct{})
	go func() {
		ps.Start(ctx, func(format string, args ...any) {})
		close(doneCh)
	}()

	time.Sleep(50 * time.Millisecond)
	cancel()

	select {
	case <-doneCh:
	case <-time.After(5 * time.Second):
		t.Fatal("Start did not return")
	}
}

func TestSupervise_ModuleCrashesThenShutdown(t *testing.T) {
	projectDir := t.TempDir()

	crashCount := 0
	mod := &fakeModule{
		name: "crasher",
		startFn: func(ctx context.Context) error {
			crashCount++
			return errors.New("crash!")
		},
	}

	ps := newProjectSupervisor("test", projectDir, []WatchModule{mod})

	ctx, cancel := context.WithCancel(context.Background())

	doneCh := make(chan struct{})
	go func() {
		ps.Start(ctx, func(format string, args ...any) {})
		close(doneCh)
	}()

	time.Sleep(2500 * time.Millisecond)
	cancel()

	select {
	case <-doneCh:
	case <-time.After(5 * time.Second):
		t.Fatal("did not finish")
	}

	if crashCount < 2 {
		t.Errorf("expected at least 2 crashes, got %d", crashCount)
	}
}

func TestRunProtectedConvertsPanicToError(t *testing.T) {
	mod := &fakeModule{
		name: "panicker",
		startFn: func(ctx context.Context) error {
			panic("boom")
		},
	}
	err := runProtected(context.Background(), newModuleEntry(mod))
	if err == nil || !strings.Contains(err.Error(), "panic: boom") {
		t.Fatalf("runProtected error = %v", err)
	}
}

func TestSupervise_ContextCancelledBeforeStart(t *testing.T) {
	projectDir := t.TempDir()

	mod := &fakeModule{
		name: "mod",
		startFn: func(ctx context.Context) error {
			<-ctx.Done()
			return ctx.Err()
		},
	}

	ps := newProjectSupervisor("test", projectDir, []WatchModule{mod})

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	doneCh := make(chan struct{})
	go func() {
		ps.Start(ctx, func(format string, args ...any) {})
		close(doneCh)
	}()

	select {
	case <-doneCh:
	case <-time.After(5 * time.Second):
		t.Fatal("did not finish")
	}
}

func TestNewProjectSupervisor_StartsWithFreshIdleClock(t *testing.T) {
	ps := newProjectSupervisor("proj", "/tmp/project", nil)
	if idle := ps.IdleFor(); idle > time.Second {
		t.Errorf("expected a freshly created supervisor to look active, IdleFor() = %v", idle)
	}
}

func TestProjectSupervisor_TouchResetsIdleFor(t *testing.T) {
	ps := newProjectSupervisor("proj", "/tmp/project", nil)

	ps.lastActivity.Store(time.Now().Add(-time.Hour).UnixNano())
	if idle := ps.IdleFor(); idle < 55*time.Minute {
		t.Fatalf("expected IdleFor() to reflect the backdated timestamp, got %v", idle)
	}

	ps.Touch()
	if idle := ps.IdleFor(); idle > time.Second {
		t.Errorf("expected Touch() to reset the idle clock, IdleFor() = %v", idle)
	}
}
