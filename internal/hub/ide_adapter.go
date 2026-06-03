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

// FilterSupportedIDEs returns only the IDEs from the input list that have
// a registered adapter. Unknown/invalid entries are silently dropped.
func FilterSupportedIDEs(ides []string) []string {
	if len(ides) == 0 {
		return nil
	}
	var result []string
	for _, name := range ides {
		if ide.GetAdapter(name) != nil {
			result = append(result, name)
		}
	}
	return result
}

// SupportedIDEs returns the list of IDE names that have registered adapters.
func SupportedIDEs() []string {
	return ide.SupportedIDEs()
}

