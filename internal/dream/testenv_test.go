package dream

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/testsupport/testenv"
)

func TestMain(m *testing.M) {
	binDir, err := os.MkdirTemp(testenv.Home(), "bin-")
	if err != nil {
		panic(err)
	}
	cli := filepath.Join(binDir, "opencode")
	if err := os.WriteFile(cli, []byte("#!/bin/sh\nexit 1\n"), 0o755); err != nil {
		panic(err)
	}
	if err := os.Setenv("PATH", binDir); err != nil {
		panic(err)
	}
	os.Exit(testenv.Run(m))
}
