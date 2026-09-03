package ide

import (
	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

type ClaudeAdapter struct {
	*FolderBasedAdapter
}

func NewClaudeAdapter() *ClaudeAdapter {
	base := NewFolderBasedAdapter(FolderConfig{
		RootDirName:  ".claude",
		RulesDir:     "rules",
		CommandsDir:  "commands",
		SkillsDir:    "skills",
		AgentsDir:    "agents",
		HookFilePath: "{active_project_dir}/.claude/settings.json",
		MCPFilePath:  "{active_project_dir}/.mcp.json",
		FileTypes: map[string]FileMode{
			"rule":    {Mode: "file", Ext: "md"},
			"command": {Mode: "file", Ext: "md"},
			"agent":   {Mode: "file", Ext: "md"},
			"skill":   {Mode: "folder", Ext: ""},
		},
	})
	return &ClaudeAdapter{base}
}

func (a *ClaudeAdapter) Sync(installed map[string]map[string]string, pp *paths.ProjectPaths, projectID string) error {
	if err := a.FolderBasedAdapter.Sync(installed, pp, projectID); err != nil {
		return err
	}
	return a.syncSessionStartHook(pp.ActiveProjectDir)
}

func (a *ClaudeAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	if err := a.FolderBasedAdapter.Remove(pp, installed); err != nil {
		return err
	}
	return a.removeSessionStartHook(pp.ActiveProjectDir)
}

func (a *ClaudeAdapter) syncSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	if err := reconcileGroupedCommandHook(
		path,
		"SessionStart",
		sessionhook.FormatSessionStart,
		projectDir,
		"claude",
	); err != nil {
		return err
	}
	if err := reconcileGroupedCommandHook(path, "SubagentStart", sessionhook.FormatSubagentStart, projectDir); err != nil {
		return err
	}
	return removeGroupedCommandHook(path, "PreToolUse", "guard-claude")
}

func (a *ClaudeAdapter) removeSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	if err := removeGroupedCommandHook(
		path,
		"SessionStart",
		sessionhook.FormatSessionStart,
		"claude",
	); err != nil {
		return err
	}
	if err := removeGroupedCommandHook(path, "SubagentStart", sessionhook.FormatSubagentStart); err != nil {
		return err
	}
	return removeGroupedCommandHook(path, "PreToolUse", "guard-claude")
}
