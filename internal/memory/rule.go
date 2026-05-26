package memory

import (
	"os"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

const memoryBlockName = "MEMORY"

func RuleContent(contexts []string) string {
	dotBrand := brand.DotDir()
	binName := brand.BinName()
	displayName := brand.DisplayName

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
		"1. Read `" + dotBrand + "/memory/project/index.md`",
		"2. Read `" + dotBrand + "/memory/user/index.md`",
		"3. If either wiki has memories, scan titles for anything relevant to the user's request",
		"4. If relevant memories found, read the entity page(s) and follow their guidance",
		"5. When you act based on a memory, tell the user: \"Following memory: '<title>'\"",
		"6. Only then proceed with the user's request",
		"",
		"> If a wiki `index.md` does not exist (new project), skip that scope and proceed.",
		"",
	}

	for _, ctx := range contexts {
		ctxPath := dotBrand + "/memory/" + ctx + "/index.md"
		lines = append(lines,
			"> Also read imported context: `"+ctxPath+"`",
		)
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
		"**How to search:** `"+binName+" memory search \"<relevant keywords>\"`",
		"",

		"## 🔒 This Framework IS Your Memory — No Other Exists",
		"",
		"- **ALWAYS** use `"+binName+" memory insert` to persist facts across sessions.",
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
		"When creating a memory, always include these four fields in `--content`:",
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
		"| You observe... | Action | Command |",
		"|---|---|---|",
		"| User says \"always/never/prefer/avoid/must\" about code | Store as convention | `"+binName+" memory insert \"<rule>\" --type convention --important` |",
		"| User corrects your behavior or approach | Store the correction | `"+binName+" memory insert \"<correction>\" --type correction --important --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| User **guides, orients, or gives direction** on how to proceed | Store the guidance | `"+binName+" memory insert \"<guidance>\" --type convention --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| User **intervenes** mid-task to change course or redirect | Store the intervention as correction | `"+binName+" memory insert \"<intervention>\" --type correction --important --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| User gives a **tip, hint, or suggestion** on how to do something | Store as skill | `"+binName+" memory insert \"<tip>\" --type skill --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| User **explains how something works** or why it's done a certain way | Store as fact | `"+binName+" memory insert \"<knowledge>\" --type fact --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| User **shows you a workflow** or operational procedure (e.g., \"run make install first\") | Store as skill | `"+binName+" memory insert \"<procedure>\" --type skill --important --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| User **repeats an instruction** they already gave (frustration signal) | Store as correction | `"+binName+" memory insert \"<what you missed>\" --type correction --important --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You **complete a task** (new feature, refactor, or significant change) | Record what was done | `"+binName+" memory insert \"<task summary>\" --type fact --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You **modify an existing feature** (behavior change, extension, or rework) | Record the modification | `"+binName+" memory insert \"<feature> modified: <summary>\" --type fact --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You **fix a bug** | Record the bug fix | `"+binName+" memory insert \"Bug fix: <summary>\" --type skill --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You make an architectural/design choice | Record the decision | `"+binName+" memory insert \"<decision>\" --type decision --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You chose X over Y with explicit trade-offs | Capture the tension | `"+binName+" memory insert \"<choice>\" --type tension --content \"Chose: X\\nOver: Y\\nBecause: ...\\nAccepting: ...\\nImpact: ...\"` |",
		"| You **discover something unexpected** during investigation | Store the discovery | `"+binName+" memory insert \"<discovery>\" --type fact --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You find a **workaround or creative solution** to a problem | Store the workaround | `"+binName+" memory insert \"<workaround>\" --type skill --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You make a **non-obvious inference** that proves correct | Store the inference | `"+binName+" memory insert \"<inference>\" --type skill --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You **solve a complicated multi-step problem** | Store the full solution | `"+binName+" memory insert \"<problem → solution>\" --type skill --important --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| You debug a non-obvious issue successfully | Save as a skill | `"+binName+" memory insert \"<solution>\" --type skill --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| User reveals a non-obvious project fact | Store the fact | `"+binName+" memory insert \"<fact>\" --type fact --content \"What: ...\\nWhy: ...\\nHow: ...\\nImpact: ...\"` |",
		"| New instruction contradicts existing memory | Replace the memory | `"+binName+" memory delete <old-id>` then `"+binName+" memory insert \"<new>\"` |",
		"| Memory is >30 days old and still relevant | Refresh it | `"+binName+" memory update <id> --content \"<refreshed>\"` |",
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
		"Default type when `--type` is omitted: `fact`.",
		"",

		"## 📖 How to Retrieve Memories",
		"",
		"Read the wiki files directly — you have full file access:",
		"",
		"| Scope | Path |",
		"|---|---|",
		"| **project** | `"+dotBrand+"/memory/project/index.md` |",
		"| **user** | `"+dotBrand+"/memory/user/index.md` |",
	)

	for _, ctx := range contexts {
		lines = append(lines,
			"| **"+ctx+"** | `"+dotBrand+"/memory/"+ctx+"/index.md` |",
		)
	}

	lines = append(lines,
		"",
		"**Retrieval steps:**",
		"1. Read `index.md` — scan the catalog (grouped by type: conventions, corrections, decisions...)",
		"2. Read the entity page for relevant memories",
		"3. Check `## Backlinks` for related memories",
		"4. **Never** grep raw memory files — the wiki is pre-compiled and faster",
		"",

		"## 📋 CLI Quick Reference",
		"",
		"```bash",
		"# Insert (default: project scope, type: fact)",
		binName+" memory insert \"<title>\" --content \"<body>\" --type <type>",
		"",
		"# Insert important convention",
		binName+" memory insert \"<title>\" --content \"<body>\" --type convention --important",
		"",
		"# Insert correction",
		binName+" memory insert \"<title>\" --content \"<body>\" --type correction --important",
		"",
		"# Insert with tags",
		binName+" memory insert \"<title>\" --content \"<body>\" --tags \"auth,security\"",
		"",
		"# Insert user-scoped memory",
		binName+" memory insert \"<title>\" --user",
		"",
		"# Update existing memory",
		binName+" memory update <id> --content \"<new body>\"",
		"",
		"# Delete",
		binName+" memory delete <id>",
		"",
		"# Search (lightweight, no AI)",
		binName+" memory search \"<term>\"",
		"",
		"# Promote/demote importance",
		binName+" memory promote <id>",
		binName+" memory demote <id>",
		"",
		"# List all",
		binName+" memory list",
		binName+" memory important",
		"",
		"# Maintenance (run periodically or when memory feels cluttered)",
		binName+" memory consolidate          # AI-driven: find duplicates, contradictions",
		binName+" memory consolidate --apply   # auto-apply safe suggestions",
		binName+" memory gc                    # find stale/empty memories (dry-run)",
		binName+" memory gc --dry-run=false    # delete GC candidates",
		"```",
		"",

		"## 🔄 Contradiction Protocol",
		"",
		"When the user's new instruction contradicts an existing memory:",
		"",
		"1. Read the wiki index to find the contradicted memory",
		"2. Delete it: `"+binName+" memory delete <id>`",
		"3. Create the replacement: `"+binName+" memory insert \"<new>\" ...`",
		"4. Confirm: \"Updated memory: removed '<old title>' and saved '<new title>'\"",
		"",

		"## 📣 Transparency Rules",
		"",
		"- **Always** tell the user when you act based on a memory: \"Following memory: '<title>'\"",
		"- **Never** ask permission to create memories — just save and confirm.",
		"- **Always** confirm memory operations: \"Memorized: '<title>'\" or \"Removed memory: '<title>'\"",
		"",

		"## ⚡ Reindex After Writes",
		"",
		"After any write (`insert`, `delete`, `update`, `promote`, `demote`), the auto-cycle",
		"runs automatically. If it fails, trigger manually:",
		"",
		"```bash",
		binName+" memory index &",
		"```",
		"",
		"Run fire-and-forget — do NOT wait.",
		"",

		"## ⛔ Critical Rules (Never Violate)",
		"",
		"1. **Read memory at session start.** Your context lives there. Skipping = repeating mistakes.",
		"2. **Never leave a correction un-memorized.** If the user corrects you, save it immediately.",
		"3. **Never edit .md memory files directly.** Always use `"+binName+" memory` commands.",
		"4. **Capture trade-offs, not just facts.** \"We chose X over Y because Z\" > \"We use X\".",
		"5. **Handle contradictions.** Remove old + create new. Don't leave conflicting memories.",
		"6. **Promote critical memories.** Conventions, corrections, and constraints should be `--important`.",
	)

	importantBlock := RenderImportantBlock("project")
	if importantBlock != "" {
		lines = append(lines, "", importantBlock)
	}

	recentBlock := RenderRecentBlock("project", 5)
	if recentBlock != "" {
		lines = append(lines, "", recentBlock)
	}

	return strings.Join(lines, "\n") + "\n"
}

var memorySkillName = brand.SkillDirName("memory")

func MemoryRouterContent(contexts []string) string {
	dotBrand := brand.DotDir()
	binName := brand.BinName()
	lines := []string{
		"# 🧠 Memory Management",
		"",
		"> Persistent memory across sessions. This framework IS your memory — no other exists.",
		"> **Full CLI reference, trigger table, and protocols are in the `" + memorySkillName + "` skill.**",
		"",
		"## 🚨 FIRST ACTION — Execute BEFORE Any Response",
		"",
		"**Execute IMMEDIATELY on every conversation start. Do NOT respond to the user first.**",
		"",
		"1. Use `view_file` to read `" + dotBrand + "/memory/project/index.md`",
		"2. Use `view_file` to read `" + dotBrand + "/memory/user/index.md`",
		"3. If any memory title relates to the user's request → read that page and say: \"Following memory: '<title>'\"",
		"",
		"> If a file does not exist (new project), skip it. Use `view_file` — NOT `cat` via run_command.",
	}

	for _, ctx := range contexts {
		ctxPath := dotBrand + "/memory/" + ctx + "/index.md"
		lines = append(lines,
			"> Also read imported context: `"+ctxPath+"`",
		)
	}

	lines = append(lines,
		"",
		"## Activation Triggers — You MUST read the `"+memorySkillName+"` skill when:",
		"",
		"### 💾 Save triggers (memorize immediately):",
		"",
		"**Every memory MUST follow the What/Why/How/Impact template in --content.**",
		"",
		"#### Task lifecycle (always memorize):",
		"- You **complete a task** (new feature, refactor, significant change) → store what/why/how/impact",
		"- You **modify an existing feature** (behavior change, extension, rework) → store what changed and impact",
		"- You **fix a bug** → store the root cause, fix, and system impact as skill",
		"",
		"#### User interaction (always memorize):",
		"- User **corrects** your behavior or approach → store as correction (--important)",
		"- User **guides or orients** on how to proceed → store as convention",
		"- User **intervenes** mid-task to redirect or change course → store as correction (--important)",
		"- User **explains how something works** or shows a procedure → store as skill/fact",
		"- User gives a **tip, hint, or suggestion** → store as skill",
		"- User **repeats an instruction** (frustration signal) → store as correction (--important)",
		"- User says \"always/never/prefer/avoid/must\" about code → store convention",
		"",
		"#### Agent discoveries (always memorize):",
		"- You make a design decision or choose between alternatives → record decision",
		"- You **discover something unexpected** or solve a non-obvious problem → store as skill",
		"- New instruction contradicts existing memory → replace it",
		"",
		"### 📖 Read triggers (consult memory before acting):",
		"- **Before implementing** any significant change → check for constraints and decisions",
		"- **When stuck**, failing repeatedly, or facing a non-obvious problem → search for past solutions",
		"- **Before proposing** architecture or a technical approach → check for prior decisions",
		"- When trying to **understand project context** → search for institutional knowledge",
		"- Memory management or maintenance tasks",
		"",
		"## 🔒 MANDATORY: Read Skill Before Acting",
		"",
		"**When ANY activation trigger above matches your current task, you MUST read the",
		"`"+memorySkillName+"` skill BEFORE executing your first memory operation.**",
		"The Quick Reference below is a cheat sheet for agents who already read the skill —",
		"it is NOT a substitute. The skill contains the full trigger→action table, memory types,",
		"contradiction protocols, and transparency rules you must follow.",
		"> **Exception:** The SESSION START PROTOCOL above is always active and does not require",
		"> reading the skill — execute it immediately on every conversation.",
		"",
		"## Quick Reference (always active)",
		"",
		"- **Insert**: `"+binName+" memory insert \"<title>\" --type <type> --content \"<body>\"`",
		"- **Delete**: `"+binName+" memory delete <id>`",
		"- **Search**: `"+binName+" memory search \"<term>\"`",
		"",
		"## ⛔ Critical Rules (always enforced)",
		"",
		"1. **Read memory at session start.** Skipping = repeating past mistakes.",
		"2. **Never leave a correction un-memorized.** Save immediately.",
		"3. **Never edit .md memory files directly.** Use `"+binName+" memory` commands.",
		"4. **ALWAYS use `"+binName+" memory insert`** — NEVER use IDE-native memory.",
		"5. **Always confirm**: \"Memorized: '<title>'\" or \"Following memory: '<title>'\".",
		"",
		"## 🔗 MANDATORY: Subagent Memory Access",
		"",
		"**When spawning subagents (via `define_subagent`, `invoke_subagent`, or any multi-agent mechanism),",
		"include these memory instructions in the subagent's system_prompt or task Prompt:**",
		"",
		"Add to every subagent prompt:",
		"\"IMPORTANT: Before starting work, read `"+dotBrand+"/memory/project/index.md` via view_file for project context, conventions, and past corrections.",
		"If any memory is relevant to your task, follow its guidance.",
		"After completing work, if you discovered something non-obvious, save it:",
		"`"+binName+" memory insert \\\"<discovery>\\\" --type skill --content \\\"<details>\\\"` \"",
		"",
		"For `define_subagent`, inject these as part of the `system_prompt` parameter.",
		"For `invoke_subagent` of existing types (like 'research'), add these to the `Prompt` parameter.",
	)

	importantBlock := RenderImportantBlock("project")
	if importantBlock != "" {
		lines = append(lines, "", importantBlock)
	}

	return strings.Join(lines, "\n") + "\n"
}

func InstallRule(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}

	contexts := AllContextDirs()

	routerContent := brand.ResolveModuleRule("memory", MemoryRouterContent(contexts))
	if err := ide.InjectManagedBlock(projectDir, ideName, memoryBlockName, routerContent); err != nil {
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

	return ide.RemoveManagedBlock(projectDir, ideName, memoryBlockName)
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
