package hub

import (
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

var ideIndependentTypes = map[ArtifactType]bool{
	TypeAST:       true,
	TypeKnowledge: true,
	TypeLanguage:  true,
	TypeFramework: true,
}

func ReconcileManagedArtifacts(registry *RegistryManager, lockfilePath string) error {
	if registry == nil || !registry.IsReady() {
		return nil
	}

	lf, err := LoadLockfile(lockfilePath)
	if err != nil || lf == nil {
		return nil
	}
	projectID := lf.Project.ID
	if projectID == "" {
		return nil
	}

	entries := registry.ListEntries("")
	if len(entries) == 0 {
		return nil
	}

	dirty := false
	projectDir := filepath.Dir(lockfilePath)

	for _, entry := range entries {
		if entry.ProjectID != projectID {
			continue
		}

		artType := entry.Type

		if ideIndependentTypes[artType] {
			if reconcileEntry(lf, artType, entry) {
				dirty = true
			}
			continue
		}

		foundOnDisk := false
		for _, ideName := range lf.IDEs {
			if artifactExistsForIDE(lf, artType, entry, ideName, projectDir) {
				foundOnDisk = true
				break
			}
		}

		_ = foundOnDisk
		if reconcileEntry(lf, artType, entry) {
			dirty = true
		}
	}

	if dirty {
		return SaveLockfile(lockfilePath, lf)
	}
	return nil
}

func reconcileEntry(lf *Lockfile, artType ArtifactType, entry *Entry) bool {
	if lf.Artifacts[artType] == nil {
		lf.Artifacts[artType] = make(map[string]*LockfileArtifactMeta)
	}

	existing := lf.Artifacts[artType][entry.ID]
	if existing == nil {
		lf.Artifacts[artType][entry.ID] = &LockfileArtifactMeta{
			Version:  entry.Latest,
			Origin:   "managed",
			RemoteID: entry.ID,
		}
		return true
	}

	if existing.Origin == "" {
		existing.Origin = "managed"
		if existing.RemoteID == "" {
			existing.RemoteID = entry.ID
		}
		return true
	}

	return false
}

func artifactExistsForIDE(lf *Lockfile, artType ArtifactType, entry *Entry, ideName, projectDir string) bool {
	if ideIndependentTypes[artType] {
		return true
	}

	if lf.Artifacts[artType] != nil {
		if _, exists := lf.Artifacts[artType][entry.ID]; exists {
			return true
		}
	}

	rootDir := ideRootDir(ideName)
	if rootDir == "" {
		return false
	}

	typeDir := ideTypeDir(artType)
	if typeDir == "" {
		return false
	}

	artPath := filepath.Join(projectDir, rootDir, typeDir, entry.ID)
	if _, err := os.Stat(artPath); err == nil {
		return true
	}

	return false
}

func ideRootDir(ideName string) string {
	roots := map[string]string{
		"antigravity": ".agents",
		"cursor":      ".cursor",
		"claude":      ".claude",
		"kiro":        ".kiro",
		"codex":       ".codex",
		"opencode":    ".opencode",
		"gemini":      ".gemini",
	}
	return roots[ideName]
}

func ideTypeDir(artType ArtifactType) string {
	dirs := map[ArtifactType]string{
		TypeRule:     "rules",
		TypeSkill:    "skills",
		TypeAgent:    "agents",
		TypeCommand:  "commands",
		TypeWorkflow: "workflows",
	}
	return dirs[artType]
}

func ReconcileManagedArtifactsFromDir(registry *RegistryManager, projectDir string) error {
	lockfilePath := filepath.Join(projectDir, brand.LockFileName())
	return ReconcileManagedArtifacts(registry, lockfilePath)
}
