package livesearch

import (
	"os"
	"path/filepath"
	"testing"
)

func TestRemoveCallsTheReclaimHookWithTheSessionID(t *testing.T) {
	mgr := NewManager(t.TempDir(), nil, nil)
	t.Cleanup(mgr.CloseAll)
	s, err := mgr.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}

	var got []string
	mgr.SetReclaim(func(id string) { got = append(got, id) })

	if err := mgr.Remove(s.ID()); err != nil {
		t.Fatalf("Remove: %v", err)
	}
	if len(got) != 1 || got[0] != s.ID() {
		t.Fatalf("reclaim was called with %v, want exactly [%s]", got, s.ID())
	}
}

func TestRemoveDeletesTheDirectoryEvenWithoutAHook(t *testing.T) {
	// The hook is optional, and its absence must not change what Remove is for.
	root := t.TempDir()
	mgr := NewManager(root, nil, nil)
	t.Cleanup(mgr.CloseAll)
	s, err := mgr.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	dir := filepath.Join(root, s.ID())

	if err := mgr.Remove(s.ID()); err != nil {
		t.Fatalf("Remove: %v", err)
	}
	if _, err := os.Stat(dir); err == nil {
		t.Error("the session directory survived Remove")
	}
}

func TestTheHookRunsAfterTheDirectoryIsGone(t *testing.T) {
	// Ordering matters: reclaiming global state is housekeeping, and deleting the
	// session is what the caller asked for. The hook must not be able to prevent it.
	root := t.TempDir()
	mgr := NewManager(root, nil, nil)
	t.Cleanup(mgr.CloseAll)
	s, err := mgr.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	dir := filepath.Join(root, s.ID())

	var dirExistedWhenHookRan bool
	mgr.SetReclaim(func(string) {
		_, err := os.Stat(dir)
		dirExistedWhenHookRan = err == nil
	})

	if err := mgr.Remove(s.ID()); err != nil {
		t.Fatalf("Remove: %v", err)
	}
	if dirExistedWhenHookRan {
		t.Error("the hook ran before the session directory was deleted")
	}
}

func TestAnUnknownSessionNeverReachesTheHook(t *testing.T) {
	mgr := NewManager(t.TempDir(), nil, nil)
	t.Cleanup(mgr.CloseAll)

	called := false
	mgr.SetReclaim(func(string) { called = true })

	if err := mgr.Remove("not-a-session-id"); err == nil {
		t.Error("removing an invalid ID should fail")
	}
	if called {
		t.Error("the hook must not run for an ID that was rejected")
	}
}
