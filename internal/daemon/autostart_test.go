//go:build linux

package daemon

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestEnsureRunning_AlreadyAliveWithoutUserManager(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "systemctl"), []byte("#!/bin/sh\nexit 1\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("HOME", root)
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(root, "config"))
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), root)
	t.Setenv("PATH", root)

	pf := NewPIDFile()
	if err := pf.Acquire(); err != nil {
		t.Fatalf("Acquire: %v", err)
	}
	defer pf.Release()

	started, err := EnsureRunning()
	if err != nil {
		t.Errorf("unexpected error: %v", err)
	}
	if started {
		t.Error("expected started=false when daemon is already alive")
	}
	if pf.IsAlive() == nil {
		t.Error("existing daemon lost its PID lock")
	}
}
