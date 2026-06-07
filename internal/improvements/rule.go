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
	dreamAddRef := brand.MCPToolRef("dream", "subject_add")
	return "TRIGGER: User asks to improve/audit/review/refactor → read `" + improvementsSkillName + "` skill FIRST. " +
		"REFLECTION: After any significant task → evaluate and memorize learnings. " +
		"DREAM: Queue deferred work via " + dreamAddRef + "."
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
