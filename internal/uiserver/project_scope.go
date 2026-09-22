package uiserver

import (
	"fmt"
	"net/http"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/hubaccess"
)

// ProjectScope is the UI boundary between a checkout and a remote Hub project.
// Exactly one address is populated; a remote project ID is never treated as a
// directory by downstream handlers.
type ProjectScope struct {
	ProjectDir string
	ProjectID  string
}

func (s ProjectScope) Remote() bool { return s.ProjectID != "" }

func (s ProjectScope) Key() string {
	if s.Remote() {
		return "hub:" + s.ProjectID
	}
	return "workspace:" + s.ProjectDir
}

func resolveProjectScope(r *http.Request, defaultProjectDir string) (ProjectScope, error) {
	projectDir := strings.TrimSpace(r.URL.Query().Get("project_dir"))
	projectID := strings.TrimSpace(r.URL.Query().Get("project_id"))
	if projectDir != "" && projectID != "" {
		return ProjectScope{}, fmt.Errorf("project_dir and project_id are mutually exclusive")
	}
	if projectID != "" {
		if err := hubaccess.ValidateProjectID(projectID); err != nil {
			return ProjectScope{}, fmt.Errorf("invalid project_id: %w", err)
		}
		return ProjectScope{ProjectID: projectID}, nil
	}
	if projectDir == "" {
		projectDir = strings.TrimSpace(defaultProjectDir)
	}
	if projectDir == "" {
		return ProjectScope{}, fmt.Errorf("project_dir or project_id is required")
	}
	return ProjectScope{ProjectDir: projectDir}, nil
}
