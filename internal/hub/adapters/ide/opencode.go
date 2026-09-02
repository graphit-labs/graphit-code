package ide

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strconv"

	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

const (
	opencodeManagedHookFile = "graphit-memory-session-start.js"
	opencodeManagedMarker   = "// Managed by Graphit: deterministic session-start memory protocol"
)

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
		HookFilePath:  "{active_project_dir}/.opencode/plugins/graphit-memory-session-start.js",
		MCPFilePath:   "{active_project_dir}/opencode.json",
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
	mcpTarget, _ := resolveConfiguredPath(a.cfg.MCPFilePath, pp.ActiveProjectDir)
	if err := a.syncOpenCodeMCP(pp.ActiveProjectDir, mcpTarget, installed); err != nil {
		return err
	}
	return a.syncSessionStartHook(pp.ActiveProjectDir)
}

func (a *OpenCodeAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	if err := a.FolderBasedAdapter.Remove(pp, installed); err != nil {
		return err
	}
	mcpTarget, _ := resolveConfiguredPath(a.cfg.MCPFilePath, pp.ActiveProjectDir)
	if err := a.removeOpenCodeMCP(pp.ActiveProjectDir, mcpTarget, installed); err != nil {
		return err
	}
	return a.removeSessionStartHook(pp.ActiveProjectDir)
}

func (a *OpenCodeAdapter) syncOpenCodeMCP(projectDir, mcpTarget string, installed map[string]map[string]string) error {
	_ = os.MkdirAll(filepath.Dir(mcpTarget), 0o755)

	targetData := map[string]any{}
	if data, err := os.ReadFile(mcpTarget); err == nil {
		_ = json.Unmarshal(data, &targetData)
	}

	existingMCP, _ := targetData["mcp"].(map[string]any)
	if existingMCP == nil {
		existingMCP = map[string]any{}
	}

	manifestPath := mcpManifestPath(projectDir, "opencode")
	previous, err := mcpManifestNames(manifestPath)
	if err != nil {
		return err
	}
	for _, name := range previous {
		delete(existingMCP, name)
	}

	desired := DesiredMCPServers(installed)
	for name, server := range desired {
		existingMCP[name] = openCodeMCPServer(server)
	}
	targetData["mcp"] = existingMCP

	out, err := json.MarshalIndent(targetData, "", "  ")
	if err != nil {
		return err
	}
	if err := os.WriteFile(mcpTarget, out, 0o644); err != nil {
		return err
	}
	return saveMCPManifest(manifestPath, desired)
}

func openCodeMCPServer(server any) any {
	standard, ok := server.(map[string]any)
	if !ok {
		return server
	}
	converted := map[string]any{}
	if command, ok := standard["command"].(string); ok {
		parts := []any{command}
		switch args := standard["args"].(type) {
		case []any:
			parts = append(parts, args...)
		case []string:
			for _, arg := range args {
				parts = append(parts, arg)
			}
		}
		converted["type"] = "local"
		converted["command"] = parts
		converted["enabled"] = true
		if env, ok := standard["env"]; ok {
			converted["environment"] = env
		}
		return converted
	}
	if url, ok := standard["url"]; ok {
		converted["type"] = "remote"
		converted["url"] = url
		converted["enabled"] = true
		if headers, ok := standard["headers"]; ok {
			converted["headers"] = headers
		}
		return converted
	}
	return server
}

func (a *OpenCodeAdapter) removeOpenCodeMCP(projectDir, mcpTarget string, installed map[string]map[string]string) error {
	data, err := os.ReadFile(mcpTarget)
	if err != nil {
		return nil
	}

	targetData := map[string]any{}
	if err := json.Unmarshal(data, &targetData); err != nil {
		return nil
	}

	names, err := mcpManifestNames(mcpManifestPath(projectDir, "opencode"))
	if err != nil {
		return err
	}
	if len(names) == 0 {
		for name := range DesiredMCPServers(installed) {
			names = append(names, name)
		}
	}
	if mcp, ok := targetData["mcp"].(map[string]any); ok {
		for _, name := range names {
			delete(mcp, name)
		}
		if len(mcp) == 0 {
			delete(targetData, "mcp")
		} else {
			targetData["mcp"] = mcp
		}
	}
	out, err := json.MarshalIndent(targetData, "", "  ")
	if err != nil {
		return err
	}
	if err := os.WriteFile(mcpTarget, out, 0o644); err != nil {
		return err
	}
	return saveMCPManifest(mcpManifestPath(projectDir, "opencode"), map[string]any{})
}

func (a *OpenCodeAdapter) syncSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	if existing, err := os.ReadFile(path); err == nil && !bytes.Contains(existing, []byte(opencodeManagedMarker)) {
		return fmt.Errorf("reconciling %s: file is not managed by Graphit", path)
	} else if err != nil && !os.IsNotExist(err) {
		return err
	}

	prompt := strconv.Quote(sessionhook.Protocol())
	content := opencodeManagedMarker + "\n" +
		"const initializedSessions = new Set()\n" +
		"const startedSessions = new Set()\n\n" +
		"export const GraphitMemorySessionStart = async () => ({\n" +
		"  event: async ({ event }) => {\n" +
		"    if (event.type === \"session.created\") startedSessions.add(event.properties.info.id)\n" +
		"    if (event.type === \"session.deleted\") {\n" +
		"      startedSessions.delete(event.properties.info.id)\n" +
		"      initializedSessions.delete(event.properties.info.id)\n" +
		"    }\n" +
		"  },\n" +
		"  \"experimental.chat.system.transform\": async (input, output) => {\n" +
		"    if (!input.sessionID || initializedSessions.has(input.sessionID)) return\n" +
		"    initializedSessions.add(input.sessionID)\n" +
		"    startedSessions.delete(input.sessionID)\n" +
		"    const protocol = " + prompt + "\n" +
		"    if (output.system.length === 0) output.system.push(protocol)\n" +
		"    else output.system.splice(0, 1, `${output.system[0]}\\n\\n${protocol}`)\n" +
		"  },\n" +
		"})\n"
	return writeFileAtomically(path, []byte(content), 0o644)
}

func (a *OpenCodeAdapter) removeSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	existing, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		return err
	}
	if !bytes.Contains(existing, []byte(opencodeManagedMarker)) {
		return nil
	}
	return os.Remove(path)
}
