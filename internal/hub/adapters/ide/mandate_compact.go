package ide

import (
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// SysReminder is appended to MCP results when a host needs an out-of-band
// reminder. Hook-delivered context currently makes that unnecessary.
var SysReminder = ""

var canonicalTriggerOrder = []string{
	"task_rule",
	"mem_rule",
	"ast_rule",
	"hub_rule",
	"doc_rule",
}

func assembleTriggers(triggers map[string]string) string {
	parts := make([]string, 0, len(triggers))
	seen := make(map[string]bool, len(triggers))
	for _, tag := range canonicalTriggerOrder {
		if content, ok := triggers[tag]; ok {
			parts = append(parts, "<"+tag+">"+content+"</"+tag+">")
			seen[tag] = true
		}
	}
	extra := make([]string, 0, len(triggers))
	for tag := range triggers {
		if !seen[tag] {
			extra = append(extra, tag)
		}
	}
	sort.Strings(extra)
	for _, tag := range extra {
		parts = append(parts, "<"+tag+">"+triggers[tag]+"</"+tag+">")
	}
	return strings.Join(parts, "\n")
}

func mandateTag() string {
	return strings.ToUpper(brand.Brand) + "_SYSTEM_MANDATE"
}

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
		"When a Graphit tool exposes `ai_optimized`, explicitly pass `true`; do not rely on its default.",
		"Adapter hooks load mandatory memory and reassert this routing at supported lifecycle boundaries. They cannot classify semantic intent, so these triggers still apply after interruptions, corrections, compaction, handoff, and resumed work.",
		"Whenever the smallest independently reportable unit finishes, update the active Graphit task immediately with what landed and what comes next; do not wait for the overall task to end or write Markdown task state.",
		"The daemon indexes writes asynchronously. After the final task-management update, every agent or subagent completion must dispatch a full Graphit sync asynchronously through its adapter stop hook and must not wait for it. Do not sync after every edit.",
	}, "\n") + "\n"
}

// MandateContext renders the resident Graphit router for hook injection.
// The map is keyed by stable module tags so output remains deterministic even
// when callers discover enabled modules in a different order.
func MandateContext(triggers map[string]string) string {
	body := assembleTriggers(triggers)
	if strings.TrimSpace(body) == "" {
		return ""
	}
	return "<" + mandateTag() + ">\n" +
		strings.TrimSpace(mandatePreamble()+"\n"+body) + "\n</" + mandateTag() + ">"
}
