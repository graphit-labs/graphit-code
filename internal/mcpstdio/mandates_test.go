package mcpstdio

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/sessioncontext"
)

func TestMandatesToolDynamicallyResolvesGlobalConfigWithoutAProject(t *testing.T) {
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
	if err := config.SaveGlobalConfig(config.ConfigMap{
		"modules": map[string]any{"ast": "false"},
	}); err != nil {
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
	for _, required := range []string{"GRAPHIT_SYSTEM_MANDATE", "graphit-memory", "graphit-hub", "graphit-knowledge"} {
		if !strings.Contains(text.Text, required) {
			t.Errorf("mandates missing %q", required)
		}
	}
	for _, excluded := range []string{"GLOBAL AST MANDATE", "Graphit session bootstrap", "project memory:", "# Installed Hub rules"} {
		if strings.Contains(text.Text, excluded) {
			t.Errorf("non-mandate content %q leaked into mandates: %s", excluded, text.Text)
		}
	}

	if err := config.SaveGlobalConfig(config.ConfigMap{
		"modules": map[string]any{"ast": "true"},
	}); err != nil {
		t.Fatal(err)
	}
	refreshed, err := session.CallTool(context.Background(), &mcp.CallToolParams{
		Name:      brand.MCPToolName("mandates"),
		Arguments: map[string]any{},
	})
	if err != nil {
		t.Fatal(err)
	}
	if refreshed.IsError || len(refreshed.Content) != 1 {
		t.Fatalf("unexpected refreshed tool result: %+v", refreshed)
	}
	refreshedText, ok := refreshed.Content[0].(*mcp.TextContent)
	if !ok || !strings.Contains(refreshedText.Text, "GLOBAL AST MANDATE") {
		t.Fatalf("updated global config was not reflected dynamically: %+v", refreshed.Content)
	}
}

func TestModuleSkillToolReturnsEveryCoreSkillAndResolvedOverride(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	t.Chdir(t.TempDir())

	rulesDir := filepath.Join(globalDir, "rules")
	if err := os.MkdirAll(rulesDir, 0o755); err != nil {
		t.Fatal(err)
	}
	override := "CUSTOM AST SKILL\n\n{{_GRAPHIT_DEFAULT_SKILL_CONTENT_}}"
	if err := os.WriteFile(filepath.Join(rulesDir, "ast_skill.md"), []byte(override), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := config.SaveGlobalConfig(config.ConfigMap{
		"modules": map[string]any{"ast": "false"},
	}); err != nil {
		t.Fatal(err)
	}

	session := testMCPClient(t)
	for _, module := range []string{"task", "memory", "ast", "hub", "knowledge"} {
		result, err := session.CallTool(context.Background(), &mcp.CallToolParams{
			Name:      brand.MCPToolName("module", "skill"),
			Arguments: map[string]any{"module": module},
		})
		if err != nil {
			t.Fatalf("%s: %v", module, err)
		}
		if result.IsError || len(result.Content) == 0 {
			t.Fatalf("%s: unexpected result: %+v", module, result)
		}
		text, ok := result.Content[0].(*mcp.TextContent)
		if !ok {
			t.Fatalf("%s: result content is %T", module, result.Content[0])
		}
		var got moduleSkillResult
		if err := json.Unmarshal([]byte(text.Text), &got); err != nil {
			t.Fatalf("%s: decode: %v", module, err)
		}
		if got.Module != module || got.Name != brand.SkillDirName(module) || got.Content == "" {
			t.Errorf("%s: incomplete result: %+v", module, got)
		}
		if module == "ast" {
			if got.Enabled {
				t.Error("ast should reflect the disabled global module setting")
			}
			if !strings.Contains(got.Content, "CUSTOM AST SKILL") || !strings.Contains(got.Content, "# Graphit AST") {
				t.Errorf("ast override/default merge missing: %s", got.Content)
			}
		}
	}
}

func TestModuleSkillToolRejectsUnknownModule(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	session := testMCPClient(t)
	result, err := session.CallTool(context.Background(), &mcp.CallToolParams{
		Name:      brand.MCPToolName("module", "skill"),
		Arguments: map[string]any{"module": "daemon"},
	})
	if err != nil {
		t.Fatal(err)
	}
	if !result.IsError || len(result.Content) != 1 {
		t.Fatalf("unexpected result: %+v", result)
	}
	text, ok := result.Content[0].(*mcp.TextContent)
	if !ok || !strings.Contains(text.Text, "not a core skill") {
		t.Fatalf("unexpected error content: %+v", result.Content)
	}
}
