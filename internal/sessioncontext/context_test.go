package sessioncontext

import (
	"errors"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/memory"
)

func TestLoadMandatoryContextReadsBothAuthoritativeScopes(t *testing.T) {
	t.Parallel()

	context, loaded := loadMandatoryContextWith("/project", func(projectDir, scope string) ([]memory.MandatoryEntry, error) {
		if projectDir != "/project" {
			t.Fatalf("project dir = %q", projectDir)
		}
		return []memory.MandatoryEntry{{Title: scope + " policy", Content: "content for " + scope}}, nil
	})
	if !loaded || !strings.Contains(context, "### project memory: project policy") || !strings.Contains(context, "### user memory: user policy") {
		t.Fatalf("mandatory scopes were not rendered: loaded=%v context=%q", loaded, context)
	}
}

func TestLoadMandatoryContextFallsBackWhenAStoreCannotOpen(t *testing.T) {
	t.Parallel()

	_, loaded := loadMandatoryContextWith("/project", func(string, string) ([]memory.MandatoryEntry, error) {
		return nil, errors.New("store unavailable")
	})
	if loaded {
		t.Fatal("store failure must preserve the MCP fallback")
	}
}
