package mcpstdio

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/sessioncontext"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
	graphtask "github.com/graphit-labs/graphit-code/internal/task"
)

// Instructions are an executable interface: routing to a misspelled or removed
// tool forces guesses and extra calls, even when every generator still compiles.
func TestCoreInstructionsReferenceRegisteredTools(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Chdir(t.TempDir())
	session := testMCPClient(t)
	listed, err := session.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	registered := make(map[string]bool, len(listed.Tools))
	for _, tool := range listed.Tools {
		registered[tool.Name] = true
	}
	instructions := map[string]string{
		"mandates":   sessioncontext.Mandates(),
		"bootstrap":  sessionhook.Protocol(),
		"checkpoint": sessionhook.UnitCompletionReminder(),
		"ast":        ast.ASTRuleContent(),
		"hub":        hub.HubRuleContent(),
		"knowledge":  knowledge.KnowledgeRuleContent(nil, "docs"),
		"memory":     memory.RuleContent(nil),
		"task":       graphtask.RuleContent(),
	}
	for module, references := range map[string]map[string]string{
		"ast": ast.SkillReferences(), "hub": hub.SkillReferences(),
		"knowledge": knowledge.SkillReferences(), "memory": memory.SkillReferences(),
		"task": graphtask.SkillReferences(),
	} {
		for path, content := range references {
			instructions[module+"/"+path] = content
		}
	}
	toolName := regexp.MustCompile("\\b" + regexp.QuoteMeta(brand.MCPToolName()) + "[a-z][a-z0-9_-]*\\b")
	for surface, content := range instructions {
		t.Run(surface, func(t *testing.T) {
			for _, name := range toolName.FindAllString(content, -1) {
				if !registered[name] {
					t.Errorf("instruction routes to unregistered tool %q", name)
				}
			}
		})
	}
}

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

func TestModuleSkillReferencesMatchInstalledResources(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Chdir(t.TempDir())
	session := testMCPClient(t)
	for module, references := range map[string]map[string]string{
		"ast": ast.SkillReferences(), "hub": hub.SkillReferences(),
		"knowledge": knowledge.SkillReferences(), "memory": memory.SkillReferences(),
		"task": graphtask.SkillReferences(),
	} {
		t.Run(module, func(t *testing.T) {
			read := func(reference string) moduleSkillResult {
				t.Helper()
				result, err := session.CallTool(context.Background(), &mcp.CallToolParams{
					Name: brand.MCPToolName("module", "skill"), Arguments: map[string]any{"module": module, "reference": reference},
				})
				if err != nil || result.IsError || len(result.Content) == 0 {
					t.Fatalf("reading %s: %v, %+v", reference, err, result)
				}
				var got moduleSkillResult
				if err := json.Unmarshal([]byte(result.Content[0].(*mcp.TextContent).Text), &got); err != nil {
					t.Fatal(err)
				}
				return got
			}
			main := read("")
			if len(main.References) != len(references) || len(references) == 0 {
				t.Fatalf("reference discovery incomplete: %v", main.References)
			}
			for _, path := range main.References {
				want, exists := references[path]
				if !exists {
					t.Fatalf("advertised unknown reference %s", path)
				}
				got := read(path)
				if got.Reference != path || got.Content != want || len(got.References) != 0 || got.Module != module {
					t.Fatalf("reference response differs from generator or loads other references: %s", path)
				}
				if strings.Contains(main.Content, want) {
					t.Fatalf("main skill eagerly includes complete reference %s", path)
				}
			}
			for _, path := range []string{"references/missing.md", "../../secret", "/etc/passwd"} {
				result, err := session.CallTool(context.Background(), &mcp.CallToolParams{
					Name: brand.MCPToolName("module", "skill"), Arguments: map[string]any{"module": module, "reference": path},
				})
				if err != nil || !result.IsError {
					t.Fatalf("invalid reference was not rejected: %s, %v", path, err)
				}
			}
		})
	}
}
