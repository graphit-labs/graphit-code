package commands

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/clirun"
)

func TestManagedChildrenDoNotStartBackgroundServices(t *testing.T) {
	sync := newSyncCmd()
	rootCmd.AddCommand(sync)
	t.Cleanup(func() { rootCmd.RemoveCommand(sync) })

	t.Setenv(clirun.SubprocessEnv(), "")
	if !startsBackgroundServices(sync) {
		t.Fatal("an interactive sync must keep ensuring the daemon and tray")
	}

	t.Setenv(clirun.SubprocessEnv(), "1")
	if startsBackgroundServices(sync) {
		t.Fatal("a sync started by the daemon must not start the daemon or tray")
	}
}
