package brand

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestTestHomeIsIsolated is the regression guard for the whole suite, not just for this
// package: it asserts that a test binary resolves its home to a throwaway directory
// rather than to the operator's.
//
// The failure it exists to catch is silent by nature. Nothing goes red when a test
// writes a project graph, a compiled wiki or a memory branch into the real
// ~/.graphit — the test passes, and the residue is only found later, by a human
// noticing that their global directory has grown temporary projects that no longer
// exist. Measured before the isolation in testhome.go existed: 160 MB of test
// fixtures across 43 AST project graphs and 39 knowledge wikis.
func TestTestHomeIsIsolated(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil {
		t.Fatalf("UserHomeDir: %v", err)
	}

	// Compared against what init actually created, not against TestHomeRoot(): another
	// test in this package reassigns Brand, which would move the root out from under
	// this assertion while the home in use stayed put.
	if isolatedTestHome == "" {
		t.Fatal("init did not isolate HOME — testing.Testing() was false inside a test binary")
	}
	if home != isolatedTestHome {
		t.Fatalf("a test binary resolved its home to %q instead of the isolated %q — "+
			"whatever this suite writes into GlobalDir() is landing in the operator's real home",
			home, isolatedTestHome)
	}
	if root := filepath.Join(os.TempDir(), "graphit-test-homes"); !strings.HasPrefix(home, root+string(os.PathSeparator)) {
		// The literal, not TestHomeRoot(), for the same reason — and because this is the
		// path `make test` sweeps, so a drift between the two would leak every run.
		t.Fatalf("isolated home %q is not under %q, which is the directory make test sweeps", home, root)
	}

	// An operator who exports the override in their shell would otherwise hand the whole
	// suite their real store, because GlobalDir() honours it before it looks at HOME.
	if v, ok := os.LookupEnv(EnvVar("GLOBAL_DIR")); ok {
		t.Fatalf("%s is still set to %q inside a test binary — the isolated home is bypassed",
			EnvVar("GLOBAL_DIR"), v)
	}

	// The global directory has to follow HOME, since that is the whole mechanism.
	if got, want := GlobalDir(), filepath.Join(home, DotDir()); got != want {
		t.Fatalf("GlobalDir() = %q; want %q", got, want)
	}

	// And it must be writable, or every test that isolates nothing but relies on this
	// would fail somewhere far less obvious than here.
	if err := os.MkdirAll(GlobalDir(), 0o755); err != nil {
		t.Fatalf("isolated GlobalDir() is not writable: %v", err)
	}
}
