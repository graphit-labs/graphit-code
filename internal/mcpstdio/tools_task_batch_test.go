package mcpstdio

import (
	"context"
	"encoding/json"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
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
