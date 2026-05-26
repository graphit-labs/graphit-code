package hub

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	gitmod "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/oklog/ulid/v2"
)

type LockfileArtifactMeta struct {
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
}

func (m *LockfileArtifactMeta) IsHubInstalled() bool {
	if m.Origin == "publish" {
		return false
	}
	if m.RemoteID != "" {
		return true
	}
	switch m.Origin {
	case "hub", "managed", "link":
		return true
	}
	return false
}

type ProjectIdentity struct {
	ID          string `json:"id"`
	Name        string `json:"name"`
	Description string `json:"description"`
}

type Lockfile struct {
	Project   ProjectIdentity                                   `json:"project"`
	IDEs      []string                                          `json:"ides,omitempty"`
	Artifacts map[ArtifactType]map[string]*LockfileArtifactMeta `json:"artifacts"`
	Config    map[string]any                                    `json:"config,omitempty"`
}

func LoadLockfile(path string) (*Lockfile, error) {
	data, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("reading lockfile: %w", err)
	}

	lf := &Lockfile{
		Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
	}
	if err := json.Unmarshal(data, lf); err != nil {
		return nil, fmt.Errorf("parsing lockfile: %w", err)
	}
	if lf.Artifacts == nil {
		lf.Artifacts = make(map[ArtifactType]map[string]*LockfileArtifactMeta)
	}

	return lf, nil
}

func SaveLockfile(path string, lf *Lockfile) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return fmt.Errorf("creating lockfile directory: %w", err)
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
		lf.Artifacts = make(map[ArtifactType]map[string]*LockfileArtifactMeta)
	}

	data, err := json.MarshalIndent(lf, "", "  ")
	if err != nil {
		return fmt.Errorf("serializing lockfile: %w", err)
	}

	return os.WriteFile(path, data, 0o644)
}

func resolveProjectIdentity(projectDir string) ProjectIdentity {
	name := filepath.Base(projectDir)

	out, err := gitmod.Default().RunOutput(projectDir, "remote", "get-url", "origin")
	if err == nil {
		url := out
		if url != "" {

			if m := regexp.MustCompile(`^git@[^:]+:(.+?)(?:\.git)?$`).FindStringSubmatch(url); m != nil {
				name = m[1]
			} else if m := regexp.MustCompile(`^https?://.+?/(.+?)(?:\.git)?$`).FindStringSubmatch(url); m != nil {

				name = m[1]
			}
		}
	}

	return ProjectIdentity{
		ID:   ulid.Make().String(),
		Name: name,
	}
}

func AddIDE(path, ide string) ([]string, error) {
	lf, err := LoadLockfile(path)
	if err != nil {
		return nil, err
	}
	if lf == nil {
		return nil, fmt.Errorf("lockfile not found — run '%s init' first", brand.BinName())
	}

	ideLower := strings.ToLower(ide)
	for _, existing := range lf.IDEs {
		if existing == ideLower {
			return lf.IDEs, nil
		}
	}
	lf.IDEs = append(lf.IDEs, ideLower)
	return lf.IDEs, SaveLockfile(path, lf)
}

func RemoveIDE(path, ide string) ([]string, error) {
	lf, err := LoadLockfile(path)
	if err != nil || lf == nil {
		return nil, err
	}

	ideLower := strings.ToLower(ide)
	updated := make([]string, 0, len(lf.IDEs))
	for _, existing := range lf.IDEs {
		if existing != ideLower {
			updated = append(updated, existing)
		}
	}
	lf.IDEs = updated
	return lf.IDEs, SaveLockfile(path, lf)
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
