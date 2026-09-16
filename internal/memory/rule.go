package memory

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
)

var (
	memorySkillName        = brand.SkillDirName("memory")
	memorySkillDescription = "Memory: recall and preserve preferences, corrections, user-provided project facts, standing guidance, and agent-discovered structural or non-obvious system knowledge across tasks."
)

func InstallSkill(projectDir, agentName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	skillContent := brand.ResolveModuleSkillIn(projectDir, "memory", RuleContent(nil))
	frontmatter, err := agent.SkillFrontmatter(memorySkillName, memorySkillDescription)
	if err != nil {
		return err
	}
	return agent.InstallManagedSkillWithReferences(projectDir, agentName, memorySkillName, frontmatter+skillContent, SkillReferences())
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
