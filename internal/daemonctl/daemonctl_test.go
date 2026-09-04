package daemonctl

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestWaitForFileLockWaitsForReadiness(t *testing.T) {
	path := filepath.Join(t.TempDir(), "daemon.pid")
	ready := make(chan struct{})
	release := make(chan struct{})
	go func() {
		time.Sleep(20 * time.Millisecond)
		file, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0o600)
		if err != nil {
			close(ready)
			return
		}
		defer file.Close()
		if err := flockExclusiveBlocking(file); err != nil {
			close(ready)
			return
		}
		close(ready)
		<-release
		flockProbeRelease(file)
	}()

	if err := waitForFileLock(path, time.Second, time.Millisecond); err != nil {
		t.Fatal(err)
	}
	<-ready
	close(release)
}

func TestWaitForFileLockHasBoundedFailure(t *testing.T) {
	start := time.Now()
	err := waitForFileLock(filepath.Join(t.TempDir(), "missing.pid"), 20*time.Millisecond, time.Millisecond)
	if err == nil {
		t.Fatal("expected readiness timeout")
	}
	if elapsed := time.Since(start); elapsed > 250*time.Millisecond {
		t.Fatalf("readiness timeout took %s", elapsed)
	}
}

func TestResolveExe_LauncherPathEmpty(t *testing.T) {
	origLauncher := os.Getenv(brand.EnvVar("LAUNCHER_PATH"))
	_ = os.Setenv(brand.EnvVar("LAUNCHER_PATH"), "")
	defer func() {
		if origLauncher != "" {
			_ = os.Setenv(brand.EnvVar("LAUNCHER_PATH"), origLauncher)
		} else {
			_ = os.Unsetenv(brand.EnvVar("LAUNCHER_PATH"))
		}
	}()

	exe := ResolveExe()
	if exe == "" {
		t.Error("expected non-empty exe when LAUNCHER_PATH is empty string")
	}
}

func TestResolveExe_LauncherPathNonExistent(t *testing.T) {
	origLauncher := os.Getenv(brand.EnvVar("LAUNCHER_PATH"))
	_ = os.Setenv(brand.EnvVar("LAUNCHER_PATH"), "/nonexistent/path/to/launcher")
	defer func() {
		if origLauncher != "" {
			_ = os.Setenv(brand.EnvVar("LAUNCHER_PATH"), origLauncher)
		} else {
			_ = os.Unsetenv(brand.EnvVar("LAUNCHER_PATH"))
		}
	}()

	exe := ResolveExe()
	if exe == "/nonexistent/path/to/launcher" {
		t.Error("should not return non-existent launcher path")
	}
	if exe == "" {
		t.Error("expected non-empty exe (fallback to os.Executable)")
	}
}

func TestResolveExe_Default(t *testing.T) {
	origLauncher := os.Getenv(brand.EnvVar("LAUNCHER_PATH"))
	_ = os.Unsetenv(brand.EnvVar("LAUNCHER_PATH"))
	defer func() {
		if origLauncher != "" {
			_ = os.Setenv(brand.EnvVar("LAUNCHER_PATH"), origLauncher)
		}
	}()

	exe := ResolveExe()
	if exe == "" {
		t.Error("expected non-empty exe path")
	}
}

func TestResolveExe_WithValidLauncherPath(t *testing.T) {
	tmpDir := t.TempDir()
	launcherPath := tmpDir + "/launcher-bin"
	if err := os.WriteFile(launcherPath, []byte("#!/bin/sh\n"), 0o755); err != nil {
		t.Fatal(err)
	}

	origLauncher := os.Getenv(brand.EnvVar("LAUNCHER_PATH"))
	_ = os.Setenv(brand.EnvVar("LAUNCHER_PATH"), launcherPath)
	defer func() {
		if origLauncher != "" {
			_ = os.Setenv(brand.EnvVar("LAUNCHER_PATH"), origLauncher)
		} else {
			_ = os.Unsetenv(brand.EnvVar("LAUNCHER_PATH"))
		}
	}()

	exe := ResolveExe()
	if exe != launcherPath {
		t.Errorf("expected %q, got %q", launcherPath, exe)
	}
}
