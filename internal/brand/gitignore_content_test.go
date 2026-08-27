package brand

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// The block is asserted against real git rather than by string comparison, because
// the property that matters is not its shape. gitignore anchors a pattern that has
// a separator in the middle, so the difference between ignoring this project's
// machine state and ignoring every nested project's instead is one "**/" prefix
// that reads as decoration. Only git can say which of the two was written.
func TestGitignoreContentIgnoresRuntimeAndGrammarsAtAnyDepthAndNothingElse(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}

	repo := t.TempDir()
	runGit(t, repo, "init")
	writeFile(t, filepath.Join(repo, ".gitignore"), GitignoreContent())

	ignored := []string{
		filepath.Join(RuntimeSubdir(), "daemon", "daemon.log"),
		filepath.Join(RuntimeSubdir(), "cache", "skills", "claude", "graphit-ast"),
		filepath.Join(RuntimeSubdir(), "cache", "artifacts", "kiro", "rule", "ast"),
		filepath.Join(RuntimeSubdir(), "ast", "export", "graph.ast"),
		filepath.Join(RuntimeSubdir(), "dream", "01ABC.md"),
		filepath.Join(RuntimeSubdir(), "mandate.hash"),
		filepath.Join(RuntimeSubdir(), "sync.stamp"),
		filepath.Join(RuntimeSubdir(), "sync.lock"),
		filepath.Join("grammars", "treesitter", "tree-sitter-go.so"),
		filepath.Join("grammars", "antlr", "antlr-plsql.grammar"),
	}
	// Repository-owned source overrides remain visible.
	tracked := []string{
		filepath.Join("ast", "queries", "go.yaml"),
		filepath.Join("rules", "ast.md"),
	}

	for _, rel := range append(append([]string{}, ignored...), tracked...) {
		writeFile(t, filepath.Join(repo, DotDir(), rel), "x\n")
	}
	nestedIgnored := []string{
		filepath.Join("internal", "pkg", DotDir(), RuntimeSubdir(), "daemon", "daemon.log"),
		filepath.Join("internal", "pkg", DotDir(), "grammars", "treesitter", "tree-sitter-go.so"),
	}
	for _, rel := range nestedIgnored {
		writeFile(t, filepath.Join(repo, rel), "x\n")
	}

	seen := map[string]bool{}
	for _, line := range strings.Split(runGit(t, repo, "status", "--porcelain", "--untracked-files=all"), "\n") {
		if fields := strings.Fields(line); len(fields) == 2 && fields[0] == "??" {
			seen[fields[1]] = true
		}
	}

	for _, rel := range ignored {
		if p := filepath.ToSlash(filepath.Join(DotDir(), rel)); seen[p] {
			t.Errorf("git sees %s — generated or local binary state is not being ignored", p)
		}
	}
	for _, rel := range nestedIgnored {
		if p := filepath.ToSlash(rel); seen[p] {
			t.Errorf("git sees %s — the pattern is anchored to the project root instead of matching at any depth", p)
		}
	}
	for _, rel := range tracked {
		if p := filepath.ToSlash(filepath.Join(DotDir(), rel)); !seen[p] {
			t.Errorf("git does not see %s — the block is hiding repository-owned overrides", p)
		}
	}
}

func runGit(t *testing.T, dir string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", args...)
	cmd.Dir = dir
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %s: %v\n%s", strings.Join(args, " "), err, out)
	}
	return string(out)
}

func writeFile(t *testing.T, path, body string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
}
