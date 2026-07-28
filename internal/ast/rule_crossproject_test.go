package ast

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// Four situations, four tools, and the skill used to describe only one of them.
// The row that gets missed is the sibling project: it already has its own graph,
// so it needs no import and no context — just its path. Missing that row costs an
// import that re-indexes a graph that already exists, or sends the agent to read
// files because nothing told it there was a graph to query.
func TestASTRuleContentDistinguishesSiblingsFromImportedContexts(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	for _, want := range []string{
		"Code That Is Not In This Repository",
		"a sibling project in the ecosystem",
		brand.MCPToolName("cluster", "projects") + "(project_dir",
		"Do **not** import a project that is registered in the ecosystem",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("skill does not separate ecosystem siblings from imported contexts: missing %q", want)
		}
	}
}

// A wrong project_dir does not error — it answers about a different codebase, or
// returns nothing and reads exactly like "that code does not exist". So the path
// has to come from a tool, never from memory.
func TestASTRuleContentWarnsThatAWrongProjectDirFailsSilently(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	if !strings.Contains(content, "does not raise an error") {
		t.Error("skill does not warn that a wrong project_dir fails silently")
	}
}

// The unconditional clause has to cover other projects as well, or the agent reads
// "query the graph first" as applying only to the repository it is sitting in.
func TestMandateTriggerExtendsGraphFirstToOtherProjects(t *testing.T) {
	t.Parallel()
	trigger := MandateTrigger()

	if !strings.Contains(trigger, "holds for OTHER projects too") {
		t.Error("mandate does not extend the graph-first rule to sibling projects")
	}
}
