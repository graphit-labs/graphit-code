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
	skillContent := brand.ResolveModuleSkill("hub", HubRuleContent())
	frontmatter, err := agent.SkillFrontmatter(hubSkillName, "Hub-first: resolve external APIs, dependencies, reusable artifacts, ecosystem projects, and Graphit configuration before model knowledge or web search.")
	if err != nil {
		return err
	}
	return agent.InstallManagedSkill(projectDir, agentName, hubSkillName, frontmatter+skillContent)
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
