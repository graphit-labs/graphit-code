package hub

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestHubRuleContent(t *testing.T) {
	t.Parallel()
	content := HubRuleContent()
	if content == "" {
		t.Error("expected non-empty content")
	}
	for _, want := range []string{
		"Hub Discovery Rule",
		"Artifact Types",
		"Project Ecosystem",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("expected content to contain %q", want)
		}
	}
}

// The mandate orders the agent to check the Hub via MCP before trusting its own
// knowledge. Until this test existed the skill never named the search tool, so
// the order arrived without the means to obey it.
func TestHubRuleContentTeachesEveryHubTool(t *testing.T) {
	t.Parallel()
	content := HubRuleContent()

	for _, action := range []string{
		"search", "show", "list", "install", "uninstall", "update",
		"link", "unlink", "submit", "projects", "type-path", "content",
	} {
		if !strings.Contains(content, brand.MCPToolName("hub", action)) {
			t.Errorf("hub skill never mentions %s", brand.MCPToolName("hub", action))
		}
	}

	// The ecosystem is how the agent resolves "the auth service" into a path it
	// can query, and how it learns what the user filed this project as.
	for _, action := range []string{"projects", "get", "set", "unset"} {
		if !strings.Contains(content, brand.MCPToolName("cluster", action)) {
			t.Errorf("hub skill never mentions %s", brand.MCPToolName("cluster", action))
		}
	}
}

// hub_list and hub_search have no project_dir: the registry is global. The skill
// used to tell the agent to pass one, and to claim hub_list reports what this
// project has installed — which it cannot, having no notion of a project.
func TestHubRuleContentDoesNotInventAProjectDirOnRegistryTools(t *testing.T) {
	t.Parallel()
	content := HubRuleContent()

	for _, bad := range []string{
		brand.MCPToolName("hub", "list") + "(project_dir",
		brand.MCPToolName("hub", "search") + "(project_dir",
	} {
		if strings.Contains(content, bad) {
			t.Errorf("skill passes a project_dir to a registry tool: %q", bad)
		}
	}
}

func TestMandateTriggerNamesTheSearchAndEcosystemTools(t *testing.T) {
	t.Parallel()
	trigger := MandateTrigger()

	// The agent picks MCP over a native tool before it opens the skill, so the
	// mandate has to name the tool that decision depends on.
	for _, want := range []string{
		brand.MCPToolName("hub", "search"),
		brand.MCPToolName("cluster", "projects"),
		brand.MCPToolName("cluster", "get"),
	} {
		if !strings.Contains(trigger, want) {
			t.Errorf("mandate never names %s", want)
		}
	}
}

func TestInstallRule(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	err := InstallRule(dir, "claude")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestInstallSkill(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	err := InstallSkill(dir, "claude")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestRemoveRule(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	// Install then remove
	_ = InstallRule(dir, "claude")
	err := RemoveRule(dir, "claude")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestRemoveSkill(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	// Install then remove
	_ = InstallSkill(dir, "claude")
	err := RemoveSkill(dir, "claude")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

// An agent that does not know project_dir is optional will keep asking the user for a
// path it does not need, and will never reach a globally installed artifact.
func TestHubRuleContentTeachesTheProjectlessInstall(t *testing.T) {
	t.Parallel()
	content := HubRuleContent()

	for _, want := range []string{
		"Installing with NO project at all",
		"`project_dir` is optional",
		"qualified identifier",
		brand.MCPToolName("hub", "install") + "(id: ",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("hub skill never explains %q", want)
		}
	}
}

// The path-keyed shape is the part an agent has to know in advance: it decides how the
// answer is read, and a skill is routinely more than one file.
func TestHubRuleContentExplainsThePathKeyedContentShape(t *testing.T) {
	t.Parallel()
	content := HubRuleContent()

	for _, want := range []string{
		"keyed by the artifact-relative path",
		"canonical",
		brand.MCPToolName("hub", "content") + "(id: ",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("hub skill never explains %q", want)
		}
	}

	// And it must point the mounted types at the tools that can actually read them,
	// rather than leaving an agent to conclude the artifact is empty.
	for _, want := range []string{
		brand.MCPToolRef("ast", "source"),
		brand.MCPToolRef("wiki", "source"),
	} {
		if !strings.Contains(content, want) {
			t.Errorf("hub skill never points a mounted artifact at %s", want)
		}
	}
}

// The global scope is read-only for stores, and a project cannot borrow a global install.
// Both are constraints an agent will otherwise discover as a confusing failure.
func TestHubRuleContentStatesTheGlobalScopeLimits(t *testing.T) {
	t.Parallel()
	content := HubRuleContent()

	for _, want := range []string{
		"READ-ONLY",
		"cannot reach a global install it never claimed",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("hub skill never states the limit %q", want)
		}
	}
}
