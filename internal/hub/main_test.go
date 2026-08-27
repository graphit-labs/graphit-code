package hub

import (
	"fmt"
	"os"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/testsupport"
)

// See internal/memory/main_test.go for the full reasoning. This package commits into
// temporary directories too, and reads the same global config, so it can reach a real
// remote and lose the same race between a git writer and t.TempDir's removal.
//
// The testsupport import is not decorative: publishing an AST artifact builds a real
// graph, which needs the grammar definitions, and those are installed rather than
// compiled in. Importing the package runs its seeding init — see
// internal/testsupport/runtimequeries.go.
func TestMain(m *testing.M) {
	if err := testsupport.SeedError(); err != nil {
		fmt.Fprintf(os.Stderr, "cannot seed grammar queries: %v\n", err)
		os.Exit(1)
	}

	git.DisableAutoMaintenance()

	for k, v := range map[string]string{
		"GIT_AUTHOR_NAME": "Test", "GIT_AUTHOR_EMAIL": "test@example.com",
		"GIT_COMMITTER_NAME": "Test", "GIT_COMMITTER_EMAIL": "test@example.com",
	} {
		_ = os.Setenv(k, v)
	}

	code := m.Run()
	os.Exit(code)
}
