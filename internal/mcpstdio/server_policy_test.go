package mcpstdio

import (
	"context"
	"net/http"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/agentpolicy"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

func TestDreamMemoryProfileRegistersExactAllowlist(t *testing.T) {
	t.Setenv(agentpolicy.EnvProfile, agentpolicy.ProfileDreamMemory)
	session := testMCPClient(t)
	listed, err := session.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	got := make(map[string]bool, len(listed.Tools))
	for _, tool := range listed.Tools {
		got[tool.Name] = true
	}
	if len(got) != len(dreamMemoryTools) {
		t.Fatalf("Dream MCP exposes %d tools, want %d; got=%v", len(got), len(dreamMemoryTools), got)
	}
	for name := range dreamMemoryTools {
		if !got[name] {
			t.Errorf("Dream MCP is missing allowlisted tool %q", name)
		}
	}
	for name := range got {
		if !dreamMemoryTools[name] {
			t.Errorf("Dream MCP exposed non-allowlisted tool %q", name)
		}
	}
}

func TestDreamMemoryProfileDeniesNewToolsByDefault(t *testing.T) {
	t.Setenv(agentpolicy.EnvProfile, agentpolicy.ProfileDreamMemory)
	if dreamMemoryTools["graphit_task_create"] || dreamMemoryTools["graphit_memory_mark_mandatory"] || dreamMemoryTools["graphit_ast_index"] {
		t.Fatal("Dream allowlist includes a forbidden mutation")
	}
}

func TestDreamMemoryProfileRequiresMutationPreconditions(t *testing.T) {
	t.Setenv(agentpolicy.EnvProfile, agentpolicy.ProfileDreamMemory)
	req := &mcp.CallToolRequest{Extra: &mcp.RequestExtra{Header: http.Header{agentpolicy.ProfileHeader: []string{agentpolicy.ProfileDreamMemory}}}}
	if err := requireDreamMemoryPrecondition(req, nil, ""); err == nil {
		t.Fatal("Dream accepted an unfenced existing-memory mutation")
	}
	revision := 3
	if err := requireDreamMemoryPrecondition(req, &revision, ""); err != nil {
		t.Fatalf("revision precondition rejected: %v", err)
	}
	if err := requireDreamMemoryPrecondition(req, nil, "hash"); err != nil {
		t.Fatalf("hash precondition rejected: %v", err)
	}
}
