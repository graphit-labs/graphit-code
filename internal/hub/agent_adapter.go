package hub

import (
	"fmt"

	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
	"github.com/graphit-labs/graphit-code/internal/paths"
)

type AgentAdapter interface {
	Sync(installedArtifacts map[string]map[string]string, pp *paths.ProjectPaths, projectID string) error
	Remove(pp *paths.ProjectPaths, installedArtifacts map[string]map[string]string) error
}

func getAgentAdapter(agentName string) (AgentAdapter, error) {
	a := agent.GetAdapter(agentName)
	if a == nil {
		return nil, fmt.Errorf("unsupported Agent: %q (supported: %v)", agentName, agent.SupportedAgents())
	}
	return a, nil
}

// FilterSupportedAgents returns only the Agents from the input list that have
// a registered adapter. Unknown/invalid entries are silently dropped.
func FilterSupportedAgents(agents []string) []string {
	if len(agents) == 0 {
		return nil
	}
	var result []string
	for _, name := range agents {
		if agent.GetAdapter(name) != nil {
			result = append(result, name)
		}
	}
	return result
}
