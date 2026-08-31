package ast

import (
	"os"
	"path/filepath"
	"testing"
)

func writePublicationMarker(t *testing.T, dir, value string) {
	t.Helper()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatalf("mkdir publication: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "marker"), []byte(value), 0o644); err != nil {
		t.Fatalf("write publication marker: %v", err)
	}
}

func readPublicationMarker(t *testing.T, dir string) string {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(dir, "marker"))
	if err != nil {
		t.Fatalf("read publication marker: %v", err)
	}
	return string(raw)
}

func TestIcebugPublicationRollbackRestoresPreviousBundle(t *testing.T) {
	parent := t.TempDir()
	finalDir := filepath.Join(parent, "graph.icebug")
	backupDir := filepath.Join(parent, "graph.icebug.backup.test")
	writePublicationMarker(t, finalDir, "next")
	writePublicationMarker(t, backupDir, "previous")

	publication := &icebugPublication{
		finalDir: finalDir, backupDir: backupDir, active: true,
	}
	if err := publication.Rollback(); err != nil {
		t.Fatalf("rollback: %v", err)
	}
	if got := readPublicationMarker(t, finalDir); got != "previous" {
		t.Fatalf("published marker after rollback = %q, want previous", got)
	}
	if _, err := os.Stat(backupDir); !os.IsNotExist(err) {
		t.Fatalf("backup survived rollback: %v", err)
	}
	if matches, err := filepath.Glob(finalDir + ".discard.*"); err != nil || len(matches) != 0 {
		t.Fatalf("discard directories after rollback = %v, err=%v", matches, err)
	}
}

func TestIcebugPublicationCommitKeepsNextBundle(t *testing.T) {
	parent := t.TempDir()
	finalDir := filepath.Join(parent, "graph.icebug")
	backupDir := filepath.Join(parent, "graph.icebug.backup.test")
	writePublicationMarker(t, finalDir, "next")
	writePublicationMarker(t, backupDir, "previous")

	publication := &icebugPublication{
		finalDir: finalDir, backupDir: backupDir, active: true,
	}
	publication.Commit()
	if got := readPublicationMarker(t, finalDir); got != "next" {
		t.Fatalf("published marker after commit = %q, want next", got)
	}
	if _, err := os.Stat(backupDir); !os.IsNotExist(err) {
		t.Fatalf("backup survived commit: %v", err)
	}
}
