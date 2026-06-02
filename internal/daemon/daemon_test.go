package daemon

import (
	"os"
	"path/filepath"
	"strings"
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
	if err := fn.Close(); err != os.ErrClosed {
		t.Errorf("expected os.ErrClosed, got %v", err)
	}
}
