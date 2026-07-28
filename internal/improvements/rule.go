package improvements

import (
	"os"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

func ImprovementsRuleContent() string {
	var b strings.Builder

	rulesRef := brand.MCPToolRef("improvements", "rules")
	rulesTool := brand.MCPToolName("improvements", "rules")

	b.WriteString("# Code Improvement Methodology Rule\n\n")
	b.WriteString("## When to Use\n\n")
	b.WriteString("When the user asks you to **autonomously improve**, **audit**, **review**,\n")
	b.WriteString("or **refactor** the codebase (or parts of it), you MUST follow the\n")
	b.WriteString("engineering analysis methodology detailed below.\n\n")

	b.WriteString("## These rules can be project-specific — read the resolved copy\n\n")
	b.WriteString("The methodology below is the **default**. A project or a Hub artifact can replace or\n")
	b.WriteString("extend it, and when it does, the text in this skill is not what applies. " + rulesRef + "\n")
	b.WriteString("returns the version actually in force:\n\n")
	b.WriteString("```\n")
	b.WriteString(rulesTool + "()                 # the resolved rules — project override if there is one\n")
	b.WriteString(rulesTool + "(default: true)    # the compiled-in default, ignoring every override\n")
	b.WriteString("```\n\n")
	b.WriteString("Call it before a review you are going to report on, and compare the two when a finding\n")
	b.WriteString("feels wrong for this codebase: the difference between them **is** the project's own\n")
	b.WriteString("standard, and reviewing against the default when an override exists means flagging as\n")
	b.WriteString("defects the very choices this project made deliberately.\n\n")
	b.WriteString("---\n\n")

	b.WriteString(Rules())

	return b.String()
}

var improvementsSkillName = brand.SkillDirName("improvements")

func MandateTrigger() string {
	return ide.ModuleMandateTrigger(
		"Code Improvement Methodology",
		improvementsSkillName,
		"improvement, audit, review, refactor, or post-task reflection",
		"During analysis and the mandatory post-task reflection, all memory, knowledge, and hub lookups MUST go through the "+brand.Brand+" MCP tools — never the CLI.",
		[]string{
			"the request is to review, audit, refactor, clean up, optimise, or harden something",
			"you just finished a task — the reflection is part of finishing, not an optional extra",
			"you are about to declare work complete",
			"you noticed something worth fixing that is outside the current change",
			"the request asks what is wrong with something, or how it could be better",
			"you are deciding what \"good\" means for this codebase rather than in general",
		},
		[]string{"improvements_rules"},
	)
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
	frontmatter := "---\nname: " + improvementsSkillName + "\ndescription: Autonomous code improvement, audit, review, refactoring methodology, and dream subjects. Use when: user asks to improve, audit, review, or refactor the codebase; user requests quality assessment or code smell detection; after completing any significant task for reflection and knowledge generation; you notice improvement patterns out of scope for the current task; you want to schedule work for later autonomous processing; you need to create, update, or codify recurring patterns as Hub artifacts.\n---\n\n"
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
