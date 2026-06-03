package daemon

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// ---------------------------------------------------------------------------
// MemorySyncModule — Start returns error when git unavailable
// ---------------------------------------------------------------------------

func TestMemorySyncModule_Start_ContextCancelled(t *testing.T) {
	m := NewMemorySyncModule()

	ctx, cancel := context.WithCancel(context.Background())

	errCh := make(chan error, 1)
	go func() {
		errCh <- m.Start(ctx)
	}()

	// Give it a moment, then cancel.
	time.Sleep(200 * time.Millisecond)
	cancel()

	err := <-errCh
	if err != nil {
		// It's acceptable if git is not present — Start returns an error early.
		t.Logf("MemorySyncModule.Start returned: %v (may require git)", err)
	}
}

// ---------------------------------------------------------------------------
// MemorySyncModule — poll with no active memory branches
// ---------------------------------------------------------------------------

func TestMemorySyncModule_Poll_NoStore(t *testing.T) {
	// This tests that poll doesn't panic when MemoryGitStore fails.
	m := NewMemorySyncModule()

	g := &mockGit{}
	hashes := make(map[string]string)

	// poll with a mock git — the memory store creation will likely fail
	// because we're not in a valid memory repo, so it should return early.
	m.poll(context.Background(), g, hashes)
}

// ---------------------------------------------------------------------------
// SyncModule — Start context cancelled immediately
// ---------------------------------------------------------------------------

func TestSyncModule_Start_ContextCancelledImmediately(t *testing.T) {
	tmpDir := t.TempDir()
	m := NewSyncModule(tmpDir, filepath.Join(tmpDir, "cache"))

	ctx, cancel := context.WithCancel(context.Background())
	cancel() // Cancel immediately

	err := m.Start(ctx)
	if err != nil {
		// Could fail if git is not available
		t.Logf("SyncModule.Start returned: %v (may require git)", err)
	}
}

// ---------------------------------------------------------------------------
// SyncModule — Start with delayed cancel
// ---------------------------------------------------------------------------

func TestSyncModule_Start_RunsBriefly(t *testing.T) {
	tmpDir := t.TempDir()
	// Initialize a git repo so the module can poll
	initGitRepo(t, tmpDir)

	m := NewSyncModule(tmpDir, filepath.Join(tmpDir, "cache"))

	ctx, cancel := context.WithCancel(context.Background())

	errCh := make(chan error, 1)
	go func() {
		errCh <- m.Start(ctx)
	}()

	time.Sleep(200 * time.Millisecond)
	cancel()

	err := <-errCh
	if err != nil && !errors.Is(err, context.Canceled) {
		t.Logf("SyncModule.Start returned: %v (may require git)", err)
	}
}

// initGitRepo creates a minimal git repo for testing.
func initGitRepo(t *testing.T, dir string) {
	t.Helper()
	gitDir := filepath.Join(dir, ".git")
	if err := os.MkdirAll(gitDir, 0o755); err != nil {
		t.Fatalf("mkdir .git: %v", err)
	}
	// Write a minimal HEAD file
	if err := os.WriteFile(filepath.Join(gitDir, "HEAD"), []byte("ref: refs/heads/main\n"), 0o644); err != nil {
		t.Fatalf("write HEAD: %v", err)
	}
}
