package daemon

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// ---------------------------------------------------------------------------
// version_check.go
// ---------------------------------------------------------------------------

func TestLauncherStampPath(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	stamp := launcherStampPath()
	expectedStamp := filepath.Join(tempHome, "."+brand.Brand, "daemon", "launcher.stamp")
	if stamp != expectedStamp {
		t.Errorf("expected %s, got %s", expectedStamp, stamp)
	}
}

func TestReadLauncherStamp_Missing(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	if readLauncherStamp() != "" {
		t.Error("expected empty stamp when file is missing")
	}
}

func TestReadLauncherStamp_Exists(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	stamp := launcherStampPath()
	if err := os.MkdirAll(filepath.Dir(stamp), 0o755); err != nil {
		t.Fatalf("failed to create stamp dir: %v", err)
	}
	if err := os.WriteFile(stamp, []byte("  my-stamp-value  \n"), 0o644); err != nil {
		t.Fatalf("failed to write stamp: %v", err)
	}
	if readLauncherStamp() != "my-stamp-value" {
		t.Errorf("expected 'my-stamp-value', got %q", readLauncherStamp())
	}
}

func TestReadLauncherStamp_EmptyContent(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	stamp := launcherStampPath()
	if err := os.MkdirAll(filepath.Dir(stamp), 0o755); err != nil {
		t.Fatalf("failed to create stamp dir: %v", err)
	}
	if err := os.WriteFile(stamp, []byte("  \n"), 0o644); err != nil {
		t.Fatalf("failed to write stamp: %v", err)
	}
	if readLauncherStamp() != "" {
		t.Errorf("expected empty string for whitespace-only stamp, got %q", readLauncherStamp())
	}
}

// ---------------------------------------------------------------------------
// daemon.go — DefaultConfig
// ---------------------------------------------------------------------------

func TestDefaultConfig(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	cfg := DefaultConfig()
	if cfg.DiscoveryInterval != 30*time.Second {
		t.Errorf("DiscoveryInterval: expected 30s, got %s", cfg.DiscoveryInterval)
	}
	if cfg.VersionCheckInterval != defaultVersionCheckInterval {
		t.Errorf("VersionCheckInterval: expected %s, got %s", defaultVersionCheckInterval, cfg.VersionCheckInterval)
	}
	expectedLog := filepath.Join(GlobalDaemonDir(), "daemon.log")
	if cfg.LogPath != expectedLog {
		t.Errorf("LogPath: expected %s, got %s", expectedLog, cfg.LogPath)
	}
	if cfg.DisableEmbedding {
		t.Error("DisableEmbedding should default to false")
	}
	if cfg.DisableDream {
		t.Error("DisableDream should default to false")
	}
	if cfg.OnEvent != nil {
		t.Error("OnEvent should default to nil")
	}
}

// ---------------------------------------------------------------------------
// daemon.go — GlobalDaemonDir
// ---------------------------------------------------------------------------

func TestGlobalDaemonDir(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	got := GlobalDaemonDir()
	expected := filepath.Join(tempHome, "."+brand.Brand, "daemon")
	if got != expected {
		t.Errorf("expected %s, got %s", expected, got)
	}
}

// ---------------------------------------------------------------------------
// daemon.go — New
// ---------------------------------------------------------------------------

func TestNewDaemon_DefaultIntervals(t *testing.T) {
	d := New(Config{}, func(string) ([]WatchModule, []func() error, error) {
		return nil, nil, nil
	})
	if d.cfg.DiscoveryInterval != 30*time.Second {
		t.Errorf("expected default DiscoveryInterval 30s, got %s", d.cfg.DiscoveryInterval)
	}
	if d.cfg.VersionCheckInterval != defaultVersionCheckInterval {
		t.Errorf("expected default VersionCheckInterval %s, got %s", defaultVersionCheckInterval, d.cfg.VersionCheckInterval)
	}
	if d.supervisors == nil {
		t.Error("supervisors map should be initialized")
	}
	if d.pid == nil {
		t.Error("pid should not be nil")
	}
}

func TestNewDaemon_CustomIntervals(t *testing.T) {
	cfg := Config{
		DiscoveryInterval:    5 * time.Second,
		VersionCheckInterval: 10 * time.Second,
	}
	d := New(cfg, nil)
	if d.cfg.DiscoveryInterval != 5*time.Second {
		t.Errorf("expected 5s, got %s", d.cfg.DiscoveryInterval)
	}
	if d.cfg.VersionCheckInterval != 10*time.Second {
		t.Errorf("expected 10s, got %s", d.cfg.VersionCheckInterval)
	}
}

func TestNewDaemon_NegativeIntervals(t *testing.T) {
	cfg := Config{
		DiscoveryInterval:    -1 * time.Second,
		VersionCheckInterval: -1 * time.Second,
	}
	d := New(cfg, nil)
	if d.cfg.DiscoveryInterval != 30*time.Second {
		t.Errorf("negative DiscoveryInterval should default to 30s, got %s", d.cfg.DiscoveryInterval)
	}
	if d.cfg.VersionCheckInterval != defaultVersionCheckInterval {
		t.Errorf("negative VersionCheckInterval should default to %s, got %s", defaultVersionCheckInterval, d.cfg.VersionCheckInterval)
	}
}

// ---------------------------------------------------------------------------
// daemon.go — event helper
// ---------------------------------------------------------------------------

func TestDaemon_Event_WithCallback(t *testing.T) {
	var gotLevel, gotMsg string
	d := &Daemon{
		cfg: Config{
			OnEvent: func(level, msg string) {
				gotLevel = level
				gotMsg = msg
			},
		},
	}
	d.event("info", "hello %s", "world")
	if gotLevel != "info" {
		t.Errorf("expected level 'info', got %q", gotLevel)
	}
	if gotMsg != "hello world" {
		t.Errorf("expected msg 'hello world', got %q", gotMsg)
	}
}

func TestDaemon_Event_NilCallback(t *testing.T) {
	d := &Daemon{cfg: Config{OnEvent: nil}}
	// Should not panic
	d.event("info", "message")
}

// ---------------------------------------------------------------------------
// daemon.go — stampChanged
// ---------------------------------------------------------------------------

func TestDaemon_StampChanged_EmptyBoot(t *testing.T) {
	d := &Daemon{bootStamp: ""}
	if d.stampChanged() {
		t.Error("stampChanged should be false when bootStamp is empty")
	}
}

func TestDaemon_StampChanged_NoFile(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	d := &Daemon{bootStamp: "v1"}
	// No stamp file exists, readLauncherStamp returns ""
	if d.stampChanged() {
		t.Error("stampChanged should be false when stamp file doesn't exist")
	}
}

func TestDaemon_StampChanged_SameStamp(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	stamp := launcherStampPath()
	if err := os.MkdirAll(filepath.Dir(stamp), 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(stamp, []byte("v1\n"), 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}
	d := &Daemon{bootStamp: "v1"}
	if d.stampChanged() {
		t.Error("stampChanged should be false when stamps match")
	}
}

func TestDaemon_StampChanged_DifferentStamp(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	stamp := launcherStampPath()
	if err := os.MkdirAll(filepath.Dir(stamp), 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(stamp, []byte("v2\n"), 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}
	d := &Daemon{bootStamp: "v1"}
	if !d.stampChanged() {
		t.Error("stampChanged should be true when stamps differ")
	}
}

// ---------------------------------------------------------------------------
// daemon.go — log
// ---------------------------------------------------------------------------

func TestDaemon_Log_NilLogFile(t *testing.T) {
	d := &Daemon{logFile: nil}
	// Should not panic
	d.log("test %s", "message")
}

func TestDaemon_Log_WritesToFile(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, err := os.Create(logPath)
	if err != nil {
		t.Fatalf("create log file: %v", err)
	}
	defer lf.Close()

	d := &Daemon{logFile: lf}
	d.log("hello %s", "daemon")

	data, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatalf("read log file: %v", err)
	}
	content := string(data)
	if !strings.Contains(content, "hello daemon") {
		t.Errorf("log file should contain 'hello daemon', got %q", content)
	}
	// Check that it has timestamp format
	if !strings.HasPrefix(content, "[") {
		t.Errorf("log line should start with '[', got %q", content)
	}
}

// ---------------------------------------------------------------------------
// daemon.go — closerFunc
// ---------------------------------------------------------------------------

func TestCloserFunc(t *testing.T) {
	called := false
	fn := closerFunc(func() error {
		called = true
		return nil
	})
	if err := fn.Close(); err != nil {
		t.Errorf("unexpected error: %v", err)
	}
	if !called {
		t.Error("closer function was not called")
	}
}

func TestCloserFunc_ReturnsError(t *testing.T) {
	fn := closerFunc(func() error {
		return os.ErrClosed
	})
	if err := fn.Close(); !errors.Is(err, os.ErrClosed) {
		t.Errorf("expected os.ErrClosed, got %v", err)
	}
}

// ---------------------------------------------------------------------------
// daemon.go — reconcileProjects
// ---------------------------------------------------------------------------

func TestDaemon_ReconcileProjects_DiscoveryError(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	d := &Daemon{
		logFile:     lf,
		supervisors: make(map[string]*ProjectSupervisor),
		builder: func(projectDir string) ([]WatchModule, []func() error, error) {
			return nil, nil, nil
		},
	}

	discoverErr := func() ([]ProjectInfo, error) {
		return nil, errors.New("discovery failed")
	}

	ctx := context.Background()
	d.reconcileProjects(ctx, discoverErr)

	data, _ := os.ReadFile(logPath)
	if !strings.Contains(string(data), "discovery failed") {
		t.Error("expected log to contain 'discovery failed'")
	}
}

func TestDaemon_ReconcileProjects_NewProject(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	projectDir := t.TempDir()
	startedCh := make(chan struct{}, 1)
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
					return ctx.Err()
				},
			}
			return []WatchModule{mod}, nil, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	discover := func() ([]ProjectInfo, error) {
		return []ProjectInfo{{ID: "p1", Dir: projectDir}}, nil
	}

	d.reconcileProjects(ctx, discover)

	if len(d.supervisors) != 1 {
		t.Errorf("expected 1 supervisor, got %d", len(d.supervisors))
	}
	if _, ok := d.supervisors["p1"]; !ok {
		t.Error("expected supervisor for project 'p1'")
	}

	select {
	case <-startedCh:
	case <-time.After(2 * time.Second):
		t.Error("module start was not called within timeout")
	}

	cancel()
}

func TestDaemon_ReconcileProjects_RemoveProject(t *testing.T) {
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

	ps := newProjectSupervisor("old-proj", "/tmp/old", nil)
	_, cancel := context.WithCancel(context.Background())
	ps.cancel = cancel
	d.supervisors["old-proj"] = ps

	ctx := context.Background()
	discover := func() ([]ProjectInfo, error) {
		return []ProjectInfo{}, nil
	}

	d.reconcileProjects(ctx, discover)

	if len(d.supervisors) != 0 {
		t.Errorf("expected 0 supervisors after removal, got %d", len(d.supervisors))
	}
}

func TestDaemon_ReconcileProjects_BuilderError(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	d := &Daemon{
		logFile:     lf,
		supervisors: make(map[string]*ProjectSupervisor),
		builder: func(dir string) ([]WatchModule, []func() error, error) {
			return nil, nil, errors.New("build error")
		},
	}

	ctx := context.Background()
	discover := func() ([]ProjectInfo, error) {
		return []ProjectInfo{{ID: "p1", Dir: "/tmp/p"}}, nil
	}

	d.reconcileProjects(ctx, discover)

	if len(d.supervisors) != 0 {
		t.Errorf("expected 0 supervisors on builder error, got %d", len(d.supervisors))
	}
}

func TestDaemon_ReconcileProjects_NoModules(t *testing.T) {
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
	discover := func() ([]ProjectInfo, error) {
		return []ProjectInfo{{ID: "p1", Dir: "/tmp/p"}}, nil
	}

	d.reconcileProjects(ctx, discover)

	if len(d.supervisors) != 0 {
		t.Errorf("expected 0 supervisors when no modules, got %d", len(d.supervisors))
	}
}

func TestDaemon_ReconcileProjects_SkipExisting(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	d := &Daemon{
		logFile:     lf,
		supervisors: make(map[string]*ProjectSupervisor),
		builder: func(dir string) ([]WatchModule, []func() error, error) {
			t.Error("builder should not be called for existing project")
			return nil, nil, nil
		},
	}

	d.supervisors["p1"] = newProjectSupervisor("p1", "/tmp/p", nil)

	ctx := context.Background()
	discover := func() ([]ProjectInfo, error) {
		return []ProjectInfo{{ID: "p1", Dir: "/tmp/p"}}, nil
	}

	d.reconcileProjects(ctx, discover)
	if len(d.supervisors) != 1 {
		t.Errorf("expected 1 supervisor, got %d", len(d.supervisors))
	}
}

func TestDaemon_ReconcileProjects_WithClosers(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	projectDir := t.TempDir()
	var closerCalled int32
	d := &Daemon{
		logFile:     lf,
		supervisors: make(map[string]*ProjectSupervisor),
		builder: func(dir string) ([]WatchModule, []func() error, error) {
			mod := &fakeModule{
				name: "test",
				startFn: func(ctx context.Context) error {
					<-ctx.Done()
					return ctx.Err()
				},
			}
			closer := func() error {
				atomic.AddInt32(&closerCalled, 1)
				return nil
			}
			return []WatchModule{mod}, []func() error{closer}, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	discover := func() ([]ProjectInfo, error) {
		return []ProjectInfo{{ID: "p1", Dir: projectDir}}, nil
	}

	d.reconcileProjects(ctx, discover)
	time.Sleep(50 * time.Millisecond)

	if len(d.supervisors) != 1 {
		t.Errorf("expected 1 supervisor, got %d", len(d.supervisors))
	}

	sup := d.supervisors["p1"]
	if len(sup.closers) != 1 {
		t.Errorf("expected 1 closer, got %d", len(sup.closers))
	}

	cancel()
	time.Sleep(100 * time.Millisecond)

	_ = atomic.LoadInt32(&closerCalled)
}

// ---------------------------------------------------------------------------
// daemon.go — shutdown
// ---------------------------------------------------------------------------

func TestDaemon_Shutdown_NoSupervisors(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	d := &Daemon{
		logFile:     lf,
		supervisors: make(map[string]*ProjectSupervisor),
		cfg: Config{
			OnEvent: func(level, msg string) {},
		},
	}

	d.shutdown()

	data, _ := os.ReadFile(logPath)
	if !strings.Contains(string(data), "daemon shutting down") {
		t.Error("expected 'daemon shutting down' in log")
	}
	if !strings.Contains(string(data), "daemon stopped") {
		t.Error("expected 'daemon stopped' in log")
	}
}

func TestDaemon_Shutdown_WithSupervisors(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	d := &Daemon{
		logFile:     lf,
		supervisors: make(map[string]*ProjectSupervisor),
		cfg: Config{
			OnEvent: func(level, msg string) {},
		},
	}

	ps := newProjectSupervisor("p1", "/tmp/p1", nil)
	_, cancel := context.WithCancel(context.Background())
	ps.cancel = cancel
	d.supervisors["p1"] = ps

	d.shutdown()

	if !ps.stopped {
		t.Error("supervisor should be stopped after shutdown")
	}
}

// ---------------------------------------------------------------------------
// daemon.go — Start (integration tests)
// ---------------------------------------------------------------------------

func TestDaemon_Start_ContextCancelled(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "daemon.log")

	cfg := Config{
		LogPath:              logPath,
		DiscoveryInterval:    100 * time.Millisecond,
		VersionCheckInterval: 100 * time.Millisecond,
	}

	d := New(cfg, func(dir string) ([]WatchModule, []func() error, error) {
		return nil, nil, nil
	})
	d.pid = &PIDFile{path: filepath.Join(tmp, "daemon.pid")}

	ctx, cancel := context.WithCancel(context.Background())

	errCh := make(chan error, 1)
	go func() {
		errCh <- d.Start(ctx, func() ([]ProjectInfo, error) {
			return nil, nil
		})
	}()

	time.Sleep(50 * time.Millisecond)
	cancel()

	err := <-errCh
	if err != nil {
		t.Errorf("expected nil error after context cancel, got %v", err)
	}
}

func TestDaemon_Start_AlreadyRunning(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "daemon.log")
	pidPath := filepath.Join(tmp, "daemon.pid")

	// Simulate a running daemon by holding an exclusive flock on the PID file.
	content := fmt.Sprintf("%d\n%s\n", os.Getpid(), time.Now().UTC().Format(time.RFC3339))
	if err := os.WriteFile(pidPath, []byte(content), 0o600); err != nil {
		t.Fatal(err)
	}
	lockFD, err := os.OpenFile(pidPath, os.O_RDWR, 0o600)
	if err != nil {
		t.Fatal(err)
	}
	defer lockFD.Close()
	if err := flockExclusive(lockFD); err != nil {
		t.Fatalf("flock: %v", err)
	}
	defer flockRelease(lockFD)

	cfg := Config{LogPath: logPath}
	d := New(cfg, nil)
	d.pid = &PIDFile{path: pidPath}

	err = d.Start(context.Background(), func() ([]ProjectInfo, error) {
		return nil, nil
	})
	if err == nil {
		t.Error("expected error for already running daemon")
	}
	if !strings.Contains(err.Error(), "already running") {
		t.Errorf("expected 'already running' in error, got %v", err)
	}
}

func TestDaemon_Start_LogDirError(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	tmp := t.TempDir()
	blockFile := filepath.Join(tmp, "block")
	if err := os.WriteFile(blockFile, []byte("x"), 0o600); err != nil {
		t.Fatal(err)
	}
	logPath := filepath.Join(blockFile, "subdir", "daemon.log")

	cfg := Config{LogPath: logPath}
	d := New(cfg, nil)
	d.pid = &PIDFile{path: filepath.Join(tmp, "nonexist.pid")}

	err := d.Start(context.Background(), func() ([]ProjectInfo, error) {
		return nil, nil
	})
	if err == nil {
		t.Error("expected error creating log dir")
	}
	if !strings.Contains(err.Error(), "creating log dir") {
		t.Errorf("expected 'creating log dir' in error, got %v", err)
	}
}

func TestDaemon_Start_DiscoveryTickerFires(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "daemon.log")

	var discoveryCount int32
	cfg := Config{
		LogPath:              logPath,
		DiscoveryInterval:    50 * time.Millisecond,
		VersionCheckInterval: 10 * time.Second,
	}

	d := New(cfg, func(dir string) ([]WatchModule, []func() error, error) {
		return nil, nil, nil
	})
	d.pid = &PIDFile{path: filepath.Join(tmp, "daemon.pid")}

	ctx, cancel := context.WithCancel(context.Background())

	errCh := make(chan error, 1)
	go func() {
		errCh <- d.Start(ctx, func() ([]ProjectInfo, error) {
			atomic.AddInt32(&discoveryCount, 1)
			return nil, nil
		})
	}()

	time.Sleep(200 * time.Millisecond)
	cancel()

	err := <-errCh
	if err != nil {
		t.Errorf("expected nil, got %v", err)
	}
	if atomic.LoadInt32(&discoveryCount) < 2 {
		t.Errorf("expected at least 2 discovery calls, got %d", atomic.LoadInt32(&discoveryCount))
	}
}
