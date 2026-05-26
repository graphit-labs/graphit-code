package mcpserver

import (
	"bytes"
	"os"
	"testing"
)

func TestMCPServerBasic(t *testing.T) {
	// 1. NewServer
	server := NewServer()
	if server == nil {
		t.Fatal("expected non-nil MCP server instance")
	}

	// 2. logVerbose
	// Redirect stderr to capture verbose log
	oldStderr := os.Stderr
	r, w, _ := os.Pipe()
	os.Stderr = w

	logVerbose(true, "test verbose log: %s", "hello")
	logVerbose(false, "should not log: %s", "world")

	w.Close()
	var buf bytes.Buffer
	os.Stderr = oldStderr
	_, _ = buf.ReadFrom(r)

	out := buf.String()
	if !stringsContains(out, "test verbose log: hello") {
		t.Errorf("expected verbose log, got %q", out)
	}
	if stringsContains(out, "should not log: world") {
		t.Errorf("unexpected log output: %q", out)
	}
}

func stringsContains(s, sub string) bool {
	return len(s) >= len(sub) && (s == sub || stringsContainsHelper(s, sub))
}

func stringsContainsHelper(s, sub string) bool {
	for i := 0; i <= len(s)-len(sub); i++ {
		if s[i:i+len(sub)] == sub {
			return true
		}
	}
	return false
}
