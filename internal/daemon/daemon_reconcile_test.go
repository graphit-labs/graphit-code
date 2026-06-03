package daemon

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// ---------------------------------------------------------------------------
// reconcileProjects — builder returns error
// ---------------------------------------------------------------------------

func TestDaemon_ReconcileProjects_BuilderError_ReturnsErr(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	d := &Daemon{
		logFile:     lf,
		supervisors: make(map[string]*ProjectSupervisor),
		builder: func(dir string) ([]WatchModule, []func() error, error) {
			return nil, nil, errors.New("builder error")
		},
	}

	ctx := context.Background()
	d.reconcileProjects(ctx, func() ([]ProjectInfo, error) {
		return []ProjectInfo{{ID: "p1", Dir: "/some/dir"}}, nil
	})

	// No supervisor should be created when builder fails
	if _, ok := d.supervisors["p1"]; ok {
		t.Error("supervisor should not be created when builder fails")
	}
}

// ---------------------------------------------------------------------------
// reconcileProjects — builder returns zero modules
// ---------------------------------------------------------------------------

func TestDaemon_ReconcileProjects_ZeroModules_Skips(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	d := &Daemon{
		logFile:     lf,
		supervisors: make(map[string]*ProjectSupervisor),
		builder: func(dir string) ([]WatchModule, []func() error, error) {
			return nil, nil, nil // zero modules
		},
	}

	ctx := context.Background()
	d.reconcileProjects(ctx, func() ([]ProjectInfo, error) {
		return []ProjectInfo{{ID: "p1", Dir: "/some/dir"}}, nil
	})

	if _, ok := d.supervisors["p1"]; ok {
		t.Error("supervisor should not be created with zero modules")
	}
}

// ---------------------------------------------------------------------------
// reconcileProjects — project removal stops supervisor
// ---------------------------------------------------------------------------

func TestDaemon_ReconcileProjects_RemovesStaleProject(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	// Pre-seed with a supervisor
	ps := newProjectSupervisor("old", "/old/dir", nil)
	_, cancel := context.WithCancel(context.Background())
	ps.cancel = cancel

	d := &Daemon{
		logFile:     lf,
		supervisors: map[string]*ProjectSupervisor{"old": ps},
		builder: func(dir string) ([]WatchModule, []func() error, error) {
			return nil, nil, nil
		},
	}

	ctx := context.Background()
	// Discover returns empty list — the "old" supervisor should be removed.
	d.reconcileProjects(ctx, func() ([]ProjectInfo, error) {
		return nil, nil
	})

	if _, ok := d.supervisors["old"]; ok {
		t.Error("stale supervisor should be removed")
	}
}

// ---------------------------------------------------------------------------
// reconcileProjects — discoverFn error
// ---------------------------------------------------------------------------

func TestDaemon_ReconcileProjects_DiscoverFnError(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	d := &Daemon{
		logFile:     lf,
		supervisors: make(map[string]*ProjectSupervisor),
	}

	ctx := context.Background()
	d.reconcileProjects(ctx, func() ([]ProjectInfo, error) {
		return nil, errors.New("discover failed")
	})

	// Should not crash, no supervisors created
	if len(d.supervisors) != 0 {
		t.Error("expected no supervisors on discovery error")
	}
}

// ---------------------------------------------------------------------------
// moduleEntry — incRestarts reaches threshold
// ---------------------------------------------------------------------------

func TestModuleEntry_IncRestartsToMax(t *testing.T) {
	t.Parallel()
	mod := &fakeModule{name: "test", startFn: func(ctx context.Context) error { return nil }}
	entry := newModuleEntry(mod)

	// Simulate restarts up to maxRestarts
	for i := 0; i < 10; i++ {
		r := entry.incRestarts()
		if r != i+1 {
			t.Errorf("restart %d: expected %d, got %d", i, i+1, r)
		}
	}

	// After 10 restarts, the module should be at maxRestarts
	if entry.restarts != 10 {
		t.Errorf("expected 10 restarts, got %d", entry.restarts)
	}

	// Reset should work
	entry.resetRestarts()
	if entry.restarts != 0 {
		t.Errorf("expected 0 after reset, got %d", entry.restarts)
	}
}

// ---------------------------------------------------------------------------
// runProtected — catches panics
// ---------------------------------------------------------------------------

func TestRunProtected_CatchesPanic(t *testing.T) {
	t.Parallel()
	mod := &fakeModule{
		name: "panicker",
		startFn: func(ctx context.Context) error {
			panic("test panic!")
		},
	}
	entry := newModuleEntry(mod)
	err := runProtected(context.Background(), entry)
	if err == nil {
		t.Fatal("expected error from panic recovery")
	}
	if !strings.Contains(err.Error(), "panic: test panic!") {
		t.Errorf("expected panic message, got %v", err)
	}
}

// ---------------------------------------------------------------------------
// runProtected — normal error
// ---------------------------------------------------------------------------

func TestRunProtected_NormalError(t *testing.T) {
	t.Parallel()
	mod := &fakeModule{
		name: "errorer",
		startFn: func(ctx context.Context) error {
			return errors.New("normal error")
		},
	}
	entry := newModuleEntry(mod)
	err := runProtected(context.Background(), entry)
	if err == nil || err.Error() != "normal error" {
		t.Errorf("expected 'normal error', got %v", err)
	}
}

// ---------------------------------------------------------------------------
// runProtected — no error
// ---------------------------------------------------------------------------

func TestRunProtected_NoError(t *testing.T) {
	t.Parallel()
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

// ---------------------------------------------------------------------------
// stampChanged — with no boot stamp
// ---------------------------------------------------------------------------

func TestDaemon_StampChanged_EmptyBootStamp(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	d := &Daemon{bootStamp: ""}
	// Should return false when boot stamp is empty
	if d.stampChanged() {
		t.Error("expected false when boot stamp is empty")
	}
}

// ---------------------------------------------------------------------------
// stampChanged — stamp unchanged
// ---------------------------------------------------------------------------

func TestDaemon_StampChanged_Unchanged(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Write the stamp file with matching content
	stamp := launcherStampPath()
	_ = os.MkdirAll(filepath.Dir(stamp), 0o755)
	_ = os.WriteFile(stamp, []byte("v1.0.0"), 0o644)

	d := &Daemon{bootStamp: "v1.0.0"}
	if d.stampChanged() {
		t.Error("expected false when stamp matches")
	}
}

// ---------------------------------------------------------------------------
// stampChanged — stamp changed
// ---------------------------------------------------------------------------

func TestDaemon_StampChanged_Changed(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	stamp := launcherStampPath()
	_ = os.MkdirAll(filepath.Dir(stamp), 0o755)
	_ = os.WriteFile(stamp, []byte("v2.0.0"), 0o644)

	d := &Daemon{bootStamp: "v1.0.0"}
	if !d.stampChanged() {
		t.Error("expected true when stamp changed")
	}
}
