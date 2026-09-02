package ast

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

var astSkillName = brand.SkillDirName("ast")

func InstallRule(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	if err := ide.UpsertMandateTrigger(projectDir, ideName, "ast_rule", MandateTrigger()); err != nil {
		return err
	}
	return InstallSkill(projectDir, ideName)
}

func InstallSkill(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	skillContent := brand.ResolveModuleSkill("ast", ASTRuleContent())
	frontmatter, err := ide.SkillFrontmatter(astSkillName, "AST-first: code discovery and structural analysis for symbols, relationships, impact, source selection, and code from installed contexts; read this skill before native search.")
	if err != nil {
		return err
	}
	return ide.InstallManagedSkill(projectDir, ideName, astSkillName, frontmatter+skillContent)
}

func RemoveRule(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	return ide.RemoveMandateTrigger(projectDir, ideName, "ast_rule")
}

func RemoveSkill(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	return ide.RemoveManagedSkill(projectDir, ideName, astSkillName)
}
