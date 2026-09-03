package ide

import (
	"os"
	"path/filepath"

	toml "github.com/pelletier/go-toml/v2"

	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

type CodexAdapter struct {
	*FolderBasedAdapter
}

func NewCodexAdapter() *CodexAdapter {
	base := NewFolderBasedAdapter(FolderConfig{
		RootDirName:   ".codex",
		RulesDir:      "rules",
		CommandsDir:   "commands",
		SkillsDir:     "skills",
		AgentsDir:     "agents",
		HookFilePath:  "{active_project_dir}/.codex/hooks.json",
		MCPFilePath:   "{active_project_dir}/.codex/config.toml",
		MCPCustomSync: true,
		FileTypes: map[string]FileMode{
			"rule":    {Mode: "file", Ext: "md"},
			"command": {Mode: "file", Ext: "md"},
			"agent":   {Mode: "file", Ext: "md"},
			"skill":   {Mode: "folder", Ext: ""},
		},
	})
	return &CodexAdapter{base}
}

func (a *CodexAdapter) Sync(installed map[string]map[string]string, pp *paths.ProjectPaths, projectID string) error {
	if err := a.FolderBasedAdapter.Sync(installed, pp, projectID); err != nil {
		return err
	}
	mcpTarget, _ := resolveConfiguredPath(a.cfg.MCPFilePath, pp.ActiveProjectDir)
	if err := a.syncCodexMCP(pp.ActiveProjectDir, mcpTarget, installed); err != nil {
		return err
	}
	return a.syncSessionStartHook(pp.ActiveProjectDir)
}

func (a *CodexAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	if err := a.FolderBasedAdapter.Remove(pp, installed); err != nil {
		return err
	}
	mcpTarget, _ := resolveConfiguredPath(a.cfg.MCPFilePath, pp.ActiveProjectDir)
	if err := a.removeCodexMCP(pp.ActiveProjectDir, mcpTarget, installed); err != nil {
		return err
	}
	return a.removeSessionStartHook(pp.ActiveProjectDir)
}

func (a *CodexAdapter) syncCodexMCP(projectDir, mcpTarget string, installed map[string]map[string]string) error {
	_ = os.MkdirAll(filepath.Dir(mcpTarget), 0o755)

	cfg := map[string]any{}
	if data, err := os.ReadFile(mcpTarget); err == nil {
		_ = toml.Unmarshal(data, &cfg)
	}

	servers, _ := cfg["mcp_servers"].(map[string]any)
	if servers == nil {
		servers = map[string]any{}
	}
	manifestPath := mcpManifestPath(projectDir, "codex")
	previous, err := mcpManifestNames(manifestPath)
	if err != nil {
		return err
	}
	for _, name := range previous {
		delete(servers, name)
	}
	desired := DesiredMCPServers(installed)
	for name, server := range desired {
		servers[name] = server
	}
	cfg["mcp_servers"] = servers

	out, err := toml.Marshal(cfg)
	if err != nil {
		return err
	}
	if err := os.WriteFile(mcpTarget, out, 0o644); err != nil {
		return err
	}
	return saveMCPManifest(manifestPath, desired)
}

func (a *CodexAdapter) removeCodexMCP(projectDir, mcpTarget string, installed map[string]map[string]string) error {
	data, err := os.ReadFile(mcpTarget)
	if err != nil {
		return nil
	}

	cfg := map[string]any{}
	if err := toml.Unmarshal(data, &cfg); err != nil {
		return nil
	}

	if servers, ok := cfg["mcp_servers"].(map[string]any); ok {
		manifestPath := mcpManifestPath(projectDir, "codex")
		names, readErr := mcpManifestNames(manifestPath)
		if readErr != nil {
			return readErr
		}
		if len(names) == 0 {
			for name := range DesiredMCPServers(installed) {
				names = append(names, name)
			}
		}
		for _, name := range names {
			delete(servers, name)
		}
		if len(servers) == 0 {
			delete(cfg, "mcp_servers")
		} else {
			cfg["mcp_servers"] = servers
		}
	}

	out, err := toml.Marshal(cfg)
	if err != nil {
		return err
	}
	if err := os.WriteFile(mcpTarget, out, 0o644); err != nil {
		return err
	}
	return saveMCPManifest(mcpManifestPath(projectDir, "codex"), map[string]any{})
}

func (a *CodexAdapter) syncSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	if err := reconcileGroupedCommandHook(
		path,
		"SessionStart",
		sessionhook.FormatSessionStart,
		projectDir,
		"codex",
	); err != nil {
		return err
	}
	if err := reconcileGroupedCommandHook(path, "SubagentStart", sessionhook.FormatSubagentStart, projectDir); err != nil {
		return err
	}
	return removeGroupedCommandHook(path, "PreToolUse", "guard-claude")
}

func (a *CodexAdapter) removeSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	if err := removeGroupedCommandHook(
		path,
		"SessionStart",
		sessionhook.FormatSessionStart,
		"codex",
	); err != nil {
		return err
	}
	if err := removeGroupedCommandHook(path, "SubagentStart", sessionhook.FormatSubagentStart); err != nil {
		return err
	}
	return removeGroupedCommandHook(path, "PreToolUse", "guard-claude")
}
