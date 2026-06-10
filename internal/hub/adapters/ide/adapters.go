package ide

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	toml "github.com/pelletier/go-toml/v2"

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
		MCPFilePath: "~/.gemini/config/mcp_config.json",
		MCPExtraPaths: []string{
			"~/.gemini/antigravity/mcp_config.json",
			"~/.gemini/settings.json",
		},
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

type CodexAdapter struct {
	*FolderBasedAdapter
}

func NewCodexAdapter() *CodexAdapter {
	base := NewFolderBasedAdapter(FolderConfig{
		RootDirName:   ".codex",
		RulesDir:      "rules",
		CommandsDir:   "commands",
		SkillsDir:     "skills",
		AgentsDir:     "agents",
		MCPFilePath:   "~/.codex/config.toml",
		MCPCustomSync: true,
		FileTypes: map[string]FileMode{
			"rule":    {Mode: "file", Ext: "md"},
			"command": {Mode: "file", Ext: "md"},
			"agent":   {Mode: "file", Ext: "md"},
			"skill":   {Mode: "folder", Ext: ""},
		},
	})
	return &CodexAdapter{base}
}

func (a *CodexAdapter) Sync(installed map[string]map[string]string, pp *paths.ProjectPaths, projectID string) error {
	if err := a.FolderBasedAdapter.Sync(installed, pp, projectID); err != nil {
		return err
	}
	mcpTarget, _ := expandHome(a.cfg.MCPFilePath)
	return a.syncCodexMCP(mcpTarget, projectID, installed)
}

func (a *CodexAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	if err := a.FolderBasedAdapter.Remove(pp, installed); err != nil {
		return err
	}
	mcpTarget, _ := expandHome(a.cfg.MCPFilePath)
	return a.removeCodexMCP(mcpTarget)
}

type codexMCPServer struct {
	Command string   `toml:"command"`
	Args    []string `toml:"args"`
	Enabled bool     `toml:"enabled"`
}

func (a *CodexAdapter) syncCodexMCP(mcpTarget, _ string, _ map[string]map[string]string) error {
	coreServerKey := brand.MCPServerName("code-stdio")
	exe := getGraphitExecutable()

	_ = os.MkdirAll(filepath.Dir(mcpTarget), 0o755)

	cfg := map[string]any{}
	if data, err := os.ReadFile(mcpTarget); err == nil {
		_ = toml.Unmarshal(data, &cfg)
	}

	servers, _ := cfg["mcp_servers"].(map[string]any)
	if servers == nil {
		servers = map[string]any{}
	}

	servers[coreServerKey] = codexMCPServer{
		Command: exe,
		Args:    []string{"mcp", "--stdio"},
		Enabled: true,
	}
	cfg["mcp_servers"] = servers

	out, err := toml.Marshal(cfg)
	if err != nil {
		return err
	}
	return os.WriteFile(mcpTarget, out, 0o644)
}

func (a *CodexAdapter) removeCodexMCP(mcpTarget string) error {
	data, err := os.ReadFile(mcpTarget)
	if err != nil {
		return nil
	}

	cfg := map[string]any{}
	if err := toml.Unmarshal(data, &cfg); err != nil {
		return nil
	}

	coreServerKey := brand.MCPServerName("code-stdio")

	if servers, ok := cfg["mcp_servers"].(map[string]any); ok {
		delete(servers, coreServerKey)
		if len(servers) == 0 {
			delete(cfg, "mcp_servers")
		} else {
			cfg["mcp_servers"] = servers
		}
	}

	out, err := toml.Marshal(cfg)
	if err != nil {
		return err
	}
	return os.WriteFile(mcpTarget, out, 0o644)
}

type OpenCodeAdapter struct {
	*FolderBasedAdapter
}

func NewOpenCodeAdapter() *OpenCodeAdapter {
	base := NewFolderBasedAdapter(FolderConfig{
		RootDirName:   ".opencode",
		RulesDir:      "agents",
		CommandsDir:   "commands",
		SkillsDir:     "skills",
		AgentsDir:     "agents",
		MCPFilePath:   "~/.config/opencode/opencode.json",
		MCPCustomSync: true,
		FileTypes: map[string]FileMode{
			"rule":    {Mode: "file", Ext: "md"},
			"command": {Mode: "file", Ext: "md"},
			"agent":   {Mode: "file", Ext: "md"},
			"skill":   {Mode: "folder", Ext: ""},
		},
	})
	return &OpenCodeAdapter{base}
}

func (a *OpenCodeAdapter) Sync(installed map[string]map[string]string, pp *paths.ProjectPaths, projectID string) error {
	if err := a.FolderBasedAdapter.Sync(installed, pp, projectID); err != nil {
		return err
	}
	mcpTarget, _ := expandHome(a.cfg.MCPFilePath)
	return a.syncOpenCodeMCP(mcpTarget, projectID)
}

func (a *OpenCodeAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	if err := a.FolderBasedAdapter.Remove(pp, installed); err != nil {
		return err
	}
	mcpTarget, _ := expandHome(a.cfg.MCPFilePath)
	return a.removeOpenCodeMCP(mcpTarget)
}

func (a *OpenCodeAdapter) syncOpenCodeMCP(mcpTarget, _ string) error {
	_ = os.MkdirAll(filepath.Dir(mcpTarget), 0o755)

	targetData := map[string]any{}
	if data, err := os.ReadFile(mcpTarget); err == nil {
		_ = json.Unmarshal(data, &targetData)
	}

	existingMCP, _ := targetData["mcp"].(map[string]any)
	if existingMCP == nil {
		existingMCP = map[string]any{}
	}

	coreServerKey := brand.MCPServerName("code-stdio")
	exe := getGraphitExecutable()

	// OpenCode format: command is an array, type is "local"
	existingMCP[coreServerKey] = map[string]any{
		"type":    "local",
		"command": []string{exe, "mcp", "--stdio"},
		"enabled": true,
	}

	targetData["mcp"] = existingMCP

	// Also write under mcpServers for cross-tool compatibility
	existingServers, _ := targetData["mcpServers"].(map[string]any)
	if existingServers == nil {
		existingServers = map[string]any{}
	}
	existingServers[coreServerKey] = map[string]any{
		"command": exe,
		"args":    []string{"mcp", "--stdio"},
		"env":     map[string]string{},
	}
	targetData["mcpServers"] = existingServers

	out, err := json.MarshalIndent(targetData, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(mcpTarget, out, 0o644)
}

func (a *OpenCodeAdapter) removeOpenCodeMCP(mcpTarget string) error {
	data, err := os.ReadFile(mcpTarget)
	if err != nil {
		return nil
	}

	targetData := map[string]any{}
	if err := json.Unmarshal(data, &targetData); err != nil {
		return nil
	}

	coreServerKey := brand.MCPServerName("code-stdio")

	if mcp, ok := targetData["mcp"].(map[string]any); ok {
		delete(mcp, coreServerKey)
		if len(mcp) == 0 {
			delete(targetData, "mcp")
		} else {
			targetData["mcp"] = mcp
		}
	}

	if servers, ok := targetData["mcpServers"].(map[string]any); ok {
		delete(servers, coreServerKey)
		if len(servers) == 0 {
			delete(targetData, "mcpServers")
		} else {
			targetData["mcpServers"] = servers
		}
	}

	out, err := json.MarshalIndent(targetData, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(mcpTarget, out, 0o644)
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
		MCPExtraPaths: []string{
			"~/.gemini/config/mcp_config.json",
			"~/.gemini/antigravity/mcp_config.json",
		},
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
	case *CodexAdapter:
		rootDir = a.cfg.RootDirName
		skillsDir = a.cfg.SkillsDir
	case *OpenCodeAdapter:
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
	case *CodexAdapter:
		rootDir = a.cfg.RootDirName
		skillsDir = a.cfg.SkillsDir
	case *OpenCodeAdapter:
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
	case *CodexAdapter:
		rootDir = a.cfg.RootDirName
		typeDir = a.getTypeDir(artifactType)
		fm = a.getFileMode(artifactType)
	case *OpenCodeAdapter:
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
	case *CodexAdapter:
		return a.getFileMode(artifactType).Mode
	case *OpenCodeAdapter:
		return a.getFileMode(artifactType).Mode
	case *FolderBasedAdapter:
		return a.getFileMode(artifactType).Mode
	default:
		return "file"
	}
}
