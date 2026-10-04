package sysutil

import (
	"os"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/testsupport/testenv"
)

func TestMain(m *testing.M) {
	if os.Getenv(gateHelperEnv) != "" {
		os.Exit(runGateHelper())
	}
	os.Exit(testenv.Run(m))
}
