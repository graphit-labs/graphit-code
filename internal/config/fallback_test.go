package config

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// TestFallbackAgentAndCLIAgree pins the invariant that makes two constants safe instead of two
// literals: FallbackCLI must be the CLI that CLIForAgent pairs with FallbackAgent.
//
// Without this, changing the default Agent and forgetting the CLI leaves a machine whose default
// Agent is one vendor's and whose default CLI is another's. Nothing fails in that state — the two
// resolution paths simply disagree, and the symptom surfaces much later as an agent invocation
// against a binary the operator never chose.
func TestFallbackAgentAndCLIAgree(t *testing.T) {
	paired := CLIForAgent(FallbackAgent)
	if paired == "" {
		t.Fatalf("CLIForAgent(%q) is empty: the fallback Agent is not one CLIForAgent knows", FallbackAgent)
	}
	if paired != FallbackCLI {
		t.Fatalf("CLIForAgent(%q) = %q but FallbackCLI = %q; the two defaults disagree", FallbackAgent, paired, FallbackCLI)
	}
}

// The fallback is reached only when nothing else has an opinion. Every layer above it still wins,
// which is what keeps changing the default from overriding anyone's configuration.
func TestFallbackAgentIsTheLastResortOnly(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

	if got := ResolveAgent("", nil, nil); got != FallbackAgent {
		t.Fatalf("with nothing configured, ResolveAgent = %q, want %q", got, FallbackAgent)
	}

	t.Setenv(ConfigEnvVar("agent"), "cursor")
	if got := ResolveAgent("", nil, nil); got != "cursor" {
		t.Fatalf("the environment must outrank the fallback, got %q", got)
	}
	if got := ResolveAgent("codex", nil, nil); got != "codex" {
		t.Fatalf("an explicit flag must outrank everything, got %q", got)
	}
}
