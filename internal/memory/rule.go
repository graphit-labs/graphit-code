package memory

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

var memorySkillName = brand.SkillDirName("memory")

func InstallSkill(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	skillContent := brand.ResolveModuleSkill("memory", RuleContent(AllContextDirs()))
	frontmatter, err := ide.SkillFrontmatter(memorySkillName, "Durable memory: project and user preferences, corrections, decisions, constraints, and non-obvious knowledge; mandatory recall is performed by adapter hooks.")
	if err != nil {
		return err
	}
	return ide.InstallManagedSkill(projectDir, ideName, memorySkillName, frontmatter+skillContent)
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
