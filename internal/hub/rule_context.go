package hub

import "github.com/graphit-labs/graphit-code/internal/hubartifact"

// InstalledRuleContext returns the instruction bodies of Hub rule artifacts
// claimed by a project. Rules are read from their authoritative installed
// artifact rather than copied into an Agent-specific rules directory.
func InstalledRuleContext(projectDir string) (string, error) {
	return hubartifact.InstalledRuleContext(projectDir)
}
