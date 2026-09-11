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
	skillContent := brand.ResolveModuleSkill("ast", ASTRuleContent())
	frontmatter, err := agent.SkillFrontmatter(astSkillName, "AST-first: code discovery and structural analysis for symbols, relationships, impact, source selection, and code from installed contexts; read this skill before native search.")
	if err != nil {
		return err
	}
	return agent.InstallManagedSkill(projectDir, agentName, astSkillName, frontmatter+skillContent)
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
