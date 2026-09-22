package hub

import (
	"context"
	"fmt"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/projectlock"
)

// The lockfile format lives in internal/projectlock, a leaf package.
//
// It moved out of here because the packages that have to read a project's membership —
// ast, knowledge, store — cannot import this one: hub imports them. The format is data
// and has no reason to sit behind a service layer.
//
// These are ALIASES, not wrappers, so `hub.Lockfile` and `projectlock.Lockfile` are the
// same type and every existing caller keeps working. New code should prefer the
// projectlock package directly.
type (
	ArtifactType         = projectlock.ArtifactType
	ProjectIdentity      = projectlock.ProjectIdentity
	Lockfile             = projectlock.Lockfile
	LockfileArtifactMeta = projectlock.ArtifactMeta
)

const (
	TypeAgent     = projectlock.TypeAgent
	TypeRule      = projectlock.TypeRule
	TypeWorkflow  = projectlock.TypeWorkflow
	TypeSkill     = projectlock.TypeSkill
	TypeKnowledge = projectlock.TypeKnowledge
	TypeAST       = projectlock.TypeAST
	TypeMCP       = projectlock.TypeMCP
	TypeCommand   = projectlock.TypeCommand
	TypePower     = projectlock.TypePower
	TypeLanguage  = projectlock.TypeLanguage
)

func LoadLockfile(path string) (*Lockfile, error) { return projectlock.Load(path) }

func SaveLockfile(path string, lf *Lockfile) error { return projectlock.Save(path, lf) }

// SetProjectClusterLabel updates the portable project lock first and then its
// local global-lock projection. The Hub registry is synchronized by lifecycle
// and publication paths, where an authenticated registry context is available.
func SetProjectClusterLabel(projectDir, expectedProjectID, key, value string) error {
	return mutateProjectCluster(projectDir, expectedProjectID, func(lf *Lockfile) error {
		return projectlock.SetClusterLabel(lf, key, value)
	})
}

// UnsetProjectClusterLabel removes a key from both the project authority and
// the local discovery projection.
func UnsetProjectClusterLabel(projectDir, expectedProjectID, key string) error {
	return mutateProjectCluster(projectDir, expectedProjectID, func(lf *Lockfile) error {
		return projectlock.UnsetClusterLabel(lf, key)
	})
}

func mutateProjectCluster(projectDir, expectedProjectID string, mutate func(*Lockfile) error) error {
	if strings.TrimSpace(projectDir) == "" {
		return fmt.Errorf("project_dir is required")
	}
	lockPath := filepath.Join(projectDir, brand.LockFileName())
	lf, err := LoadLockfile(lockPath)
	if err != nil {
		return fmt.Errorf("cannot load project lockfile: %w", err)
	}
	if lf == nil {
		return fmt.Errorf("cannot load project lockfile: lockfile not found")
	}
	if lf.Project.ID == "" {
		return fmt.Errorf("project has no ID")
	}
	if expectedProjectID != "" && expectedProjectID != lf.Project.ID {
		return fmt.Errorf("project ID mismatch: lock has %q, request has %q", lf.Project.ID, expectedProjectID)
	}
	before := projectlock.NormalizeCluster(lf.Project.Cluster)
	if err := mutate(lf); err != nil {
		return err
	}
	if err := SaveLockfile(lockPath, lf); err != nil {
		return err
	}
	mgr, err := NewGlobalLockManager()
	if err == nil {
		err = mgr.RegisterProject(lf.Project.ID, projectDir,
			WithProjectName(lf.Project.Name),
			WithProjectDescription(lf.Project.Description),
			WithProjectCluster(lf.Project.Cluster),
		)
	}
	if err == nil {
		return nil
	}
	// Do not report a successful mutation with a stale local projection. Restore
	// the portable authority best-effort and surface the projection failure.
	lf.Project.Cluster = before
	_ = SaveLockfile(lockPath, lf)
	return fmt.Errorf("project cluster projection: %w", err)
}

// SyncProjectMetadata publishes the lock's identity and discovery metadata to
// an authenticated Hub registry. It is shared by init, update and publication.
func SyncProjectMetadata(ctx context.Context, registry *RegistryManager, lf *Lockfile) error {
	if registry == nil || !registry.IsReady() || lf == nil || lf.Project.ID == "" {
		return nil
	}
	_, err := registry.UpsertProjectWithCluster(ctx, lf.Project.ID, lf.Project.Name, lf.Project.Description, lf.Project.Cluster)
	return err
}

func AddAgent(path, agent string) ([]string, error) {
	lf, err := LoadLockfile(path)
	if err != nil {
		return nil, err
	}
	if lf == nil {
		return nil, fmt.Errorf("lockfile not found — run '%s init' first", brand.BinName())
	}

	agentLower := strings.ToLower(agent)
	for _, existing := range lf.Agents {
		if existing == agentLower {
			return lf.Agents, nil
		}
	}
	lf.Agents = append(lf.Agents, agentLower)
	return lf.Agents, SaveLockfile(path, lf)
}

func RemoveAgent(path, agent string) ([]string, error) {
	lf, err := LoadLockfile(path)
	if err != nil || lf == nil {
		return nil, err
	}

	agentLower := strings.ToLower(agent)
	updated := make([]string, 0, len(lf.Agents))
	for _, existing := range lf.Agents {
		if existing != agentLower {
			updated = append(updated, existing)
		}
	}
	lf.Agents = updated
	return lf.Agents, SaveLockfile(path, lf)
}

var validIDRe = regexp.MustCompile(`^[a-zA-Z0-9._@/\-]+$`)

func ValidateArtifactID(id string) error {
	if id == "" {
		return fmt.Errorf("artifact ID must not be empty")
	}
	if strings.Contains(id, "..") || strings.HasPrefix(id, "/") || strings.Contains(id, "//") {
		return fmt.Errorf("invalid artifact ID (path traversal): %q", id)
	}
	if !validIDRe.MatchString(id) {
		return fmt.Errorf("invalid characters in artifact ID: %q", id)
	}
	return nil
}
