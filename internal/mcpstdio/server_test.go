package mcpstdio

import (
	"testing"
)

func TestMCPServerBasic(t *testing.T) {
	server := NewServer()
	if server == nil {
		t.Fatal("expected non-nil MCP server instance")
	}
}
