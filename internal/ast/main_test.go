package ast

import (
	"fmt"
	"os"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/testsupport"
	"github.com/graphit-labs/graphit-code/internal/testsupport/testenv"
)

// TestMain checks that this package got the grammar definitions it parses with.
//
// They are not compiled in — they are installed into the runtime directory under the
// operator's home, and a test binary has no installer. testsupport seeds them from this
// checkout in its own init(), which has to happen before this package's init() reads
// the directory; see internal/testsupport/runtimequeries.go for the ordering and for
// why depending on the developer's installed copy was hiding a machine-dependent suite.
//
// Fatal rather than skipped: without languages, .sql parses produce nothing and the
// tests that depend on them fail with an empty graph instead of a missing grammar,
// which sends the next reader after the wrong bug.
func TestMain(m *testing.M) {
	if err := testsupport.SeedError(); err != nil {
		fmt.Fprintf(os.Stderr, "cannot seed grammar queries: %v\n", err)
		os.Exit(1)
	}
	os.Exit(testenv.Run(m))
}
