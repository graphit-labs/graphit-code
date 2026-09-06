package projectlock

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	gitmod "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
	"github.com/oklog/ulid/v2"
)

// ArtifactType is the family an artifact belongs to. It is the first key of the
// artifact map, so it lives here with the format rather than with the Hub logic.
type ArtifactType string

const (
	TypeAgent     ArtifactType = "agent"
	TypeRule      ArtifactType = "rule"
	TypeWorkflow  ArtifactType = "workflow"
	TypeSkill     ArtifactType = "skill"
	TypeKnowledge ArtifactType = "knowledge"
	TypeAST       ArtifactType = "ast"
	TypeMCP       ArtifactType = "mcp"
	TypeCommand   ArtifactType = "command"
	TypePower     ArtifactType = "power"
	TypeLanguage  ArtifactType = "language"
)

// VersionLocal is the version of an artifact that did not come from the Hub: a local
// import, or a link to a sibling project. It is a real value rather than an empty
// string so that "no version recorded" and "there is no version to record" stay
// distinguishable.
const VersionLocal = "local"

// Origin values. A local import and a link are both origin-less as far as the Hub is
// concerned, but they resolve differently, so they are named apart.
const (
	OriginHub     = "hub"
	OriginManaged = "managed"
	OriginPublish = "publish"
	OriginLink    = "link"
	OriginLocal   = "local"
)

type ArtifactMeta struct {
	Version     string   `json:"version"`
	Hash        string   `json:"hash,omitempty"`
	InstalledBy []string `json:"installed_by,omitempty"`
	Members     []string `json:"members,omitempty"`
	ProjectID   string   `json:"project_id,omitempty"`
	Alias       string   `json:"alias,omitempty"`
	RemoteID    string   `json:"remote_id,omitempty"`
	Origin      string   `json:"origin,omitempty"`

	LinkSource       string `json:"link_source,omitempty"`
	RequestedVersion string `json:"requested_version,omitempty"`

	SourcePath string `json:"source_path,omitempty"`
}

func (m *ArtifactMeta) IsHubInstalled() bool {
	if m.Origin == OriginPublish {
		return false
	}
	if m.RemoteID != "" {
		return true
	}
	switch m.Origin {
	case OriginHub, OriginManaged, OriginLink:
		return true
	}
	return false
}

// IsLocal reports whether an artifact came from this machine rather than the Hub — a
// local import or a link to a sibling. Those resolve from SourcePath.
func (m *ArtifactMeta) IsLocal() bool {
	return m.Origin == OriginLocal || m.Origin == OriginLink || m.Version == VersionLocal
}

type ProjectIdentity struct {
	ID          string `json:"id"`
	Name        string `json:"name"`
	Description string `json:"description"`
	// Ephemeral marks a throwaway workspace — a live search session — which gets a
	// lockfile so the agent CLI can find itself, but must not acquire stores of its
	// own. See store.IsEphemeralProject.
	Ephemeral bool `json:"ephemeral,omitempty"`
}

type Lockfile struct {
	Project   ProjectIdentity                           `json:"project"`
	IDEs      []string                                  `json:"ides,omitempty"`
	Artifacts map[ArtifactType]map[string]*ArtifactMeta `json:"artifacts"`
	Config    map[string]any                            `json:"config,omitempty"`
}

// RelSourcePath turns an absolute directory into what SourcePath stores: a
// slash-separated path relative to the project.
//
// It falls back to the absolute path when a relative one cannot exist — different
// Windows volumes, which is the only case filepath.Rel refuses. Such a lockfile does
// not travel, but neither does the arrangement it describes.
func RelSourcePath(projectDir, absSource string) string {
	rel, err := filepath.Rel(projectDir, absSource)
	if err != nil {
		return filepath.ToSlash(absSource)
	}
	return filepath.ToSlash(rel)
}

// SourceDir resolves a recorded SourcePath back to an absolute directory.
//
// An absolute stored value is returned as-is, which covers the cross-volume fallback
// above.
func SourceDir(projectDir, stored string) string {
	if stored == "" {
		return ""
	}
	p := filepath.FromSlash(stored)
	if filepath.IsAbs(p) {
		return p
	}
	return filepath.Clean(filepath.Join(projectDir, p))
}

func Load(path string) (*Lockfile, error) {
	data, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("reading lockfile: %w", err)
	}

	lf := &Lockfile{
		Artifacts: make(map[ArtifactType]map[string]*ArtifactMeta),
	}
	if err := json.Unmarshal(data, lf); err != nil {
		return nil, fmt.Errorf("parsing lockfile: %w", err)
	}
	if lf.Artifacts == nil {
		lf.Artifacts = make(map[ArtifactType]map[string]*ArtifactMeta)
	}

	return lf, nil
}

func Save(path string, lf *Lockfile) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return fmt.Errorf("creating lockfile directory: %w", err)
	}
	guardPath := brand.ProjectRuntimePath(filepath.Dir(path), "identity.lock")
	guard, err := lockfile.Acquire(guardPath, 5*time.Second)
	if err != nil {
		return fmt.Errorf("locking project identity: %w", err)
	}
	defer guard.Release()

	existing, err := Load(path)
	if err != nil {
		return err
	}
	if existing != nil && existing.Project.ID != "" {
		if lf.Project.ID != "" && lf.Project.ID != existing.Project.ID {
			return fmt.Errorf("project identity is immutable: existing %q, requested %q", existing.Project.ID, lf.Project.ID)
		}
		lf.Project.ID = existing.Project.ID
	}

	if lf.Project.ID == "" {
		existingName := lf.Project.Name
		existingDesc := lf.Project.Description
		lf.Project = resolveProjectIdentity(filepath.Dir(path))
		if existingName != "" {
			lf.Project.Name = existingName
		}
		if existingDesc != "" && lf.Project.Description == "" {
			lf.Project.Description = existingDesc
		}
	}
	if lf.Artifacts == nil {
		lf.Artifacts = make(map[ArtifactType]map[string]*ArtifactMeta)
	}

	data, err := json.MarshalIndent(lf, "", "  ")
	if err != nil {
		return fmt.Errorf("serializing lockfile: %w", err)
	}

	tmp, err := os.CreateTemp(filepath.Dir(path), ".graphit-lock-*.tmp")
	if err != nil {
		return fmt.Errorf("creating temporary lockfile: %w", err)
	}
	tmpPath := tmp.Name()
	defer func() { _ = os.Remove(tmpPath) }()
	if err := tmp.Chmod(0o644); err != nil {
		_ = tmp.Close()
		return err
	}
	if _, err := tmp.Write(data); err != nil {
		_ = tmp.Close()
		return fmt.Errorf("writing temporary lockfile: %w", err)
	}
	if err := tmp.Sync(); err != nil {
		_ = tmp.Close()
		return fmt.Errorf("syncing temporary lockfile: %w", err)
	}
	if err := tmp.Close(); err != nil {
		return fmt.Errorf("closing temporary lockfile: %w", err)
	}
	if err := os.Rename(tmpPath, path); err != nil {
		return fmt.Errorf("replacing lockfile: %w", err)
	}
	return nil
}

// EnsureIdentity creates the minimal project lockfile when a stateful operation first needs it.
// Concurrent callers serialize through Save and all observe the same immutable ULID.
func EnsureIdentity(path string) (*Lockfile, error) {
	lf, err := Load(path)
	if err != nil {
		return nil, err
	}
	if lf != nil && lf.Project.ID != "" {
		return lf, nil
	}
	if lf == nil {
		lf = &Lockfile{Artifacts: make(map[ArtifactType]map[string]*ArtifactMeta)}
	}
	if err := Save(path, lf); err != nil {
		return nil, err
	}
	return Load(path)
}

func resolveProjectIdentity(projectDir string) ProjectIdentity {
	name := filepath.Base(projectDir)

	out, err := gitmod.Default().RunOutput(projectDir, "remote", "get-url", "origin")
	if err == nil {
		url := out
		if url != "" {
			if m := regexp.MustCompile(`^git@[^:]+:(.+?)(?:\.git)?$`).FindStringSubmatch(url); m != nil {
				name = filepath.Base(filepath.FromSlash(m[1]))
			} else if m := regexp.MustCompile(`^https?://.+?/(.+?)(?:\.git)?$`).FindStringSubmatch(url); m != nil {
				name = filepath.Base(filepath.FromSlash(m[1]))
			}
		}
	}
	name = strings.ToLower(strings.TrimSpace(name))
	name = regexp.MustCompile(`[^a-z0-9._-]+`).ReplaceAllString(name, "-")
	name = strings.Trim(name, ".-_")
	if len(name) > 64 {
		name = strings.TrimRight(name[:64], ".-_")
	}
	if name == "" {
		name = "project"
	}

	return ProjectIdentity{
		ID:   ulid.Make().String(),
		Name: name,
	}
}
