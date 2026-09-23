package daemonservice

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestResolveExecutableUsesDirectlyInvokedBinary(t *testing.T) {
	t.Setenv(brand.EnvVar("LAUNCHER_PATH"), "")
	want, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	want, err = filepath.Abs(want)
	if err != nil {
		t.Fatal(err)
	}
	got, err := ResolveExecutable()
	if err != nil || got != want {
		t.Fatalf("resolved executable = %q, error = %v; want %q", got, err, want)
	}
}
