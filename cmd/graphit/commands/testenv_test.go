package commands

import (
	"os"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/clirun"
	_ "github.com/graphit-labs/graphit-code/internal/testsupport"
	"github.com/graphit-labs/graphit-code/internal/testsupport/testenv"
)

// TestMain doubles as the CLI. Commands such as `sync` run their steps as child
// processes of the graphit executable, which inside a test is this test binary; started
// that way it executes the requested command instead of re-running the suite.
func TestMain(m *testing.M) {
	if clirun.IsSubprocess() {
		rootCmd.SetArgs(os.Args[1:])
		Execute()
		os.Exit(0)
	}
	_ = os.Setenv("GRAPHIT_TEST_CLI", "1")
	os.Exit(testenv.Run(m))
}
