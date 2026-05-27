package memory

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
)

// MemoryInsertOpts is a view-agnostic DTO for validated memory insertion.
type MemoryInsertOpts struct {
	Title     string
	Content   string
	Type      string
	Tags      string
	Scope     string
	Important bool
}

// MemorySearchResult is a view-agnostic DTO for keyword search output.
type MemorySearchResult struct {
	ID    string
	Title string
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
		hash, err := UserHashFromGit()
		if err != nil {
			return nil, fmt.Errorf("cannot determine user identity: %w", err)
		}
		scopeID = hash
	} else {
		scope = MemoryScopeProject
		lockPath := filepath.Join(s.projectDir, brand.LockFileName())
		lf, err := hub.LoadLockfile(lockPath)
		if err != nil || lf == nil {
			return nil, fmt.Errorf("project not initialised at %s — run '%s init' first", s.projectDir, brand.BinName())
		}
		scopeID = lf.Project.ID
	}

	ms, _ := NewMemoryGitStore()
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
		Type:      MemoryType(opts.Type),
		Tags:      tags,
	})
	if err != nil {
		return "", err
	}

	return slug, nil
}

func (s *MemoryAppService) SearchByKeyword(term, scope string) ([]MemorySearchResult, error) {
	if scope == "" {
		scope = "project"
	}

	// NOTE: RawDir depends on CWD for "project" scope — tech debt
	origWd, _ := os.Getwd()
	_ = os.Chdir(s.projectDir)
	defer func() { _ = os.Chdir(origWd) }()

	dir := RawDir(scope)
	if dir == "" {
		return nil, nil
	}

	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}

	termLower := strings.ToLower(term)
	var results []MemorySearchResult
	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
			continue
		}
		absPath := filepath.Join(dir, e.Name())
		data, readErr := os.ReadFile(absPath)
		if readErr != nil {
			continue
		}
		if strings.Contains(strings.ToLower(string(data)), termLower) {
			title, _ := ParseMemoryMetaPublic(absPath)
			id := strings.TrimSuffix(e.Name(), ".md")
			results = append(results, MemorySearchResult{ID: id, Title: title})
		}
	}

	return results, nil
}

// ParseTags splits a comma-separated string into trimmed, non-empty tags.
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
