package brand

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestTestHomeIsIsolated(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil {
		t.Fatalf("UserHomeDir: %v", err)
	}

	if isolatedTestHome == "" {
		t.Fatal("init did not isolate HOME — testing.Testing() was false inside a test binary")
	}
	if home != isolatedTestHome {
		t.Fatalf("a test binary resolved its home to %q instead of the isolated %q — "+
			"whatever this suite writes into GlobalDir() is landing in the operator's real home",
			home, isolatedTestHome)
	}
	if root := filepath.Join(os.TempDir(), "graphit-test-homes"); !strings.HasPrefix(home, root+string(os.PathSeparator)) {
		t.Fatalf("isolated home %q is not under %q, which is the directory make test sweeps", home, root)
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
