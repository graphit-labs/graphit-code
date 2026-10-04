// Package hubartifact locates installed Hub artifacts on disk and reads the installed
// rules a project claims. It needs only the project lockfile, so the hook binary can
// load rules without the Hub registry or its stores.
package hubartifact

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/projectlock"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// TypeFolderMap is the resource folder of each artifact type.
var TypeFolderMap = map[projectlock.ArtifactType]string{
	projectlock.TypeAgent:     "agents",
	projectlock.TypeRule:      "rules",
	projectlock.TypeWorkflow:  "workflows",
	projectlock.TypeSkill:     "skills",
	projectlock.TypeKnowledge: "knowledge",
	projectlock.TypeAST:       "ast",
	projectlock.TypeMCP:       "mcp-servers",
	projectlock.TypeCommand:   "commands",
	projectlock.TypePower:     "powers",
	projectlock.TypeLanguage:  "languages",
}

// CacheDirIn is where an installed artifact lives under the global directory.
func CacheDirIn(globalRoot string, artType projectlock.ArtifactType, id, version, projectID string) string {
	return filepath.Join(globalRoot, "artifacts", "modules", store.SanitizeSegment(projectID), string(artType), store.SanitizeSegment(id), store.VersionPathSegment(version))
}

// FindCanonicalFile returns the artifact's main file in dir, or the first file there.
func FindCanonicalFile(artType, dir string) string {
	canonical := map[string]string{
		"rule": "RULE.md", "command": "COMMAND.md",
		"agent": "AGENT.md", "skill": "SKILL.md",
	}
	if name, ok := canonical[artType]; ok {
		path := filepath.Join(dir, name)
		if _, err := os.Stat(path); err == nil {
			return path
		}
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		return ""
	}
	for _, e := range entries {
		if !e.IsDir() {
			return filepath.Join(dir, e.Name())
		}
	}
	return ""
}

// ResolveDir returns the directory an installed artifact is read from.
func ResolveDir(meta *projectlock.ArtifactMeta, artType projectlock.ArtifactType, artID string, pp *paths.ProjectPaths) string {
	if meta.LinkSource != "" {
		return meta.LinkSource
	}

	globalDir := brand.GlobalDir()
	if globalDir != "" {
		remoteID := meta.RemoteID
		if remoteID == "" {
			remoteID = artID
		}
		return CacheDirIn(globalDir, artType, remoteID, meta.Version, meta.ProjectID)
	}

	folder := TypeFolderMap[artType]
	return filepath.Join(pp.ResourcesDir, folder, artID, meta.Version)
}

// InstalledRuleContext returns the instruction bodies of the Hub rules the project
// claims, read from each installed artifact. A rule that cannot be read is reported
// in the error while the others are still returned.
func InstalledRuleContext(projectDir string) (string, error) {
	lf, err := projectlock.Load(filepath.Join(projectDir, brand.LockFileName()))
	if err != nil {
		return "", err
	}
	if lf == nil {
		return "", nil
	}
	rules := lf.Artifacts[projectlock.TypeRule]

	ids := make([]string, 0, len(rules))
	for id := range rules {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	projectPaths := paths.GetPathsForProject("", projectDir)

	sections := make([]string, 0, len(ids))
	failures := make([]string, 0)
	for _, id := range ids {
		source := FindCanonicalFile(string(projectlock.TypeRule), ResolveDir(rules[id], projectlock.TypeRule, id, projectPaths))
		if source == "" {
			failures = append(failures, id+": RULE.md not found")
			continue
		}
		content, readErr := os.ReadFile(source)
		if readErr != nil {
			failures = append(failures, id+": "+readErr.Error())
			continue
		}
		body := stripInstructionFrontmatter(string(content))
		if body == "" {
			continue
		}
		sections = append(sections, "## Hub rule: "+id+"\n"+body)
	}

	context := ""
	if len(sections) > 0 {
		context = "# Installed Hub rules\n\n" + strings.Join(sections, "\n\n")
	}
	if len(failures) > 0 {
		return context, fmt.Errorf("loading installed Hub rules: %s", strings.Join(failures, "; "))
	}
	return context, nil
}

func stripInstructionFrontmatter(content string) string {
	trimmed := strings.TrimSpace(content)
	if !strings.HasPrefix(trimmed, "---\n") {
		return trimmed
	}
	lines := strings.Split(trimmed, "\n")
	for i := 1; i < len(lines); i++ {
		if strings.TrimSpace(lines[i]) == "---" {
			return strings.TrimSpace(strings.Join(lines[i+1:], "\n"))
		}
	}
	return trimmed
}
