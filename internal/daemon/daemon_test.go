package daemon

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestDaemonHelpers(t *testing.T) {
	tempHome, err := os.MkdirTemp("", "graphit-daemon-test-*")
	if err != nil {
		t.Fatalf("failed to create temp home: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempHome) }()

	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// 1. Stamp paths
	stamp := launcherStampPath()
	expectedStamp := filepath.Join(tempHome, "."+brand.Brand, "daemon", "launcher.stamp")
	if stamp != expectedStamp {
		t.Errorf("expected %s, got %s", expectedStamp, stamp)
	}

	// 2. Read stamp when missing
	if readLauncherStamp() != "" {
		t.Error("expected empty stamp when file is missing")
	}

	// 3. Write and read stamp
	err = os.MkdirAll(filepath.Dir(stamp), 0755)
	if err != nil {
		t.Fatalf("failed to create stamp dir: %v", err)
	}
	err = os.WriteFile(stamp, []byte("  my-stamp-value  \n"), 0644)
	if err != nil {
		t.Fatalf("failed to write stamp: %v", err)
	}

	if readLauncherStamp() != "my-stamp-value" {
		t.Errorf("expected 'my-stamp-value', got %q", readLauncherStamp())
	}
}
