package task

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
)

var skillName = brand.SkillDirName("task")

const skillDescription = "Deterministic project work, including analysis-only tasks: exhaustive specifications, durable results, dependencies, claims, progress, handoff, completion, and prior-task search."

func InstallSkill(projectDir, agentName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	content := brand.ResolveModuleSkill("task", RuleContent())
	frontmatter, err := agent.SkillFrontmatter(skillName, skillDescription)
	if err != nil {
		return err
	}
	return agent.InstallManagedSkill(projectDir, agentName, skillName, frontmatter+content)
}

func RemoveSkill(projectDir, agentName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	return agent.RemoveManagedSkill(projectDir, agentName, skillName)
}
