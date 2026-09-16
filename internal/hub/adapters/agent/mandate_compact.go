package agent

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
	b.WriteString("Before " + domain + ", read `" + skillName + "` if its instructions are not in the current context; then use its Graphit MCP tools.\n")
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
		"Graphit routes project work through the enabled modules below; each agent and subagent follows the same contract.",
		"Match the current action; read its skill immediately before first use. Reuse loaded instructions for the same project/overrides; reload only needed skills after those change or compaction loses content. Never preload all skills.",
		"Use Graphit MCP before native discovery in a matched domain. If a required tool is unavailable, use default native tools and record the limitation; never substitute the Graphit CLI for MCP.",
		"Pass `ai_optimized: true` when supported. Start with narrow queries and compact results, read selected sources, and stop when evidence answers the question. Reuse ids, sources and fresh results across modules; expand only to close a concrete gap.",
		"Memory: constraints; Knowledge: contracts; AST: code; Hub: ecosystem/artifacts, cluster first; Task: plan/execution/handoff. Use only needed modules.",
		"`project_dir` is call-local. Persist project identity and relative paths in shared content, never a machine-specific checkout root; resolve it again on each host.",
		"Hooks load mandatory memory and restore routing at lifecycle boundaries. After interruptions, corrections or handoff, resume from durable task state and revise affected plans before execution.",
		"Checkpoint each independently reportable work unit in the active Task with evidence and the next step. After the final task update, the adapter stop hook dispatches a full sync asynchronously; do not duplicate it, wait for it, or sync after every edit.",
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
