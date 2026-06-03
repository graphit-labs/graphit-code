package daemon

import (
	"os"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// ---------------------------------------------------------------------------
// EnsureRunning — daemon already alive
// ---------------------------------------------------------------------------

func TestEnsureRunning_AlreadyAlive(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Write a PID file with our own PID so IsAlive returns non-nil.
	pf := NewPIDFile()
	if err := pf.Write(); err != nil {
		t.Fatalf("Write: %v", err)
	}

	started, err := EnsureRunning()
	if err != nil {
		t.Errorf("unexpected error: %v", err)
	}
	if started {
		t.Error("expected started=false when daemon is already alive")
	}
}

// ---------------------------------------------------------------------------
// resolveDaemonExe — edge cases
// ---------------------------------------------------------------------------

func TestResolveDaemonExe_LauncherPathEmpty(t *testing.T) {
	origLauncher := os.Getenv(brand.EnvVar("LAUNCHER_PATH"))
	_ = os.Setenv(brand.EnvVar("LAUNCHER_PATH"), "")
	defer func() {
		if origLauncher != "" {
			_ = os.Setenv(brand.EnvVar("LAUNCHER_PATH"), origLauncher)
		} else {
			_ = os.Unsetenv(brand.EnvVar("LAUNCHER_PATH"))
		}
	}()

	exe := resolveDaemonExe()
	// Falls back to os.Executable() which should return a valid path.
	if exe == "" {
		t.Error("expected non-empty exe when LAUNCHER_PATH is empty string")
	}
}

func TestResolveDaemonExe_LauncherPathPointsToNonExistentFile(t *testing.T) {
	origLauncher := os.Getenv(brand.EnvVar("LAUNCHER_PATH"))
	_ = os.Setenv(brand.EnvVar("LAUNCHER_PATH"), "/nonexistent/path/to/launcher")
	defer func() {
		if origLauncher != "" {
			_ = os.Setenv(brand.EnvVar("LAUNCHER_PATH"), origLauncher)
		} else {
			_ = os.Unsetenv(brand.EnvVar("LAUNCHER_PATH"))
		}
	}()

	exe := resolveDaemonExe()
	// The launcher path doesn't exist, so it should fall back to os.Executable().
	if exe == "/nonexistent/path/to/launcher" {
		t.Error("should not return non-existent launcher path")
	}
	if exe == "" {
		t.Error("expected non-empty exe (fallback to os.Executable)")
	}
}
