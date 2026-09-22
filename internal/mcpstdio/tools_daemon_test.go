package mcpstdio

import (
	"context"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

func TestDaemonToolsOnlyExposeStatus(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	client := testMCPClient(t)
	listed, err := client.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}

	statusName := brand.MCPToolName("daemon", "status")
	var daemonTools []string
	for _, tool := range listed.Tools {
		if strings.HasPrefix(tool.Name, brand.MCPToolName("daemon")+"_") {
			daemonTools = append(daemonTools, tool.Name)
		}
	}

	if len(daemonTools) != 1 || daemonTools[0] != statusName {
		t.Fatalf("daemon tools = %v; want only %q", daemonTools, statusName)
	}

	result, err := client.CallTool(context.Background(), &mcp.CallToolParams{
		Name:      statusName,
		Arguments: map[string]any{"ai_optimized": false},
	})
	if err != nil {
		t.Fatalf("calling %s: %v", statusName, err)
	}
	if result.IsError || len(result.Content) == 0 {
		t.Fatalf("status result = %+v; want a successful status payload", result)
	}
}
