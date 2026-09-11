package mcpstdio

import (
	"context"
	"encoding/json"
	"strings"
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

func TestWikiToolContractsDoNotExposeMemoryAsAWikiScope(t *testing.T) {
	session := testMCPClient(t)
	listed, err := session.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]bool{}
	for _, action := range []string{"search", "browse", "log", "xrefs", "embed", "source"} {
		want[brand.MCPToolName("wiki", action)] = false
	}
	for _, tool := range listed.Tools {
		if _, ok := want[tool.Name]; !ok {
			continue
		}
		encoded, err := json.Marshal(tool.InputSchema)
		if err != nil {
			t.Fatal(err)
		}
		contract := strings.ToLower(tool.Description + "\n" + string(encoded))
		if strings.Contains(contract, "memory") {
			t.Errorf("%s still exposes Memory through a Wiki contract: %s", tool.Name, contract)
		}
		want[tool.Name] = true
	}
	for tool, seen := range want {
		if !seen {
			t.Errorf("tool %s was not listed", tool)
		}
	}
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

func TestAgentToolSchemasExposeNoIDEField(t *testing.T) {
	session := testMCPClient(t)
	listed, err := session.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]bool{
		brand.MCPToolName("init"):             false,
		brand.MCPToolName("sync"):             false,
		brand.MCPToolName("update"):           false,
		brand.MCPToolName("remove"):           false,
		brand.MCPToolName("hub", "install"):   false,
		brand.MCPToolName("hub", "uninstall"): false,
		brand.MCPToolName("hub", "update"):    false,
		brand.MCPToolName("hub", "link"):      false,
		brand.MCPToolName("hub", "unlink"):    false,
		brand.MCPToolName("hub", "type-path"): false,
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
		if _, ok := schema.Properties["agent"]; !ok {
			t.Errorf("%s schema does not expose agent: %s", tool.Name, encoded)
		}
		if _, ok := schema.Properties["ide"]; ok {
			t.Errorf("%s schema still exposes removed ide field: %s", tool.Name, encoded)
		}
		want[tool.Name] = true
	}
	for tool, seen := range want {
		if !seen {
			t.Errorf("tool %s was not listed", tool)
		}
	}
}
