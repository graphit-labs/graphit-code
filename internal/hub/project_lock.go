package hub

import (
	"fmt"
	"path/filepath"
)

type SiblingProject struct {
	Dir          string              `json:"dir"`
	Name         string              `json:"name,omitempty"`
	Description  string              `json:"description,omitempty"`
	Cluster      map[string][]string `json:"cluster,omitempty"`
	RegisteredAt string              `json:"registeredAt"`
}

func GetClusterProjects(projectDir string, filterLabel ...string) (map[string]*SiblingProject, error) {
	mgr, err := NewGlobalLockManager()
	if err != nil {
		return nil, fmt.Errorf("global lock: %w", err)
	}

	lock, err := mgr.Load()
	if err != nil {
		return nil, fmt.Errorf("load global lock: %w", err)
	}

	currentID, currentInst := resolveCurrentProject(projectDir, lock)
	if currentID == "" {
		return nil, fmt.Errorf("project is not initialized or registered in global lock")
	}

	var labelKey string
	if len(filterLabel) > 0 && filterLabel[0] != "" {
		labelKey = filterLabel[0]
	}

	projects := make(map[string]*SiblingProject)
	counts := make(map[string]int)

	if labelKey == "" || hasLabelKey(currentInst, labelKey) {
		counts[currentID]++
		projects[currentID] = &SiblingProject{
			Dir:          currentInst.Dir,
			Name:         currentInst.Name,
			Description:  currentInst.Description,
			Cluster:      currentInst.Cluster,
			RegisteredAt: currentInst.RegisteredAt,
		}
	}

	for id, entry := range lock.Projects {
		for _, inst := range entry.Instances {
			if sameDir(inst.Dir, projectDir) {
				continue
			}
			if labelKey != "" {
				if !sharesLabel(currentInst, &inst, labelKey) {
					continue
				}
			} else if !isClusterSibling(currentInst, &inst) {
				continue
			}
			key := id
			counts[id]++
			if counts[id] > 1 {
				key = fmt.Sprintf("%s#%d", id, counts[id])
			}
			projects[key] = &SiblingProject{
				Dir:          inst.Dir,
				Name:         inst.Name,
				Description:  inst.Description,
				Cluster:      inst.Cluster,
				RegisteredAt: inst.RegisteredAt,
			}
		}
	}
	return projects, nil
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

func hasLabelKey(inst *InstanceEntry, key string) bool {
	vals, ok := inst.Cluster[key]
	return ok && len(vals) > 0
}

func sharesLabel(current, candidate *InstanceEntry, key string) bool {
	currentVals, ok := current.Cluster[key]
	if !ok {
		return false
	}
	candidateVals, ok := candidate.Cluster[key]
	if !ok {
		return false
	}
	for _, cv := range currentVals {
		for _, candV := range candidateVals {
			if cv == candV {
				return true
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
