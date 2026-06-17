package improvements

import (
	"os"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

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

func MandateTrigger() string {
	memInsertRef := brand.MCPToolRef("memory", "insert")

	return `
# Code Improvement Methodology

> Autonomous code improvement, audit, review and refactoring methodology.
> Includes a **mandatory post-task reflection phase** for knowledge generation.
> **Full analysis methodology is in the ` + "`" + brand.SkillDirName("improvements") + "`" + ` skill.**

## Activation Triggers — You MUST read the ` + "`" + brand.SkillDirName("improvements") + "`" + ` skill when:

- User asks to **improve**, **audit**, **review**, or **refactor** the codebase
- User requests autonomous quality assessment
- User asks for code smell detection or best-practice enforcement
- After completing any significant task — for the **reflection & knowledge generation** phase
- When you notice code patterns that could be improved but are out of scope for the current task
- When you want to schedule work for later autonomous processing
- When you need to create, update, or codify recurring patterns as Hub artifacts (skills, commands, rules)

## MANDATORY: Read Skill Before Acting

**When ANY activation trigger above matches your current task, you MUST read the
` + "`" + brand.SkillDirName("improvements") + "`" + ` skill BEFORE starting any analysis, review, or improvement work.**
The skill contains the full engineering analysis methodology, phase-by-phase workflow,
and post-task reflection protocol. Do NOT improvise your own review process.

## Critical Rules

- When triggered, you MUST read and follow the full engineering analysis methodology
  documented in the skill. Do NOT improvise your own review process.
- After any improvement session, you MUST execute the **Post-Task Reflection**
  phase: reflect, update memories via ` + memInsertRef + `, and stage new artifacts for the Hub.
`
}

func InstallRule(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}

	if err := ide.UpsertMandateTrigger(projectDir, ideName, "imp_rule", MandateTrigger()); err != nil {
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

	return ide.RemoveMandateTrigger(projectDir, ideName, "imp_rule")
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
