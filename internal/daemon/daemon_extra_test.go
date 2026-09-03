package daemon

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestDaemon_Start_PIDWriteError(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "daemon.log")

	pidBase := filepath.Join(tmp, "pidblock")
	_ = os.WriteFile(pidBase, []byte("x"), 0o600)
	pidPath := filepath.Join(pidBase, "sub", "daemon.pid")

	cfg := Config{LogPath: logPath}
	d := New(cfg, nil)
	d.pid = &PIDFile{path: pidPath}

	err := d.Start(context.Background(), func() ([]ProjectInfo, error) {
		return nil, nil
	})
	if err == nil {
		t.Error("expected error when PID write fails")
	}
	if !strings.Contains(err.Error(), "acquiring pid lock") {
		t.Errorf("expected 'acquiring pid lock' in error, got %v", err)
	}
}

func TestDaemon_Start_LogFileOpenError(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	tmp := t.TempDir()
	logDir := filepath.Join(tmp, "logdir")
	_ = os.MkdirAll(logDir, 0o755)
	logPath := logDir

	cfg := Config{LogPath: logPath}
	d := New(cfg, nil)
	d.pid = &PIDFile{path: filepath.Join(tmp, "daemon.pid")}

	err := d.Start(context.Background(), func() ([]ProjectInfo, error) {
		return nil, nil
	})
	if err == nil {
		t.Error("expected error when log file open fails")
	}
	if !strings.Contains(err.Error(), "opening log file") {
		t.Errorf("expected 'opening log file' in error, got %v", err)
	}
}

func TestDaemon_Start_RemovesPIDOnExit(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "daemon.log")
	pidPath := filepath.Join(tmp, "daemon.pid")

	cfg := Config{
		LogPath:              logPath,
		DiscoveryInterval:    100 * time.Millisecond,
		VersionCheckInterval: 100 * time.Millisecond,
	}
	d := New(cfg, func(dir string) ([]WatchModule, []func() error, error) {
		return nil, nil, nil
	})
	d.pid = &PIDFile{path: pidPath}

	ctx, cancel := context.WithCancel(context.Background())

	errCh := make(chan error, 1)
	go func() {
		errCh <- d.Start(ctx, func() ([]ProjectInfo, error) {
			return nil, nil
		})
	}()

	time.Sleep(50 * time.Millisecond)
	cancel()

	<-errCh

	data, err := os.ReadFile(pidPath)
	if os.IsNotExist(err) {
		t.Error("PID file must not be deleted on exit (would break flock guard for concurrent starters)")
		return
	}
	if err != nil {
		t.Fatalf("reading pid file: %v", err)
	}
	if len(strings.TrimSpace(string(data))) != 0 {
		t.Errorf("PID file should be empty after exit, got %q", string(data))
	}
}

func TestDaemon_Start_EmitsEvents(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "daemon.log")

	var events []string
	cfg := Config{
		LogPath:              logPath,
		DiscoveryInterval:    100 * time.Millisecond,
		VersionCheckInterval: 10 * time.Second,
		OnEvent: func(level, msg string) {
			events = append(events, level+":"+msg)
		},
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
	<-errCh

	foundRunning := false
	for _, ev := range events {
		if strings.HasPrefix(ev, "running:") {
			foundRunning = true
		}
	}
	if !foundRunning {
		t.Error("expected 'running' event to be emitted")
	}
}

func TestDaemon_Log_WritesTimestamp(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, err := os.Create(logPath)
	if err != nil {
		t.Fatalf("create: %v", err)
	}
	defer lf.Close()

	d := &Daemon{logFile: lf}
	d.log("test message %d", 42)

	data, _ := os.ReadFile(logPath)
	content := string(data)
	if !strings.Contains(content, "test message 42") {
		t.Errorf("expected 'test message 42' in log, got %q", content)
	}
	if !strings.Contains(content, "[20") {
		t.Errorf("expected timestamp in log, got %q", content)
	}
}

func TestDaemon_ReconcileProjects_MultipleClosers(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	projectDir := t.TempDir()
	closerCount := 0
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
			closer1 := func() error { closerCount++; return nil }
			closer2 := func() error { closerCount++; return nil }
			return []WatchModule{mod}, []func() error{closer1, closer2}, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	discover := func() ([]ProjectInfo, error) {
		return []ProjectInfo{{ID: "p1", Dir: projectDir}}, nil
	}

	d.reconcileProjects(ctx, discover)

	if sup, ok := d.supervisors["p1"]; ok {
		if len(sup.closers) != 2 {
			t.Errorf("expected 2 closers, got %d", len(sup.closers))
		}
	} else {
		t.Error("supervisor p1 not found")
	}

	cancel()
	time.Sleep(100 * time.Millisecond)
}

func TestCloserFunc_NilError(t *testing.T) {
	t.Parallel()
	fn := closerFunc(func() error { return nil })
	if err := fn.Close(); err != nil {
		t.Errorf("expected nil error, got %v", err)
	}
}

func TestCloserFunc_WithWrappedError(t *testing.T) {
	t.Parallel()
	fn := closerFunc(func() error {
		return fmt.Errorf("wrapped: %w", os.ErrClosed)
	})
	err := fn.Close()
	if err == nil {
		t.Fatal("expected error")
	}
	if !strings.Contains(err.Error(), "wrapped") {
		t.Errorf("expected wrapped error, got %v", err)
	}
}

func TestDaemon_Log_ConcurrentWrites(t *testing.T) {
	tmp := t.TempDir()
	logPath := filepath.Join(tmp, "test.log")
	lf, _ := os.Create(logPath)
	defer lf.Close()

	d := &Daemon{logFile: lf}

	done := make(chan struct{})
	for i := 0; i < 50; i++ {
		go func(n int) {
			d.log("message %d", n)
			done <- struct{}{}
		}(i)
	}
	for i := 0; i < 50; i++ {
		<-done
	}

	data, _ := os.ReadFile(logPath)
	lines := strings.Split(strings.TrimSpace(string(data)), "\n")
	if len(lines) != 50 {
		t.Errorf("expected 50 log lines, got %d", len(lines))
	}
}
