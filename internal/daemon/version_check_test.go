package daemon

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestReadLauncherStamp_MultilineContent(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	stamp := launcherStampPath()
	if err := os.MkdirAll(filepath.Dir(stamp), 0o755); err != nil {
		t.Fatal(err)
	}
	// Write multiline content — TrimSpace should strip trailing newlines.
	if err := os.WriteFile(stamp, []byte("v2.0.1\nextra-line\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	got := readLauncherStamp()
	// TrimSpace strips trailing whitespace but keeps content between.
	if got != "v2.0.1\nextra-line" {
		t.Errorf("expected 'v2.0.1\\nextra-line', got %q", got)
	}
}

// version_check.go — launcherStampPath components

func TestLauncherStampPath_ContainsBrandDir(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	stamp := launcherStampPath()
	if !filepath.IsAbs(stamp) {
		t.Errorf("expected absolute path, got %q", stamp)
	}
	expectedDir := filepath.Join(tempHome, "."+brand.Brand, "daemon")
	if filepath.Dir(stamp) != expectedDir {
		t.Errorf("expected dir %q, got %q", expectedDir, filepath.Dir(stamp))
	}
	if filepath.Base(stamp) != "launcher.stamp" {
		t.Errorf("expected basename 'launcher.stamp', got %q", filepath.Base(stamp))
	}
}

func TestCronMarker_Format(t *testing.T) {
	t.Parallel()
	marker := cronMarker()
	// Should start with "# "
	if len(marker) < 3 || marker[:2] != "# " {
		t.Errorf("marker should start with '# ', got %q", marker)
	}
	// Should be uppercase
	suffix := marker[2:]
	for _, r := range suffix {
		if r >= 'a' && r <= 'z' {
			t.Errorf("marker should be uppercase, got %q", marker)
			break
		}
	}
}

func TestIsSchedulerInstalled_ReturnsBool(t *testing.T) {
	// Should not panic and should return a boolean.
	result := IsSchedulerInstalled()
	// We just verify it doesn't panic. The actual value depends on the system.
	_ = result
}
