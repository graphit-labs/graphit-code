//go:build lancedb

package commands

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/store"
)

// Exercise the actual index commands, including both memory scopes. Internal
// artifact locations should stay out of the terminal output.
func TestSyncOutputIndexCommandsOmitStoragePaths(t *testing.T) {
	_, cleanup := setupTestHome(t)
	defer cleanup()
	globalDir := t.TempDir()
	t.Setenv("GRAPHIT_GLOBAL_DIR", globalDir)
	root := t.TempDir()
	t.Chdir(root)
	if _, err := store.EnsureProjectID(root); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "README.md"), []byte("# Test project\n\nTest docs.\n"), 0600); err != nil {
		t.Fatal(err)
	}
	var got string
	for _, args := range [][]string{{"knowledge", "index"}, {"memory", "index"}, {"memory", "index", "--user"}} {
		out, err := executeCommand(args...)
		if err != nil {
			t.Fatalf("%v: %v", args, err)
		}
		got += out
	}
	for _, want := range []string{"Wiki generated:", "project memory indexes are current", "user memory indexes are current"} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q: %s", want, got)
		}
	}
	if strings.Contains(got, globalDir) {
		t.Errorf("unexpected storage path: %s", got)
	}
}
