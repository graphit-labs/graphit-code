package agent

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"

	toml "github.com/pelletier/go-toml/v2"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

// KimiAdapter targets Kimi Code. Project artifacts live in .kimi-code, while
// Kimi's hook table is global. A project registry keeps that shared hook alive
// until the last Graphit-enabled project removes it.
type KimiAdapter struct {
	*FolderBasedAdapter
}

func NewKimiAdapter() *KimiAdapter {
	return &KimiAdapter{NewFolderBasedAdapter(FolderConfig{
		RootDirName:      ".kimi-code",
		SkillsDir:        "skills",
		AgentsDir:        "agents",
		HookFilePath:     "~/.kimi-code/config.toml",
		MCPFilePath:      "{active_project_dir}/.kimi-code/mcp.json",
		UnsupportedTypes: map[string]bool{"command": true},
		FileTypes: map[string]FileMode{
			"rule": {Mode: "file", Ext: "md"}, "agent": {Mode: "file", Ext: "md"},
			"skill": {Mode: "folder"},
		},
	})}
}

func (a *KimiAdapter) Sync(installed map[string]map[string]string, pp *paths.ProjectPaths, projectID string) error {
	if err := a.FolderBasedAdapter.Sync(installed, pp, projectID); err != nil {
		return err
	}
	return a.syncSessionStartHook(pp.ActiveProjectDir)
}

func (a *KimiAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	if err := a.FolderBasedAdapter.Remove(pp, installed); err != nil {
		return err
	}
	return a.removeSessionStartHook(pp.ActiveProjectDir)
}

func (a *KimiAdapter) syncSessionStartHook(projectDir string) error {
	refs, err := loadKimiHookProjects()
	if err != nil {
		return err
	}
	canonical := canonicalProjectPath(projectDir)
	_, existed := refs[canonical]
	refs[canonical] = true
	if err := saveKimiHookProjects(refs); err != nil {
		return err
	}
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err == nil {
		err = reconcileKimiHooks(path)
	}
	if err != nil && !existed {
		delete(refs, canonical)
		_ = saveKimiHookProjects(refs)
	}
	return err
}

func (a *KimiAdapter) removeSessionStartHook(projectDir string) error {
	refs, err := loadKimiHookProjects()
	if err != nil {
		return err
	}
	delete(refs, canonicalProjectPath(projectDir))
	if err := saveKimiHookProjects(refs); err != nil {
		return err
	}
	if len(refs) > 0 {
		return nil
	}
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	return removeKimiHooks(path)
}

func kimiHookRegistryPath() string {
	return filepath.Join(brand.GlobalDir(), "runtime", "adapters", "kimi-projects.json")
}

func canonicalProjectPath(path string) string {
	abs, err := filepath.Abs(path)
	if err != nil {
		return filepath.Clean(path)
	}
	if resolved, err := filepath.EvalSymlinks(abs); err == nil {
		return resolved
	}
	return filepath.Clean(abs)
}

func loadKimiHookProjects() (map[string]bool, error) {
	refs := map[string]bool{}
	data, err := os.ReadFile(kimiHookRegistryPath())
	if os.IsNotExist(err) {
		return refs, nil
	}
	if err != nil {
		return nil, err
	}
	var projects []string
	if err := json.Unmarshal(data, &projects); err != nil {
		return nil, fmt.Errorf("parsing Kimi hook registry: %w", err)
	}
	for _, project := range projects {
		refs[project] = true
	}
	return refs, nil
}

func saveKimiHookProjects(refs map[string]bool) error {
	path := kimiHookRegistryPath()
	if len(refs) == 0 {
		if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
			return err
		}
		return nil
	}
	projects := make([]string, 0, len(refs))
	for project := range refs {
		projects = append(projects, project)
	}
	sort.Strings(projects)
	data, err := json.Marshal(projects)
	if err != nil {
		return err
	}
	return writeFileAtomically(path, append(data, '\n'), 0o644)
}

func reconcileKimiHooks(path string) error {
	root, err := readTOMLObject(path)
	if err != nil {
		return err
	}
	hooks, err := kimiHookEntries(root["hooks"])
	if err != nil {
		return fmt.Errorf("reconciling %s: %w", path, err)
	}
	formats := map[string]string{
		"SessionStart":     sessionhook.FormatSessionStart,
		"SubagentStart":    sessionhook.FormatSubagentStart,
		"UserPromptSubmit": sessionhook.FormatPlainContext,
		"PostToolUse":      sessionhook.FormatPostToolUse,
		"SubagentStop":     sessionhook.FormatStop,
		"Stop":             sessionhook.FormatStop,
		"SessionEnd":       sessionhook.FormatSessionEnd,
	}
	for event, format := range formats {
		hooks = filterKimiHook(hooks, event, format)
		command := sessionHookCommand(format)
		if event == "SubagentStop" || event == "Stop" || event == "SessionEnd" {
			command = finalSyncHookCommand(format)
		}
		hooks = append(hooks, map[string]any{"event": event, "command": command, "timeout": int64(30)})
	}
	sort.SliceStable(hooks, func(i, j int) bool { return fmt.Sprint(hooks[i]["event"]) < fmt.Sprint(hooks[j]["event"]) })
	root["hooks"] = hooks
	return writeTOMLObject(path, root)
}

func removeKimiHooks(path string) error {
	root, err := readTOMLObjectIfExists(path)
	if err != nil || root == nil {
		return err
	}
	hooks, err := kimiHookEntries(root["hooks"])
	if err != nil {
		return err
	}
	for event, format := range map[string]string{
		"SessionStart": sessionhook.FormatSessionStart, "SubagentStart": sessionhook.FormatSubagentStart,
		"UserPromptSubmit": sessionhook.FormatPlainContext, "PostToolUse": sessionhook.FormatPostToolUse,
		"SubagentStop": sessionhook.FormatStop, "Stop": sessionhook.FormatStop,
		"SessionEnd": sessionhook.FormatSessionEnd,
	} {
		hooks = filterKimiHook(hooks, event, format)
	}
	if len(hooks) == 0 {
		delete(root, "hooks")
	} else {
		root["hooks"] = hooks
	}
	if len(root) == 0 {
		if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
			return err
		}
		return nil
	}
	return writeTOMLObject(path, root)
}

func filterKimiHook(hooks []map[string]any, event, format string) []map[string]any {
	remaining := hooks[:0]
	for _, hook := range hooks {
		if fmt.Sprint(hook["event"]) == event && isManagedSessionCommand(hook["command"], format) {
			continue
		}
		remaining = append(remaining, hook)
	}
	return remaining
}

func kimiHookEntries(value any) ([]map[string]any, error) {
	if value == nil {
		return []map[string]any{}, nil
	}
	switch entries := value.(type) {
	case []map[string]any:
		return entries, nil
	case []any:
		out := make([]map[string]any, 0, len(entries))
		for _, value := range entries {
			entry, ok := value.(map[string]any)
			if !ok {
				return nil, fmt.Errorf("hooks must be an array of TOML tables")
			}
			out = append(out, entry)
		}
		return out, nil
	default:
		return nil, fmt.Errorf("hooks must be an array of TOML tables")
	}
}

func readTOMLObject(path string) (map[string]any, error) {
	root, err := readTOMLObjectIfExists(path)
	if root == nil && err == nil {
		root = map[string]any{}
	}
	return root, err
}

func readTOMLObjectIfExists(path string) (map[string]any, error) {
	data, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	root := map[string]any{}
	if err := toml.Unmarshal(data, &root); err != nil {
		return nil, fmt.Errorf("parsing Kimi config %s: %w", path, err)
	}
	return root, nil
}

func writeTOMLObject(path string, root map[string]any) error {
	data, err := toml.Marshal(root)
	if err != nil {
		return err
	}
	return writeFileAtomically(path, data, 0o644)
}
