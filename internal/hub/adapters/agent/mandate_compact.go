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
		"Each agent and subagent follows the enabled Graphit modules below.",
		"Match the current action; read its skill immediately before first use. Reuse loaded instructions for the same project/overrides; reload only needed skills after target/overrides change or compaction loses them. Never preload all skills.",
		"Use Graphit MCP before native discovery, including cluster neighbors: their returned `dir` is `project_dir` for target module reads. If a required tool is unavailable, record the limitation and use default native tools; never substitute the Graphit CLI.",
		"Pass `ai_optimized: true` when supported. Use narrow queries, compact results and selected sources; stop when evidence answers. Reuse fresh evidence; expand for concrete gaps.",
		"Memory: facts/lessons; Task: sessions, analysis/decisions and work state; Knowledge: contracts; AST: code; Hub: cluster-first discovery. New questions at any stage trigger needed recall; reuse sufficient evidence.",
		"`project_dir` is call-local. Persist project identity and relative paths in shared content, never a machine-specific checkout root; resolve it again on each host.",
		"Hooks load mandatory memory and restore routing at lifecycle boundaries. After interruptions, corrections or handoff, resume durable session/task state and revise changed requests before execution.",
		"Checkpoint each independently reportable work unit in the active Task and session. Close delivered sessions explicitly. After the final task update, the adapter stop hook dispatches a full sync asynchronously; do not duplicate it, wait for it, or sync after every edit.",
		// You own every delegate you open, including dismissing it. The rule is
		// written for the normal case — a host that runs subagents — because
		// writing it for the exception is what leaves delegates stranded.
		//
		// The fallback clause is what makes this safe on a host with no subagent:
		// the role document is installed either way, so the work still happens,
		// just in this window. Without it the rule would simply fail there.
		"Delegate recall to `" + brand.Brand + "-scout`, impact review to `" + brand.Brand + "-tracker` and transcription to `" + brand.Brand + "-scribe`. A delegate stays open: it reports without closing anything and waits. Send later questions to the one that owns them — unclear state, an expectation that did not hold, a why — rather than investigating yourself; dismiss it explicitly when done, or it waits forever. Acceptance, checkpointing and session lifecycle are never delegated. Exception: where your host cannot run them, read that role's agents-directory document and perform it yourself.",
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
