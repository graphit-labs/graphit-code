package ide

import (
	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

type CursorAdapter struct {
	*FolderBasedAdapter
}

func NewCursorAdapter() *CursorAdapter {
	base := NewFolderBasedAdapter(FolderConfig{
		RootDirName:  ".cursor",
		RulesDir:     "rules",
		CommandsDir:  "commands",
		SkillsDir:    "skills",
		AgentsDir:    "agents",
		HookFilePath: "{active_project_dir}/.cursor/hooks.json",
		MCPFilePath:  "{active_project_dir}/.cursor/mcp.json",
		FileTypes: map[string]FileMode{
			"rule":    {Mode: "file", Ext: "mdc"},
			"command": {Mode: "file", Ext: "md"},
			"agent":   {Mode: "file", Ext: "md"},
			"skill":   {Mode: "folder", Ext: ""},
		},
	})
	return &CursorAdapter{base}
}

func (a *CursorAdapter) Sync(installed map[string]map[string]string, pp *paths.ProjectPaths, projectID string) error {
	if err := a.FolderBasedAdapter.Sync(installed, pp, projectID); err != nil {
		return err
	}
	return a.syncSessionStartHook(pp.ActiveProjectDir)
}

func (a *CursorAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	if err := a.FolderBasedAdapter.Remove(pp, installed); err != nil {
		return err
	}
	return a.removeSessionStartHook(pp.ActiveProjectDir)
}

func (a *CursorAdapter) syncSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	return reconcileDirectCommandHook(
		path,
		"sessionStart",
		sessionhook.FormatAdditionalContext,
		"cursor",
	)
}

func (a *CursorAdapter) removeSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	return removeDirectCommandHook(
		path,
		"sessionStart",
		sessionhook.FormatAdditionalContext,
		"cursor",
	)
}
