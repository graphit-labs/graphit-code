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

	if _, ok := d.supervisors["p1"]; ok {
		t.Error("supervisor should not be created when builder fails")
	}
}

func TestDaemon_ReconcileProjects_ZeroModules_Skips(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	d := &Daemon{
		logFile:     lf,
		supervisors: make(map[string]*ProjectSupervisor),
		builder: func(dir string) ([]WatchModule, []func() error, error) {
			return nil, nil, nil
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

func TestDaemon_ReconcileProjects_RemovesStaleProject(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

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
	d.reconcileProjects(ctx, func() ([]ProjectInfo, error) {
		return nil, nil
	})

	if _, ok := d.supervisors["old"]; ok {
		t.Error("stale supervisor should be removed")
	}
}

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

	if len(d.supervisors) != 0 {
		t.Error("expected no supervisors on discovery error")
	}
}

func TestModuleEntry_IncRestartsToMax(t *testing.T) {
	t.Parallel()
	mod := &fakeModule{name: "test", startFn: func(ctx context.Context) error { return nil }}
	entry := newModuleEntry(mod)

	for i := 0; i < 10; i++ {
		r := entry.incRestarts()
		if r != i+1 {
			t.Errorf("restart %d: expected %d, got %d", i, i+1, r)
		}
	}

	if entry.restarts != 10 {
		t.Errorf("expected 10 restarts, got %d", entry.restarts)
	}

	entry.resetRestarts()
	if entry.restarts != 0 {
		t.Errorf("expected 0 after reset, got %d", entry.restarts)
	}
}

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

func TestDaemon_StampChanged_EmptyBootStamp(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	d := &Daemon{bootStamp: ""}
	if d.stampChanged() {
		t.Error("expected false when boot stamp is empty")
	}
}

func TestDaemon_StampChanged_Unchanged(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	stamp := launcherStampPath()
	_ = os.MkdirAll(filepath.Dir(stamp), 0o755)
	_ = os.WriteFile(stamp, []byte("v1.0.0"), 0o644)

	d := &Daemon{bootStamp: "v1.0.0"}
	if d.stampChanged() {
		t.Error("expected false when stamp matches")
	}
}

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

func TestDaemon_ReconcileProjects_ParksInactiveProject(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	projectDir := t.TempDir()
	staleFile := filepath.Join(projectDir, "old.txt")
	if err := os.WriteFile(staleFile, []byte("stale"), 0o644); err != nil {
		t.Fatal(err)
	}
	old := time.Now().Add(-2 * time.Hour)
	if err := os.Chtimes(staleFile, old, old); err != nil {
		t.Fatal(err)
	}

	builderCalled := false
	d := &Daemon{
		cfg:         Config{ProjectActivityWindow: 30 * time.Minute},
		logFile:     lf,
		supervisors: make(map[string]*ProjectSupervisor),
		parked:      make(map[string]ProjectInfo),
		builder: func(dir string) ([]WatchModule, []func() error, error) {
			builderCalled = true
			return nil, nil, nil
		},
	}

	ctx := context.Background()
	discover := func() ([]ProjectInfo, error) {
		return []ProjectInfo{{ID: "p1", Dir: projectDir}}, nil
	}

	d.reconcileProjects(ctx, discover)

	if len(d.supervisors) != 0 {
		t.Errorf("expected no supervisor for an inactive project, got %d", len(d.supervisors))
	}
	if _, ok := d.parked["p1"]; !ok {
		t.Error("expected inactive project to be parked")
	}
	if builderCalled {
		t.Error("builder should not run for a parked project")
	}
}

func TestDaemon_ReconcileProjects_PromotesActiveProject(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	projectDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(projectDir, "fresh.txt"), []byte("fresh"), 0o644); err != nil {
		t.Fatal(err)
	}

	startedCh := make(chan struct{}, 1)
	stoppedCh := make(chan struct{})
	d := &Daemon{
		cfg:         Config{ProjectActivityWindow: 30 * time.Minute},
		logFile:     lf,
		supervisors: make(map[string]*ProjectSupervisor),
		parked:      make(map[string]ProjectInfo),
		builder: func(dir string) ([]WatchModule, []func() error, error) {
			mod := &fakeModule{
				name: "test-mod",
				startFn: func(ctx context.Context) error {
					select {
					case startedCh <- struct{}{}:
					default:
					}
					<-ctx.Done()
					close(stoppedCh)
					return ctx.Err()
				},
			}
			return []WatchModule{mod}, nil, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())

	discover := func() ([]ProjectInfo, error) {
		return []ProjectInfo{{ID: "p1", Dir: projectDir}}, nil
	}

	d.reconcileProjects(ctx, discover)

	if _, ok := d.supervisors["p1"]; !ok {
		t.Fatal("expected supervisor for a recently active project")
	}
	if _, ok := d.parked["p1"]; ok {
		t.Error("an active project should not be parked")
	}

	select {
	case <-startedCh:
	case <-time.After(2 * time.Second):
		t.Error("module start was not called within timeout")
	}

	cancel()
	select {
	case <-stoppedCh:
	case <-time.After(2 * time.Second):
		t.Error("module did not stop within timeout")
	}
}

func TestDaemon_ReconcileProjects_DemotesIdleSupervisor(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	projectDir := t.TempDir()
	staleFile := filepath.Join(projectDir, "old.txt")
	if err := os.WriteFile(staleFile, []byte("stale"), 0o644); err != nil {
		t.Fatal(err)
	}
	old := time.Now().Add(-2 * time.Hour)
	if err := os.Chtimes(staleFile, old, old); err != nil {
		t.Fatal(err)
	}

	ps := newProjectSupervisor("p1", projectDir, nil)
	ps.lastActivity.Store(time.Now().Add(-time.Hour).UnixNano())
	_, cancel := context.WithCancel(context.Background())
	ps.cancel = cancel

	builderCalled := false
	d := &Daemon{
		cfg:         Config{ProjectActivityWindow: 30 * time.Minute},
		logFile:     lf,
		supervisors: map[string]*ProjectSupervisor{"p1": ps},
		parked:      make(map[string]ProjectInfo),
		builder: func(dir string) ([]WatchModule, []func() error, error) {
			builderCalled = true
			return nil, nil, nil
		},
	}

	ctx := context.Background()
	discover := func() ([]ProjectInfo, error) {
		return []ProjectInfo{{ID: "p1", Dir: projectDir}}, nil
	}

	d.reconcileProjects(ctx, discover)

	if _, ok := d.supervisors["p1"]; ok {
		t.Error("expected idle supervisor to be stopped and removed")
	}
	if _, ok := d.parked["p1"]; !ok {
		t.Error("expected idle project to be parked instead of dropped")
	}
	if builderCalled {
		t.Error("builder should not run for a project that stays inactive after demotion")
	}
}

func TestDaemon_ReconcileProjects_ActivityWindowDisabled_AlwaysSupervises(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	projectDir := t.TempDir()
	staleFile := filepath.Join(projectDir, "old.txt")
	if err := os.WriteFile(staleFile, []byte("stale"), 0o644); err != nil {
		t.Fatal(err)
	}
	old := time.Now().Add(-2 * time.Hour)
	if err := os.Chtimes(staleFile, old, old); err != nil {
		t.Fatal(err)
	}

	startedCh := make(chan struct{}, 1)
	stoppedCh := make(chan struct{})
	d := &Daemon{
		logFile:     lf,
		supervisors: make(map[string]*ProjectSupervisor),
		builder: func(dir string) ([]WatchModule, []func() error, error) {
			mod := &fakeModule{
				name: "test-mod",
				startFn: func(ctx context.Context) error {
					select {
					case startedCh <- struct{}{}:
					default:
					}
					<-ctx.Done()
					close(stoppedCh)
					return ctx.Err()
				},
			}
			return []WatchModule{mod}, nil, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())

	discover := func() ([]ProjectInfo, error) {
		return []ProjectInfo{{ID: "p1", Dir: projectDir}}, nil
	}

	d.reconcileProjects(ctx, discover)

	if _, ok := d.supervisors["p1"]; !ok {
		t.Fatal("expected a stale project to still be supervised when the activity window is disabled")
	}

	select {
	case <-startedCh:
	case <-time.After(2 * time.Second):
		t.Error("module start was not called within timeout")
	}

	cancel()
	select {
	case <-stoppedCh:
	case <-time.After(2 * time.Second):
		t.Error("module did not stop within timeout")
	}
}

type activityFakeModule struct {
	name       string
	startFn    func(ctx context.Context) error
	onActivity func()
}

func (m *activityFakeModule) Name() string                    { return m.name }
func (m *activityFakeModule) Start(ctx context.Context) error { return m.startFn(ctx) }
func (m *activityFakeModule) SetActivityCallback(cb func())   { m.onActivity = cb }

func TestDaemon_ReconcileProjects_WiresActivityCallback(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	projectDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(projectDir, "fresh.txt"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}

	startedCh := make(chan struct{}, 1)
	stoppedCh := make(chan struct{})
	mod := &activityFakeModule{
		name: "activity-mod",
		startFn: func(ctx context.Context) error {
			select {
			case startedCh <- struct{}{}:
			default:
			}
			<-ctx.Done()
			close(stoppedCh)
			return ctx.Err()
		},
	}

	d := &Daemon{
		cfg:         Config{ProjectActivityWindow: 30 * time.Minute},
		logFile:     lf,
		supervisors: make(map[string]*ProjectSupervisor),
		parked:      make(map[string]ProjectInfo),
		builder: func(dir string) ([]WatchModule, []func() error, error) {
			return []WatchModule{mod}, nil, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())

	discover := func() ([]ProjectInfo, error) {
		return []ProjectInfo{{ID: "p1", Dir: projectDir}}, nil
	}

	d.reconcileProjects(ctx, discover)

	sup, ok := d.supervisors["p1"]
	if !ok {
		t.Fatal("expected supervisor to be created")
	}
	if mod.onActivity == nil {
		t.Fatal("expected SetActivityCallback to be wired by reconcileProjects")
	}

	select {
	case <-startedCh:
	case <-time.After(2 * time.Second):
		t.Fatal("module start was not called within timeout")
	}

	sup.lastActivity.Store(time.Now().Add(-time.Hour).UnixNano())
	mod.onActivity()
	if idle := sup.IdleFor(); idle > time.Second {
		t.Errorf("expected the wired callback to Touch() the supervisor, IdleFor() = %v", idle)
	}

	cancel()
	select {
	case <-stoppedCh:
	case <-time.After(2 * time.Second):
		t.Error("module did not stop within timeout")
	}
}
