package agent

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

const deepCodeNotifyKey = ".deepcode/hooks/graphit-notify"

// DeepCodeAdapter targets the Deep Code agent recommended by DeepSeek. Deep
// Code exposes a completion notification callback, not a general lifecycle hook
// API, so bootstrap/checkpoint instructions are maintained in AGENTS.md and the
// callback dispatches the final asynchronous sync.
type DeepCodeAdapter struct {
	*FolderBasedAdapter
}

func NewDeepCodeAdapter() *DeepCodeAdapter {
	return &DeepCodeAdapter{NewFolderBasedAdapter(FolderConfig{
		RootDirName:      ".deepcode",
		SkillsDir:        "skills",
		HookFilePath:     "{active_project_dir}/.deepcode/settings.json",
		MCPFilePath:      "{active_project_dir}/.deepcode/settings.json",
		UnsupportedTypes: map[string]bool{"command": true, "agent": true},
		FileTypes: map[string]FileMode{
			"rule": {Mode: "file", Ext: "md"}, "skill": {Mode: "folder"},
		},
	})}
}

func (a *DeepCodeAdapter) Sync(installed map[string]map[string]string, pp *paths.ProjectPaths, projectID string) error {
	if err := a.validateNotifyOwnership(pp.ActiveProjectDir); err != nil {
		return err
	}
	if err := a.FolderBasedAdapter.Sync(installed, pp, projectID); err != nil {
		return err
	}
	if err := a.syncDeepCodeInstructions(pp.ActiveProjectDir, installed); err != nil {
		return err
	}
	return a.syncSessionStartHook(pp.ActiveProjectDir)
}

func (a *DeepCodeAdapter) Remove(pp *paths.ProjectPaths, installed map[string]map[string]string) error {
	if err := a.FolderBasedAdapter.Remove(pp, installed); err != nil {
		return err
	}
	if err := a.removeDeepCodeInstructions(pp.ActiveProjectDir); err != nil {
		return err
	}
	return a.removeSessionStartHook(pp.ActiveProjectDir)
}

func (a *DeepCodeAdapter) notifyScriptRelativePath() string {
	ext := ".sh"
	if runtime.GOOS == "windows" {
		ext = ".cmd"
	}
	return deepCodeNotifyKey + ext
}

func (a *DeepCodeAdapter) validateNotifyOwnership(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	root, err := readJSONObjectIfExists(path)
	if err != nil || root == nil {
		return err
	}
	desired := a.notifyScriptPath(projectDir)
	previous, _ := readDeepCodeNotifyManifest(projectDir)
	if notify, ok := root["notify"].(string); ok && notify != "" && notify != desired && notify != previous {
		return fmt.Errorf("deep Code notify is already configured as %q; Graphit will not overwrite it", notify)
	}
	return nil
}

func (a *DeepCodeAdapter) syncSessionStartHook(projectDir string) error {
	scriptPath := a.notifyScriptPath(projectDir)
	var content string
	if runtime.GOOS == "windows" {
		content = "@echo off\r\nstart \"\" /b " + quoteWindowsCommandArgument(brand.BinNameWindows()) + " _session-hook --format no-output --sync >nul 2>&1\r\n"
	} else {
		content = "#!/bin/sh\n" + quoteHookCommandArgument(runtime.GOOS, brand.BinName()) + " _session-hook --format no-output --sync </dev/null >/dev/null 2>&1 &\n"
	}
	if err := writeFileAtomically(scriptPath, []byte(content), 0o755); err != nil {
		return err
	}
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	root, err := readJSONObject(path)
	if err != nil {
		return err
	}
	root["notify"] = scriptPath
	if err := writeJSONObject(path, root); err != nil {
		return err
	}
	return writeDeepCodeNotifyManifest(projectDir, scriptPath)
}

func (a *DeepCodeAdapter) removeSessionStartHook(projectDir string) error {
	path, err := resolveConfiguredPath(a.cfg.HookFilePath, projectDir)
	if err != nil {
		return err
	}
	root, err := readJSONObjectIfExists(path)
	if err != nil {
		return err
	}
	if root != nil {
		previous, _ := readDeepCodeNotifyManifest(projectDir)
		if notify, _ := root["notify"].(string); notify == a.notifyScriptPath(projectDir) || notify != "" && notify == previous {
			delete(root, "notify")
			if err := writeOrRemoveJSONObject(path, root); err != nil {
				return err
			}
		}
	}
	_ = os.Remove(a.notifyScriptPath(projectDir))
	_ = os.Remove(deepCodeNotifyManifestPath(projectDir))
	hooksDir := filepath.Join(projectDir, ".deepcode", "hooks")
	if entries, err := os.ReadDir(hooksDir); err == nil && len(entries) == 0 {
		_ = os.Remove(hooksDir)
	}
	return nil
}

func (a *DeepCodeAdapter) notifyScriptPath(projectDir string) string {
	return filepath.Join(canonicalProjectPath(projectDir), filepath.FromSlash(a.notifyScriptRelativePath()))
}

func deepCodeNotifyManifestPath(projectDir string) string {
	return brand.ProjectRuntimePath(projectDir, "cache", "hooks", "deepcode-notify.json")
}

func readDeepCodeNotifyManifest(projectDir string) (string, error) {
	data, err := os.ReadFile(deepCodeNotifyManifestPath(projectDir))
	if os.IsNotExist(err) {
		return "", nil
	}
	if err != nil {
		return "", err
	}
	var path string
	if err := json.Unmarshal(data, &path); err != nil {
		return "", err
	}
	return path, nil
}

func writeDeepCodeNotifyManifest(projectDir, path string) error {
	data, err := json.Marshal(path)
	if err != nil {
		return err
	}
	return writeFileAtomically(deepCodeNotifyManifestPath(projectDir), append(data, '\n'), 0o644)
}

func (a *DeepCodeAdapter) syncDeepCodeInstructions(projectDir string, installed map[string]map[string]string) error {
	var block strings.Builder
	block.WriteString("# Graphit lifecycle compatibility\n\n")
	block.WriteString(sessionhook.Protocol())
	block.WriteString("\n\n")
	block.WriteString(sessionhook.CoreInvariant())
	block.WriteString("\n\n")
	block.WriteString(sessionhook.UnitCompletionReminder())
	block.WriteString("\n\nDeep Code exposes only a completion `notify` callback. Apply the bootstrap and checkpoint instructions above at the corresponding semantic boundaries; the callback dispatches final sync.\n")

	ids := make([]string, 0, len(installed))
	for id := range installed {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	for _, id := range ids {
		artifact := installed[id]
		artType := artifact["type"]
		if artType != "rule" && artType != "agent" {
			continue
		}
		source := a.findCanonicalSource(artType, artifact["path"])
		if source == "" {
			continue
		}
		data, err := os.ReadFile(source)
		if err != nil {
			continue
		}
		fmt.Fprintf(&block, "\n## Graphit Hub %s: %s\n\n%s\n", artType, id, strings.TrimSpace(string(data)))
	}
	return reconcileMarkdownManagedBlock(filepath.Join(projectDir, ".deepcode", "AGENTS.md"), "deepcode", block.String())
}

func (a *DeepCodeAdapter) removeDeepCodeInstructions(projectDir string) error {
	return reconcileMarkdownManagedBlock(filepath.Join(projectDir, ".deepcode", "AGENTS.md"), "deepcode", "")
}

func reconcileMarkdownManagedBlock(path, name, content string) error {
	start := "<!-- " + brand.ManagedBlockMarker() + " START: " + name + " -->"
	end := "<!-- " + brand.ManagedBlockMarker() + " END: " + name + " -->"
	existing, err := os.ReadFile(path)
	if err != nil && !os.IsNotExist(err) {
		return err
	}
	base := string(existing)
	if i := strings.Index(base, start); i >= 0 {
		if j := strings.Index(base[i+len(start):], end); j >= 0 {
			base = strings.TrimSpace(base[:i] + base[i+len(start)+j+len(end):])
		}
	}
	content = strings.TrimSpace(content)
	if content != "" {
		managed := start + "\n" + content + "\n" + end
		if strings.TrimSpace(base) != "" {
			base = strings.TrimSpace(base) + "\n\n" + managed
		} else {
			base = managed
		}
	}
	base = strings.TrimSpace(base)
	if base == "" {
		if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
			return err
		}
		return nil
	}
	return writeFileAtomically(path, []byte(base+"\n"), 0o644)
}
