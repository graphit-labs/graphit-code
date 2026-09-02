package ide

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	gitblk "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

var geminiBlockMarker = brand.ManagedBlockMarker()

func injectGeminiManagedBlock(filePath, content string) error {
	return gitblk.InjectBlockStyled(filePath, content, geminiBlockMarker, "", gitblk.HTMLBlockStyle)
}

func removeGeminiManagedBlock(filePath string) error {
	_, err := gitblk.RemoveBlockStyled(filePath, geminiBlockMarker, true, gitblk.HTMLBlockStyle)
	return err
}

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
	if err := a.syncGeminiMD(pp.ActiveProjectDir); err != nil {
		return err
	}
	return a.syncSessionStartHook(pp.ActiveProjectDir)
}

func (a *GeminiAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	if err := a.FolderBasedAdapter.Remove(pp, installed); err != nil {
		return err
	}
	if err := removeGeminiManagedBlock(filepath.Join(pp.ActiveProjectDir, "AGENTS.md")); err != nil {
		return err
	}
	return a.removeSessionStartHook(pp.ActiveProjectDir)
}

func (a *GeminiAdapter) syncGeminiMD(projectDir string) error {
	rulesDir := filepath.Join(projectDir, ".gemini", "rules")
	entries, err := os.ReadDir(rulesDir)
	if err != nil {
		return nil
	}

	var imports []string
	for _, e := range entries {
		if !e.IsDir() && strings.HasSuffix(e.Name(), ".md") {
			imports = append(imports, fmt.Sprintf("@.gemini/rules/%s", e.Name()))
		}
	}
	sort.Strings(imports)

	target := filepath.Join(projectDir, "AGENTS.md")
	if len(imports) > 0 {
		return injectGeminiManagedBlock(target, strings.Join(imports, "\n"))
	}
	return removeGeminiManagedBlock(target)
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
		"gemini",
	); err != nil {
		return err
	}
	if err := reconcileGroupedCommandHook(path, "BeforeAgent", sessionhook.FormatBeforeAgent); err != nil {
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
