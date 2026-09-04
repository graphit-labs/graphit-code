package hub

import (
	"strings"
	"testing"
)

func TestHubSkillCompactContract(t *testing.T) {
	t.Parallel()
	content := HubRuleContent()
	for _, want := range []string{
		"before relying on model knowledge or web search", "Search is discovery only", "Never infer another project's path", "primary vendor documentation", "artifact metadata", "installed rule, skill, command, or agent files",
		"graphit_hub_search", "graphit_hub_show", "graphit_hub_content", "graphit_hub_list", "graphit_hub_install",
		"graphit_hub_uninstall", "graphit_hub_update", "graphit_hub_link", "graphit_hub_unlink", "graphit_hub_submit",
		"graphit_hub_projects", "graphit_hub_type_path", "graphit_cluster_projects", "graphit_cluster_get", "graphit_cluster_set",
		"graphit_cluster_unset", "graphit_config_list", "graphit_config_get", "graphit_config_set", "graphit_config_unset",
	} {
		if !strings.Contains(content, want) {
			t.Fatalf("compact Hub skill missing %q", want)
		}
	}
	if len(content) > 5000 {
		t.Fatalf("Hub skill exceeded its token budget: %d bytes", len(content))
	}
	mandate := MandateTrigger()
	if len(mandate) > 1400 {
		t.Fatalf("Hub mandate exceeded its resident token budget: %d bytes", len(mandate))
	}
}
