package task

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/projectlock"
)

func TestOpenDoesNotCreateIdentityUntilMutationNeedsTheStore(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv("GRAPHIT_HUB_BUCKET", "")
	projectDir := t.TempDir()
	lockPath := filepath.Join(projectDir, brand.LockFileName())

	service, err := Open(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(lockPath); !os.IsNotExist(err) {
		t.Fatalf("pure open created project identity: %v", err)
	}
	if err := service.ensureIdentity(); err != nil {
		t.Fatal(err)
	}
	lock, err := projectlock.Load(lockPath)
	if err != nil {
		t.Fatal(err)
	}
	if lock == nil || lock.Project.ID == "" {
		t.Fatalf("stateful open did not create identity: %#v", lock)
	}
}
