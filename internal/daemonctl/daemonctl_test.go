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
	locked := make(chan error, 1)
	writePID := make(chan struct{})
	written := make(chan error, 1)
	release := make(chan struct{})
	go func() {
		file, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0o600)
		if err != nil {
			locked <- err
			return
		}
		defer file.Close()
		if err := flockExclusiveBlocking(file); err != nil {
			locked <- err
			return
		}
		locked <- nil
		<-writePID
		_, err = file.WriteString("12345\n")
		written <- err
		<-release
		flockProbeRelease(file)
	}()
	defer close(release)
	result := make(chan error, 1)
	go func() { result <- waitForFileLock(path, time.Second, time.Millisecond) }()
	if err := <-locked; err != nil {
		t.Fatal(err)
	}
	select {
	case err := <-result:
		t.Fatalf("lock without PID reported ready: %v", err)
	case <-time.After(20 * time.Millisecond):
	}
	close(writePID)
	if err := <-written; err != nil {
		t.Fatal(err)
	}
	if err := <-result; err != nil {
		t.Fatal(err)
	}
}

func TestWaitForFileLockRejectsStalePID(t *testing.T) {
	path := filepath.Join(t.TempDir(), "daemon.pid")
	if err := os.WriteFile(path, []byte("12345\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	file, err := os.OpenFile(path, os.O_RDWR, 0o600)
	if err != nil {
		t.Fatal(err)
	}
	defer file.Close()
	if err := flockExclusiveBlocking(file); err != nil {
		t.Fatal(err)
	}
	defer flockProbeRelease(file)
	if err := waitForFileLock(path, 0, 0, 12345); err == nil {
		t.Fatal("stale PID reported ready")
	}
	if err := file.Truncate(0); err != nil {
		t.Fatal(err)
	}
	if _, err := file.WriteString("67890\n"); err != nil {
		t.Fatal(err)
	}
	if err := waitForFileLock(path, 0, 0, 12345); err != nil {
		t.Fatal(err)
	}
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
