package agent

import (
	"crypto/sha256"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// managedAgentMarker identifies a file this tool owns. Removal only deletes
// files carrying it, so an agent the user wrote by hand survives, even if it
// happens to share a name with one of ours.
const managedAgentMarker = "GRAPHIT_MANAGED_AGENT"

// InstallManagedAgent writes one delegated role into the host's agent
// directory, in that host's own document format.
//
// It mirrors the managed-skill installer, including the content hash cache:
// without it every periodic sync rewrites identical files and wakes the file
// watcher for nothing.
func InstallManagedAgent(projectDir, agentName string, descriptor AgentDescriptor) error {
	adapter := GetAdapter(agentName)
	if adapter == nil {
		return fmt.Errorf("unknown Agent: %s", agentName)
	}
	base, ok := folderBase(adapter)
	if !ok {
		return fmt.Errorf("unsupported adapter type for agent installation")
	}
	if base.cfg.UnsupportedTypes["agent"] || base.cfg.AgentsDir == "" {
		return nil
	}
	if err := validateSkillName(descriptor.Name); err != nil {
		return err
	}

	fileName, content, err := AgentDocument(agentName, descriptor)
	if err != nil {
		return err
	}
	agentDir := filepath.Join(projectDir, base.cfg.RootDirName, base.cfg.AgentsDir)
	if err := os.MkdirAll(agentDir, 0o755); err != nil {
		return fmt.Errorf("creating agent dir: %w", err)
	}

	target := filepath.Join(agentDir, fileName)
	hashFile := agentHashCachePath(projectDir, base.cfg.RootDirName, descriptor.Name)
	newHash := fmt.Sprintf("%x", sha256.Sum256([]byte(content)))
	if cached, err := os.ReadFile(hashFile); err == nil && strings.TrimSpace(string(cached)) == newHash {
		if _, err := os.Stat(target); err == nil {
			return nil
		}
	}
	if err := os.WriteFile(target, []byte(content), 0o644); err != nil {
		return fmt.Errorf("writing agent %s: %w", descriptor.Name, err)
	}
	if err := os.MkdirAll(filepath.Dir(hashFile), 0o755); err == nil {
		_ = os.WriteFile(hashFile, []byte(newHash), 0o644)
	}
	return nil
}

// RemoveManagedAgent deletes a role this tool installed. A file without the
// managed marker is left alone: the user may have replaced it deliberately.
func RemoveManagedAgent(projectDir, agentName, roleName string) error {
	adapter := GetAdapter(agentName)
	if adapter == nil {
		return fmt.Errorf("unknown Agent: %s", agentName)
	}
	base, ok := folderBase(adapter)
	if !ok || base.cfg.AgentsDir == "" {
		return nil
	}
	agentDir := filepath.Join(projectDir, base.cfg.RootDirName, base.cfg.AgentsDir)
	_ = os.Remove(agentHashCachePath(projectDir, base.cfg.RootDirName, roleName))

	for _, ext := range []string{".md", ".toml"} {
		target := filepath.Join(agentDir, roleName+ext)
		data, err := os.ReadFile(target)
		if err != nil {
			continue
		}
		if !strings.Contains(string(data), managedAgentMarker) {
			continue
		}
		if err := os.Remove(target); err != nil && !os.IsNotExist(err) {
			return fmt.Errorf("removing agent %s: %w", roleName, err)
		}
	}
	return nil
}

func agentHashCachePath(projectDir, rootDirName, roleName string) string {
	adapterKey := strings.TrimPrefix(rootDirName, ".")
	return brand.ProjectRuntimePath(projectDir, "cache", "agents", adapterKey, roleName)
}
