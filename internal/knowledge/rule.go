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
	skillContent := brand.ResolveModuleSkill("knowledge", KnowledgeRuleContent(InstalledContextsIn(projectDir), docsDir))
	frontmatter, err := agent.SkillFrontmatter(knowledgeSkillName, "Knowledge-first: project documentation, wiki retrieval, architecture, decisions, specifications, and provenance; use wiki tools before reading documentation files.")
	if err != nil {
		return err
	}
	return agent.InstallManagedSkill(projectDir, agentName, knowledgeSkillName, frontmatter+skillContent)
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
