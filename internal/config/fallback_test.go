package config

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// TestFallbackIDEAndCLIAgree pins the invariant that makes two constants safe instead of two
// literals: FallbackCLI must be the CLI that CLIForIDE pairs with FallbackIDE.
//
// Without this, changing the default IDE and forgetting the CLI leaves a machine whose default
// IDE is one vendor's and whose default CLI is another's. Nothing fails in that state — the two
// resolution paths simply disagree, and the symptom surfaces much later as an agent invocation
// against a binary the operator never chose.
func TestFallbackIDEAndCLIAgree(t *testing.T) {
	paired := CLIForIDE(FallbackIDE)
	if paired == "" {
		t.Fatalf("CLIForIDE(%q) is empty: the fallback IDE is not one CLIForIDE knows", FallbackIDE)
	}
	if paired != FallbackCLI {
		t.Fatalf("CLIForIDE(%q) = %q but FallbackCLI = %q; the two defaults disagree", FallbackIDE, paired, FallbackCLI)
	}
}

// The fallback is reached only when nothing else has an opinion. Every layer above it still wins,
// which is what keeps changing the default from overriding anyone's configuration.
func TestFallbackIDEIsTheLastResortOnly(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

	if got := ResolveIDE("", nil, nil); got != FallbackIDE {
		t.Fatalf("with nothing configured, ResolveIDE = %q, want %q", got, FallbackIDE)
	}

	t.Setenv(brand.EnvVar("IDE"), "cursor")
	if got := ResolveIDE("", nil, nil); got != "cursor" {
		t.Fatalf("the environment must outrank the fallback, got %q", got)
	}
	if got := ResolveIDE("codex", nil, nil); got != "codex" {
		t.Fatalf("an explicit flag must outrank everything, got %q", got)
	}
}
