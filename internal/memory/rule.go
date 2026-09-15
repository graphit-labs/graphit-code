package memory

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
)

var (
	memorySkillName        = brand.SkillDirName("memory")
	memorySkillDescription = "Durable memory for preferences, corrections, decisions, constraints, user-provided project facts, standing guidance, and agent-discovered structural or non-obvious system knowledge; mandatory recall is performed by adapter hooks."
)

func InstallSkill(projectDir, agentName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	skillContent := brand.ResolveModuleSkill("memory", RuleContent(nil))
	frontmatter, err := agent.SkillFrontmatter(memorySkillName, memorySkillDescription)
	if err != nil {
		return err
	}
	return agent.InstallManagedSkill(projectDir, agentName, memorySkillName, frontmatter+skillContent)
}

func RemoveSkill(projectDir, agentName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	return agent.RemoveManagedSkill(projectDir, agentName, memorySkillName)
}
