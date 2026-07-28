package hub

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// config has no skill of its own by design: a sixth mandate block costs context
// in every session, and the situations that need these tools — a module that
// returns nothing, a docs tree that is not where you assumed — are already
// framework questions, which is what this skill covers.
func TestHubRuleContentTeachesConfiguration(t *testing.T) {
	t.Parallel()
	content := HubRuleContent()

	for _, action := range []string{"list", "get", "set", "unset"} {
		if !strings.Contains(content, brand.MCPToolName("config", action)) {
			t.Errorf("hub skill never mentions %s", brand.MCPToolName("config", action))
		}
	}
}

// Three facts that turn config into a diagnostic instead of a guess: docs_dir
// defaults to the whole project, an environment variable outranks both config
// files and shows up in neither, and modules.<name> reads backwards.
func TestHubRuleContentExplainsTheConfigTraps(t *testing.T) {
	t.Parallel()
	content := HubRuleContent()

	for _, want := range []string{
		"knowledge.docs_dir",
		brand.EnvPrefix() + "_KNOWLEDGE_DOCS_DIR",
		"modules.dream",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("hub skill does not mention %q", want)
		}
	}
	if !strings.Contains(content, "opt-in") {
		t.Error("hub skill does not warn that some modules are off unless explicitly enabled")
	}
}
