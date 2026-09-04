package commands

import (
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
)

func TestAssignPublishingProjectUsesTheLockfileIdentity(t *testing.T) {
	dir := t.TempDir()
	lockPath := filepath.Join(dir, brand.LockFileName())
	if err := hub.SaveLockfile(lockPath, &hub.Lockfile{
		Project:   hub.ProjectIdentity{ID: "01PROJECT", Name: "payments"},
		Artifacts: make(map[hub.ArtifactType]map[string]*hub.LockfileArtifactMeta),
	}); err != nil {
		t.Fatalf("SaveLockfile: %v", err)
	}

	meta := &hub.Entry{ID: "payments-ast", Type: hub.TypeAST}
	if err := assignPublishingProject(meta, dir); err != nil {
		t.Fatalf("assignPublishingProject: %v", err)
	}
	if meta.ProjectID != "01PROJECT" {
		t.Fatalf("ProjectID = %q, want %q", meta.ProjectID, "01PROJECT")
	}
}

func TestAssignPublishingProjectRejectsADirectoryWithoutALockfile(t *testing.T) {
	meta := &hub.Entry{ID: "payments-knowledge", Type: hub.TypeKnowledge}
	err := assignPublishingProject(meta, t.TempDir())
	if err == nil {
		t.Fatal("expected missing lockfile to fail")
	}
}
