package hub

import (
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

func HubRuleContent() string {
	return strings.Join([]string{
		"# Graphit Hub",
		"",
		"Use this skill before relying on model knowledge or web search for an external library, framework, API, agent, reusable artifact, Graphit configuration, or another ecosystem project.",
		"",
		"## Lookup order",
		"",
		"1. For ecosystem projects, call `" + brand.MCPToolName("hub", "projects") + "` or `" + brand.MCPToolName("cluster", "projects") + "`; use the returned project id/path with that project's AST or wiki tools.",
		"2. For artifacts, call `" + brand.MCPToolName("hub", "list") + "` when installed inventory may answer, otherwise `" + brand.MCPToolName("hub", "search") + "`. Search is discovery only.",
		"3. Read the selected artifact with `" + brand.MCPToolName("hub", "show") + "`. Install it only when the current task needs local use, then query its installed AST/knowledge context.",
		"4. If no relevant Hub artifact exists, use primary vendor documentation or web research and state that this is the fallback.",
		"",
		"Never infer another project's path, read its files directly, or treat a search title as content. Registry calls without a project operate on globally installed artifacts; project linking is explicit.",
		"",
		"## Mutations",
		"",
		"`" + brand.MCPToolName("hub", "install") + "`, `" + brand.MCPToolName("hub", "update") + "`, and `" + brand.MCPToolName("hub", "uninstall") + "` manage installed artifacts. `" + brand.MCPToolName("hub", "link") + "`/`" + brand.MCPToolName("hub", "unlink") + "` change project claims. `" + brand.MCPToolName("hub", "submit") + "` publishes reusable work. Do not perform these state changes merely to inspect an artifact.",
		"",
		"Use `" + brand.MCPToolName("hub", "type", "path") + "` before creating a reusable rule/skill/command/agent. Use `" + brand.MCPToolName("config", "list") + "`/`get` to inspect configuration and `set`/`unset` only when requested. `" + brand.MCPToolName("cluster", "get") + "`/`set`/`unset` manage project grouping.",
		"",
		"Tool index: `graphit_hub_search`, `graphit_hub_show`, `graphit_hub_content`, `graphit_hub_list`, `graphit_hub_install`, `graphit_hub_uninstall`, `graphit_hub_update`, `graphit_hub_link`, `graphit_hub_unlink`, `graphit_hub_submit`, `graphit_hub_projects`, `graphit_hub_type_path`, `graphit_cluster_projects`, `graphit_cluster_get`, `graphit_cluster_set`, `graphit_cluster_unset`, `graphit_config_list`, `graphit_config_get`, `graphit_config_set`, `graphit_config_unset`.",
	}, "\n") + "\n"
}

func MandateTrigger() string {
	return ide.ModuleMandateTrigger(
		"Hub Discovery",
		hubSkillName,
		"external systems, reusable artifacts, ecosystem projects, or Graphit configuration",
		"",
		[]string{
			"using an external library, framework, SDK, API, service, or repository",
			"answering from model knowledge or web search about an external system",
			"resolving another project, shared solution, reusable artifact, or artifact destination",
			"inspecting or changing Graphit Hub, cluster, or framework configuration",
		},
		[]string{"hub_search", "hub_show", "hub_list", "hub_projects", "config_get"},
	)
}
