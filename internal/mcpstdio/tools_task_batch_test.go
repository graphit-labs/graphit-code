package mcpstdio

import (
	"context"
	"encoding/json"
	"net/http"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/mcpproxy"
	graphtask "github.com/graphit-labs/graphit-code/internal/task"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

func TestTaskBatchToolSchema(t *testing.T) {
	session := testMCPClient(t)
	listed, err := session.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	name := brand.MCPToolName("task", "batch")
	for _, tool := range listed.Tools {
		if tool.Name != name {
			continue
		}
		encoded, err := json.Marshal(tool.InputSchema)
		if err != nil {
			t.Fatal(err)
		}
		var schema struct {
			Properties map[string]any `json:"properties"`
			Required   []string       `json:"required"`
		}
		if err := json.Unmarshal(encoded, &schema); err != nil {
			t.Fatal(err)
		}
		if _, ok := schema.Properties["operations"]; !ok {
			t.Fatal("task batch schema does not expose operations")
		}
		if len(schema.Required) == 0 {
			t.Fatal("task batch schema does not require its envelope fields")
		}
		return
	}
	t.Fatalf("tool %s was not listed", name)
}

func TestTaskRevisionToolSchemas(t *testing.T) {
	session := testMCPClient(t)
	listed, err := session.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	wanted := map[string][]string{
		brand.MCPToolName("task", "revise"):             {"expected_revision", "reason", "claim_token"},
		brand.MCPToolName("task", "check", "supersede"): {"expected_revision", "reason", "check_id", "claim_token"},
	}
	for _, tool := range listed.Tools {
		fields, ok := wanted[tool.Name]
		if !ok {
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
		for _, field := range fields {
			if _, ok := schema.Properties[field]; !ok {
				t.Errorf("tool %s does not expose %s", tool.Name, field)
			}
		}
		delete(wanted, tool.Name)
	}
	for name := range wanted {
		t.Errorf("tool %s was not listed", name)
	}
}

func TestTaskExportToolSchema(t *testing.T) {
	session := testMCPClient(t)
	listed, err := session.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	name := brand.MCPToolName("task", "export")
	for _, tool := range listed.Tools {
		if tool.Name != name {
			continue
		}
		encoded, err := json.Marshal(tool.InputSchema)
		if err != nil {
			t.Fatal(err)
		}
		var schema struct {
			Properties map[string]any `json:"properties"`
			Required   []string       `json:"required"`
		}
		if err := json.Unmarshal(encoded, &schema); err != nil {
			t.Fatal(err)
		}
		if _, ok := schema.Properties["project_dir"]; !ok {
			t.Fatal("task export schema does not expose project_dir")
		}
		if _, ok := schema.Properties["id"]; !ok {
			t.Fatal("task export schema does not expose optional id")
		}
		for _, field := range schema.Required {
			if field == "id" {
				t.Fatal("task export id must be optional for all-task export")
			}
		}
		return
	}
	t.Fatalf("tool %s was not listed", name)
}

func TestTaskActorPrefersStableProxySession(t *testing.T) {
	request := &mcp.CallToolRequest{Extra: &mcp.RequestExtra{Header: http.Header{mcpproxy.AgentSessionHeader: []string{"stable-session"}}}}
	if got, want := taskActor(request, ""), graphtask.AgentIDForSession("stable-session"); got != want {
		t.Fatalf("task actor = %q, want %q", got, want)
	}
	if got := taskActor(request, "explicit-agent"); got != "explicit-agent" {
		t.Fatalf("explicit task actor = %q", got)
	}
}
