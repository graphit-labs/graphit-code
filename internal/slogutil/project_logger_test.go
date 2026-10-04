package slogutil

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestProjectLogger(t *testing.T) {
	root := t.TempDir()
	for _, message := range []string{"first run", "second run"} {
		logger, closeLog, err := ProjectLogger(root, "ast")
		if err != nil {
			t.Fatal(err)
		}
		logger.Warn(message)
		closeLog()
	}
	data, err := os.ReadFile(filepath.Join(root, ".graphit/runtime/daemon/daemon.log"))
	if err != nil {
		t.Fatal(err)
	}
	for _, marker := range []string{"first run", "second run", "module=ast"} {
		if !strings.Contains(string(data), marker) {
			t.Fatalf("missing %q: %s", marker, data)
		}
	}
}

func TestProjectLoggerOpenFailure(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, ".graphit"), []byte("blocked"), 0600); err != nil {
		t.Fatal(err)
	}
	logger, closeLog, err := ProjectLogger(root, "ast")
	if err == nil || logger != nil || closeLog != nil {
		t.Fatal("failed sink silently accepted")
	}
}
