package ide

import (
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// ModuleMandateTrigger is resident routing, not a second copy of the skill.
// Keep concrete request shapes here; procedures and edge cases belong in the
// skill loaded after a trigger matches.
func ModuleMandateTrigger(heading, skillName, domain, alwaysClause string, triggers, tools []string) string {
	var b strings.Builder
	b.WriteString("\n# " + heading + "\n")
	b.WriteString("When the next action involves " + domain + ", read `" + skillName + "` once before that action and use its Graphit MCP tools.\n")
	if len(triggers) > 0 {
		b.WriteString("Triggers:\n")
		for _, trigger := range triggers {
			b.WriteString("- " + trigger + "\n")
		}
	}
	if len(tools) > 0 {
		b.WriteString("Core tools: ")
		for i, tool := range tools {
			if i > 0 {
				b.WriteString(", ")
			}
			b.WriteString("`" + brand.MCPToolName(tool) + "`")
		}
		b.WriteString(". The skill routes the remaining tools.\n")
	}
	if strings.TrimSpace(alwaysClause) != "" {
		b.WriteString(alwaysClause + "\n")
	}
	return b.String()
}

func mandatePreamble() string {
	return strings.Join([]string{
		"Graphit is the project knowledge and code-navigation layer.",
		"For each action, match only the current action against the module triggers below. If one matches, read that skill once in the session immediately before acting; do not preload unrelated skills or reread one already loaded.",
		"Within a matched domain, prefer Graphit MCP over native search, file walking, web/model knowledge, or IDE memory. This applies to every agent and subagent. If the required Graphit tool is unavailable in the current agent, continue with that agent's default native tools. Do not substitute the Graphit CLI for MCP.",
		"Adapter hooks load mandatory memory and reassert this routing at supported lifecycle boundaries. They cannot classify semantic intent, so these triggers still apply after interruptions, corrections, compaction, handoff, and resumed work.",
		"The daemon indexes writes asynchronously. Use `" + brand.MCPToolName("sync") + "` only when proven cross-module freshness is required or before completing code-changing work; do not sync after every edit.",
	}, "\n") + "\n"
}
