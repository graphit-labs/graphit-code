package knowledge

import (
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
)

var knowledgeSkillName = brand.SkillDirName("knowledge")

func resolveDocsDirFromProject(projectDir string) string {
	var projectCfg config.ConfigMap
	if lf, err := hub.LoadLockfile(filepath.Join(projectDir, brand.LockFileName())); err == nil && lf != nil {
		projectCfg = lf.Config
	}
	return config.ResolveDocsDir(nil, projectCfg)
}

func InstallSkill(projectDir, agentName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	docsDir := resolveDocsDirFromProject(projectDir)
	skillContent := brand.ResolveModuleSkillIn(projectDir, "knowledge", KnowledgeRuleContent(InstalledContextsIn(projectDir), docsDir))
	frontmatter, err := agent.SkillFrontmatter(knowledgeSkillName, "Knowledge: retrieve and maintain user/technical documentation by business domain; verify code/documentation consistency for every changed work unit using wiki and implementation evidence.")
	if err != nil {
		return err
	}
	return agent.InstallManagedSkillWithReferences(projectDir, agentName, knowledgeSkillName, frontmatter+skillContent, SkillReferences())
}

func RemoveSkill(projectDir, agentName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	return agent.RemoveManagedSkill(projectDir, agentName, knowledgeSkillName)
}
