package knowledge

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
)

// readmeBaseName is the document every repository has, whatever its extension.
const readmeBaseName = "readme"

// ScopeFor returns the wiki scope for a project: the configured documentation
// tree, plus the project root's README unless knowledge.include_readme turns it
// off.
//
// The README is in the list independently of knowledge.docs_dir because it is
// the one page a reader reaches for first and it is, by convention, not in the
// docs tree. Scoping the wiki to docs/ without this would have quietly dropped
// the front page of every project.
func ScopeFor(projectDir string, inlineCfg, projectCfg config.ConfigMap) WikiScope {
	scope := WikiScope{Subdir: config.ResolveDocsDir(inlineCfg, projectCfg)}

	if !config.ResolveKnowledgeIncludeReadme(inlineCfg, projectCfg) {
		return scope
	}
	if readme := RootReadme(projectDir, config.ResolveKnowledgeExtensions(inlineCfg, projectCfg)); readme != "" {
		scope.ExtraFiles = append(scope.ExtraFiles, readme)
	}
	return scope
}

// RootReadme returns the name of the README at the top of projectDir, or "" when
// there is none the wiki can read.
//
// It reads the directory rather than probing a fixed list of names so that
// README.md, readme.markdown, README.rst and README.adoc all work without
// enumerating the cross product of casing and extension. The extension still has
// to be one the wiki indexes — a README.pdf is not a document this pipeline can
// chunk. When a project has more than one, the first in filename order wins,
// which is os.ReadDir's order.
func RootReadme(projectDir string, exts map[string]bool) string {
	entries, err := os.ReadDir(projectDir)
	if err != nil {
		return ""
	}
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		name := e.Name()
		ext := strings.ToLower(filepath.Ext(name))
		if !exts[ext] {
			continue
		}
		if strings.EqualFold(strings.TrimSuffix(name, filepath.Ext(name)), readmeBaseName) {
			return name
		}
	}
	return ""
}
