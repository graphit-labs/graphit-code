package ide

import (
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
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
	HookFilePath  string
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
	_ string,
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
		case "rule":
			// Rule artifacts are read from their installed Hub source by the
			// session hook. They are never copied into host rule directories.
			continue

		case "command", "workflow":
			typeDir := a.getTypeDir(artType)
			if typeDir == "" {
				continue
			}
			if err := a.copyArtifact(pp.ActiveProjectDir, artType, sourcePath, filepath.Join(baseDir, typeDir), localName); err != nil {
				continue
			}

		case "skill":
			if a.cfg.SkillsDir == "" {
				continue
			}
			if err := a.copyArtifact(pp.ActiveProjectDir, artType, sourcePath, filepath.Join(baseDir, a.cfg.SkillsDir), localName); err != nil {
				continue
			}

		case "agent":
			agentSrc := a.findCanonicalSource(artType, sourcePath)
			if agentSrc == "" {
				continue
			}
			if a.cfg.AgentsDir != "" {
				_ = a.copyArtifact(pp.ActiveProjectDir, artType, sourcePath, filepath.Join(baseDir, a.cfg.AgentsDir), localName)
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
			continue
		}
	}

	if a.cfg.AgentsFile != "" && compiledAgents.Len() > 0 {
		agentsTarget := filepath.Join(pp.ActiveProjectDir, a.cfg.AgentsFile)
		if err := os.WriteFile(agentsTarget, []byte(strings.TrimSpace(compiledAgents.String())+"\n"), 0o644); err != nil {
			return err
		}
	}

	for _, mp := range a.cfg.allMCPPaths() {
		mcpTarget, err := resolveConfiguredPath(mp, pp.ActiveProjectDir)
		if err != nil {
			return fmt.Errorf("resolving MCP config path %q: %w", mp, err)
		}
		if err := a.syncAllMCP(pp.ActiveProjectDir, mcpTarget, installed); err != nil {
			return fmt.Errorf("syncing MCP config %s: %w", mcpTarget, err)
		}
	}

	return nil
}

func (a *FolderBasedAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	baseDir := a.baseDir(pp.ActiveProjectDir)

	if installed != nil {
		for eid, edata := range installed {
			artType := edata["type"]
			if artType == "rule" {
				continue
			}
			localName := eid
			typeDir := a.getTypeDir(artType)
			if typeDir == "" {
				continue
			}
			_ = os.Remove(artifactHashCachePath(pp.ActiveProjectDir, a.cfg.RootDirName, artType, localName))

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
	}

	if a.cfg.AgentsFile != "" {
		_ = os.Remove(filepath.Join(pp.ActiveProjectDir, a.cfg.AgentsFile))
	}

	for _, mp := range a.cfg.allMCPPaths() {
		mcpTarget, _ := resolveConfiguredPath(mp, pp.ActiveProjectDir)
		_ = a.removeMCPConfig(pp.ActiveProjectDir, mcpTarget, installed)
	}
	return nil
}

func (a *FolderBasedAdapter) folderBasedAdapter() *FolderBasedAdapter {
	return a
}

func resolveConfiguredPath(configuredPath, projectDir string) (string, error) {
	configuredPath = strings.ReplaceAll(configuredPath, "{active_project_dir}", projectDir)
	return expandHome(os.ExpandEnv(configuredPath))
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

// DesiredMCPServers is the set of MCP servers a project should declare: the graphit
// server, plus every server contributed by an installed MCP artifact.
//
// Exported because the adapters are not the only writer of MCP configuration. The
// live search's ephemeral project writes its own, project-local, because every path
// an adapter knows is under the home directory and a throwaway project has no
// business in the user's real configuration. Sharing this function is what keeps the
// two writers describing the same servers — including the artifact-contributed ones,
// which a second implementation would almost certainly forget.
func DesiredMCPServers(installed map[string]map[string]string) map[string]any {
	desiredServers := map[string]any{}

	mcpExe, mcpArgs, mcpEnv := getMCPProxyConfig()
	desiredServers[brand.MCPServerName("code-stdio")] = map[string]any{
		"command": mcpExe,
		"args":    mcpArgs,
		"env":     mcpEnv,
	}

	for _, edata := range installed {
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
		for k, v := range conf {
			desiredServers[k] = v
		}
	}

	return desiredServers
}

func mcpManifestNames(path string) ([]string, error) {
	data, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	var names []string
	if err := json.Unmarshal(data, &names); err != nil {
		return nil, err
	}
	return names, nil
}

func saveMCPManifest(path string, desired map[string]any) error {
	if len(desired) == 0 {
		if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
			return err
		}
		return nil
	}
	names := make([]string, 0, len(desired))
	for name := range desired {
		names = append(names, name)
	}
	sort.Strings(names)
	data, err := json.Marshal(names)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	return os.WriteFile(path, append(data, '\n'), 0o644)
}

func mcpManifestPath(projectDir, adapter string) string {
	return brand.ProjectRuntimePath(projectDir, "cache", "mcp", adapter+".json")
}

func (a *FolderBasedAdapter) mcpManifestPath(projectDir string) string {
	adapter := strings.TrimPrefix(a.cfg.RootDirName, ".")
	if adapter == "" {
		adapter = "project"
	}
	return mcpManifestPath(projectDir, adapter)
}

func (a *FolderBasedAdapter) syncAllMCP(projectDir, mcpTarget string, installed map[string]map[string]string) error {
	return reconcileMCPFile(mcpTarget, a.mcpManifestPath(projectDir), DesiredMCPServers(installed))
}

func (a *FolderBasedAdapter) removeMCPConfig(projectDir, mcpTarget string, installed map[string]map[string]string) error {
	return reconcileMCPFileWithPrevious(
		mcpTarget,
		a.mcpManifestPath(projectDir),
		map[string]any{},
		DesiredMCPServers(installed),
	)
}

func reconcileMCPFile(mcpTarget, manifestPath string, desiredServers map[string]any) error {
	return reconcileMCPFileWithPrevious(mcpTarget, manifestPath, desiredServers, nil)
}

func reconcileMCPFileWithPrevious(
	mcpTarget string,
	manifestPath string,
	desiredServers map[string]any,
	fallbackPrevious map[string]any,
) error {
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

	previous, err := mcpManifestNames(manifestPath)
	if err != nil {
		return err
	}
	for name := range fallbackPrevious {
		previous = append(previous, name)
	}
	for _, name := range previous {
		delete(existingServers, name)
	}

	for name, server := range desiredServers {
		existingServers[name] = server
	}
	if len(existingServers) == 0 {
		delete(targetData, "mcpServers")
	} else {
		targetData["mcpServers"] = existingServers
	}

	out, err := json.MarshalIndent(targetData, "", "  ")
	if err != nil {
		return err
	}
	if existing, readErr := os.ReadFile(mcpTarget); readErr != nil || string(existing) != string(out)+"\n" {
		if err := os.WriteFile(mcpTarget, append(out, '\n'), 0o644); err != nil {
			return err
		}
	}
	return saveMCPManifest(manifestPath, desiredServers)
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
	return []string{a.cfg.CommandsDir, a.cfg.SkillsDir, a.cfg.AgentsDir}
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

func artifactHashCachePath(projectDir, rootDirName, artType, localName string) string {
	adapterKey := strings.TrimPrefix(rootDirName, ".")
	return brand.ProjectRuntimePath(projectDir, "cache", "artifacts", adapterKey, artType, localName)
}

func computeSourceHash(fm FileMode, sourcePath string) (string, error) {
	h := sha256.New()
	if fm.Mode == "folder" {
		var files []string
		_ = filepath.Walk(sourcePath, func(p string, info os.FileInfo, err error) error {
			if err == nil && !info.IsDir() {
				files = append(files, p)
			}
			return err
		})
		sort.Strings(files)
		for _, f := range files {
			data, err := os.ReadFile(f)
			if err != nil {
				return "", err
			}
			rel, _ := filepath.Rel(sourcePath, f)
			fmt.Fprintf(h, "%s\x00", filepath.ToSlash(rel))
			h.Write(data)
		}
	} else {
		data, err := os.ReadFile(sourcePath)
		if err != nil {
			return "", err
		}
		h.Write(data)
	}
	return fmt.Sprintf("%x", h.Sum(nil)), nil
}

func (a *FolderBasedAdapter) copyArtifact(projectDir, artType, sourcePath, targetDir, localName string) error {
	if _, err := os.Stat(sourcePath); err != nil {
		return err
	}
	fm := a.getFileMode(artType)

	var srcFile string
	if fm.Mode != "folder" {
		srcFile = a.findCanonicalSource(artType, sourcePath)
		if srcFile == "" {
			return fmt.Errorf("no canonical source found in %s", sourcePath)
		}
	}

	hashFile := artifactHashCachePath(projectDir, a.cfg.RootDirName, artType, localName)
	hashSrc := sourcePath
	if fm.Mode != "folder" {
		hashSrc = srcFile
	}

	var destPath string
	if fm.Mode == "folder" {
		destPath = filepath.Join(targetDir, localName)
	} else {
		destPath = filepath.Join(targetDir, localName+"."+fm.Ext)
	}

	newHash, hashErr := computeSourceHash(fm, hashSrc)
	if hashErr == nil {
		if cached, err := os.ReadFile(hashFile); err == nil {
			parts := strings.SplitN(strings.TrimSpace(string(cached)), ":", 2)
			if len(parts) == 2 && parts[0] == newHash {
				if fi, statErr := os.Stat(destPath); statErr == nil {
					mtimeMatch := false
					if cachedMtime, parseErr := strconv.ParseInt(parts[1], 10, 64); parseErr == nil {
						mtimeMatch = fi.ModTime().UnixNano() == cachedMtime
					}
					if mtimeMatch {
						return nil
					}
					if destHash, rehashErr := computeSourceHash(fm, destPath); rehashErr == nil && destHash == newHash {
						if mkErr := os.MkdirAll(filepath.Dir(hashFile), 0o755); mkErr == nil {
							entry := fmt.Sprintf("%s:%d", newHash, fi.ModTime().UnixNano())
							_ = os.WriteFile(hashFile, []byte(entry), 0o644)
						}
						return nil
					}
				}
			}
		}
	}

	var copyErr error
	if fm.Mode == "folder" {
		_ = os.RemoveAll(destPath)
		copyErr = copyDirAll(sourcePath, destPath)
	} else {
		copyErr = copyFile(srcFile, destPath)
	}

	if copyErr == nil && hashErr == nil {
		if fi, statErr := os.Stat(destPath); statErr == nil {
			if err := os.MkdirAll(filepath.Dir(hashFile), 0o755); err == nil {
				entry := fmt.Sprintf("%s:%d", newHash, fi.ModTime().UnixNano())
				_ = os.WriteFile(hashFile, []byte(entry), 0o644)
			}
		}
	}
	return copyErr
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
	srcInfo, err := os.Stat(src)
	if err != nil {
		return err
	}
	if dstInfo, err := os.Stat(dst); err == nil && dstInfo.Size() == srcInfo.Size() {
		srcData, serr := os.ReadFile(src)
		dstData, derr := os.ReadFile(dst)
		if serr == nil && derr == nil && string(srcData) == string(dstData) {
			return nil
		}
	}
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

func getMCPProxyConfig() (exe string, args []string, env map[string]string) {
	return getGraphitExecutable(), []string{"mcp", "--stdio"}, map[string]string{}
}
