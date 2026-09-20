package sessioncontext

import (
	"errors"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
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

// Handing a role the whole router defeats the point of delegating to it: the
// performer pays for rules covering tools its role will never touch.
func TestMandateRouterCarriesOnlyTheRoleModules(t *testing.T) {
	t.Parallel()

	coordinator := loadMandateContextForRole("", nil, sessionhook.Role{})
	for _, tag := range []string{"<task_rule>", "<mem_rule>", "<ast_rule>", "<hub_rule>", "<doc_rule>"} {
		if !strings.Contains(coordinator, tag) {
			t.Fatalf("a coordinator needs every enabled module, missing %s", tag)
		}
	}

	tracker := loadMandateContextForRole("", nil, sessionhook.RoleTracker)
	for _, want := range []string{"<ast_rule>", "<doc_rule>", "<task_rule>"} {
		if !strings.Contains(tracker, want) {
			t.Fatalf("tracker lost a module it uses: %s", want)
		}
	}
	for _, unwanted := range []string{"<mem_rule>", "<hub_rule>"} {
		if strings.Contains(tracker, unwanted) {
			t.Fatalf("tracker carries %s, a module its role does not use", unwanted)
		}
	}

	scribe := loadMandateContextForRole("", nil, sessionhook.RoleScribe)
	for _, unwanted := range []string{"<ast_rule>", "<doc_rule>", "<hub_rule>"} {
		if strings.Contains(scribe, unwanted) {
			t.Fatalf("scribe carries %s, a module its role does not use", unwanted)
		}
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
