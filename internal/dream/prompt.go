package dream

import (
	"fmt"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/memory"
)

func buildDreamPrompt(projectDir, sessionID, ide string, outcomes []*memory.ConsolidationOutcome) string {
	context := buildDreamContext(projectDir, sessionID, ide)
	analysisRules := brand.ResolveModuleRule("dream", "")
	envelope := buildDreamEnvelope(projectDir, sessionID)

	var b strings.Builder
	b.WriteString(context)
	b.WriteString(buildConsolidationBriefing(outcomes))
	b.WriteString(analysisRules)
	b.WriteString(envelope)
	return b.String()
}

// buildConsolidationBriefing tells the agent what the runner already did to the
// memory store. Without it the agent re-derives the same duplicates, proposes
// changes that have already happened, and reports them as its own work.
func buildConsolidationBriefing(outcomes []*memory.ConsolidationOutcome) string {
	var b strings.Builder
	b.WriteString("## 🧹 Memory Consolidation Already Ran\n\n")

	if len(outcomes) == 0 {
		b.WriteString("No consolidation ran this session (the memory module is off, or no AI client was available).\n")
		b.WriteString("Treat the memory store as unsanitised: if you notice duplicates or contradictions while\n")
		b.WriteString("working, record them in your report under Recommendations rather than deleting anything.\n\n")
		return b.String()
	}

	b.WriteString("**Before you started, the runner sanitised the memory store deterministically.**\n")
	b.WriteString("This is already done — do not redo it, and do not report it as your own work.\n\n")
	for _, o := range outcomes {
		if o == nil {
			continue
		}
		_, _ = fmt.Fprintf(&b, "- `%s` scope: %d memories analysed, %d actions applied, %d refused, %d failed\n",
			o.Scope, o.Analysed, len(o.Applied), len(o.Skipped), len(o.Failed))
		for _, skipped := range o.Skipped {
			_, _ = fmt.Fprintf(&b, "  - refused (%s, `%s`): %s\n", skipped.Type, skipped.Kept, skipped.Skipped)
		}
	}
	b.WriteString("\nThe refused actions are the interesting ones: the runner declined them to avoid losing\n")
	b.WriteString("knowledge. Where a refusal needs judgement — a memory flagged for review, an important\n")
	b.WriteString("memory that looks obsolete — you can resolve it by **updating** the memory with better\n")
	b.WriteString("content. You still must not delete memories; removal only happens through the runner,\n")
	b.WriteString("where the content is carried into a survivor first.\n\n")
	return b.String()
}

func buildDreamContext(projectDir, sessionID, ide string) string {
	var b strings.Builder

	b.WriteString("# Dream Session — Autonomous Skill Generation & Knowledge Mining\n\n")

	_, _ = fmt.Fprintf(&b, "**Dream ID**: `%s`\n", sessionID)
	_, _ = fmt.Fprintf(&b, "**Project**: `%s`\n", projectDir)
	_, _ = fmt.Fprintf(&b, "**Started**: `%s`\n", time.Now().UTC().Format(time.RFC3339))
	_, _ = fmt.Fprintf(&b, "**IDE**: `%s`\n", ide)
	_, _ = fmt.Fprintf(&b, "**Tool**: `%s`\n\n", brand.BinName())

	b.WriteString("## Context\n\n")
	b.WriteString("You are an autonomous AI agent running locally during an idle ")
	b.WriteString("period of this project. Your mission is to mine conversation history, ")
	b.WriteString("evaluate and improve existing skills, generate new skills from recurring patterns, ")
	b.WriteString("create integration skills for external developers, and extract undocumented ")
	b.WriteString("knowledge into persistent memories.\n\n")

	b.WriteString("You are NOT making code changes. You are generating and improving **IDE artifacts** ")
	b.WriteString("(skills, rules, commands, memories) that make future conversations more effective.\n\n")

	_, _ = fmt.Fprintf(&b, "You are operating with IDE context `%s`. ", ide)
	b.WriteString("Use this when interacting with the hub, syncing rules, or any IDE-scoped operations. ")
	_, _ = fmt.Fprintf(&b, "For example: `%s sync --ide %s` or `%s hub install --ide %s`.\n\n", brand.BinName(), ide, brand.BinName(), ide)

	// Phase 1: OBSERVE
	b.WriteString("## Your Mission — 5-Phase Architecture\n\n")

	b.WriteString("### Phase 1: OBSERVE — Understand the System\n\n")
	b.WriteString("Before analyzing anything, build a complete understanding of the project:\n\n")
	_, _ = fmt.Fprintf(&b, "1. Read the project's knowledge wiki (`%s/knowledge/project/index.md`)\n", brand.DotDir())
	_, _ = fmt.Fprintf(&b, "2. Read ALL project and user memories (`%s/memory/project/index.md` and `%s/memory/user/index.md`)\n", brand.DotDir(), brand.DotDir())
	b.WriteString("3. Query the AST graph to understand code structure, public APIs, and module boundaries\n")
	b.WriteString("4. Scan installed IDE artifacts — list ALL existing skills, rules, and commands in the project\n")
	b.WriteString("5. Study architectural decisions (ADRs) in `docs/decisions/` if they exist\n")
	_, _ = fmt.Fprintf(&b, "6. Read **all** existing dream reports in `%s` for progressive continuity\n", ReportsDir(projectDir))
	b.WriteString("   - Pay special attention to **\"Recommendations for Future\"** sections\n")
	b.WriteString("   - Address pending recommendations from previous dream sessions\n\n")

	b.WriteString("### Phase 2: EXTRACT — Mine Conversation History\n\n")
	b.WriteString("A critical part of your mission is to **mine the developer's conversation history** ")
	b.WriteString("for recurring patterns, unmet needs, and knowledge gaps.\n\n")

	b.WriteString("#### How to Access Conversations\n\n")
	b.WriteString("Conversation transcripts are stored as JSONL files. Each line is a JSON object with fields:\n")
	b.WriteString("- `source`: who performed the action (USER_EXPLICIT, MODEL, SYSTEM)\n")
	b.WriteString("- `type`: the step type (USER_INPUT, PLANNER_RESPONSE, VIEW_FILE, etc.)\n")
	b.WriteString("- `content`: the text content of the step\n")
	b.WriteString("- `tool_calls`: array of tool calls made\n\n")
	b.WriteString("Read the **most recent 15–20 conversations** and systematically identify patterns.\n\n")

	b.WriteString("#### Pattern Extraction Pipeline\n\n")
	b.WriteString("Apply **semantic clustering** (group by intent, not surface words) to find:\n\n")
	b.WriteString("1. **Recurring User Requests** (≥2 occurrences → skill candidate) — The developer asks for the same type of operation in multiple sessions\n")
	b.WriteString("2. **Multi-Step Workflows** — Sequences of 3+ tool calls that consistently appear together → command candidate\n")
	b.WriteString("3. **User Corrections & Preferences** — Moments where the developer corrects the agent → convention/memory candidate\n")
	b.WriteString("4. **Undocumented Decisions** — Architectural or design decisions discussed but never memorized → memory candidate\n")
	b.WriteString("5. **Integration Patterns** — How the developer instructs the agent to work with external systems/APIs → integration skill candidate\n")
	b.WriteString("6. **Recurring Errors** — Same error appearing across conversations → signals systemic problem needing root-cause fix or skill\n")
	b.WriteString("7. **Abandoned or Incomplete Work** — Conversations where work started but wasn't finished → flag in recommendations\n\n")

	b.WriteString("#### Actions to Take\n\n")
	b.WriteString("| Pattern Found | Action |\n")
	b.WriteString("|---------------|--------|\n")
	b.WriteString("| Same task requested ≥2 times | Create a **Skill** or **Command** to automate it |\n")
	b.WriteString("| User correction / \"always do X\" | `graphit memory insert` with type `convention` or `correction` |\n")
	b.WriteString("| Repeated multi-step workflow | Create a **Command** artifact with the full sequence |\n")
	b.WriteString("| Implicit convention not codified | Create a **Rule** artifact to enforce it |\n")
	b.WriteString("| Undocumented decision or context | Create a memory (`decision`) or ADR |\n")
	b.WriteString("| Recurring error with same root cause | Create a **Skill** documenting the fix pattern |\n")
	b.WriteString("| Integration pattern with external system | Create an **Integration Skill** (see Phase 4) |\n")
	b.WriteString("| Abandoned important work | Flag in dream report under Recommendations for Future |\n\n")

	b.WriteString("### Phase 3: DIAGNOSE — Evaluate Existing Skill Effectiveness (Self-Healing Loop)\n\n")
	b.WriteString("For each installed skill in the project, apply the **4-phase self-healing loop**:\n\n")

	b.WriteString("#### Step 1: TRACE\n")
	b.WriteString("Search conversations for sessions where this skill was activated. Look for:\n")
	b.WriteString("- `view_file SKILL.md` calls followed by agent actions\n")
	b.WriteString("- Explicit mentions of the skill name in agent reasoning\n")
	b.WriteString("- Tool calls that match the skill's activation triggers\n\n")

	b.WriteString("#### Step 2: SCORE\n")
	b.WriteString("Did the agent's actions after reading the skill produce the expected outcome?\n")
	b.WriteString("- Check for user corrections AFTER skill activation (signals skill failure)\n")
	b.WriteString("- Check if the agent achieved the task successfully\n")
	b.WriteString("- Check if the agent had to deviate from the skill's instructions\n\n")

	b.WriteString("#### Step 3: DIAGNOSE\n")
	b.WriteString("If the skill failed, classify the root cause:\n\n")
	b.WriteString("| Root Cause Category | Description | Fix Strategy |\n")
	b.WriteString("|---------------------|-------------|-------------|\n")
	b.WriteString("| `UNCLEAR_INSTRUCTION` | The skill was ambiguous or vague | Add specificity, concrete examples |\n")
	b.WriteString("| `MISSING_STEP` | The skill omitted a required step | Add the missing step with context |\n")
	b.WriteString("| `WRONG_TRIGGER` | The skill activated in the wrong context | Refine activation trigger conditions |\n")
	b.WriteString("| `MISSING_EXAMPLE` | The agent didn't understand the pattern | Add concrete before/after examples |\n")
	b.WriteString("| `STALE_CONTENT` | The skill references outdated APIs/patterns | Update to current codebase state |\n")
	b.WriteString("| `INCOMPLETE_COVERAGE` | The skill covers happy path but not edge cases | Expand with edge case handling |\n\n")

	b.WriteString("#### Step 4: FIX\n")
	b.WriteString("Modify the skill **in-place** with the diagnosed improvement.\n")
	b.WriteString("Log every fix in the dream report with the before/after rationale.\n\n")

	b.WriteString("### Phase 4: CREATE — Generate New Artifacts\n\n")

	b.WriteString("#### Skill Crystallization Protocol\n\n")
	b.WriteString("For each candidate pattern from Phase 2 with ≥2 occurrences:\n\n")
	_, _ = fmt.Fprintf(&b, "1. Resolve the artifact path by calling the `%s` MCP tool (type=skill, name=<name>) — never the CLI\n", brand.MCPToolName("hub", "type-path"))
	b.WriteString("2. Create `SKILL.md` following this structure:\n")
	b.WriteString("   - **YAML frontmatter** with `name` and `description`. `name` must equal the skill's directory name — lowercase letters, digits and single separating hyphens, at most 64 characters — and `description` is at most 1024 characters.\n")
	b.WriteString("     **Quote the description.** It is prose, so it almost certainly contains `: `, and a plain YAML scalar may not: a strict parser reads the colon as a nested mapping and rejects the whole frontmatter. The skill is then not degraded, it is invisible — the IDE discovers no metadata and never offers the skill, with nothing logged to say why.\n")
	b.WriteString("     ```yaml\n")
	b.WriteString("     ---\n")
	b.WriteString("     name: error-handling-patterns\n")
	b.WriteString("     description: \"Use when: wrapping or returning errors in this project. Codifies the wrapped error pattern with context enrichment.\"\n")
	b.WriteString("     ---\n")
	b.WriteString("     ```\n")
	b.WriteString("   - **Activation Triggers** — when should the agent read this skill?\n")
	b.WriteString("   - **Instructions** — step-by-step procedure\n")
	b.WriteString("   - **Examples** — concrete before/after demonstrations\n")
	b.WriteString("   - **Common Mistakes** — pitfalls to avoid\n")
	b.WriteString("3. Include **provenance**: \"Generated from conversations: [list of conversation IDs]\"\n\n")

	b.WriteString("#### Integration Pattern Skills\n\n")
	b.WriteString("Analyze the project's public APIs, data models, and conventions to create skills that ")
	b.WriteString("teach external developers/agents how to properly integrate with this system:\n\n")
	b.WriteString("- **API Contracts** — Required request formats, response envelopes, error codes\n")
	b.WriteString("- **Authentication Flows** — How to authenticate, token refresh, session management\n")
	b.WriteString("- **Data Format Conventions** — Naming patterns, serialization, validation rules\n")
	b.WriteString("- **Error Handling Patterns** — Standard error types, retry strategies, circuit breakers\n")
	b.WriteString("- **Common Pitfalls** — Mistakes that external integrators commonly make\n\n")

	b.WriteString("Use AST queries to discover the project's public surface:\n")
	b.WriteString("```cypher\n")
	b.WriteString("MATCH (f:Function) WHERE f.is_exported = true RETURN f.name, f.path, f.docstring\n")
	b.WriteString("```\n\n")

	b.WriteString("#### Memory Generation\n\n")
	b.WriteString("For conventions, corrections, and decisions extracted in Phase 2:\n\n")
	_, _ = fmt.Fprintf(&b, "1. Use the `%s` MCP tool with proper type classification (convention/correction/decision/skill)\n",
		brand.MCPToolName("memory", "insert"))
	b.WriteString("2. **Deduplication**: search before creating, so you extend the store instead of widening it\n")
	_, _ = fmt.Fprintf(&b, "   - Search with the `%s` MCP tool first\n", brand.MCPToolName("memory", "search"))
	_, _ = fmt.Fprintf(&b, "   - If an existing memory covers the topic but is incomplete or now wrong, use `%s` to correct it rather than adding a second memory beside it\n",
		brand.MCPToolName("memory", "update"))
	b.WriteString("3. Include clear provenance: which conversation(s) the knowledge came from\n\n")

	b.WriteString("### Phase 5: VALIDATE — Reflection & Reporting\n\n")
	b.WriteString("Generate the dream report (see report structure below).\n")
	b.WriteString("Assess whether more patterns remain to be extracted → continue or signal deep sleep.\n\n")

	b.WriteString("#### Memory Corruption Prevention\n\n")
	b.WriteString("- **Never overwrite a skill** without logging the before/after rationale in the report\n")
	b.WriteString("- **Never delete an existing memory** — only update or create\n")
	b.WriteString("- **Always check for existing skills** before creating duplicates\n")
	b.WriteString("- **Include provenance** (conversation IDs) in every generated artifact\n")
	b.WriteString("- **Version all modifications**: note what changed and why in the dream report\n\n")

	return b.String()
}

func buildDreamEnvelope(projectDir, sessionID string) string {
	var b strings.Builder
	reportPath := filepath.Join(ReportsDir(projectDir), sessionID+reportExt)

	b.WriteString("### Document Everything\n\n")
	b.WriteString("Reports you write are always visible to the developer.\n\n")
	_, _ = fmt.Fprintf(&b, "Write the report to `%s`. If you cannot write files in this\n", reportPath)
	b.WriteString("environment, return it as your answer and the runner will save it there — but say so\n")
	b.WriteString("explicitly, because that also means none of the other artifacts got created.\n\n")
	b.WriteString("The runner appends its own memory-consolidation audit to whichever version survives,\n")
	b.WriteString("so do not transcribe that section yourself.\n\n")

	b.WriteString("**CRITICAL: The dream report is the primary deliverable of every session.** ")
	b.WriteString("It must be comprehensive, detailed, and self-contained — a developer reading it ")
	b.WriteString("should fully understand what conversations were analyzed, what patterns were found, ")
	b.WriteString("what skills were created or improved, and why. ")
	b.WriteString("Treat this as a professional audit report, not a summary.\n\n")

	b.WriteString("The report MUST follow this structure:\n\n")

	b.WriteString("```markdown\n")
	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "title: Dream Session %s\n", sessionID)
	_, _ = fmt.Fprintf(&b, "created: %s\n", time.Now().UTC().Format(time.RFC3339))
	b.WriteString("type: dream-report\n")
	b.WriteString("---\n\n")

	b.WriteString("# Dream Report\n\n")

	b.WriteString("## Conversations Analyzed\n\n")
	b.WriteString("List every conversation reviewed in this session:\n\n")
	b.WriteString("| # | Conversation ID | Title/Summary | Date | Patterns Found |\n")
	b.WriteString("|---|----------------|---------------|------|----------------|\n")
	b.WriteString("| 1 | `<id>` | <brief title> | <date> | <count> |\n\n")
	b.WriteString("If no conversation logs were found, state: \"No conversation logs available.\"\n\n")

	b.WriteString("## Recurring Patterns Identified\n\n")
	b.WriteString("Document ALL patterns found during conversation mining:\n\n")
	b.WriteString("| # | Pattern | Frequency | Source Conversations | Action Taken |\n")
	b.WriteString("|---|---------|-----------|---------------------|-------------|\n")
	b.WriteString("| 1 | e.g., \"User frequently asks for X\" | N times | `<id1>`, `<id2>` | Created skill / memory / rule |\n\n")

	b.WriteString("## Skills Created\n\n")
	b.WriteString("List each NEW skill generated in this session:\n\n")
	b.WriteString("| # | Name | Type | Purpose | Path | Provenance |\n")
	b.WriteString("|---|------|------|---------|------|------------|\n")
	b.WriteString("| 1 | `<name>` | skill/command/rule | <description> | <path> | Conversations: `<id1>`, `<id2>` |\n\n")

	b.WriteString("## Skills Improved (Self-Healing Loop)\n\n")
	b.WriteString("List each existing skill that was diagnosed and fixed:\n\n")
	b.WriteString("| # | Skill | Root Cause | Diagnosis | Fix Applied | Conversations Affected |\n")
	b.WriteString("|---|-------|-----------|-----------|-------------|----------------------|\n")
	b.WriteString("| 1 | `<skill-name>` | UNCLEAR_INSTRUCTION | <what was wrong> | <what was fixed> | `<id1>`, `<id2>` |\n\n")
	b.WriteString("For each improved skill, include the **before/after diff** showing what changed.\n\n")

	b.WriteString("## Integration Skills Created\n\n")
	b.WriteString("List skills created for external developers/agents:\n\n")
	b.WriteString("| # | Name | Target System | Purpose | Path |\n")
	b.WriteString("|---|------|--------------|---------|------|\n")
	b.WriteString("| 1 | `<name>` | <system/API> | <what it teaches> | <path> |\n\n")
	b.WriteString("If no integration skills were needed, state: \"No integration patterns identified in this session.\"\n\n")

	b.WriteString("## Memories Created\n\n")
	b.WriteString("List each memory created or updated based on conversation analysis:\n\n")
	b.WriteString("| # | Title | Type | Rationale | Source |\n")
	b.WriteString("|---|-------|------|-----------|--------|\n")
	b.WriteString("| 1 | `<title>` | convention/correction/decision/skill | <why this was worth memorizing> | Conversation `<id>` |\n\n")

	b.WriteString("## Artifact Health Report\n\n")
	b.WriteString("Document the evaluation of existing IDE artifacts:\n\n")
	b.WriteString("### Skills Evaluated\n\n")
	b.WriteString("| Skill | Status | Action | Details |\n")
	b.WriteString("|-------|--------|--------|--------|\n")
	b.WriteString("| `<skill-name>` | ✅ Healthy / ⚠️ Needs Fix / 🔀 Merged / ✂️ Split / ❌ Deleted | <what was done> | <reasoning> |\n\n")
	b.WriteString("### Rules & Commands Evaluated\n\n")
	b.WriteString("| Artifact | Type | Status | Action |\n")
	b.WriteString("|----------|------|--------|--------|\n")
	b.WriteString("| `<name>` | rule/command | ✅/⚠️/❌ | <what was done> |\n\n")
	b.WriteString("If no artifacts exist yet, state: \"No existing artifacts found. Created N new artifacts based on analysis.\"\n\n")

	b.WriteString("## Recommendations for Future Dreams\n\n")
	b.WriteString("List patterns and opportunities that were identified but NOT acted upon:\n\n")
	b.WriteString("### Patterns Needing More Data\n")
	b.WriteString("- <pattern with only 1 occurrence — watching for recurrence>\n\n")
	b.WriteString("### Skills Needing Re-evaluation\n")
	b.WriteString("- <skill that was fixed but needs verification after more usage>\n\n")
	b.WriteString("### Pending Integration Analysis\n")
	b.WriteString("- <external system that needs deeper analysis for integration skill>\n\n")
	b.WriteString("These recommendations will be reviewed by the next dream session. Be specific enough\n")
	b.WriteString("that a future agent can act on them without re-analyzing the entire history.\n\n")

	b.WriteString("## Addressed Recommendations from Previous Dreams\n\n")
	b.WriteString("If you addressed any recommendations from previous dream reports, list them here:\n\n")
	b.WriteString("| Source Dream | Recommendation | Status |\n")
	b.WriteString("|-------------|---------------|--------|\n")
	b.WriteString("| `<dream-id>` | <recommendation title> | ✅ Resolved / ⚠️ Partially resolved / ❌ No longer applicable |\n\n")
	b.WriteString("If no previous dreams exist or no recommendations were pending, state: \"No prior recommendations to address.\"\n")

	b.WriteString("```\n\n")

	b.WriteString("### Deep Sleep — Signal When Done\n\n")
	b.WriteString("After completing your analysis, you MUST decide whether there are further patterns to extract.\n\n")
	b.WriteString("**If you found NO more patterns to extract** (all conversations have been mined, all skills are healthy, ")
	b.WriteString("all memories are up to date), signal **deep sleep** by creating:\n\n")
	_, _ = fmt.Fprintf(&b, "Create an empty file at: `%s/dream/%s%s`\n\n", brand.DotDir(), sessionID, DeepSleepSentinelName())
	b.WriteString("This tells the daemon that this dream cycle is complete. No more sessions will run until ")
	b.WriteString("the developer makes new changes to the project and a new cycle begins.\n\n")
	b.WriteString("**If you DID create or improve artifacts**, do NOT create this file — the daemon will schedule ")
	b.WriteString("another session to continue progressive work.\n\n")

	b.WriteString("## Rules\n\n")
	b.WriteString("- Do NOT make code changes — only generate/improve skills, rules, commands, and memories\n")
	b.WriteString("- Do NOT delete existing skills without documenting the rationale in the dream report\n")
	b.WriteString("- Do NOT delete existing memories — only update or create new ones\n")
	b.WriteString("- Do NOT modify security-sensitive files (credentials, vault configs, .env files)\n")
	b.WriteString("- ALWAYS check for existing skills before creating duplicates\n")
	b.WriteString("- ALWAYS include provenance (conversation IDs) in generated artifacts\n")
	b.WriteString("- ALWAYS log skill modifications with before/after rationale\n")
	b.WriteString("- NEVER introduce breaking changes to existing skills without justification\n")
	_, _ = fmt.Fprintf(&b, "- ALWAYS create `%s/dream/%s%s` if you found nothing to improve\n",
		brand.DotDir(), sessionID, DeepSleepSentinelName())
	_, _ = fmt.Fprintf(&b, "- NEVER create `%s/dream/%s%s` if you created or modified any artifacts\n",
		brand.DotDir(), sessionID, DeepSleepSentinelName())

	return b.String()
}

func buildDreamArtifact(sessionID, agentOutput, diagnostic string) string {
	var b strings.Builder

	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "title: Dream Session %s\n", sessionID)
	_, _ = fmt.Fprintf(&b, "created: %s\n", time.Now().UTC().Format(time.RFC3339))
	b.WriteString("type: dream-report\n")
	b.WriteString("---\n\n")

	b.WriteString("# Dream Report\n\n")
	_, _ = fmt.Fprintf(&b, "**Dream ID**: `%s`\n", sessionID)
	_, _ = fmt.Fprintf(&b, "**Timestamp**: `%s`\n\n", time.Now().UTC().Format(time.RFC3339))

	// Before the output, not after: this section says the output below is probably
	// not what was asked for, and that is worth knowing before reading it.
	if diagnostic != "" {
		b.WriteString("## ⚠️ No artifacts were produced\n\n")
		b.WriteString(diagnostic)
		b.WriteString("\n")
	}

	b.WriteString("## Agent Output\n\n")
	b.WriteString(agentOutput)
	b.WriteString("\n")

	return b.String()
}
