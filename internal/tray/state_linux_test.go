//go:build linux

package tray

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
)

func TestCaptureKeepsTrayControlsWhenUserManagerUnavailable(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "systemctl"), []byte("#!/bin/sh\nexit 1\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), root)
	t.Setenv("PATH", root)
	stopped := capture()
	if stopped.state != "Daemon stopped; service unavailable" || !stopped.canStart || stopped.canStop {
		t.Fatalf("stopped tray state: %+v", stopped)
	}
	lock, err := lockfile.TryAcquire(daemonctl.PIDFilePath())
	if err != nil {
		t.Fatal(err)
	}
	defer lock.Release()
	running := capture()
	if running.state != "Daemon running directly" || running.canStart || !running.canStop {
		t.Fatalf("running tray state: %+v", running)
	}
}
