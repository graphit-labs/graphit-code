package task

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

var skillName = brand.SkillDirName("task")

func InstallSkill(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	content := brand.ResolveModuleSkill("task", RuleContent())
	frontmatter, err := ide.SkillFrontmatter(skillName, "Deterministic tasks: shared LanceDB work, dependencies, claims, progress, handoff, completion, and prior-task search.")
	if err != nil {
		return err
	}
	return ide.InstallManagedSkill(projectDir, ideName, skillName, frontmatter+content)
}

func RemoveSkill(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	return ide.RemoveManagedSkill(projectDir, ideName, skillName)
}
