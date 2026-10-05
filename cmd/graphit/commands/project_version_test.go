package commands

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/projectversion"
)

// --project-version is a root flag, so every command accepts it, and it reaches the
// store resolvers of this process and of the CLI processes it starts through the
// environment pin.
func TestProjectVersionFlagPinsTheProcessAndItsChildren(t *testing.T) {
	t.Setenv(projectversion.EnvVersion(), "")
	for _, path := range [][]string{{"ast", "index"}, {"memory", "list"}, {"task", "list"}, {"sync"}} {
		cmd, _, err := rootCmd.Find(path)
		if err != nil {
			t.Fatalf("%v: %v", path, err)
		}
		if cmd.Flags().Lookup("project-version") == nil && cmd.InheritedFlags().Lookup("project-version") == nil {
			t.Errorf("%v does not accept --project-version", path)
		}
	}

	if err := rootCmd.PersistentFlags().Set("project-version", " feature/x "); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = rootCmd.PersistentFlags().Set("project-version", "") })
	pinProjectVersion()
	if got := os.Getenv(projectversion.EnvVersion()); got != "feature/x" {
		t.Fatalf("%s = %q, want feature/x", projectversion.EnvVersion(), got)
	}

	// The pin applies to the project this process started in — here, the repository
	// holding the test package.
	if got := projectversion.Current(".").Version; got != "feature/x" {
		t.Fatalf("the process's own project resolved %q", got)
	}
	if got := projectversion.Current(t.TempDir()).Version; got != "main" {
		t.Fatalf("another project inherited the pin: %q", got)
	}
}

// A worktree arrives with the committed lockfile but was never initialised: sync
// registers it as an instance of the project so the daemon supervises it.
func TestSyncRegistersTheCheckoutAsAProjectInstance(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), "")
	worktree := t.TempDir()
	lf := &hub.Lockfile{}
	lf.Project.ID = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
	lf.Project.Name = "demo"
	if err := os.WriteFile(filepath.Join(worktree, brand.LockFileName()),
		[]byte(`{"project":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","name":"demo"}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	registerCheckout(worktree, lf)
	registerCheckout(worktree, lf)

	mgr, err := hub.NewGlobalLockManager()
	if err != nil {
		t.Fatal(err)
	}
	active, err := mgr.ListActiveProjects()
	if err != nil {
		t.Fatal(err)
	}
	instances := 0
	for _, p := range active {
		if p.ID == lf.Project.ID && p.Dir == worktree {
			instances++
		}
	}
	if instances != 1 {
		t.Fatalf("worktree registered %d times, want once: %#v", instances, active)
	}
}
