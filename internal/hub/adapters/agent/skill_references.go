package agent

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// InstallManagedSkillWithReferences installs selectively loaded resources before
// publishing the skill that links to them. Unchanged resources retain their
// mtime so periodic synchronization does not trigger unnecessary file events.
func InstallManagedSkillWithReferences(projectDir, agentName, skillName, content string, references map[string]string) error {
	adapter := GetAdapter(agentName)
	if adapter == nil {
		return fmt.Errorf("unknown Agent: %s", agentName)
	}
	if err := validateSkillName(skillName); err != nil {
		return err
	}
	skillDir := GetSkillDir(adapter, projectDir, skillName)
	if skillDir == "" {
		return fmt.Errorf("unsupported adapter type for skill installation")
	}
	paths := make([]string, 0, len(references))
	for path := range references {
		// Reference keys are portable, canonical paths under references/. Never
		// allow a resource to replace SKILL.md or escape the managed skill.
		local := filepath.FromSlash(path)
		if !filepath.IsLocal(local) || filepath.ToSlash(filepath.Clean(local)) != path || strings.Contains(path, "\\") || !strings.HasPrefix(path, "references/") {
			return fmt.Errorf("invalid skill reference path %q", path)
		}
		paths = append(paths, path)
	}
	sort.Strings(paths)
	for _, path := range paths {
		file := filepath.Join(skillDir, filepath.FromSlash(path))
		if err := rejectReferenceSymlinks(skillDir, filepath.FromSlash(path)); err != nil {
			return err
		}
		data := []byte(references[path])
		if current, err := os.ReadFile(file); err == nil && bytes.Equal(current, data) {
			continue
		}
		if err := os.MkdirAll(filepath.Dir(file), 0o755); err != nil {
			return fmt.Errorf("creating skill reference directory: %w", err)
		}
		if err := os.WriteFile(file, data, 0o644); err != nil {
			return fmt.Errorf("writing skill reference %s: %w", path, err)
		}
	}
	return installSkillForAdapter(adapter, projectDir, skillName, content)
}

func rejectReferenceSymlinks(skillDir, relative string) error {
	path := skillDir
	for _, part := range append([]string{""}, strings.Split(relative, string(filepath.Separator))...) {
		path = filepath.Join(path, part)
		info, err := os.Lstat(path)
		if os.IsNotExist(err) {
			return nil
		}
		if err != nil {
			return fmt.Errorf("inspecting skill reference path: %w", err)
		}
		if info.Mode()&os.ModeSymlink != 0 {
			return fmt.Errorf("skill reference path is a symlink: %s", path)
		}
	}
	return nil
}
