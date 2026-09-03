package ide

import (
	"fmt"

	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

const (
	kiroManagedHookName = "graphit-memory-session-start"
	kiroSearchGuardName = "graphit-native-search-guard"
)

type KiroAdapter struct {
	*FolderBasedAdapter
}

func NewKiroAdapter() *KiroAdapter {
	base := NewFolderBasedAdapter(FolderConfig{
		RootDirName:  ".kiro",
		RulesDir:     "steering",
		CommandsDir:  "hooks",
		SkillsDir:    "skills",
		AgentsDir:    "agents",
		HookFilePath: "{active_project_dir}/.kiro/hooks/graphit-memory.json",
		MCPFilePath:  "{active_project_dir}/.kiro/settings/mcp.json",
		FileTypes: map[string]FileMode{
			"rule":    {Mode: "file", Ext: "md"},
			"command": {Mode: "file", Ext: "md"},
			"agent":   {Mode: "file", Ext: "md"},
			"skill":   {Mode: "folder", Ext: ""},
		},
	})
	return &KiroAdapter{base}
}

func (a *KiroAdapter) Sync(installed map[string]map[string]string, pp *paths.ProjectPaths, projectID string) error {
	if err := a.FolderBasedAdapter.Sync(installed, pp, projectID); err != nil {
		return err
	}
	return a.syncSessionStartHook(pp.ActiveProjectDir)
}

func (a *KiroAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	if err := a.FolderBasedAdapter.Remove(pp, installed); err != nil {
		return err
	}
	return a.removeSessionStartHook(pp.ActiveProjectDir)
}

func (a *KiroAdapter) syncSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	root, err := readJSONObject(path)
	if err != nil {
		return err
	}
	hooks, err := childArray(root, "hooks")
	if err != nil {
		return fmt.Errorf("reconciling %s: %w", path, err)
	}
	hooks = filterNamedHooks(filterNamedHooks(filterNamedHooks(hooks, kiroManagedHookName), kiroManagedHookName+"-cli"), kiroSearchGuardName)
	hooks = append(hooks, map[string]any{
		"name":        kiroManagedHookName,
		"description": "Initialize Graphit memory before the first response.",
		"enabled":     true,
		"trigger":     "SessionStart",
		"action": map[string]any{
			"type":    "command",
			"command": sessionHookCommand(sessionhook.FormatPlainContext, projectDir),
		},
	})
	hooks = append(hooks, map[string]any{
		"name":        kiroManagedHookName + "-cli",
		"description": "Initialize Graphit memory when the CLI agent starts.",
		"enabled":     true,
		"trigger":     "AgentSpawn",
		"action": map[string]any{
			"type":    "command",
			"command": sessionHookCommand(sessionhook.FormatPlainContext, projectDir),
		},
	})
	root["version"] = "v1"
	root["hooks"] = hooks
	return writeJSONObject(path, root)
}

func (a *KiroAdapter) removeSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	root, err := readJSONObjectIfExists(path)
	if err != nil || root == nil {
		return err
	}
	hooks, ok := root["hooks"].([]any)
	if !ok {
		return nil
	}
	remaining := filterNamedHooks(filterNamedHooks(filterNamedHooks(hooks, kiroManagedHookName), kiroManagedHookName+"-cli"), kiroSearchGuardName)
	if len(remaining) == 0 {
		delete(root, "hooks")
	} else {
		root["hooks"] = remaining
	}
	return writeOrRemoveJSONObject(path, root)
}
