package daemon

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestProjectRecentlyActiveAfterFileDeletion(t *testing.T) {
	projectDir := t.TempDir()
	oldFile := filepath.Join(projectDir, "old.go")
	if err := os.WriteFile(oldFile, []byte("package old\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	old := time.Now().Add(-2 * time.Hour)
	if err := os.Chtimes(oldFile, old, old); err != nil {
		t.Fatal(err)
	}
	if err := os.Chtimes(projectDir, old, old); err != nil {
		t.Fatal(err)
	}

	d := &Daemon{}
	if d.projectRecentlyActive(projectDir, time.Hour) {
		t.Fatal("old project should be parked")
	}
	target := filepath.Join(projectDir, "removed.go")
	if err := os.WriteFile(target, []byte("package old\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Remove(target); err != nil {
		t.Fatal(err)
	}
	if !d.projectRecentlyActive(projectDir, time.Hour) {
		t.Fatal("deleting a file should wake the parked project")
	}
}
