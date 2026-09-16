package ast

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
)

var astSkillName = brand.SkillDirName("ast")

func InstallSkill(projectDir, agentName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	skillContent := brand.ResolveModuleSkillIn(projectDir, "ast", ASTRuleContent())
	frontmatter, err := agent.SkillFrontmatter(astSkillName, "AST: replace local code search, file reads and symbol navigation with graph/source queries; inspect callers, metrics and change impact, including installed contexts.")
	if err != nil {
		return err
	}
	return agent.InstallManagedSkillWithReferences(projectDir, agentName, astSkillName, frontmatter+skillContent, SkillReferences())
}

func RemoveSkill(projectDir, agentName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	return agent.RemoveManagedSkill(projectDir, agentName, astSkillName)
}
