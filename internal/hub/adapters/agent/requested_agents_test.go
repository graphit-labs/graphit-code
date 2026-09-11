package agent

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
)

func TestRequestedAgentAdaptersSyncUpdateRemoveCycle(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("GRAPHIT_GLOBAL_DIR", filepath.Join(home, ".graphit"))

	for _, name := range []string{"qwen", "kimi", "deepcode"} {
		t.Run(name, func(t *testing.T) {
			project := t.TempDir()
			sources := filepath.Join(project, "sources")
			installed := map[string]map[string]string{}
			for _, item := range []struct{ id, kind, canonical, content string }{
				{"project-rule", "rule", "RULE.md", "RULE TOKEN"},
				{"project-command", "command", "COMMAND.md", "COMMAND TOKEN"},
				{"project-skill", "skill", "SKILL.md", "SKILL TOKEN"},
				{"project-agent", "agent", "AGENT.md", "AGENT TOKEN"},
			} {
				dir := filepath.Join(sources, item.id)
				if err := os.MkdirAll(dir, 0o755); err != nil {
					t.Fatal(err)
				}
				if err := os.WriteFile(filepath.Join(dir, item.canonical), []byte(item.content), 0o644); err != nil {
					t.Fatal(err)
				}
				installed[item.id] = map[string]string{"type": item.kind, "path": dir}
			}
			mcpDir := filepath.Join(sources, "project-mcp")
			if err := os.MkdirAll(mcpDir, 0o755); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(filepath.Join(mcpDir, "mcp.json"), []byte(`{"hub-server":{"command":"hub-command"}}`), 0o644); err != nil {
				t.Fatal(err)
			}
			installed["project-mcp"] = map[string]string{"type": "mcp", "path": mcpDir}

			adapter := GetAdapter(name)
			base, _ := folderBase(adapter)
			mcpPath, err := resolveConfiguredPath(base.cfg.MCPFilePath, project)
			if err != nil {
				t.Fatal(err)
			}
			if err := os.MkdirAll(filepath.Dir(mcpPath), 0o755); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(mcpPath, []byte(`{"mcpServers":{"user-server":{"command":"user"}},"userField":"keep"}`), 0o644); err != nil {
				t.Fatal(err)
			}
			if name == "deepcode" {
				if err := os.WriteFile(filepath.Join(project, ".deepcode", "AGENTS.md"), []byte("USER AGENTS\n"), 0o644); err != nil {
					t.Fatal(err)
				}
			}

			pp := &paths.ProjectPaths{ActiveProjectDir: project}
			for i := 0; i < 2; i++ {
				if err := adapter.Sync(installed, pp, "cycle-project"); err != nil {
					t.Fatalf("sync %d: %v", i+1, err)
				}
			}
			mcp, _ := os.ReadFile(mcpPath)
			for _, token := range []string{"user-server", "userField", "hub-server", brand.MCPServerName("code-stdio")} {
				if !strings.Contains(string(mcp), token) {
					t.Fatalf("%s MCP missing %q: %s", name, token, mcp)
				}
			}
			switch name {
			case "qwen":
				assertFileContains(t, filepath.Join(project, ".qwen", "commands", "project-command.md"), "COMMAND TOKEN")
				assertFileContains(t, filepath.Join(project, ".qwen", "agents", "project-agent.md"), "AGENT TOKEN")
			case "kimi":
				if _, err := os.Stat(filepath.Join(project, ".kimi-code", "commands")); !os.IsNotExist(err) {
					t.Fatalf("Kimi commands must be explicitly unsupported: %v", err)
				}
				assertFileContains(t, filepath.Join(project, ".kimi-code", "agents", "project-agent.md"), "AGENT TOKEN")
			case "deepcode":
				if _, err := os.Stat(filepath.Join(project, ".deepcode", "commands")); !os.IsNotExist(err) {
					t.Fatalf("Deep Code commands must be explicitly unsupported: %v", err)
				}
				assertFileContains(t, filepath.Join(project, ".deepcode", "AGENTS.md"), "RULE TOKEN")
				assertFileContains(t, filepath.Join(project, ".deepcode", "AGENTS.md"), "AGENT TOKEN")
				assertFileContains(t, filepath.Join(project, ".deepcode", "AGENTS.md"), "USER AGENTS")
			}
			assertFileContains(t, filepath.Join(project, base.cfg.RootDirName, "skills", "project-skill", "SKILL.md"), "SKILL TOKEN")

			for i := 0; i < 2; i++ {
				if err := adapter.Remove(pp, installed); err != nil {
					t.Fatalf("remove %d: %v", i+1, err)
				}
			}
			mcp, _ = os.ReadFile(mcpPath)
			if !strings.Contains(string(mcp), "user-server") || !strings.Contains(string(mcp), "userField") || strings.Contains(string(mcp), "hub-server") || strings.Contains(string(mcp), brand.MCPServerName("code-stdio")) {
				t.Fatalf("%s selective MCP removal failed: %s", name, mcp)
			}
			if name == "deepcode" {
				assertFileContains(t, filepath.Join(project, ".deepcode", "AGENTS.md"), "USER AGENTS")
			}
		})
	}
}

func assertFileContains(t *testing.T, path, token string) {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil || !strings.Contains(string(data), token) {
		t.Fatalf("%s missing %q: %q, %v", path, token, data, err)
	}
}
