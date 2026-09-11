package agent

import (
	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

// QwenAdapter targets Qwen Code's project-scoped .qwen layout.
type QwenAdapter struct {
	*FolderBasedAdapter
}

func NewQwenAdapter() *QwenAdapter {
	return &QwenAdapter{NewFolderBasedAdapter(FolderConfig{
		RootDirName:  ".qwen",
		RulesDir:     "rules",
		CommandsDir:  "commands",
		SkillsDir:    "skills",
		AgentsDir:    "agents",
		HookFilePath: "{active_project_dir}/.qwen/settings.json",
		MCPFilePath:  "{active_project_dir}/.qwen/settings.json",
		FileTypes: map[string]FileMode{
			"rule": {Mode: "file", Ext: "md"}, "command": {Mode: "file", Ext: "md"},
			"agent": {Mode: "file", Ext: "md"}, "skill": {Mode: "folder"},
		},
	})}
}

func (a *QwenAdapter) Sync(installed map[string]map[string]string, pp *paths.ProjectPaths, projectID string) error {
	if err := a.FolderBasedAdapter.Sync(installed, pp, projectID); err != nil {
		return err
	}
	return a.syncSessionStartHook(pp.ActiveProjectDir)
}

func (a *QwenAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	if err := a.FolderBasedAdapter.Remove(pp, installed); err != nil {
		return err
	}
	return a.removeSessionStartHook(pp.ActiveProjectDir)
}

func (a *QwenAdapter) syncSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	steps := []struct {
		event, format string
		final         bool
	}{
		{"SessionStart", sessionhook.FormatSessionStart, false},
		{"SubagentStart", sessionhook.FormatSubagentStart, false},
		{"UserPromptSubmit", sessionhook.FormatUserPrompt, false},
		{"PostToolUse", sessionhook.FormatPostToolUse, false},
		{"SubagentStop", sessionhook.FormatStop, true},
		{"Stop", sessionhook.FormatStop, true},
		{"SessionEnd", sessionhook.FormatSessionEnd, true},
	}
	for _, step := range steps {
		if step.final {
			if err := reconcileGroupedFinalSyncHook(path, step.event, step.format); err != nil {
				return err
			}
		} else if err := reconcileGroupedCommandHook(path, step.event, step.format, "qwen"); err != nil {
			return err
		}
	}
	return nil
}

func (a *QwenAdapter) removeSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	for _, step := range []struct{ event, format string }{
		{"SessionStart", sessionhook.FormatSessionStart}, {"SubagentStart", sessionhook.FormatSubagentStart},
		{"UserPromptSubmit", sessionhook.FormatUserPrompt}, {"PostToolUse", sessionhook.FormatPostToolUse},
		{"SubagentStop", sessionhook.FormatStop}, {"Stop", sessionhook.FormatStop},
		{"SessionEnd", sessionhook.FormatSessionEnd},
	} {
		if err := removeGroupedCommandHook(path, step.event, step.format, "qwen"); err != nil {
			return err
		}
	}
	return nil
}
