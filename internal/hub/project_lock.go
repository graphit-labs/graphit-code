package hub

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

const ProjectLockFile = "cluster.lock.json"

type ProjectLock struct {
	Version  int                          `json:"version"`
	Cluster  map[string][]string          `json:"cluster,omitempty"`
	Projects map[string]*SiblingProject   `json:"projects"`
}

type SiblingProject struct {
	Dir          string              `json:"dir"`
	Name         string              `json:"name,omitempty"`
	Description  string              `json:"description,omitempty"`
	Cluster      map[string][]string `json:"cluster,omitempty"`
	RegisteredAt string              `json:"registeredAt"`
}

func SyncProjectLock(projectDir string) {
	mgr, err := NewGlobalLockManager()
	if err != nil {
		return
	}

	lock, err := mgr.Load()
	if err != nil {
		return
	}

	currentID, currentInst := resolveCurrentProject(projectDir, lock)
	if currentID == "" {
		return
	}

	siblings := make(map[string]*SiblingProject)
	counts := make(map[string]int)

	for id, entry := range lock.Projects {
		for _, inst := range entry.Instances {
			if id == currentID && sameDir(inst.Dir, currentInst.Dir) {
				continue
			}
			if !isClusterSibling(currentInst, &inst) {
				continue
			}
			key := id
			counts[id]++
			if counts[id] > 1 {
				key = fmt.Sprintf("%s#%d", id, counts[id])
			}
			siblings[key] = &SiblingProject{
				Dir:          inst.Dir,
				Name:         inst.Name,
				Description:  inst.Description,
				Cluster:      inst.Cluster,
				RegisteredAt: inst.RegisteredAt,
			}
		}
	}

	pl := &ProjectLock{
		Version:  1,
		Cluster:  currentInst.Cluster,
		Projects: siblings,
	}

	dotDir := filepath.Join(projectDir, brand.DotDir())
	if err := os.MkdirAll(dotDir, 0o755); err != nil {
		return
	}
	data, err := json.MarshalIndent(pl, "", "  ")
	if err != nil {
		return
	}
	_ = os.WriteFile(filepath.Join(dotDir, ProjectLockFile), data, 0o644)
}

func resolveCurrentProject(projectDir string, lock *GlobalHubLock) (string, *InstanceEntry) {
	absDir, err := filepath.Abs(projectDir)
	if err != nil {
		absDir = projectDir
	}
	for id, entry := range lock.Projects {
		for i := range entry.Instances {
			inst := &entry.Instances[i]
			instAbs, err := filepath.Abs(inst.Dir)
			if err != nil {
				instAbs = inst.Dir
			}
			if instAbs == absDir {
				return id, inst
			}
		}
	}
	return "", nil
}

func isClusterSibling(current, candidate *InstanceEntry) bool {
	currentHasLabels := len(current.Cluster) > 0
	candidateHasLabels := len(candidate.Cluster) > 0

	if !currentHasLabels {
		return !candidateHasLabels
	}

	if !candidateHasLabels {
		return false
	}

	for k, currentVals := range current.Cluster {
		candidateVals, ok := candidate.Cluster[k]
		if !ok {
			continue
		}
		for _, cv := range currentVals {
			for _, candV := range candidateVals {
				if cv == candV {
					return true
				}
			}
		}
	}
	return false
}

func sameDir(a, b string) bool {
	absA, _ := filepath.Abs(a)
	absB, _ := filepath.Abs(b)
	return absA == absB
}
