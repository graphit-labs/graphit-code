package ide

import (
	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

type GeminiAdapter struct {
	*FolderBasedAdapter
}

func NewGeminiAdapter() *GeminiAdapter {
	base := NewFolderBasedAdapter(FolderConfig{
		RootDirName:  ".gemini",
		RulesDir:     "rules",
		CommandsDir:  "commands",
		SkillsDir:    "skills",
		AgentsDir:    "agents",
		HookFilePath: "{active_project_dir}/.gemini/settings.json",
		MCPFilePath:  "{active_project_dir}/.gemini/settings.json",
		FileTypes: map[string]FileMode{
			"rule":    {Mode: "file", Ext: "md"},
			"command": {Mode: "file", Ext: "md"},
			"agent":   {Mode: "file", Ext: "md"},
			"skill":   {Mode: "folder", Ext: ""},
		},
	})
	return &GeminiAdapter{base}
}

func (a *GeminiAdapter) Sync(installed map[string]map[string]string, pp *paths.ProjectPaths, projectID string) error {
	if err := a.FolderBasedAdapter.Sync(installed, pp, projectID); err != nil {
		return err
	}
	return a.syncSessionStartHook(pp.ActiveProjectDir)
}

func (a *GeminiAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	if err := a.FolderBasedAdapter.Remove(pp, installed); err != nil {
		return err
	}
	return a.removeSessionStartHook(pp.ActiveProjectDir)
}

func (a *GeminiAdapter) syncSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	if err := reconcileGroupedCommandHook(
		path,
		"SessionStart",
		sessionhook.FormatSessionStart,
		projectDir,
		"gemini",
	); err != nil {
		return err
	}
	if err := reconcileGroupedCommandHook(path, "BeforeAgent", sessionhook.FormatBeforeAgent, projectDir); err != nil {
		return err
	}
	return removeGroupedCommandHook(path, "BeforeTool", "guard-gemini")
}

func (a *GeminiAdapter) removeSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	if err := removeGroupedCommandHook(
		path,
		"SessionStart",
		sessionhook.FormatSessionStart,
		"gemini",
	); err != nil {
		return err
	}
	if err := removeGroupedCommandHook(path, "BeforeAgent", sessionhook.FormatBeforeAgent); err != nil {
		return err
	}
	return removeGroupedCommandHook(path, "BeforeTool", "guard-gemini")
}
