package ide

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

func sessionHookCommand(format, projectDir string) string {
	return strconv.Quote(getGraphitExecutable()) + " _session-hook --format " + format +
		" --project-dir " + strconv.Quote(projectDir)
}

func isManagedSessionCommand(value any, format string, legacyAdapters ...string) bool {
	command, ok := value.(string)
	if !ok {
		return false
	}
	if strings.Contains(command, "_session-hook --format "+format) {
		return true
	}
	for _, adapter := range legacyAdapters {
		if strings.Contains(command, "_session-hook --adapter "+adapter) {
			return true
		}
	}
	return false
}

func reconcileDirectCommandHook(path, event, format, projectDir string, legacyAdapters ...string) error {
	return reconcileDirectCommandHookMatched(path, event, "", format, projectDir, legacyAdapters...)
}

func reconcileDirectCommandHookMatched(path, event, matcher, format, projectDir string, legacyAdapters ...string) error {
	root, err := readJSONObject(path)
	if err != nil {
		return err
	}
	hooks, err := childObject(root, "hooks")
	if err != nil {
		return fmt.Errorf("reconciling %s: %w", path, err)
	}
	entries, err := childArray(hooks, event)
	if err != nil {
		return fmt.Errorf("reconciling %s: %w", path, err)
	}
	entries = filterDirectCommandHooks(entries, format, legacyAdapters...)
	entry := map[string]any{"command": sessionHookCommand(format, projectDir)}
	if matcher != "" {
		entry["matcher"] = matcher
	}
	entries = append(entries, entry)
	hooks[event] = entries
	root["version"] = 1
	root["hooks"] = hooks
	return writeJSONObject(path, root)
}

func removeDirectCommandHook(path, event, format string, legacyAdapters ...string) error {
	root, err := readJSONObjectIfExists(path)
	if err != nil || root == nil {
		return err
	}
	hooks, ok := root["hooks"].(map[string]any)
	if !ok {
		return nil
	}
	entries, ok := hooks[event].([]any)
	if !ok {
		return nil
	}
	remaining := filterDirectCommandHooks(entries, format, legacyAdapters...)
	if len(remaining) == 0 {
		delete(hooks, event)
	} else {
		hooks[event] = remaining
	}
	if len(hooks) == 0 {
		delete(root, "hooks")
	} else {
		root["hooks"] = hooks
	}
	return writeOrRemoveJSONObject(path, root)
}

func filterDirectCommandHooks(entries []any, format string, legacyAdapters ...string) []any {
	remaining := make([]any, 0, len(entries))
	for _, entry := range entries {
		item, ok := entry.(map[string]any)
		if ok && isManagedSessionCommand(item["command"], format, legacyAdapters...) {
			continue
		}
		remaining = append(remaining, entry)
	}
	return remaining
}

func reconcileGroupedCommandHook(path, event, format, projectDir string, legacyAdapters ...string) error {
	return reconcileGroupedCommandHookMatched(path, event, "", format, projectDir, legacyAdapters...)
}

func reconcileGroupedCommandHookMatched(path, event, matcher, format, projectDir string, legacyAdapters ...string) error {
	root, err := readJSONObject(path)
	if err != nil {
		return err
	}
	hooks, err := childObject(root, "hooks")
	if err != nil {
		return fmt.Errorf("reconciling %s: %w", path, err)
	}
	groups, err := childArray(hooks, event)
	if err != nil {
		return fmt.Errorf("reconciling %s: %w", path, err)
	}
	groups = filterGroupedCommandHooks(groups, format, legacyAdapters...)
	group := map[string]any{
		"hooks": []any{map[string]any{
			"type":    "command",
			"command": sessionHookCommand(format, projectDir),
		}},
	}
	if matcher != "" {
		group["matcher"] = matcher
	}
	groups = append(groups, group)
	hooks[event] = groups
	root["hooks"] = hooks
	return writeJSONObject(path, root)
}

func removeGroupedCommandHook(path, event, format string, legacyAdapters ...string) error {
	root, err := readJSONObjectIfExists(path)
	if err != nil || root == nil {
		return err
	}
	hooks, ok := root["hooks"].(map[string]any)
	if !ok {
		return nil
	}
	groups, ok := hooks[event].([]any)
	if !ok {
		return nil
	}
	remaining := filterGroupedCommandHooks(groups, format, legacyAdapters...)
	if len(remaining) == 0 {
		delete(hooks, event)
	} else {
		hooks[event] = remaining
	}
	if len(hooks) == 0 {
		delete(root, "hooks")
	} else {
		root["hooks"] = hooks
	}
	return writeOrRemoveJSONObject(path, root)
}

func filterGroupedCommandHooks(groups []any, format string, legacyAdapters ...string) []any {
	remainingGroups := make([]any, 0, len(groups))
	for _, groupValue := range groups {
		group, ok := groupValue.(map[string]any)
		if !ok {
			remainingGroups = append(remainingGroups, groupValue)
			continue
		}
		handlers, ok := group["hooks"].([]any)
		if !ok {
			remainingGroups = append(remainingGroups, groupValue)
			continue
		}
		remainingHandlers := make([]any, 0, len(handlers))
		for _, handlerValue := range handlers {
			handler, ok := handlerValue.(map[string]any)
			if ok && isManagedSessionCommand(handler["command"], format, legacyAdapters...) {
				continue
			}
			remainingHandlers = append(remainingHandlers, handlerValue)
		}
		if len(remainingHandlers) == 0 {
			continue
		}
		group["hooks"] = remainingHandlers
		remainingGroups = append(remainingGroups, group)
	}
	return remainingGroups
}

func filterNamedHooks(hooks []any, name string) []any {
	remaining := make([]any, 0, len(hooks))
	for _, hookValue := range hooks {
		hook, ok := hookValue.(map[string]any)
		if ok && hook["name"] == name {
			continue
		}
		remaining = append(remaining, hookValue)
	}
	return remaining
}

func containsManagedCommand(value any, format string, legacyAdapters ...string) bool {
	switch current := value.(type) {
	case map[string]any:
		for key, child := range current {
			if key == "command" && isManagedSessionCommand(child, format, legacyAdapters...) {
				return true
			}
			if containsManagedCommand(child, format, legacyAdapters...) {
				return true
			}
		}
	case []any:
		for _, child := range current {
			if containsManagedCommand(child, format, legacyAdapters...) {
				return true
			}
		}
	}
	return false
}

func readJSONObject(path string) (map[string]any, error) {
	root, err := readJSONObjectIfExists(path)
	if root == nil && err == nil {
		root = map[string]any{}
	}
	return root, err
}

func readJSONObjectIfExists(path string) (map[string]any, error) {
	data, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	var root map[string]any
	if err := json.Unmarshal(data, &root); err != nil {
		return nil, fmt.Errorf("parsing hook configuration %s: %w", path, err)
	}
	if root == nil {
		return nil, fmt.Errorf("parsing hook configuration %s: root must be a JSON object", path)
	}
	return root, nil
}

func childObject(parent map[string]any, key string) (map[string]any, error) {
	value, ok := parent[key]
	if !ok {
		return map[string]any{}, nil
	}
	child, ok := value.(map[string]any)
	if !ok {
		return nil, fmt.Errorf("%q must be a JSON object", key)
	}
	return child, nil
}

func childArray(parent map[string]any, key string) ([]any, error) {
	value, ok := parent[key]
	if !ok {
		return []any{}, nil
	}
	child, ok := value.([]any)
	if !ok {
		return nil, fmt.Errorf("%q must be a JSON array", key)
	}
	return child, nil
}

func writeOrRemoveJSONObject(path string, root map[string]any) error {
	if len(root) == 0 {
		if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
			return err
		}
		return nil
	}
	return writeJSONObject(path, root)
}

func writeJSONObject(path string, root map[string]any) error {
	data, err := json.MarshalIndent(root, "", "  ")
	if err != nil {
		return err
	}
	data = append(data, '\n')
	return writeFileAtomically(path, data, 0o644)
}

func writeFileAtomically(path string, data []byte, mode os.FileMode) error {
	if current, err := os.ReadFile(path); err == nil && bytes.Equal(current, data) {
		return nil
	}
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	tmp, err := os.CreateTemp(dir, ".graphit-session-hook-*")
	if err != nil {
		return err
	}
	tmpPath := tmp.Name()
	defer func() { _ = os.Remove(tmpPath) }()
	if err := tmp.Chmod(mode); err != nil {
		_ = tmp.Close()
		return err
	}
	if _, err := tmp.Write(data); err != nil {
		_ = tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	if err := os.Rename(tmpPath, path); err != nil {
		return err
	}
	return nil
}
