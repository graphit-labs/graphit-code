package hub

import (
	"context"
	"fmt"
	"os"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// ArtifactSummary is a view-agnostic DTO for hub artifact listings and search results.
type ArtifactSummary struct {
	ID          string `json:"id"`
	Name        string `json:"name"`
	Type        string `json:"type"`
	Description string `json:"description,omitempty"`
	Latest      string `json:"latest,omitempty"`
}

// HubAppService centralises hub operations shared across views (CLI, MCP, UI).
type HubAppService struct {
	projectDir string
}

func NewHubAppService(projectDir string) *HubAppService {
	return &HubAppService{projectDir: projectDir}
}

func (s *HubAppService) ResolveIDE(inputIDE string) string {
	if inputIDE != "" {
		return inputIDE
	}
	if env := os.Getenv(brand.EnvVar("IDE")); env != "" {
		return env
	}
	return "claude"
}

func (s *HubAppService) ListSummary(ctx context.Context, artifactType string) ([]ArtifactSummary, error) {
	reg, err := NewRegistryManager(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to load hub registry: %w", err)
	}

	entries := reg.ListEntries(ArtifactType(artifactType))
	if len(entries) == 0 {
		return nil, nil
	}

	summaries := make([]ArtifactSummary, 0, len(entries))
	for _, e := range entries {
		name := e.Name
		if name == "" {
			name = e.ID
		}
		summaries = append(summaries, ArtifactSummary{
			ID:          e.ID,
			Name:        name,
			Type:        string(e.Type),
			Description: e.Description,
			Latest:      e.Latest,
		})
	}
	return summaries, nil
}

func (s *HubAppService) SearchSummary(ctx context.Context, query, artifactType string) ([]ArtifactSummary, error) {
	reg, err := NewRegistryManager(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to load hub registry: %w", err)
	}

	entries := reg.SearchEntries(query, ArtifactType(artifactType))
	if len(entries) == 0 {
		return nil, nil
	}

	summaries := make([]ArtifactSummary, 0, len(entries))
	for _, e := range entries {
		name := e.Name
		if name == "" {
			name = e.ID
		}
		summaries = append(summaries, ArtifactSummary{
			ID:          e.ID,
			Name:        name,
			Type:        string(e.Type),
			Description: e.Description,
		})
	}
	return summaries, nil
}
