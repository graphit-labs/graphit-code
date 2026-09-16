package knowledge

import (
	"os"
	"path"
	"path/filepath"
	"regexp"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
)

func TestKnowledgeReferenceInstallTracksProjectDocumentationRoot(t *testing.T) {
	projectDir := t.TempDir()
	for _, docsDir := range []string{"handbook", "manual/product"} {
		lock := &hub.Lockfile{Config: config.ConfigMap{"knowledge": map[string]any{"docs_dir": docsDir}}}
		if err := hub.SaveLockfile(filepath.Join(projectDir, brand.LockFileName()), lock); err != nil {
			t.Fatal(err)
		}
		if err := InstallSkill(projectDir, "codex"); err != nil {
			t.Fatal(err)
		}
		skillDir := agent.GetSkillDir(agent.GetAdapter("codex"), projectDir, knowledgeSkillName)
		main, err := os.ReadFile(filepath.Join(skillDir, "SKILL.md"))
		if err != nil {
			t.Fatal(err)
		}
		if !strings.HasSuffix(string(main), KnowledgeRuleContent(nil, docsDir)) {
			t.Fatalf("installed entrypoint did not adopt project documentation root %q", docsDir)
		}
		for reference, content := range SkillReferences() {
			if !strings.Contains(string(main), "]("+reference+")") {
				t.Errorf("installed reference %q is not discoverable from SKILL.md", reference)
			}
			if strings.Contains(string(main), content) {
				t.Errorf("conditional reference %q was eagerly embedded", reference)
			}
			got, err := os.ReadFile(filepath.Join(skillDir, filepath.FromSlash(reference)))
			if err != nil || string(got) != content {
				t.Fatalf("installed reference %q differs from its generated source: %v", reference, err)
			}
		}
	}
}

// Check navigation in the complete worked artifact, rather than asserting that
// the prose happens to contain a list of desired quality words.
func TestKnowledgeWorkedDomainNavigation(t *testing.T) {
	pages := map[string]string{}
	pagePattern := regexp.MustCompile(`(?s)## Page: ([^\n]+)\n\n~~~~markdown\n(.*?)\n~~~~`)
	for _, match := range pagePattern.FindAllStringSubmatch(knowledgeWorkedDomain, -1) {
		pages[match[1]] = match[2]
	}
	if len(pages) == 0 {
		t.Fatal("worked documentation set contains no complete page artifacts")
	}
	links := regexp.MustCompile(`\[[^\]]+\]\(([^)]+)\)`)
	headings := regexp.MustCompile(`(?m)^#{1,6} (.+)$`)
	for name, content := range pages {
		for _, link := range links.FindAllStringSubmatch(content, -1) {
			parts := strings.SplitN(link[1], "#", 2)
			if strings.Contains(parts[0], "://") {
				continue
			}
			target := name
			if parts[0] != "" {
				target = path.Clean(path.Join(path.Dir(name), parts[0]))
			}
			page, exists := pages[target]
			if !exists {
				t.Errorf("example page %s links to missing page %s", name, target)
				continue
			}
			if len(parts) == 2 {
				found := false
				for _, heading := range headings.FindAllStringSubmatch(page, -1) {
					if strings.ReplaceAll(strings.ToLower(heading[1]), " ", "-") == parts[1] {
						found = true
					}
				}
				if !found {
					t.Errorf("example page %s links to missing section %s", name, link[1])
				}
			}
		}
	}
}
