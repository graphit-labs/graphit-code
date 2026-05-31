package ide

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
)

type Adapter interface {
	Sync(installedArtifacts map[string]map[string]string, pp *paths.ProjectPaths, projectID string) error

	Remove(pp *paths.ProjectPaths, installedArtifacts map[string]map[string]string) error

	ScanLocal(projectDir string) []LocalArtifact

	MCPConfig() string
}

type LocalArtifact struct {
	ID     string
	Type   string
	Path   string
	IsFile bool
}

type FileMode struct {
	Mode string
	Ext  string
}

type FolderConfig struct {
	RootDirName   string
	RulesDir      string
	CommandsDir   string
	SkillsDir     string
	AgentsDir     string
	AgentsFile    string
	MCPFilePath   string
	MCPExtraPaths []string
	MCPCustomSync bool

	FileTypes map[string]FileMode
}

func (c *FolderConfig) allMCPPaths() []string {
	if c.MCPCustomSync {
		return nil
	}
	var paths []string
	if c.MCPFilePath != "" {
		paths = append(paths, c.MCPFilePath)
	}
	paths = append(paths, c.MCPExtraPaths...)
	return paths
}

var canonicalSourceNames = map[string]string{
	"rule":    "RULE.md",
	"command": "COMMAND.md",
	"agent":   "AGENT.md",
	"skill":   "SKILL.md",
}

var defaultFileTypes = map[string]FileMode{
	"rule":     {Mode: "file", Ext: "md"},
	"command":  {Mode: "file", Ext: "md"},
	"workflow": {Mode: "file", Ext: "md"},
	"agent":    {Mode: "file", Ext: "md"},
	"skill":    {Mode: "folder", Ext: ""},
}

type FolderBasedAdapter struct {
	cfg FolderConfig
}

func NewFolderBasedAdapter(cfg FolderConfig) *FolderBasedAdapter {
	if cfg.FileTypes == nil {
		cfg.FileTypes = defaultFileTypes
	}
	if cfg.RulesDir == "" {
		cfg.RulesDir = "rules"
	}
	if cfg.CommandsDir == "" {
		cfg.CommandsDir = "commands"
	}
	if cfg.SkillsDir == "" {
		cfg.SkillsDir = "skills"
	}
	return &FolderBasedAdapter{cfg: cfg}
}

func (a *FolderBasedAdapter) Sync(
	installed map[string]map[string]string,
	pp *paths.ProjectPaths,
	projectID string,
) error {
	baseDir := a.baseDir(pp.ActiveProjectDir)

	for _, d := range a.typeDirs() {
		if d != "" {
			if err := os.MkdirAll(filepath.Join(baseDir, d), 0o755); err != nil {
				return err
			}
		}
	}

	var compiledAgents strings.Builder

	ids := make([]string, 0, len(installed))
	for id := range installed {
		ids = append(ids, id)
	}
	sort.Strings(ids)

	for _, eid := range ids {
		edata := installed[eid]
		sourcePath := edata["path"]
		artType := edata["type"]
		localName := eid

		switch artType {
		case "rule", "command", "workflow":
			typeDir := a.getTypeDir(artType)
			if typeDir == "" {
				continue
			}
			if err := a.copyArtifact(artType, sourcePath, filepath.Join(baseDir, typeDir), localName); err != nil {
				continue
			}

		case "skill":
			if a.cfg.SkillsDir == "" {
				continue
			}
			if err := a.copyArtifact(artType, sourcePath, filepath.Join(baseDir, a.cfg.SkillsDir), localName); err != nil {
				continue
			}

		case "agent":
			agentSrc := a.findCanonicalSource(artType, sourcePath)
			if agentSrc == "" {
				continue
			}
			if a.cfg.AgentsDir != "" {
				_ = a.copyArtifact(artType, sourcePath, filepath.Join(baseDir, a.cfg.AgentsDir), localName)
			} else if a.cfg.RulesDir != "" && a.cfg.AgentsFile == "" {

				dest := filepath.Join(baseDir, a.cfg.RulesDir, fmt.Sprintf("%s_agent.md", localName))
				_ = copyFile(agentSrc, dest)
			}

			if a.cfg.AgentsFile != "" {
				content, err := os.ReadFile(agentSrc)
				if err == nil {
					fmt.Fprintf(&compiledAgents, "\n# --- AGENT CONTEXT: %s ---\n%s\n",
						strings.ToUpper(localName), string(content))
				}
			}

		case "mcp":
			for _, mp := range a.cfg.allMCPPaths() {
				mcpTarget := os.ExpandEnv(strings.ReplaceAll(mp,
					"{active_project_dir}", pp.ActiveProjectDir))
				mcpTarget, _ = expandHome(mcpTarget)
				_ = a.syncMCP(eid, sourcePath, mcpTarget, projectID, installed)
			}
		}
	}

	if a.cfg.AgentsFile != "" && compiledAgents.Len() > 0 {
		agentsTarget := filepath.Join(pp.ActiveProjectDir, a.cfg.AgentsFile)
		if err := os.WriteFile(agentsTarget, []byte(strings.TrimSpace(compiledAgents.String())+"\n"), 0o644); err != nil {
			return err
		}
	}

	for _, mp := range a.cfg.allMCPPaths() {
		mcpTarget := os.ExpandEnv(strings.ReplaceAll(mp,
			"{active_project_dir}", pp.ActiveProjectDir))
		mcpTarget, _ = expandHome(mcpTarget)
		_ = a.syncAllMCP(mcpTarget, projectID, installed)
	}

	return nil
}

func (a *FolderBasedAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	baseDir := a.baseDir(pp.ActiveProjectDir)

	if installed != nil {
		for eid, edata := range installed {
			artType := edata["type"]
			localName := eid
			typeDir := a.getTypeDir(artType)
			if typeDir == "" {
				continue
			}

			fm := a.getFileMode(artType)
			if fm.Mode == "file" {
				target := filepath.Join(baseDir, typeDir, localName+"."+fm.Ext)
				_ = os.Remove(target)
			}
			target := filepath.Join(baseDir, typeDir, localName)
			_ = os.RemoveAll(target)
		}

		for _, d := range a.typeDirs() {
			if d == "" {
				continue
			}
			sub := filepath.Join(baseDir, d)
			if info, err := os.ReadDir(sub); err == nil && len(info) == 0 {
				_ = os.Remove(sub)
			}
		}

		if info, err := os.ReadDir(baseDir); err == nil && len(info) == 0 {
			_ = os.Remove(baseDir)
		}
	} else if baseDir != "" {
		_ = os.RemoveAll(baseDir)
	}

	if a.cfg.AgentsFile != "" {
		_ = os.Remove(filepath.Join(pp.ActiveProjectDir, a.cfg.AgentsFile))
	}

	for _, mp := range a.cfg.allMCPPaths() {
		mcpTarget := os.ExpandEnv(strings.ReplaceAll(mp,
			"{active_project_dir}", pp.ActiveProjectDir))
		mcpTarget, _ = expandHome(mcpTarget)
		_ = a.removeMCPClaims(mcpTarget, projectIDFrom(installed), installed)
	}
	return nil
}

func (a *FolderBasedAdapter) ScanLocal(projectDir string) []LocalArtifact {
	var results []LocalArtifact
	baseDir := a.baseDir(projectDir)

	typeMap := map[string]string{
		"rule":    a.cfg.RulesDir,
		"command": a.cfg.CommandsDir,
		"skill":   a.cfg.SkillsDir,
	}
	if a.cfg.AgentsDir != "" {
		typeMap["agent"] = a.cfg.AgentsDir
	}

	for artType, dirName := range typeMap {
		if dirName == "" {
			continue
		}
		scanDir := filepath.Join(baseDir, dirName)
		entries, err := os.ReadDir(scanDir)
		if err != nil {
			continue
		}
		fm := a.getFileMode(artType)
		for _, entry := range entries {
			full := filepath.Join(scanDir, entry.Name())

			if entry.Type()&os.ModeSymlink != 0 {
				continue
			}
			if fm.Mode == "file" && !entry.IsDir() {
				id := strings.TrimSuffix(entry.Name(), "."+fm.Ext)
				results = append(results, LocalArtifact{ID: id, Type: artType, Path: full, IsFile: true})
			} else if fm.Mode == "folder" && entry.IsDir() {

				if artType == "skill" && brand.CoreSkillIDs()[entry.Name()] {
					continue
				}
				results = append(results, LocalArtifact{ID: entry.Name(), Type: artType, Path: full, IsFile: false})
			}
		}
	}
	return results
}

func (a *FolderBasedAdapter) MCPConfig() string {
	if a.cfg.MCPFilePath == "" {
		return ""
	}
	expanded, _ := expandHome(a.cfg.MCPFilePath)
	return expanded
}

func getGraphitExecutable() string {
	if p := os.Getenv(brand.EnvVar("LAUNCHER_PATH")); p != "" {
		return p
	}
	if p, err := exec.LookPath(brand.BinName()); err == nil {
		if eval, err := filepath.EvalSymlinks(p); err == nil {
			return eval
		}
		return p
	}
	return brand.BinName()
}

func (a *FolderBasedAdapter) syncAllMCP(mcpTarget, projectID string, installed map[string]map[string]string) error {
	desiredServers := map[string]any{}

	// Auto-install/update core MCP stdio server
	coreServerKey := brand.MCPServerName("code-stdio")
	desiredServers[coreServerKey] = map[string]any{
		"command": getGraphitExecutable(),
		"args":    []string{"mcp", "--stdio"},
		"env":     map[string]string{},
	}

	for eid, edata := range installed {
		if edata["type"] != "mcp" {
			continue
		}
		mcpJSON := findMCPJSON(edata["path"])
		if mcpJSON == "" {
			continue
		}
		data, err := os.ReadFile(mcpJSON)
		if err != nil {
			continue
		}
		var conf map[string]any
		if err := json.Unmarshal(data, &conf); err != nil {
			continue
		}
		_ = eid
		for k, v := range conf {
			desiredServers[k] = v
		}
	}

	return reconcileMCPFile(mcpTarget, projectID, desiredServers)
}

func (a *FolderBasedAdapter) syncMCP(eid, sourcePath, mcpTarget, projectID string, installed map[string]map[string]string) error {
	return a.syncAllMCP(mcpTarget, projectID, installed)
}

func (a *FolderBasedAdapter) removeMCPClaims(mcpTarget, projectID string, _ map[string]map[string]string) error {
	return reconcileMCPFile(mcpTarget, projectID, map[string]any{})
}

func reconcileMCPFile(mcpTarget, projectID string, desiredServers map[string]any) error {
	if mcpTarget == "" {
		return nil
	}

	_ = os.MkdirAll(filepath.Dir(mcpTarget), 0o755)

	targetData := map[string]any{}
	if data, err := os.ReadFile(mcpTarget); err == nil {
		_ = json.Unmarshal(data, &targetData)
	}

	existingServers, _ := targetData["mcpServers"].(map[string]any)
	if existingServers == nil {
		existingServers = map[string]any{}
	}

	managed := map[string][]string{}
	if raw, ok := targetData[brand.ManagedMCPKey()].(map[string]any); ok {
		for k, v := range raw {
			if refs, ok := v.([]any); ok {
				strs := make([]string, 0, len(refs))
				for _, r := range refs {
					if s, ok := r.(string); ok {
						strs = append(strs, s)
					}
				}
				managed[k] = strs
			}
		}
	}

	for k, refs := range managed {
		updated := refs[:0]
		for _, ref := range refs {
			if ref != projectID {
				updated = append(updated, ref)
			}
		}
		managed[k] = updated
	}

	for key, conf := range desiredServers {
		if _, ok := managed[key]; !ok {
			managed[key] = []string{}
		}
		alreadyClaimed := false
		for _, ref := range managed[key] {
			if ref == projectID {
				alreadyClaimed = true
				break
			}
		}
		if !alreadyClaimed && projectID != "" {
			managed[key] = append(managed[key], projectID)
		}
		existingServers[key] = conf
	}

	for k, refs := range managed {
		if len(refs) == 0 {
			delete(existingServers, k)
			delete(managed, k)
		}
	}

	targetData["mcpServers"] = existingServers
	targetData[brand.ManagedMCPKey()] = managed

	out, err := json.MarshalIndent(targetData, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(mcpTarget, out, 0o644)
}

func (a *FolderBasedAdapter) baseDir(projectDir string) string {
	if a.cfg.RootDirName == "" {
		return projectDir
	}
	return filepath.Join(projectDir, a.cfg.RootDirName)
}

func (a *FolderBasedAdapter) getTypeDir(artType string) string {
	if artType == "workflow" {
		artType = "command"
	}
	switch artType {
	case "rule":
		return a.cfg.RulesDir
	case "command":
		return a.cfg.CommandsDir
	case "skill":
		return a.cfg.SkillsDir
	case "agent":
		return a.cfg.AgentsDir
	}
	return ""
}

func (a *FolderBasedAdapter) typeDirs() []string {
	return []string{a.cfg.RulesDir, a.cfg.CommandsDir, a.cfg.SkillsDir, a.cfg.AgentsDir}
}

func (a *FolderBasedAdapter) getFileMode(artType string) FileMode {
	if artType == "workflow" {
		artType = "command"
	}
	if fm, ok := a.cfg.FileTypes[artType]; ok {
		return fm
	}
	return FileMode{Mode: "file", Ext: "md"}
}

func (a *FolderBasedAdapter) findCanonicalSource(artType, sourcePath string) string {
	if canonical, ok := canonicalSourceNames[artType]; ok {
		path := filepath.Join(sourcePath, canonical)
		if _, err := os.Stat(path); err == nil {
			return path
		}
	}

	entries, err := os.ReadDir(sourcePath)
	if err != nil {
		return ""
	}
	for _, e := range entries {
		if !e.IsDir() {
			return filepath.Join(sourcePath, e.Name())
		}
	}
	return ""
}

func (a *FolderBasedAdapter) copyArtifact(artType, sourcePath, targetDir, localName string) error {
	if _, err := os.Stat(sourcePath); err != nil {
		return err
	}
	fm := a.getFileMode(artType)
	if fm.Mode == "folder" {
		dest := filepath.Join(targetDir, localName)
		_ = os.RemoveAll(dest)
		return copyDirAll(sourcePath, dest)
	}
	srcFile := a.findCanonicalSource(artType, sourcePath)
	if srcFile == "" {
		return fmt.Errorf("no canonical source found in %s", sourcePath)
	}
	dest := filepath.Join(targetDir, localName+"."+fm.Ext)
	return copyFile(srcFile, dest)
}

func findMCPJSON(artifactPath string) string {
	for _, name := range []string{"mcp.json", "MCP.json"} {
		p := filepath.Join(artifactPath, name)
		if _, err := os.Stat(p); err == nil {
			return p
		}
	}
	return ""
}

func projectIDFrom(installed map[string]map[string]string) string {
	if installed == nil {
		return ""
	}
	for _, edata := range installed {
		if id := edata["project_id"]; id != "" {
			return id
		}
	}
	return ""
}

func expandHome(path string) (string, error) {
	if !strings.HasPrefix(path, "~/") {
		return path, nil
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return path, err
	}
	return filepath.Join(home, path[2:]), nil
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer func() { _ = in.Close() }()
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return err
	}
	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	defer func() { _ = out.Close() }()
	_, err = io.Copy(out, in)
	return err
}

func copyDirAll(src, dst string) error {
	return filepath.Walk(src, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		rel, _ := filepath.Rel(src, path)
		dest := filepath.Join(dst, rel)
		if info.IsDir() {
			return os.MkdirAll(dest, info.Mode())
		}
		return copyFile(path, dest)
	})
}
