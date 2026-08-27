package hub

import (
	"fmt"
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
