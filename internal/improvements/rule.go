package improvements

import (
	"os"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

const improvementsBlockName = "IMPROVEMENTS"

func ImprovementsRuleContent() string {
	var b strings.Builder

	b.WriteString("# Code Improvement Methodology Rule\n\n")
	b.WriteString("## When to Use\n\n")
	b.WriteString("When the user asks you to **autonomously improve**, **audit**, **review**,\n")
	b.WriteString("or **refactor** the codebase (or parts of it), you MUST follow the\n")
	b.WriteString("engineering analysis methodology detailed below.\n\n")
	b.WriteString("---\n\n")

	b.WriteString(Rules())

	return b.String()
}

var improvementsSkillName = brand.SkillDirName("improvements")

func ImprovementsRouterContent() string {
	dreamAddRef := brand.MCPToolRef("dream", "subject_add")
	dreamAdd := brand.MCPToolName("dream", "subject_add")
	dreamListRef := brand.MCPToolRef("dream", "subject_list")
	dreamRemoveRef := brand.MCPToolRef("dream", "subject_remove")

	lines := []string{
		"# 🔧 Code Improvement Methodology",
		"",
		"> Autonomous code improvement, audit, review, refactoring methodology, and dream subjects.",
		"> Includes a **mandatory post-task reflection phase** for knowledge generation.",
		"> **Full analysis methodology is in the `" + improvementsSkillName + "` skill.**",
		"",
		"## Activation Triggers — You MUST read the `" + improvementsSkillName + "` skill when:",
		"",
		"- User asks to **improve**, **audit**, **review**, or **refactor** the codebase",
		"- User requests autonomous quality assessment",
		"- User asks for code smell detection or best-practice enforcement",
		"- After completing any significant task — for the **reflection & knowledge generation** phase",
		"- When you notice code patterns that could be improved but are out of scope for the current task",
		"- When you want to schedule work for later autonomous processing",
		"- When you need to create, update, or codify recurring patterns as Hub artifacts (skills, commands, rules)",
		"",
		"## 🔒 MANDATORY: Read Skill Before Acting",
		"",
		"**When ANY activation trigger above matches your current task, you MUST read the",
		"`" + improvementsSkillName + "` skill BEFORE starting any analysis, review, or improvement work.**",
		"The skill contains the full engineering analysis methodology, phase-by-phase workflow,",
		"and post-task reflection protocol. Do NOT improvise your own review process.",
		"",
		"## 💤 Dream Subjects — Queue Work for Autonomous Dreams",
		"",
		"When the user asks you to **schedule**, **queue**, or **leave something for the next dream**,",
		"create a dream subject. The Dream module picks up pending subjects during idle periods.",
		"",
		"- **Add via MCP**: call " + dreamAddRef + " with `title: \"Title\"`, `body: \"Detailed instructions\"` (always passing absolute `project_dir` parameter)",
		"- **Add via file**: Create `" + brand.DotDir() + "/dream/subjects/<slug>.md` with a `# Title` heading and instructions",
		"- **List subjects**: call " + dreamListRef + " (always passing absolute `project_dir` parameter)",
		"- **Remove**: call " + dreamRemoveRef + " with `slug: \"<slug>\"` (always passing absolute `project_dir` parameter)",
		"- **Completion**: The dream agent creates `<slug>.done.md` when finished",
		"",
		"## 🔗 Subagent Improvements",
		"",
		"**When spawning subagents, include in their prompt:**",
		"\"If you notice code patterns that could be improved but are outside your current scope, create a dream subject by calling the " + dreamAddRef + " tool: `" + dreamAdd + "` with `title: \"<improvement title>\"`, `body: \"<detailed description of what to improve and why>\"` (always passing absolute `project_dir` parameter).\"",
		"",
		"## ⛔ Critical Rules",
		"",
		"- When triggered, you MUST read and follow the full engineering analysis methodology",
		"  documented in the skill. Do NOT improvise your own review process.",
		"- After any improvement session, you MUST execute the **Post-Task Reflection**",
		"  phase: reflect, update memories, and stage new artifacts for the Hub.",
		"",
		"## 🔗 MANDATORY: Subagent Propagation",
		"",
		"**When you orchestrate subagents (via `define_subagent`, `invoke_subagent`, or any",
		"multi-agent mechanism), you MUST inject this entire instruction block into each",
		"subagent's system prompt or task instructions. Do NOT skip or summarize it.**",
		"",
		"When creating a subagent, include everything from the `" + strings.ToUpper(brand.Brand) + " " + improvementsBlockName + " BLOCK` start sentinel",
		"to the `END " + strings.ToUpper(brand.Brand) + " " + improvementsBlockName + " BLOCK` sentinel (inclusive) in the subagent's prompt.",
		"This is a conditional instruction — only act on it when you are actually launching a subagent.",
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

	routerContent := brand.ResolveModuleRule("improvements_rule", ImprovementsRouterContent())
	if err := ide.InjectManagedBlock(projectDir, ideName, improvementsBlockName, routerContent); err != nil {
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
	skillContent := brand.ResolveModuleSkill("improvements", ImprovementsRuleContent())
	frontmatter := "---\nname: " + improvementsSkillName + "\ndescription: Autonomous code improvement, audit, review, refactoring methodology, and dream subjects. Use after completing any significant task for reflection, when the user asks to improve/audit/review code, or when you discover improvement opportunities to queue for later processing.\n---\n\n"
	return ide.InstallManagedSkill(projectDir, ideName, improvementsSkillName, frontmatter+skillContent)
}

func RemoveRule(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}

	return ide.RemoveManagedBlock(projectDir, ideName, improvementsBlockName)
}

func RemoveSkill(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	return ide.RemoveManagedSkill(projectDir, ideName, improvementsSkillName)
}
