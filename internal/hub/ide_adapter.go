package hub

import (
	"fmt"

	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
	"github.com/graphit-labs/graphit-code/internal/paths"
)

type IDEAdapter interface {
	Sync(installedArtifacts map[string]map[string]string, pp *paths.ProjectPaths, projectID string) error
	Remove(pp *paths.ProjectPaths, installedArtifacts map[string]map[string]string) error
}

func getIDEAdapter(ideName string) (IDEAdapter, error) {
	a := ide.GetAdapter(ideName)
	if a == nil {
		return nil, fmt.Errorf("unsupported IDE: %q (supported: %v)", ideName, ide.SupportedIDEs())
	}
	return a, nil
}
