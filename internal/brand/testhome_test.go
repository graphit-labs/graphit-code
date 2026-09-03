package brand

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/testsupport/testenv"
)

func TestTestHomeIsIsolated(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil {
		t.Fatalf("UserHomeDir: %v", err)
	}

	if testenv.Home() == "" {
		t.Fatal("test environment did not create an isolated home")
	}
	if home != testenv.Home() {
		t.Fatalf("UserHomeDir() = %q, want isolated home %q", home, testenv.Home())
	}
	root := os.Getenv("GRAPHIT_TEST_HOME_ROOT")
	if root == "" {
		root = filepath.Join(os.TempDir(), "graphit-test-homes")
	}
	root, err = filepath.Abs(root)
	if err != nil {
		t.Fatalf("absolute test home root: %v", err)
	}
	if !strings.HasPrefix(home, root+string(os.PathSeparator)) {
		t.Fatalf("isolated home %q is not under %q", home, root)
	}

	if v, ok := os.LookupEnv(EnvVar("GLOBAL_DIR")); ok {
		t.Fatalf("%s is still set to %q inside a test binary — the isolated home is bypassed",
			EnvVar("GLOBAL_DIR"), v)
	}

	if got, want := GlobalDir(), filepath.Join(home, DotDir()); got != want {
		t.Fatalf("GlobalDir() = %q; want %q", got, want)
	}

	if err := os.MkdirAll(GlobalDir(), 0o755); err != nil {
		t.Fatalf("isolated GlobalDir() is not writable: %v", err)
	}
}
