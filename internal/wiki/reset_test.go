package wiki

import (
	"os"
	"path/filepath"
	"testing"
)

func TestResetDirClearsAndRecreatesTheTarget(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "wiki")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "old"), []byte("old"), 0o644); err != nil {
		t.Fatal(err)
	}
	got, err := ResetDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	if got != dir {
		t.Fatalf("ResetDir = %q, want %q", got, dir)
	}
	if _, err := os.Stat(filepath.Join(dir, "old")); !os.IsNotExist(err) {
		t.Fatalf("old entry survived reset: %v", err)
	}
}

func TestResetDirRefusesAnEmptyPath(t *testing.T) {
	if _, err := ResetDir(""); err == nil {
		t.Fatal("ResetDir accepted an empty path")
	}
}
