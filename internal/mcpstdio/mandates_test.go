package mcpstdio

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/sessioncontext"
)

func TestMandatesToolReturnsOnlyTheHookMandatesFromServerProject(t *testing.T) {
	projectDir := t.TempDir()
	nestedDir := filepath.Join(projectDir, "packages", "api")
	if err := os.MkdirAll(nestedDir, 0o755); err != nil {
		t.Fatal(err)
	}
	ruleDir := filepath.Join(t.TempDir(), "rule")
	if err := os.MkdirAll(ruleDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ruleDir, "RULE.md"), []byte("REMOTE SESSION RULE"), 0o644); err != nil {
		t.Fatal(err)
	}
	lf := &hub.Lockfile{
		Artifacts: map[hub.ArtifactType]map[string]*hub.LockfileArtifactMeta{
			hub.TypeRule: {"remote-session": {LinkSource: ruleDir}},
		},
		Config: map[string]any{"modules": map[string]any{"ast": "false"}},
	}
	if err := hub.SaveLockfile(filepath.Join(projectDir, brand.LockFileName()), lf); err != nil {
		t.Fatal(err)
	}
	t.Chdir(nestedDir)

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

	want := sessioncontext.Mandates(projectDir)
	if text.Text != want {
		t.Fatalf("MCP content differs from hook mandates\n--- got ---\n%s\n--- want ---\n%s", text.Text, want)
	}
	for _, required := range []string{"GRAPHIT_SYSTEM_MANDATE", "graphit-memory"} {
		if !strings.Contains(text.Text, required) {
			t.Errorf("mandates missing %q", required)
		}
	}
	if strings.Contains(text.Text, "graphit-ast`") {
		t.Errorf("disabled AST mandate leaked into mandates: %s", text.Text)
	}
	for _, excluded := range []string{"REMOTE SESSION RULE", "Graphit session bootstrap", "project memory:"} {
		if strings.Contains(text.Text, excluded) {
			t.Errorf("non-mandate content %q leaked into mandates: %s", excluded, text.Text)
		}
	}

	lf.Config = map[string]any{}
	if err := hub.SaveLockfile(filepath.Join(projectDir, brand.LockFileName()), lf); err != nil {
		t.Fatal(err)
	}
	refreshed, err := session.CallTool(context.Background(), &mcp.CallToolParams{
		Name:      brand.MCPToolName("mandates"),
		Arguments: map[string]any{},
	})
	if err != nil {
		t.Fatal(err)
	}
	refreshedText, ok := refreshed.Content[0].(*mcp.TextContent)
	if !ok || !strings.Contains(refreshedText.Text, "graphit-ast`") {
		t.Fatalf("mandates were not rebuilt from the current lockfile: %+v", refreshed.Content)
	}
}
