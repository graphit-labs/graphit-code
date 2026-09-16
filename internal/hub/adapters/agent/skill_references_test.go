package agent

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestManagedSkillReferencesAcrossAdapters(t *testing.T) {
	for _, name := range SupportedAgents() {
		t.Run(name, func(t *testing.T) {
			project := t.TempDir()
			refs := map[string]string{"references/planning.md": "planning example", "references/examples/feature.md": "feature example"}
			install := func() {
				t.Helper()
				if err := InstallManagedSkillWithReferences(project, name, "graphit-example", "skill body", refs); err != nil {
					t.Fatal(err)
				}
			}
			install()
			dir := GetSkillDir(GetAdapter(name), project, "graphit-example")
			for path, want := range refs {
				got, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(path)))
				if err != nil || string(got) != want {
					t.Fatalf("reference %s = %q, %v; want %q", path, got, err, want)
				}
			}
			file := filepath.Join(dir, "references", "planning.md")
			stableTime := time.Unix(1000000000, 0)
			if err := os.Chtimes(file, stableTime, stableTime); err != nil {
				t.Fatal(err)
			}
			install()
			stat, err := os.Stat(file)
			if err != nil || !stat.ModTime().Equal(stableTime) {
				t.Fatalf("unchanged reference was rewritten: %v, %v", stat, err)
			}
			if err := os.Remove(file); err != nil {
				t.Fatal(err)
			}
			refs["references/examples/feature.md"] = "updated feature"
			install()
			for path, want := range refs {
				got, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(path)))
				if err != nil || string(got) != want {
					t.Fatalf("resource not restored/updated: %s = %q, %v", path, got, err)
				}
			}
			custom := filepath.Join(dir, "references", "custom.md")
			if err := os.WriteFile(custom, []byte("project notes"), 0o644); err != nil {
				t.Fatal(err)
			}
			install()
			if got, err := os.ReadFile(custom); err != nil || string(got) != "project notes" {
				t.Fatalf("unowned reference changed: %q, %v", got, err)
			}
		})
	}
}

func TestManagedSkillReferencesRejectInvalidPaths(t *testing.T) {
	for _, path := range []string{"", "../escape.md", "/tmp/escape.md", "SKILL.md", "references/../escape.md", "references//example.md", "references\\example.md"} {
		t.Run(path, func(t *testing.T) {
			project := t.TempDir()
			if err := InstallManagedSkillWithReferences(project, "codex", "graphit-example", "body", map[string]string{path: "bad"}); err == nil {
				t.Fatalf("accepted invalid path %q", path)
			}
			if _, err := os.Stat(GetSkillDir(GetAdapter("codex"), project, "graphit-example")); !os.IsNotExist(err) {
				t.Fatalf("invalid bundle modified the filesystem: %v", err)
			}
		})
	}
}

func TestManagedSkillReferencesRejectSymlink(t *testing.T) {
	project, outside := t.TempDir(), t.TempDir()
	dir := GetSkillDir(GetAdapter("codex"), project, "graphit-example")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(outside, filepath.Join(dir, "references")); err != nil {
		t.Fatal(err)
	}
	if err := InstallManagedSkillWithReferences(project, "codex", "graphit-example", "body", map[string]string{"references/example.md": "example"}); err == nil {
		t.Fatal("accepted a symlink outside the skill")
	}
	if entries, err := os.ReadDir(outside); err != nil || len(entries) != 0 {
		t.Fatalf("wrote outside the skill: %v, %v", entries, err)
	}
}
