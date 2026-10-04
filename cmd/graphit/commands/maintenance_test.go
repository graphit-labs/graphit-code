package commands

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/spf13/cobra"
)

// Project-scoped maintenance must fail outside a project, and must not leave a
// half-initialised project behind in whatever directory it was started from.
func TestProjectMaintenanceCommandsRefuseADirectoryThatIsNotAProject(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	for name, build := range map[string]func() *cobra.Command{
		"task maintain":   newTaskMaintainCmd,
		"memory maintain": newMemoryMaintainCmd,
		"memory embed":    newMemoryEmbedCmd,
		"dream run":       newDreamRunCmd,
	} {
		t.Run(name, func(t *testing.T) {
			dir := t.TempDir()
			t.Chdir(dir)
			cmd := build()
			cmd.SetArgs(nil)
			cmd.SilenceErrors, cmd.SilenceUsage = true, true
			if err := cmd.Execute(); err == nil {
				t.Fatal("succeeded outside a project")
			}
			if _, err := os.Stat(filepath.Join(dir, brand.LockFileName())); err == nil {
				t.Fatal("created a lockfile in a directory that was not a project")
			}
		})
	}
}
