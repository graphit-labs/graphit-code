package hub

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
)

// InstalledRuleContext returns the instruction bodies of Hub rule artifacts
// claimed by a project. Rules are read from their authoritative installed
// artifact rather than copied into an IDE-specific rules directory.
func InstalledRuleContext(projectDir string) (string, error) {
	lf, err := LoadLockfile(filepath.Join(projectDir, brand.LockFileName()))
	if err != nil {
		return "", err
	}
	if lf == nil {
		return "", nil
	}
	rules := lf.Artifacts[TypeRule]

	ids := make([]string, 0, len(rules))
	for id := range rules {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	projectPaths := paths.GetPathsForProject("", projectDir)

	sections := make([]string, 0, len(ids))
	failures := make([]string, 0)
	for _, id := range ids {
		source := findCanonicalFile(string(TypeRule), resolveArtifactPath(rules[id], TypeRule, id, projectPaths))
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
