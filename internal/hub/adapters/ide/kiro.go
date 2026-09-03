package ide

import (
	"fmt"

	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

const (
	kiroManagedHookName   = "graphit-memory-session-start"
	kiroResumeHookName    = "graphit-resume-routing"
	kiroUnitHookName      = "graphit-task-checkpoint"
	kiroTaskHookName      = "graphit-task-completed"
	kiroFinalSyncHookName = "graphit-final-sync"
	kiroSearchGuardName   = "graphit-native-search-guard"
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
	for _, name := range []string{
		kiroManagedHookName,
		kiroManagedHookName + "-cli",
		kiroResumeHookName,
		kiroUnitHookName,
		kiroTaskHookName,
		kiroFinalSyncHookName,
		kiroSearchGuardName,
	} {
		hooks = filterNamedHooks(hooks, name)
	}
	hooks = append(hooks, map[string]any{
		"name":        kiroManagedHookName,
		"description": "Initialize Graphit memory before the first response.",
		"enabled":     true,
		"trigger":     "SessionStart",
		"action": map[string]any{
			"type":    "command",
			"command": sessionHookCommand(sessionhook.FormatPlainContext),
		},
	})
	hooks = append(hooks, map[string]any{
		"name":        kiroResumeHookName,
		"description": "Reassert Graphit-first routing on every submitted or resumed turn.",
		"enabled":     true,
		"trigger":     "UserPromptSubmit",
		"action": map[string]any{
			"type":   "agent",
			"prompt": sessionhook.CoreInvariant(),
		},
	})
	hooks = append(hooks, map[string]any{
		"name":        kiroUnitHookName,
		"description": "Checkpoint task management after the smallest completed work unit.",
		"enabled":     true,
		"trigger":     "PostToolUse",
		"action": map[string]any{
			"type":    "command",
			"command": sessionHookCommand(sessionhook.FormatPlainUnit),
		},
	})
	hooks = append(hooks, map[string]any{
		"name":        kiroTaskHookName,
		"description": "Update task management as soon as a spec task completes.",
		"enabled":     true,
		"trigger":     "PostTaskExec",
		"action": map[string]any{
			"type":   "agent",
			"prompt": sessionhook.UnitCompletionReminder(),
		},
	})
	hooks = append(hooks, map[string]any{
		"name":        kiroFinalSyncHookName,
		"description": "Dispatch a complete Graphit sync asynchronously whenever the agent stops.",
		"enabled":     true,
		"trigger":     "Stop",
		"action": map[string]any{
			"type":    "command",
			"command": finalSyncHookCommand(sessionhook.FormatNoOutput),
		},
	})
	hooks = append(hooks, map[string]any{
		"name":        kiroManagedHookName + "-cli",
		"description": "Initialize Graphit memory when the CLI agent starts.",
		"enabled":     true,
		"trigger":     "AgentSpawn",
		"action": map[string]any{
			"type":    "command",
			"command": sessionHookCommand(sessionhook.FormatPlainContext),
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
	remaining := hooks
	for _, name := range []string{
		kiroManagedHookName,
		kiroManagedHookName + "-cli",
		kiroResumeHookName,
		kiroUnitHookName,
		kiroTaskHookName,
		kiroFinalSyncHookName,
		kiroSearchGuardName,
	} {
		remaining = filterNamedHooks(remaining, name)
	}
	if len(remaining) == 0 {
		delete(root, "hooks")
	} else {
		root["hooks"] = remaining
	}
	return writeOrRemoveJSONObject(path, root)
}
