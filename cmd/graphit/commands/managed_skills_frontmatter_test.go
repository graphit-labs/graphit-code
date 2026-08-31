package commands

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"unicode/utf8"

	"gopkg.in/yaml.v3"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
)

// A SKILL.md whose frontmatter does not parse is not a degraded skill, it is an
// invisible one: a strict IDE discovers no metadata, never offers the skill, and
// logs nothing. This test installs every managed skill through its real
// generator and reads back what an IDE would read.
func TestManagedSkillFrontmatterIsValid(t *testing.T) {
	generators := map[string]func(string, string) error{
		"ast":       ast.InstallSkill,
		"hub":       hub.InstallSkill,
		"knowledge": knowledge.InstallSkill,
		"memory":    memory.InstallSkill,
	}

	for _, ideName := range ide.SupportedIDEs() {
		adapter := ide.GetAdapter(ideName)
		if adapter == nil {
			t.Fatalf("GetAdapter(%q) returned nil for a supported IDE", ideName)
		}

		for module, install := range generators {
			t.Run(ideName+"/"+module, func(t *testing.T) {
				projectDir := t.TempDir()
				if err := install(projectDir, ideName); err != nil {
					t.Fatalf("installing the %s skill for %s: %v", module, ideName, err)
				}

				skillName := brand.SkillDirName(module)
				skillDir := ide.GetSkillDir(adapter, projectDir, skillName)
				if skillDir == "" {
					t.Fatalf("GetSkillDir returned no path for %s/%s", ideName, module)
				}

				data, err := os.ReadFile(filepath.Join(skillDir, "SKILL.md"))
				if err != nil {
					t.Fatalf("reading the installed skill: %v", err)
				}
				content := string(data)

				block, body := splitFrontmatter(t, content)

				var fields map[string]string
				if err := yaml.Unmarshal([]byte(block), &fields); err != nil {
					t.Fatalf("frontmatter is not valid YAML — the skill would be invisible to a strict IDE: %v\nfrontmatter:\n%s", err, block)
				}
				if len(fields) != 2 {
					t.Errorf("frontmatter has %d fields (%v), want exactly name and description", len(fields), fields)
				}

				// The name is what the IDE matches against the directory: a
				// mismatch costs the slash command, and an invalid character
				// costs the whole skill.
				if fields["name"] != filepath.Base(skillDir) {
					t.Errorf("name = %q but the skill directory is %q", fields["name"], filepath.Base(skillDir))
				}
				if n := utf8.RuneCountInString(fields["name"]); n > ide.MaxSkillNameLength {
					t.Errorf("name is %d characters, over the %d-character limit", n, ide.MaxSkillNameLength)
				}

				description := fields["description"]
				if strings.TrimSpace(description) == "" {
					t.Error("description is empty, and it is what the IDE matches a request against")
				}
				if n := utf8.RuneCountInString(description); n > ide.MaxSkillDescriptionLength {
					t.Errorf("description is %d characters, over the %d-character limit", n, ide.MaxSkillDescriptionLength)
				}

				if strings.TrimSpace(body) == "" {
					t.Error("the skill has frontmatter but no body")
				}
			})
		}
	}
}

// Every managed description is prose with a colon in it — "Use when: ...",
// "MANDATORY: ..." — which is exactly the shape a plain YAML scalar cannot hold.
// Asserting the colon is still there keeps the test above honest: it is what
// makes valid frontmatter evidence of correct quoting rather than of bland
// content.
func TestManagedSkillDescriptionsStillContainAColon(t *testing.T) {
	descriptions := map[string]func(string, string) error{
		"ast":       ast.InstallSkill,
		"hub":       hub.InstallSkill,
		"knowledge": knowledge.InstallSkill,
		"memory":    memory.InstallSkill,
	}

	for module, install := range descriptions {
		t.Run(module, func(t *testing.T) {
			projectDir := t.TempDir()
			if err := install(projectDir, "kiro"); err != nil {
				t.Fatalf("installing the %s skill: %v", module, err)
			}

			skillName := brand.SkillDirName(module)
			skillDir := ide.GetSkillDir(ide.GetAdapter("kiro"), projectDir, skillName)
			data, err := os.ReadFile(filepath.Join(skillDir, "SKILL.md"))
			if err != nil {
				t.Fatalf("reading the installed skill: %v", err)
			}

			block, _ := splitFrontmatter(t, string(data))
			var fields map[string]string
			if err := yaml.Unmarshal([]byte(block), &fields); err != nil {
				t.Fatalf("frontmatter is not valid YAML: %v", err)
			}
			if !strings.Contains(fields["description"], ": ") {
				t.Errorf("the %s description no longer contains %q, so the frontmatter test above no longer proves quoting works: %q", module, ": ", fields["description"])
			}
		})
	}
}

func splitFrontmatter(t *testing.T, content string) (block, body string) {
	t.Helper()
	if !strings.HasPrefix(content, "---\n") {
		t.Fatalf("the skill does not open with a frontmatter delimiter:\n%.120s", content)
	}
	rest := strings.TrimPrefix(content, "---\n")
	end := strings.Index(rest, "\n---")
	if end < 0 {
		t.Fatalf("the frontmatter block is not closed:\n%.200s", content)
	}
	return rest[:end], strings.TrimPrefix(rest[end:], "\n---")
}
