package task

import (
	"os"
	"path"
	"path/filepath"
	"regexp"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
)

func TestTaskGeneratedReferenceBundle(t *testing.T) {
	refs := SkillReferences()
	if len(refs) == 0 {
		t.Fatal("Task has no selectively loaded references")
	}
	docs := map[string]string{"SKILL.md": RuleContent()}
	for name, content := range refs {
		if path.Clean(name) != name || !strings.HasPrefix(name, "references/") || strings.TrimSpace(content) == "" {
			t.Fatalf("invalid generated reference %q", name)
		}
		if strings.Contains(RuleContent(), content) {
			t.Fatalf("reference %q is eagerly embedded in SKILL.md", name)
		}
		docs[name] = content
	}
	links := regexp.MustCompile(`\[[^\]]+\]\(([^)]+)\)`)
	reached := map[string]bool{"SKILL.md": true}
	var visit func(string)
	visit = func(name string) {
		for _, match := range links.FindAllStringSubmatch(docs[name], -1) {
			target := strings.SplitN(match[1], "#", 2)[0]
			if target == "" || strings.Contains(target, "://") {
				continue
			}
			resolved := path.Clean(path.Join(path.Dir(name), target))
			if _, ok := docs[resolved]; !ok {
				t.Fatalf("generated %s links to missing resource %s", name, resolved)
			}
			if !reached[resolved] {
				reached[resolved] = true
				visit(resolved)
			}
		}
	}
	visit("SKILL.md")
	for name := range refs {
		if !reached[name] {
			t.Errorf("generated reference %q cannot be discovered from SKILL.md", name)
		}
	}

	project := t.TempDir()
	if err := InstallSkill(project, "codex"); err != nil {
		t.Fatal(err)
	}
	dir := agent.GetSkillDir(agent.GetAdapter("codex"), project, skillName)
	for name, want := range docs {
		got, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(name)))
		if err != nil {
			t.Fatal(err)
		}
		if name == "SKILL.md" {
			if !strings.HasSuffix(string(got), want) {
				t.Fatal("installed skill does not contain the generated entrypoint")
			}
		} else if string(got) != want {
			t.Errorf("installed resource %q differs from its generated source", name)
		}
	}
}

func TestTaskSkillReferencesDoNotShareMutableMap(t *testing.T) {
	first := SkillReferences()
	for name := range first {
		first[name] = "caller mutation"
	}
	for name, content := range SkillReferences() {
		if content == "caller mutation" {
			t.Fatalf("caller mutation leaked into reference %q", name)
		}
	}
}
