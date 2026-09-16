package commands

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"unicode/utf8"

	"gopkg.in/yaml.v3"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
	graphtask "github.com/graphit-labs/graphit-code/internal/task"
)

// A SKILL.md whose frontmatter does not parse is not a degraded skill, it is an
// invisible one: a strict Agent discovers no metadata, never offers the skill, and
// logs nothing. This test installs every managed skill through its real
// generator and reads back what an Agent would read.
func TestManagedSkillFrontmatterIsValid(t *testing.T) {
	generators := map[string]func(string, string) error{
		"ast":       ast.InstallSkill,
		"hub":       hub.InstallSkill,
		"knowledge": knowledge.InstallSkill,
		"memory":    memory.InstallSkill,
		"task":      graphtask.InstallSkill,
	}

	for _, agentName := range agent.SupportedAgents() {
		adapter := agent.GetAdapter(agentName)
		if adapter == nil {
			t.Fatalf("GetAdapter(%q) returned nil for a supported Agent", agentName)
		}

		for module, install := range generators {
			t.Run(agentName+"/"+module, func(t *testing.T) {
				projectDir := t.TempDir()
				if err := install(projectDir, agentName); err != nil {
					t.Fatalf("installing the %s skill for %s: %v", module, agentName, err)
				}

				skillName := brand.SkillDirName(module)
				skillDir := agent.GetSkillDir(adapter, projectDir, skillName)
				if skillDir == "" {
					t.Fatalf("GetSkillDir returned no path for %s/%s", agentName, module)
				}

				data, err := os.ReadFile(filepath.Join(skillDir, "SKILL.md"))
				if err != nil {
					t.Fatalf("reading the installed skill: %v", err)
				}
				content := string(data)

				block, body := splitFrontmatter(t, content)

				var fields map[string]string
				if err := yaml.Unmarshal([]byte(block), &fields); err != nil {
					t.Fatalf("frontmatter is not valid YAML — the skill would be invisible to a strict Agent: %v\nfrontmatter:\n%s", err, block)
				}
				if len(fields) != 2 {
					t.Errorf("frontmatter has %d fields (%v), want exactly name and description", len(fields), fields)
				}

				if fields["name"] != filepath.Base(skillDir) {
					t.Errorf("name = %q but the skill directory is %q", fields["name"], filepath.Base(skillDir))
				}
				if n := utf8.RuneCountInString(fields["name"]); n > agent.MaxSkillNameLength {
					t.Errorf("name is %d characters, over the %d-character limit", n, agent.MaxSkillNameLength)
				}

				description := fields["description"]
				if strings.TrimSpace(description) == "" {
					t.Error("description is empty, and it is what the Agent matches a request against")
				}
				if n := utf8.RuneCountInString(description); n > agent.MaxSkillDescriptionLength {
					t.Errorf("description is %d characters, over the %d-character limit", n, agent.MaxSkillDescriptionLength)
				}

				if strings.TrimSpace(body) == "" {
					t.Error("the skill has frontmatter but no body")
				}
			})
		}
	}
}

func TestManagedSkillBodiesMatchAcrossAdapters(t *testing.T) {
	generators := map[string]func(string, string) error{
		"ast":       ast.InstallSkill,
		"hub":       hub.InstallSkill,
		"knowledge": knowledge.InstallSkill,
		"memory":    memory.InstallSkill,
		"task":      graphtask.InstallSkill,
	}
	references := map[string]map[string]string{
		"ast": ast.SkillReferences(), "hub": hub.SkillReferences(),
		"knowledge": knowledge.SkillReferences(), "memory": memory.SkillReferences(),
		"task": graphtask.SkillReferences(),
	}

	_, sourceFile, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("resolving repository root")
	}
	repoRoot := filepath.Clean(filepath.Join(filepath.Dir(sourceFile), "..", "..", ".."))

	for module, install := range generators {
		t.Run(module, func(t *testing.T) {
			var canonical string
			versionedCopies := 0
			for _, agentName := range agent.SupportedAgents() {
				projectDir := t.TempDir()
				if err := install(projectDir, agentName); err != nil {
					t.Fatalf("installing the %s skill for %s: %v", module, agentName, err)
				}
				adapter := agent.GetAdapter(agentName)
				skillDir := agent.GetSkillDir(adapter, projectDir, brand.SkillDirName(module))
				data, err := os.ReadFile(filepath.Join(skillDir, "SKILL.md"))
				if err != nil {
					t.Fatalf("reading the %s skill for %s: %v", module, agentName, err)
				}
				_, body := splitFrontmatter(t, string(data))
				for path, want := range references[module] {
					if strings.TrimSpace(want) == "" || !strings.Contains(body, "]("+path+")") {
						t.Fatalf("%s reference %s is empty or not linked from SKILL.md", module, path)
					}
					installed, err := os.ReadFile(filepath.Join(skillDir, filepath.FromSlash(path)))
					if err != nil || string(installed) != want {
						t.Fatalf("%s/%s reference %s differs from its generator: %v", agentName, module, path, err)
					}
				}
				if canonical == "" {
					canonical = body
				} else if body != canonical {
					t.Fatalf("%s generated a different %s skill body", agentName, module)
				}

				relativeSkillDir, err := filepath.Rel(projectDir, skillDir)
				if err != nil {
					t.Fatalf("resolving the %s skill path for %s: %v", module, agentName, err)
				}
				versioned, err := os.ReadFile(filepath.Join(repoRoot, relativeSkillDir, "SKILL.md"))
				if os.IsNotExist(err) {
					continue
				}
				if err != nil {
					t.Fatalf("reading the versioned %s skill for %s: %v", module, agentName, err)
				}
				versionedCopies++
				if string(versioned) != string(data) {
					t.Fatalf("the versioned %s skill for %s differs from its canonical generator", module, agentName)
				}
				for path, want := range references[module] {
					versionedReference, err := os.ReadFile(filepath.Join(repoRoot, relativeSkillDir, filepath.FromSlash(path)))
					if err != nil || string(versionedReference) != want {
						t.Fatalf("the versioned %s reference %s for %s differs from its canonical generator: %v", module, path, agentName, err)
					}
				}
			}
			if versionedCopies == 0 {
				t.Fatalf("no versioned %s skill copy was verified", module)
			}
		})
	}
}

// Installing for another checkout must resolve that checkout's instructions,
// not silently borrow overrides from the process working directory.
func TestManagedSkillsResolveTargetProjectOverrides(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	callerDir := t.TempDir()
	t.Chdir(callerDir)
	generators := map[string]func(string, string) error{
		"ast": ast.InstallSkill, "hub": hub.InstallSkill,
		"knowledge": knowledge.InstallSkill, "memory": memory.InstallSkill,
		"task": graphtask.InstallSkill,
	}
	writeOverride := func(projectDir, module, body string) {
		t.Helper()
		rulesDir := filepath.Join(projectDir, brand.DotDir(), "rules")
		if err := os.MkdirAll(rulesDir, 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(rulesDir, module+"_skill.md"), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	for module, install := range generators {
		writeOverride(callerDir, module, "WRONG CHECKOUT")
		for _, agentName := range agent.SupportedAgents() {
			t.Run(module+"/"+agentName, func(t *testing.T) {
				projectDir := t.TempDir()
				want := "# Target " + module + "\nProject-specific instructions.\n"
				writeOverride(projectDir, module, want)
				if err := install(projectDir, agentName); err != nil {
					t.Fatal(err)
				}
				skillDir := agent.GetSkillDir(agent.GetAdapter(agentName), projectDir, brand.SkillDirName(module))
				data, err := os.ReadFile(filepath.Join(skillDir, "SKILL.md"))
				if err != nil {
					t.Fatal(err)
				}
				_, body := splitFrontmatter(t, string(data))
				if strings.TrimSpace(body) != strings.TrimSpace(want) {
					t.Fatalf("installed skill did not use target project override: %s", body)
				}
			})
		}
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
