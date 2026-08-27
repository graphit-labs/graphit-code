package commands

import (
	"strings"
	"testing"
)

func TestConfigOutputRedactsTheS3Secret(t *testing.T) {
	_, cleanup := setupTestHome(t)
	defer cleanup()

	if _, err := executeCommand("config", "--global", "hub.secret_access_key", "never-print-this"); err != nil {
		t.Fatalf("set secret: %v", err)
	}
	out, err := executeCommand("config", "--list", "--global")
	if err != nil {
		t.Fatalf("list global config: %v", err)
	}
	if strings.Contains(out, "never-print-this") || !strings.Contains(out, "***") {
		t.Fatalf("secret was not redacted: %q", out)
	}
}
