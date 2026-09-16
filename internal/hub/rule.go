package hub

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
)

var hubSkillName = brand.SkillDirName("hub")

func InstallSkill(projectDir, agentName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	skillContent := brand.ResolveModuleSkillIn(projectDir, "hub", HubRuleContent())
	frontmatter, err := agent.SkillFrontmatter(hubSkillName, "Hub: locate ecosystem projects locally, then in the published catalog; resolve Hub artifacts and Graphit configuration. Public technologies need no Hub lookup.")
	if err != nil {
		return err
	}
	return agent.InstallManagedSkillWithReferences(projectDir, agentName, hubSkillName, frontmatter+skillContent, SkillReferences())
}

func RemoveSkill(projectDir, agentName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	return agent.RemoveManagedSkill(projectDir, agentName, hubSkillName)
}
