package task

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
)

var skillName = brand.SkillDirName("task")

const skillDescription = "Plan and execute project work with specifications, dependency-ordered tasks, acceptance evidence and resumable handoffs; use for features, fixes, analysis and backlog changes."

func InstallSkill(projectDir, agentName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	content := brand.ResolveModuleSkillIn(projectDir, "task", RuleContent())
	frontmatter, err := agent.SkillFrontmatter(skillName, skillDescription)
	if err != nil {
		return err
	}
	return agent.InstallManagedSkillWithReferences(projectDir, agentName, skillName, frontmatter+content, SkillReferences())
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

// SkillReferences returns a fresh map so callers cannot mutate later installations.
func SkillReferences() map[string]string {
	return map[string]string{
		"references/planning.md":       taskPlanningReference,
		"references/worked-feature.md": taskWorkedExamples,
		"references/worked-system.md":  taskWorkedSystem,
		"references/execution.md":      taskExecutionReference,
	}
}
