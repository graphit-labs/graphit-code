package ide

import (
	"fmt"

	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

const (
	antigravityManagedHookName = "graphit-memory-session-start"
	antigravitySearchGuardName = "graphit-native-search-guard"
)

type AntigravityAdapter struct {
	*FolderBasedAdapter
}

func NewAntigravityAdapter() *AntigravityAdapter {
	base := NewFolderBasedAdapter(FolderConfig{
		RootDirName:  ".agents",
		RulesDir:     "rules",
		CommandsDir:  "workflows",
		SkillsDir:    "skills",
		AgentsDir:    "agents",
		HookFilePath: "{active_project_dir}/.agents/hooks.json",
		MCPFilePath:  "{active_project_dir}/.agents/mcp_config.json",
		FileTypes: map[string]FileMode{
			"rule":    {Mode: "file", Ext: "md"},
			"command": {Mode: "file", Ext: "md"},
			"agent":   {Mode: "file", Ext: "md"},
			"skill":   {Mode: "folder", Ext: ""},
		},
	})
	return &AntigravityAdapter{base}
}

func (a *AntigravityAdapter) Sync(installed map[string]map[string]string, pp *paths.ProjectPaths, projectID string) error {
	if err := a.FolderBasedAdapter.Sync(installed, pp, projectID); err != nil {
		return err
	}
	return a.syncSessionStartHook(pp.ActiveProjectDir)
}

func (a *AntigravityAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	if err := a.FolderBasedAdapter.Remove(pp, installed); err != nil {
		return err
	}
	return a.removeSessionStartHook(pp.ActiveProjectDir)
}

func (a *AntigravityAdapter) syncSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	root, err := readJSONObject(path)
	if err != nil {
		return err
	}
	if existing, ok := root[antigravityManagedHookName]; ok && !containsManagedCommand(existing, sessionhook.FormatFirstInvocation, "antigravity") {
		return fmt.Errorf("reconciling %s: hook name %q is owned by the user", path, antigravityManagedHookName)
	}
	root[antigravityManagedHookName] = map[string]any{
		"PreInvocation": []any{map[string]any{
			"type":    "command",
			"command": sessionHookCommand(sessionhook.FormatFirstInvocation),
			"timeout": 10,
		}},
	}
	if existing, ok := root[antigravitySearchGuardName]; ok && containsManagedCommand(existing, "guard-antigravity") {
		delete(root, antigravitySearchGuardName)
	}
	return writeJSONObject(path, root)
}

func (a *AntigravityAdapter) removeSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	root, err := readJSONObjectIfExists(path)
	if err != nil || root == nil {
		return err
	}
	if existing, ok := root[antigravityManagedHookName]; ok && containsManagedCommand(existing, sessionhook.FormatFirstInvocation, "antigravity") {
		delete(root, antigravityManagedHookName)
	}
	if existing, ok := root[antigravitySearchGuardName]; ok && containsManagedCommand(existing, "guard-antigravity") {
		delete(root, antigravitySearchGuardName)
	}
	return writeOrRemoveJSONObject(path, root)
}
