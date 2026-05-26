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
)

func NewAntigravityAdapter() *FolderBasedAdapter {
	return NewFolderBasedAdapter(FolderConfig{
		RootDirName: ".agents",
		RulesDir:    "rules",
		CommandsDir: "workflows",
		SkillsDir:   "skills",
		AgentsDir:   "agents",
		MCPFilePath: "~/.gemini/antigravity/mcp_config.json",
		FileTypes: map[string]FileMode{
			"rule":    {Mode: "file", Ext: "md"},
			"command": {Mode: "file", Ext: "md"},
			"agent":   {Mode: "file", Ext: "md"},
			"skill":   {Mode: "folder", Ext: ""},
		},
	})
}

func NewCursorAdapter() *FolderBasedAdapter {
	return NewFolderBasedAdapter(FolderConfig{
		RootDirName: ".cursor",
		RulesDir:    "rules",
		CommandsDir: "commands",
		SkillsDir:   "skills",
		AgentsDir:   "agents",
		MCPFilePath: "~/.cursor/mcp.json",
		FileTypes: map[string]FileMode{
			"rule":    {Mode: "file", Ext: "mdc"},
			"command": {Mode: "file", Ext: "md"},
			"agent":   {Mode: "file", Ext: "md"},
			"skill":   {Mode: "folder", Ext: ""},
		},
	})
}

var claudeBlockMarker = brand.ManagedBlockMarker()

func injectClaudeManagedBlock(filePath, content string) error {
	return gitblk.InjectBlockStyled(filePath, content, claudeBlockMarker, "", gitblk.HTMLBlockStyle)
}

func removeClaudeManagedBlock(filePath string) error {
	_, err := gitblk.RemoveBlockStyled(filePath, claudeBlockMarker, true, gitblk.HTMLBlockStyle)
	return err
}

type ClaudeAdapter struct {
	*FolderBasedAdapter
}

func NewClaudeAdapter() *ClaudeAdapter {
	base := NewFolderBasedAdapter(FolderConfig{
		RootDirName: ".claude",
		RulesDir:    "rules",
		CommandsDir: "commands",
		SkillsDir:   "skills",
		AgentsDir:   "agents",
		MCPFilePath: "~/.claude.json",
		FileTypes: map[string]FileMode{
			"rule":    {Mode: "file", Ext: "md"},
			"command": {Mode: "file", Ext: "md"},
			"agent":   {Mode: "file", Ext: "md"},
			"skill":   {Mode: "folder", Ext: ""},
		},
	})
	return &ClaudeAdapter{base}
}

func (a *ClaudeAdapter) Sync(installed map[string]map[string]string, pp *paths.ProjectPaths, projectID string) error {
	if err := a.FolderBasedAdapter.Sync(installed, pp, projectID); err != nil {
		return err
	}
	return a.syncClaudeMD(pp.ActiveProjectDir)
}

func (a *ClaudeAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	if err := a.FolderBasedAdapter.Remove(pp, installed); err != nil {
		return err
	}
	return removeClaudeManagedBlock(filepath.Join(pp.ActiveProjectDir, "CLAUDE.md"))
}

func (a *ClaudeAdapter) syncClaudeMD(projectDir string) error {
	target := filepath.Join(projectDir, "CLAUDE.md")
	return injectClaudeManagedBlock(target, "@AGENTS.md")
}

func NewKiroAdapter() *FolderBasedAdapter {
	return NewFolderBasedAdapter(FolderConfig{
		RootDirName: ".kiro",
		RulesDir:    "steering",
		CommandsDir: "hooks",
		SkillsDir:   "skills",
		AgentsDir:   "agents",
		MCPFilePath: "~/.kiro/settings/mcp.json",
		FileTypes: map[string]FileMode{
			"rule":    {Mode: "file", Ext: "md"},
			"command": {Mode: "file", Ext: "md"},
			"agent":   {Mode: "file", Ext: "md"},
			"skill":   {Mode: "folder", Ext: ""},
		},
	})
}

func NewCodexAdapter() *FolderBasedAdapter {
	return NewFolderBasedAdapter(FolderConfig{
		RootDirName: ".codex",
		RulesDir:    "rules",
		CommandsDir: "commands",
		SkillsDir:   "skills",
		AgentsDir:   "agents",
		MCPFilePath: "~/.codex/config.toml",
		FileTypes: map[string]FileMode{
			"rule":    {Mode: "file", Ext: "md"},
			"command": {Mode: "file", Ext: "md"},
			"agent":   {Mode: "file", Ext: "md"},
			"skill":   {Mode: "folder", Ext: ""},
		},
	})
}

func NewOpenCodeAdapter() *FolderBasedAdapter {
	return NewFolderBasedAdapter(FolderConfig{
		RootDirName: ".opencode",
		RulesDir:    "agents",
		CommandsDir: "commands",
		SkillsDir:   "skills",
		AgentsDir:   "agents",
		MCPFilePath: "~/.config/opencode/opencode.json",
		FileTypes: map[string]FileMode{
			"rule":    {Mode: "file", Ext: "md"},
			"command": {Mode: "file", Ext: "md"},
			"agent":   {Mode: "file", Ext: "md"},
			"skill":   {Mode: "folder", Ext: ""},
		},
	})
}

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
		RootDirName: ".gemini",
		RulesDir:    "rules",
		CommandsDir: "commands",
		SkillsDir:   "skills",
		AgentsDir:   "agents",
		MCPFilePath: "~/.gemini/settings.json",
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
	return a.syncGeminiMD(pp.ActiveProjectDir)
}

func (a *GeminiAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	if err := a.FolderBasedAdapter.Remove(pp, installed); err != nil {
		return err
	}
	return removeGeminiManagedBlock(filepath.Join(pp.ActiveProjectDir, "AGENTS.md"))
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

func GetAdapter(ide string) Adapter {
	switch strings.ToLower(ide) {
	case "antigravity":
		return NewAntigravityAdapter()
	case "cursor":
		return NewCursorAdapter()
	case "claude", "claude-code":
		return NewClaudeAdapter()
	case "kiro":
		return NewKiroAdapter()
	case "codex":
		return NewCodexAdapter()
	case "opencode":
		return NewOpenCodeAdapter()
	case "gemini", "gemini-code":
		return NewGeminiAdapter()
	default:
		return nil
	}
}

func SupportedIDEs() []string {
	return []string{"antigravity", "cursor", "claude", "kiro", "codex", "opencode", "gemini"}
}

func GlobalRulesFile(ide string) string {
	switch strings.ToLower(ide) {
	case "antigravity":
		return "AGENTS.md"
	case "gemini", "gemini-code":
		return "AGENTS.md"
	case "claude", "claude-code":
		return "AGENTS.md"
	case "cursor":
		return "AGENTS.md"
	case "kiro":
		return "AGENTS.md"
	case "codex":
		return "AGENTS.md"
	case "opencode":
		return "AGENTS.md"
	default:
		return "AGENTS.md"
	}
}

func InjectManagedBlock(projectDir, ide, markerName, content string) error {
	globalFile := GlobalRulesFile(ide)
	targetPath := filepath.Join(projectDir, globalFile)

	if err := os.MkdirAll(filepath.Dir(targetPath), 0o755); err != nil {
		return fmt.Errorf("creating rules dir: %w", err)
	}

	marker := blockMarkerForName(markerName)
	return injectBlock(targetPath, marker, content)
}

func RemoveManagedBlock(projectDir, ide, markerName string) error {
	globalFile := GlobalRulesFile(ide)
	targetPath := filepath.Join(projectDir, globalFile)
	marker := blockMarkerForName(markerName)
	return removeBlock(targetPath, marker)
}

func InstallManagedSkill(projectDir, ideName, skillName, content string) error {
	adapter := GetAdapter(ideName)
	if adapter == nil {
		return fmt.Errorf("unknown IDE: %s", ideName)
	}
	return installSkillForAdapter(adapter, projectDir, skillName, content)
}

func RemoveManagedSkill(projectDir, ideName, skillName string) error {
	adapter := GetAdapter(ideName)
	if adapter == nil {
		return fmt.Errorf("unknown IDE: %s", ideName)
	}
	return removeSkillForAdapter(adapter, projectDir, skillName)
}

func installSkillForAdapter(adapter Adapter, projectDir, skillName, content string) error {
	var rootDir, skillsDir string

	switch a := adapter.(type) {
	case *GeminiAdapter:
		rootDir = a.cfg.RootDirName
		skillsDir = a.cfg.SkillsDir
	case *ClaudeAdapter:
		rootDir = a.cfg.RootDirName
		skillsDir = a.cfg.SkillsDir
	case *FolderBasedAdapter:
		rootDir = a.cfg.RootDirName
		skillsDir = a.cfg.SkillsDir
	default:
		return fmt.Errorf("unsupported adapter type for skill installation")
	}

	if skillsDir == "" {
		skillsDir = "skills"
	}

	skillDir := filepath.Join(projectDir, rootDir, skillsDir, skillName)
	if err := os.MkdirAll(skillDir, 0o755); err != nil {
		return fmt.Errorf("creating skill dir: %w", err)
	}

	skillFile := filepath.Join(skillDir, "SKILL.md")
	return os.WriteFile(skillFile, []byte(content), 0o644)
}

func removeSkillForAdapter(adapter Adapter, projectDir, skillName string) error {
	var rootDir, skillsDir string

	switch a := adapter.(type) {
	case *GeminiAdapter:
		rootDir = a.cfg.RootDirName
		skillsDir = a.cfg.SkillsDir
	case *ClaudeAdapter:
		rootDir = a.cfg.RootDirName
		skillsDir = a.cfg.SkillsDir
	case *FolderBasedAdapter:
		rootDir = a.cfg.RootDirName
		skillsDir = a.cfg.SkillsDir
	default:
		return nil
	}

	if skillsDir == "" {
		skillsDir = "skills"
	}

	skillDir := filepath.Join(projectDir, rootDir, skillsDir, skillName)
	return os.RemoveAll(skillDir)
}

func blockMarkerForName(name string) string {
	return strings.ToUpper(brand.Brand) + " " + strings.ToUpper(name) + " BLOCK"
}

func injectBlock(filePath, marker, content string) error {
	return gitblk.InjectBlockStyled(filePath, content, marker, "", gitblk.HTMLBlockStyle)
}

func removeBlock(filePath, marker string) error {
	_, err := gitblk.RemoveBlockStyled(filePath, marker, false, gitblk.HTMLBlockStyle)
	return err
}

func ArtifactTypePath(projectDir, ideName, artifactType, artifactName string) (string, error) {
	adapter := GetAdapter(ideName)
	if adapter == nil {
		return "", fmt.Errorf("unknown IDE: %s", ideName)
	}

	var rootDir string
	var typeDir string
	var fm FileMode

	switch a := adapter.(type) {
	case *GeminiAdapter:
		rootDir = a.cfg.RootDirName
		typeDir = a.getTypeDir(artifactType)
		fm = a.getFileMode(artifactType)
	case *ClaudeAdapter:
		rootDir = a.cfg.RootDirName
		typeDir = a.getTypeDir(artifactType)
		fm = a.getFileMode(artifactType)
	case *FolderBasedAdapter:
		rootDir = a.cfg.RootDirName
		typeDir = a.getTypeDir(artifactType)
		fm = a.getFileMode(artifactType)
	default:
		return "", fmt.Errorf("unsupported adapter type for IDE %s", ideName)
	}

	if typeDir == "" {
		return "", fmt.Errorf("unknown artifact type %q for IDE %s", artifactType, ideName)
	}

	base := filepath.Join(projectDir, rootDir, typeDir)

	if fm.Mode == "folder" {
		return filepath.Join(base, artifactName), nil
	}

	return filepath.Join(base, artifactName+"."+fm.Ext), nil
}

func GetFileMode(ideName, artifactType string) string {
	adapter := GetAdapter(ideName)
	if adapter == nil {
		return "file"
	}

	switch a := adapter.(type) {
	case *GeminiAdapter:
		return a.getFileMode(artifactType).Mode
	case *ClaudeAdapter:
		return a.getFileMode(artifactType).Mode
	case *FolderBasedAdapter:
		return a.getFileMode(artifactType).Mode
	default:
		return "file"
	}
}
