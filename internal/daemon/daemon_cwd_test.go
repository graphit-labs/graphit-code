package daemon

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// The daemon must not keep the cwd it inherited. It outlives whoever spawned it, and
// when that directory is a test's t.TempDir() it is deleted moments later — after
// which every handler reaching for os.Getwd() fails, while handlers resolving from an
// explicit project_dir keep working. That split is what made the symptom look like it
// belonged to one module.
func TestDaemonLeavesTheDirectoryItWasSpawnedFrom(t *testing.T) {
	spawnDir := t.TempDir()

	orig, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	t.Cleanup(func() { _ = os.Chdir(orig) })

	if err := os.Chdir(spawnDir); err != nil {
		t.Fatalf("chdir: %v", err)
	}

	chdirToStableDir()

	got, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd after chdir: %v", err)
	}
	gotReal, _ := filepath.EvalSymlinks(got)
	spawnReal, _ := filepath.EvalSymlinks(spawnDir)
	if gotReal == spawnReal {
		t.Errorf("daemon stayed in the directory that spawned it (%s); "+
			"when that is a t.TempDir() it is deleted and os.Getwd() starts failing", got)
	}
	var globalReal string
	if want := brand.GlobalDir(); want != "" {
		globalReal, _ = filepath.EvalSymlinks(want)
		if gotReal != globalReal {
			t.Errorf("cwd = %s, want the brand global dir %s", got, want)
		}
	}
	if strings.HasPrefix(gotReal, os.TempDir()) && gotReal != globalReal {
		t.Errorf("cwd %s is under the temp dir, which is exactly what must not happen", got)
	}
}
