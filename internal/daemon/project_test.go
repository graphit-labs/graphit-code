package daemon

import (
	"context"
	"testing"
)

// ---------------------------------------------------------------------------
// newProjectSupervisor
// ---------------------------------------------------------------------------

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
			t.Errorf("module[%d]: expected initial state Stopped, got %s", i, entry.state)
		}
	}
}

func TestNewProjectSupervisor_Empty(t *testing.T) {
	ps := newProjectSupervisor("empty", "/tmp/empty", nil)
	if len(ps.modules) != 0 {
		t.Errorf("expected 0 modules, got %d", len(ps.modules))
	}
}

// ---------------------------------------------------------------------------
// ProjectSupervisor — AddCloser
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// ProjectSupervisor — Status
// ---------------------------------------------------------------------------

func TestProjectSupervisor_Status(t *testing.T) {
	mods := []WatchModule{
		&fakeModule{name: "a"},
		&fakeModule{name: "b"},
	}
	ps := newProjectSupervisor("test", "/tmp", mods)

	// Set different states
	ps.modules[0].setStarted()
	ps.modules[1].setState(ModuleFailed)
	ps.modules[1].setError(errForTest("test error"))

	statuses := ps.Status()
	if len(statuses) != 2 {
		t.Fatalf("expected 2 statuses, got %d", len(statuses))
	}
	if statuses[0].Name != "a" || statuses[0].State != ModuleRunning {
		t.Errorf("status[0]: expected a/running, got %s/%s", statuses[0].Name, statuses[0].State)
	}
	if statuses[1].Name != "b" || statuses[1].State != ModuleFailed {
		t.Errorf("status[1]: expected b/failed, got %s/%s", statuses[1].Name, statuses[1].State)
	}
	if statuses[1].LastError != "test error" {
		t.Errorf("status[1].LastError: expected 'test error', got %q", statuses[1].LastError)
	}
}

func errForTest(msg string) error {
	return &testError{msg}
}

type testError struct {
	msg string
}

func (e *testError) Error() string { return e.msg }

// ---------------------------------------------------------------------------
// ProjectSupervisor — Stop
// ---------------------------------------------------------------------------

func TestProjectSupervisor_Stop(t *testing.T) {
	ps := newProjectSupervisor("test", "/tmp", nil)
	// Stop without Start should not panic
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

// ---------------------------------------------------------------------------
// ProjectSupervisor — projectLog
// ---------------------------------------------------------------------------

func TestProjectSupervisor_ProjectLog_NoFile(t *testing.T) {
	ps := newProjectSupervisor("test", "/tmp", nil)
	// Should not panic with nil projectLogFile and nil globalLogFn
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
