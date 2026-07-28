package hub

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// A sibling project registered in the ecosystem already has its own indexed graph,
// compiled wiki and memories. So exploring it is not a degraded mode: the moment
// the agent knows the path, it has every tool over there that it has here. The
// failure this protocol prevents is answering from model knowledge, guessing a
// path, or grepping an unfamiliar tree — all while an index sat right there.
func TestHubRuleContentMandatesTheCrossProjectProtocol(t *testing.T) {
	t.Parallel()
	content := HubRuleContent()

	if !strings.Contains(content, "the ecosystem comes first") {
		t.Error("hub skill has no mandatory ordering for exploring another project")
	}

	// The protocol is only actionable if it names the tools to use on the sibling —
	// code, documentation, recent change and rationale, all four.
	for _, want := range []string{
		brand.MCPToolName("ast", "search") + "(project_dir: \"<sibling dir>\"",
		brand.MCPToolName("ast", "source") + "(project_dir: \"<sibling dir>\"",
		brand.MCPToolName("knowledge", "search") + "(project_dir: \"<sibling dir>\"",
		brand.MCPToolName("wiki", "search") + "(project_dir: \"<sibling dir>\"",
		brand.MCPToolName("wiki", "log") + "(project_dir: \"<sibling dir>\"",
		brand.MCPToolName("memory", "search") + "(project_dir: \"<sibling dir>\"",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("protocol does not show how to explore the sibling: missing %q", want)
		}
	}

	// The point that makes the whole thing cheap: no install, no link, no import —
	// project_dir is just a parameter.
	if !strings.Contains(content, "`project_dir` is a parameter") {
		t.Error("protocol does not say that reaching another project is only a different parameter value")
	}
}

// Two mistakes the protocol exists to stop, both cheap to make: re-importing a
// project that already has a graph, and orienting yourself with ls or grep on its
// tree when its own graph and wiki answer better.
func TestHubRuleContentForbidsNativeExplorationOfSiblings(t *testing.T) {
	t.Parallel()
	content := HubRuleContent()

	for _, want := range []string{
		"Importing a registered sibling as an AST context",
		"to orient yourself",
		// hub_link is verbose, symlinks one artifact into THIS project, and grants
		// no access that passing project_dir does not already give.
		"to \"get access to\" a sibling",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("anti-pattern table is missing %q", want)
		}
	}
}

// The agent decides between MCP and a native tool before it opens any skill, so
// the ordering has to be in the mandate too, not only in the skill body.
func TestMandateTriggerCarriesTheEcosystemFirstOrder(t *testing.T) {
	t.Parallel()
	trigger := MandateTrigger()

	if !strings.Contains(trigger, "resolve it in the ecosystem FIRST") {
		t.Error("mandate does not tell the agent to resolve another project in the ecosystem first")
	}
}
