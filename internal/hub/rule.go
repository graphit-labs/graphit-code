package hub

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

var hubSkillName = brand.SkillDirName("hub")

func InstallSkill(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	skillContent := brand.ResolveModuleSkill("hub", HubRuleContent())
	frontmatter, err := ide.SkillFrontmatter(hubSkillName, "Hub-first: resolve external APIs, dependencies, reusable artifacts, ecosystem projects, and Graphit configuration before model knowledge or web search.")
	if err != nil {
		return err
	}
	return ide.InstallManagedSkill(projectDir, ideName, hubSkillName, frontmatter+skillContent)
}

func RemoveSkill(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	return ide.RemoveManagedSkill(projectDir, ideName, hubSkillName)
}
