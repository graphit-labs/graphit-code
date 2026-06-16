package daemonctl

import (
	"os"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

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
