package mcpstdio

import (
	"context"
	"encoding/json"
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func testMCPClient(t *testing.T) *mcp.ClientSession {
	t.Helper()
	ctx, cancel := context.WithCancel(context.Background())
	serverTransport, clientTransport := mcp.NewInMemoryTransports()
	server := NewServer()
	errCh := make(chan error, 1)
	go func() { errCh <- server.Run(ctx, serverTransport) }()
	client := mcp.NewClient(&mcp.Implementation{Name: "pagination-test", Version: "1"}, nil)
	session, err := client.Connect(ctx, clientTransport, nil)
	if err != nil {
		cancel()
		t.Fatal(err)
	}
	t.Cleanup(func() {
		_ = session.Close()
		cancel()
		<-errCh
	})
	return session
}

func TestSearchToolSchemasExposeIndependentPagination(t *testing.T) {
	session := testMCPClient(t)
	listed, err := session.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]bool{
		brand.MCPToolName("ast", "query"):        false,
		brand.MCPToolName("ast", "search"):       false,
		brand.MCPToolName("knowledge", "search"): false,
		brand.MCPToolName("memory", "search"):    false,
		brand.MCPToolName("task", "search"):      false,
		brand.MCPToolName("wiki", "search"):      false,
	}
	for _, tool := range listed.Tools {
		if _, ok := want[tool.Name]; !ok {
			continue
		}
		encoded, err := json.Marshal(tool.InputSchema)
		if err != nil {
			t.Fatal(err)
		}
		var schema struct {
			Properties map[string]any `json:"properties"`
		}
		if err := json.Unmarshal(encoded, &schema); err != nil {
			t.Fatal(err)
		}
		for _, field := range []string{"page_size", "cursor"} {
			if _, ok := schema.Properties[field]; !ok {
				t.Errorf("%s schema does not expose %s", tool.Name, field)
			}
		}
		want[tool.Name] = true
	}
	for tool, seen := range want {
		if !seen {
			t.Errorf("tool %s was not listed", tool)
		}
	}
}
