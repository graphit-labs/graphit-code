package prep

import (
	"crypto/sha256"
	"fmt"
	"os"
	"path/filepath"
	"sort"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
	"github.com/graphit-labs/graphit-code/internal/livesearch"
	"github.com/graphit-labs/graphit-code/internal/store"
)

type ProjectArtifact struct {
	livesearch.Artifact
	Name        string `json:"name"`
	Description string `json:"description,omitempty"`
	ProjectName string `json:"project_name"`
	Unavailable string `json:"unavailable,omitempty"`
	// Paths are server-resolved capabilities, never accepted from a request.
	path       string
	projectDir string
	isFile     bool
	context    store.ContextRecord
}

func (a ProjectArtifact) BelongsTo(projectDir string) bool {
	return projectDir != "" && filepath.Clean(a.projectDir) == filepath.Clean(projectDir)
}

func instanceID(dir string) string {
	return fmt.Sprintf("%x", sha256.Sum256([]byte(filepath.Clean(dir))))[:24]
}

// ProjectCatalog enumerates registered instances. A copied/stale registration
// whose current lock names another project is never accepted as that identity.
func ProjectCatalog(agentName string) ([]ProjectArtifact, error) {
	if err := ValidateAgent(agentName); err != nil {
		return nil, err
	}
	manager, err := hub.NewGlobalLockManager()
	if err != nil {
		return nil, err
	}
	registry, err := manager.Load()
	if err != nil {
		return nil, err
	}
	result := []ProjectArtifact{}
	for id, project := range registry.Projects {
		if project == nil {
			continue
		}
		for _, instance := range project.Instances {
			if store.ProjectID(instance.Dir) != id {
				continue
			}
			items, err := projectArtifacts(agentName, id, instance)
			if err != nil {
				return nil, err
			}
			result = append(result, items...)
		}
	}
	sort.Slice(result, func(i, j int) bool {
		a, b := result[i], result[j]
		return a.ProjectName+a.InstanceID+a.Type+a.ProjectKind+a.ID < b.ProjectName+b.InstanceID+b.Type+b.ProjectKind+b.ID
	})
	return result, nil
}

func projectArtifacts(agentName, id string, instance hub.InstanceEntry) ([]ProjectArtifact, error) {
	dir := instance.Dir
	name := instance.Name
	if name == "" {
		name = filepath.Base(dir)
	}
	result := []ProjectArtifact{}
	add := func(artifactID, kind, typ, path, version string, isFile bool, rec store.ContextRecord) {
		result = append(result, ProjectArtifact{Artifact: livesearch.Artifact{ID: artifactID, Type: typ, Version: version, Source: "project", ProjectID: id, InstanceID: instanceID(dir), ProjectKind: kind}, Name: artifactID, ProjectName: name, Description: instance.Description, path: path, projectDir: dir, isFile: isFile, context: rec})
	}
	for _, typ := range []string{store.KindAST, store.KindKnowledge} {
		path := store.ASTProjectDir(dir)
		if typ == store.KindKnowledge {
			path = store.KnowledgeProjectDir(dir)
		}
		if entries, err := os.ReadDir(path); err == nil && len(entries) > 0 {
			add(name, "own_context", typ, dir, "local", false, store.ContextRecord{})
		}
		for contextName, rec := range store.ListContexts(dir, typ) {
			add(contextName, "installed_context", typ, rec.SourcePath, rec.Version, false, rec)
		}
	}
	adapter := agent.GetAdapter(canonicalAgent(agentName))
	if adapter == nil {
		return result, nil
	}
	seen := map[string]bool{}
	for _, local := range adapter.ScanLocal(dir) {
		key := local.Type + "/" + local.ID
		if seen[key] {
			continue
		}
		seen[key] = true
		add(local.ID, "file", local.Type, local.Path, "local", local.IsFile, store.ContextRecord{})
	}
	// Adapter scans may omit installed packages until their projections are synced.
	lf, err := hub.LoadLockfile(filepath.Join(dir, brand.LockFileName()))
	if err != nil {
		return nil, err
	}
	if lf != nil {
		flat, err := hub.InstalledArtifacts(agentName, dir)
		if err != nil {
			return nil, err
		}
		for typ, entries := range lf.Artifacts {
			if string(typ) == store.KindAST || string(typ) == store.KindKnowledge {
				continue
			}
			for artifactID, meta := range entries {
				if meta == nil || seen[string(typ)+"/"+artifactID] {
					continue
				}
				record := flat[artifactID]
				if record["type"] != string(typ) {
					continue
				}
				path := record["path"]
				info, err := os.Stat(path)
				if err != nil {
					continue
				}
				add(artifactID, "file", string(typ), path, meta.Version, !info.IsDir(), store.ContextRecord{})
			}
		}
	}
	for i := range result {
		item := &result[i]
		if item.ProjectKind == "file" {
			switch item.Type {
			case "skill", "agent", "rule", "command", "workflow":
			default:
				item.Unavailable = "This local artifact type cannot be prepared in an isolated investigation yet."
			}
		}
	}
	return result, nil
}

func resolveProjectArtifact(agentName string, ref livesearch.Artifact) (ProjectArtifact, error) {
	catalog, err := ProjectCatalog(agentName)
	if err != nil {
		return ProjectArtifact{}, err
	}
	for _, item := range catalog {
		if item.Artifact == ref {
			if item.Unavailable != "" {
				return ProjectArtifact{}, fmt.Errorf("%s: %s", ref.ID, item.Unavailable)
			}
			return item, nil
		}
	}
	return ProjectArtifact{}, fmt.Errorf("project artifact %q is no longer available for this project instance and agent; refresh the catalogue", ref.ID)
}
