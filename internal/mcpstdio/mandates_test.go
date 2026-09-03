package mcpstdio

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/sessioncontext"
)

func TestMandatesToolReturnsProjectIndependentFrameworkMandates(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	projectlessDir := t.TempDir()
	t.Chdir(projectlessDir)

	rulesDir := filepath.Join(globalDir, "rules")
	if err := os.MkdirAll(rulesDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(rulesDir, "ast.md"), []byte("GLOBAL AST MANDATE"), 0o644); err != nil {
		t.Fatal(err)
	}

	session := testMCPClient(t)
	result, err := session.CallTool(context.Background(), &mcp.CallToolParams{
		Name:      brand.MCPToolName("mandates"),
		Arguments: map[string]any{},
	})
	if err != nil {
		t.Fatal(err)
	}
	if result.IsError || len(result.Content) != 1 {
		t.Fatalf("unexpected tool result: %+v", result)
	}
	text, ok := result.Content[0].(*mcp.TextContent)
	if !ok {
		t.Fatalf("result content is %T", result.Content[0])
	}

	want := sessioncontext.Mandates()
	if text.Text != want {
		t.Fatalf("MCP content differs from framework mandates\n--- got ---\n%s\n--- want ---\n%s", text.Text, want)
	}
	for _, required := range []string{"GRAPHIT_SYSTEM_MANDATE", "GLOBAL AST MANDATE", "graphit-memory", "graphit-hub", "graphit-knowledge"} {
		if !strings.Contains(text.Text, required) {
			t.Errorf("mandates missing %q", required)
		}
	}
	for _, excluded := range []string{"Graphit session bootstrap", "project memory:", "# Installed Hub rules"} {
		if strings.Contains(text.Text, excluded) {
			t.Errorf("non-mandate content %q leaked into mandates: %s", excluded, text.Text)
		}
	}
}
