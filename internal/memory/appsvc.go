package memory

import (
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/store"
)

// MemoryInsertOpts is a view-agnostic DTO for validated memory insertion.
type MemoryInsertOpts struct {
	Title     string
	Content   string
	Type      string
	Tags      string
	Scope     string
	Important bool
	Mandatory bool
}

// MemoryAppService centralises memory operations shared across views (CLI, MCP, UI).
type MemoryAppService struct {
	projectDir string
}

func NewMemoryAppService(projectDir string) *MemoryAppService {
	return &MemoryAppService{projectDir: projectDir}
}

func (s *MemoryAppService) NewMemorySvc(userScope bool) (*MemoryService, error) {
	var scope MemoryScope
	var scopeID string

	if userScope {
		scope = MemoryScopeUser
		userID, err := UserScopeID()
		if err != nil {
			return nil, fmt.Errorf("cannot determine user identity: %w", err)
		}
		scopeID = userID
	} else {
		scope = MemoryScopeProject
		projectID, err := store.EnsureProjectID(s.projectDir)
		if err != nil {
			return nil, err
		}
		scopeID = projectID
	}

	ms, _ := NewMemoryStore()
	svc := NewMemoryService(scope, scopeID, ms)
	if err := svc.EnsureInitialised(); err != nil {
		_ = err
	}
	return svc, nil
}

func (s *MemoryAppService) InsertValidated(opts MemoryInsertOpts) (string, error) {
	if opts.Type != "" && !ValidMemoryType(opts.Type) {
		return "", fmt.Errorf("invalid memory type %q — valid types: convention, correction, decision, tension, fact, skill", opts.Type)
	}

	userScope := opts.Scope == "user"
	svc, err := s.NewMemorySvc(userScope)
	if err != nil {
		return "", err
	}
	defer func() { _ = svc.Close() }()

	tags := ParseTags(opts.Tags)

	slug, err := svc.AddMemory(opts.Title, opts.Content, MemoryOpts{
		Important: opts.Important,
		Mandatory: opts.Mandatory,
		Type:      MemoryType(opts.Type),
		Tags:      tags,
	})
	if err != nil {
		return "", err
	}

	return slug, nil
}

func ParseTags(tagsCSV string) []string {
	if tagsCSV == "" {
		return nil
	}
	var tags []string
	for _, t := range strings.Split(tagsCSV, ",") {
		t = strings.TrimSpace(t)
		if t != "" {
			tags = append(tags, t)
		}
	}
	return tags
}
