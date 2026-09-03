package memory

import (
	"os"
	"testing"
)

func TestMain(m *testing.M) {
	home, err := os.MkdirTemp("", "graphit-memory-test-home-")
	if err == nil {
		_ = os.Setenv("HOME", home)
		_ = os.Setenv("USERPROFILE", home)
	}

	code := m.Run()

	if home != "" {
		_ = os.RemoveAll(home)
	}
	os.Exit(code)
}
