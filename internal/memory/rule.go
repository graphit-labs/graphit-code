package memory

import (
	"os"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)



func RuleContent(contexts []string) string {
	_ = contexts // contexts are managed by the wiki DB; callers may pass them but they're not used in the rule text
	displayName := brand.DisplayName

	memInsertRef := brand.MCPToolRef("memory", "insert")
	memInsert := brand.MCPToolName("memory", "insert")
	memDelete := brand.MCPToolName("memory", "delete")
	memDeleteRef := brand.MCPToolRef("memory", "delete")
	memUpdate := brand.MCPToolName("memory", "update")
	memSearch := brand.MCPToolName("memory", "search")
	memSearchRef := brand.MCPToolRef("memory", "search")
	memQuery := brand.MCPToolName("memory", "query")
	memPromote := brand.MCPToolName("memory", "promote")
	memDemote := brand.MCPToolName("memory", "demote")
	memList := brand.MCPToolName("memory", "list")
	memImportant := brand.MCPToolName("memory", "important")
	memConsolidate := brand.MCPToolName("memory", "consolidate")
	memGc := brand.MCPToolName("memory", "gc")
	memIndexRef := brand.MCPToolRef("memory", "index")
	memWildcard := "`" + brand.Brand + "_memory_*`"

	lines := []string{
		"# Memory Management Rule",
		"",
		"> This rule is auto-managed by " + displayName + ". Do not edit this block manually.",
		"",
		"## 🚨 SESSION START PROTOCOL — Execute BEFORE Any Response",
		"",
		"**These steps are MANDATORY. Execute them BEFORE responding to the user's first message.**",
		"**Skipping them means you WILL repeat mistakes the user already corrected.**",
		"",
		"1. Call " + memSearchRef + " with context from the user's request to find relevant memories",
		"2. If relevant memories found, read the entity page(s) and follow their guidance",
		"3. Only then proceed with the user's request",
		"",
		"> If the memory wiki does not exist yet (new project), skip and proceed.",
		"",
	}

	lines = append(lines,
		"",
		"## 📖 When to Read Memory (Beyond Session Start)",
		"",
		"**Memory reading is NOT limited to session start.** You MUST also consult memory",
		"in these situations — proactively, without being asked:",
		"",
		"| Situation | What to search for | Why |",
		"|---|---|---|",
		"| **Before implementing** any significant change | Conventions, decisions, constraints about affected modules | Avoid violating established patterns or repeating rejected approaches |",
		"| **When stuck** or your approach isn't working (2+ failed attempts) | Skills, workarounds, corrections related to the problem | The solution may already exist — you may be repeating a known mistake |",
		"| **Non-obvious error** or unexpected behavior | Facts, skills, debugging steps about the affected area | Someone (you or another agent) may have debugged this before |",
		"| **Trying to understand** \"why is it done this way?\" | Decisions, tensions, facts about the module/pattern | The rationale is captured in memory — don't guess |",
		"| **Multiple attempts failing** (3+ tries on same problem) | Corrections, skills, workarounds | STOP and read before trying again — you are likely repeating a known error |",
		"| **Before proposing** architecture or technical approach | Decisions, tensions, conventions that constrain options | Avoid contradicting prior decisions the user already made |",
		"| **User seems frustrated** or repeats an instruction | Corrections about your behavior | You may be ignoring a correction already memorized |",
		"",
		"**How to search:** call " + memSearchRef + " tool (passing absolute `project_dir` parameter)",
		"",
		"## 🔒 This Framework IS Your Memory — No Other Exists",
		"",
		"- **ALWAYS** use the " + memInsertRef + " tool (passing absolute `project_dir` parameter) to persist facts across sessions.",
		"- **NEVER** use IDE-native memory, model memory, or any built-in \"remember\" feature.",
		"- **ALWAYS** read the memory wiki to recall — never rely on what you \"think you know\".",
		"- This applies to ALL persistent facts: conventions, corrections, decisions, preferences.",
		"",
		"## 🎯 Trigger → Action Table",
		"",
		"**When you observe a trigger, execute the corresponding action immediately.**",
		"**Do NOT ask for permission. Do NOT skip. Confirm with: \"Done, I've memorized: <title>\"**",
		"",
		"### 💾 Save Triggers — Memorize Immediately",
		"",
		"**Every memory MUST follow the structured content template below (What/Why/How/Impact).**",
		"This ensures memories are complete and actionable across sessions.",
		"",
		"#### 📝 Memory Content Template",
		"",
		"When creating a memory, always include these four fields in the `content` parameter:",
		"",
		"```",
		"What: <what was done or what happened>",
		"Why: <why it was done — the motivation, root cause, or user intent>",
		"How: <how it was resolved — the approach, steps taken, or implementation>",
		"Impact: <how it impacted the system — side effects, files changed, behavior changes>",
		"```",
		"",
		"#### Trigger Table",
		"",
		"| You observe... | Action | Tool Call (always pass absolute `project_dir` parameter) |",
		"|---|---|---|",
		"| User says \"always/never/prefer/avoid/must\" about code | Store as convention | " + memInsertRef + " with `title: \"<rule>\"`, `type: \"convention\"`, `important: true` |",
		"| User corrects your behavior or approach | Store the correction | " + memInsertRef + " with `title: \"<correction>\"`, `type: \"correction\"`, `important: true`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| User **gives any instruction or directive** (even without \"always/never\") | **Evaluate for memory** — determine if it contains a convention, preference, correction, fact, or skill worth persisting. If yes, memorize it with the appropriate type. NEVER just say \"understood\" or confirm comprehension without evaluating. | " + memInsertRef + " with appropriate `type` if the instruction is worth persisting |",
		"| User **guides, orients, or gives direction** on how to proceed | Store the guidance | " + memInsertRef + " with `title: \"<guidance>\"`, `type: \"convention\"`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| User **intervenes** mid-task to change course or redirect | Store the intervention as correction | " + memInsertRef + " with `title: \"<intervention>\"`, `type: \"correction\"`, `important: true`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| User gives a **tip, hint, or suggestion** on how to do something | Store as skill | " + memInsertRef + " with `title: \"<tip>\"`, `type: \"skill\"`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| User **explains how something works** or why it's done a certain way | Store as fact | " + memInsertRef + " with `title: \"<knowledge>\"`, `type: \"fact\"`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| User **shows you a workflow** or operational procedure (e.g., \"run make install first\") | Store as skill | " + memInsertRef + " with `title: \"<procedure>\"`, `type: \"skill\"`, `important: true`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| User **repeats an instruction** they already gave (frustration signal) | Store as correction | " + memInsertRef + " with `title: \"<what you missed>\"`, `type: \"correction\"`, `important: true`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You **complete a task** (new feature, refactor, or significant change) | Record what was done | " + memInsertRef + " with `title: \"<task summary>\"`, `type: \"fact\"`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You **modify an existing feature** (behavior change, extension, or rework) | Record the modification | " + memInsertRef + " with `title: \"<feature> modified: <summary>\"`, `type: \"fact\"`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You **fix a bug** | Record the bug fix | " + memInsertRef + " with `title: \"Bug fix: <summary>\"`, `type: \"skill\"`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You make an architectural/design choice | Record the decision | " + memInsertRef + " with `title: \"<decision>\"`, `type: \"decision\"`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You chose X over Y with explicit trade-offs | Capture the tension | " + memInsertRef + " with `title: \"<choice>\"`, `type: \"tension\"`, `content: \"Chose: X\\nOver: Y\\nBecause: ...\\nAccepting: ...\\nImpact: ...\"` |",
		"| You **discover something unexpected** during investigation | Store the discovery | " + memInsertRef + " with `title: \"<discovery>\"`, `type: \"fact\"`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You find a **workaround or creative solution** to a problem | Store the workaround | " + memInsertRef + " with `title: \"<workaround>\"`, `type: \"skill\"`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You make a **non-obvious inference** that proves correct | Store the inference | " + memInsertRef + " with `title: \"<inference>\"`, `type: \"skill\"`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You **solve a complicated multi-step problem** | Store the full solution | " + memInsertRef + " with `title: \"<problem → solution>\"`, `type: \"skill\"`, `important: true`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You debug a non-obvious issue successfully | Save as a skill | " + memInsertRef + " with `title: \"<solution>\"`, `type: \"skill\"`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| User reveals a non-obvious project fact | Store the fact | " + memInsertRef + " with `title: \"<fact>\"`, `type: \"fact\"`, `content: \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| New instruction contradicts existing memory | Replace the memory | " + memDeleteRef + " with `id: \"<old-id>\"` then " + memInsertRef + " with `title: \"<new>\"` |",
		"| Memory is >30 days old and still relevant | Refresh it | `" + memUpdate + "` with `id: \"<id>\"`, `content: \"<refreshed>\"` |",
		"",
		"### 📖 Read Triggers — Consult Memory Before Acting",
		"",
		"| You observe... | Action | What to search for |",
		"|---|---|---|",
		"| You're about to **implement a significant change** | Read memory BEFORE coding | Constraints, conventions, decisions about affected modules |",
		"| You're **stuck or your approach isn't working** (2+ failed attempts) | Read memory for past solutions | Skills, workarounds, debugging steps related to the problem |",
		"| You encounter a **non-obvious error or behavior** | Read memory for known issues | The problem may have been solved before — check skills and facts |",
		"| You need to understand **project context or \"why\"** | Read memory for institutional knowledge | Decisions, tensions, and facts about the area you're working on |",
		"| **Multiple attempts have failed** (3+ tries on same problem) | STOP and read memory | You may be repeating a known mistake — check corrections and skills |",
		"| You're about to **propose architecture or an approach** | Read memory for prior decisions | Decisions, tensions, conventions that constrain options |",
		"| Build/test fails unexpectedly | Check memory for known issues | Read wiki for past debugging skills |",
		"",
		"## 📁 Memory Types",
		"",
		"Every memory has a `type` that determines how it is stored and surfaced:",
		"",
		"| Type | When to use | Typical importance |",
		"|---|---|---|",
		"| `convention` | Coding standards, style patterns, project rules | ✅ important |",
		"| `correction` | User corrected your behavior — never repeat the mistake | ✅ important |",
		"| `decision` | Architectural or design decisions with rationale | depends |",
		"| `tension` | Trade-off choices: chose X over Y because Z, accepting W | depends |",
		"| `fact` | Non-obvious project facts, environment details | rarely |",
		"| `skill` | Debugged workflows, reusable solution patterns | rarely |",
		"",
		"Default type when `type` is omitted: `fact`.",
		"",
		"## 📖 How to Retrieve Memories",
		"",
		"**ALWAYS use MCP tools — NEVER read index.md files directly.**",
		"The wiki database is compiled, BM25-indexed, and pre-optimized for retrieval.",
		"Reading raw .md files is slower, wastes tokens, and bypasses ranking.",
		"",
		"**Scope parameter:** `scope: \"project\"` (default) = project-specific memories. `scope: \"user\"` = personal cross-project memories.",
		"- `" + memSearch + "` searches raw `.md` memory files via text matching (Tier 1 — lightweight, no AI)",
		"- `" + memQuery + "` queries the compiled memory wiki via AI synthesis (Tier 3 — deep, multi-turn)",
		"",
		"| What you need | MCP tool | Why |",
		"|---|---|---|",
		"| Search memories by keyword/context | " + memSearchRef + " | Text matching on raw files, instant, ~200 tokens |",
		"| AI-powered memory consultation | `" + memQuery + "` | Synthesizes relevant memories from wiki using AI |",
		"| List all memories | `" + memList + "` | Structured catalog, grouped by type |",
		"| List important memories only | `" + memImportant + "` | High-priority conventions, corrections |",
	)

	lines = append(lines,
		"",
		"**Retrieval steps:**",
		"1. Call " + memSearchRef + " with query context — get ranked results",
		"2. If results reference related memories, call " + memSearchRef + " again with refined query",
		"3. For deep consultation, call `" + memQuery + "` with a natural language question",
		"4. **Never** read .md memory files directly or grep raw memory files",
		"",
		"## 📋 MCP Tools Reference",
		"",
		"All memory actions must be executed via the corresponding MCP tools. Always pass the absolute `project_dir` parameter.",
		"",
		"```",
		"# Insert (default: project scope, type: fact)",
		memInsert + "(project_dir: \"/path/to/project\", title: \"<title>\", content: \"<body>\", type: \"<type>\")",
		"",
		"# Insert important convention",
		memInsert + "(project_dir: \"/path/to/project\", title: \"<title>\", content: \"<body>\", type: \"convention\", important: true)",
		"",
		"# Insert correction",
		memInsert + "(project_dir: \"/path/to/project\", title: \"<title>\", content: \"<body>\", type: \"correction\", important: true)",
		"",
		"# Insert with tags",
		memInsert + "(project_dir: \"/path/to/project\", title: \"<title>\", content: \"<body>\", tags: \"auth,security\")",
		"",
		"# Insert user-scoped memory",
		memInsert + "(project_dir: \"/path/to/project\", title: \"<title>\", content: \"<body>\", scope: \"user\")",
		"",
		"# Update existing memory",
		memUpdate + "(project_dir: \"/path/to/project\", id: \"<id>\", content: \"<new body>\")",
		"",
		"# Delete",
		memDelete + "(project_dir: \"/path/to/project\", id: \"<id>\")",
		"",
		"# Search (lightweight, no AI)",
		memSearch + "(project_dir: \"/path/to/project\", query: \"<term>\")",
		"",
		"# AI Consultation (Search with AI response synthesis)",
		memQuery + "(project_dir: \"/path/to/project\", query: \"<natural language question>\")",
		"",
		"# Promote/demote importance",
		memPromote + "(project_dir: \"/path/to/project\", id: \"<id>\")",
		memDemote + "(project_dir: \"/path/to/project\", id: \"<id>\")",
		"",
		"# List all",
		memList + "(project_dir: \"/path/to/project\")",
		memImportant + "(project_dir: \"/path/to/project\")",
		"",
		"# Maintenance (run periodically or when memory feels cluttered)",
		memConsolidate + "(project_dir: \"/path/to/project\")                # AI-driven: find duplicates, contradictions",
		memConsolidate + "(project_dir: \"/path/to/project\", apply: true)    # auto-apply safe suggestions",
		memGc + "(project_dir: \"/path/to/project\")                         # find stale/empty memories (dry-run)",
		memGc + "(project_dir: \"/path/to/project\", dry_run: false)         # delete GC candidates",
		"```",
		"",
		"## 🔄 Contradiction Protocol",
		"",
		"When the user's new instruction contradicts an existing memory:",
		"",
		"1. Read the wiki index to find the contradicted memory",
		"2. Delete it: call " + memDeleteRef + " tool with `id: \"<id>\"` (passing absolute `project_dir`)",
		"3. Create the replacement: call " + memInsertRef + " tool with `title: \"<new>\"` ...",
		"4. Confirm: \"Updated memory: removed '<old title>' and saved '<new title>'\"",
		"",
		"## 📣 Transparency Rules",
		"",
		"- **Never** ask permission to create memories — just save silently.",
		"",
		"## ⚡ Reindex After Writes",
		"",
		"After any write (`insert`, `delete`, `update`, `promote`, `demote`), the auto-cycle",
		"runs automatically. If it fails, trigger manually calling the " + memIndexRef + " tool (passing absolute `project_dir` parameter).",
		"",
		"## ⛔ Critical Rules (Never Violate)",
		"",
		"1. **Read memory at session start.** Your context lives there. Skipping = repeating mistakes.",
		"2. **Never leave a correction un-memorized.** If the user corrects you, save it immediately.",
		"3. **Never edit .md memory files directly.** Always use " + memWildcard + " MCP tools.",
		"4. **Capture trade-offs, not just facts.** \"We chose X over Y because Z\" > \"We use X\".",
		"5. **Handle contradictions.** Remove old + create new. Don't leave conflicting memories.",
		"6. **Promote critical memories.** Conventions, corrections, and constraints should be marked important.",
		"7. **NEVER just say \"understood\" or confirm comprehension.** When the user gives an instruction, ALWAYS evaluate whether it should be memorized. If it contains a convention, preference, correction, workflow, fact, or any persistent knowledge, create a memory immediately. Only skip memorization if the instruction is purely about an ephemeral, one-shot action with no future relevance.",
	)

	return strings.Join(lines, "\n") + "\n"
}

var memorySkillName = brand.SkillDirName("memory")

func MandateTrigger() string {
	memInsertRef := brand.MCPToolRef("memory", "insert")
	memSearchRef := brand.MCPToolRef("memory", "search")
	memQueryRef := brand.MCPToolRef("memory", "query")
	return "SESSION_START: Call " + memSearchRef + " BEFORE first response to recall relevant context. " +
		"SCOPE: scope:\"project\" (default) for project memories, scope:\"user\" for personal cross-project memories. " +
		"SEARCH: " + memSearchRef + " = lightweight text match on raw files. " + memQueryRef + " = AI synthesis from compiled wiki. " +
		"NEVER read .graphit/memory/*/index.md directly. " +
		"SAVE: User corrects/guides/instructs → " + memInsertRef + " immediately. Task done → " + memInsertRef + ". Design decision → " + memInsertRef + ". " +
		"READ: Before significant changes or when stuck (2+ failures) → " + memSearchRef + ". " +
		"RULE: This framework IS your memory. Never use IDE/model memory. Never say 'understood' without evaluating if the instruction should be memorized."
}



func InstallRule(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}

	if err := ide.UpsertMandateTrigger(projectDir, ideName, "mem_rule", MandateTrigger()); err != nil {
		return err
	}

	return InstallSkill(projectDir, ideName)
}

func InstallSkill(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	contexts := AllContextDirs()
	skillContent := brand.ResolveModuleSkill("memory", RuleContent(contexts))
	frontmatter := "---\nname: " + memorySkillName + "\ndescription: Persistent memory across sessions. MANDATORY: Read memory indexes at the START of every conversation before responding. Use when the user corrects you, teaches you something, or when you make design decisions. Also read memory when stuck or when implementing significant changes to check for prior constraints.\n---\n\n"
	return ide.InstallManagedSkill(projectDir, ideName, memorySkillName, frontmatter+skillContent)
}

func RemoveRule(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}

	return ide.RemoveMandateTrigger(projectDir, ideName, "mem_rule")
}

func RemoveSkill(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	return ide.RemoveManagedSkill(projectDir, ideName, memorySkillName)
}
