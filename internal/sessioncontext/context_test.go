package sessioncontext

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

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

	for name, context := range map[string]string{
		"coordinator": coordinator,
		"tracker":     tracker,
		"scribe":      scribe,
	} {
		for _, want := range []string{"Persistence boundary for every module", "only non-sensitive content inherent to the project", "private user memory", "otherwise do not persist it"} {
			if !strings.Contains(context, want) {
				t.Fatalf("%s lost the cross-module persistence boundary %q", name, want)
			}
		}
	}
}
