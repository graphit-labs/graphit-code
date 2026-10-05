package git

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// A linked worktree has a `.git` file, not a directory; its hooks are the ones in
// the repository's shared git directory, for both registration styles.
func TestHooksInstallFromALinkedWorktree(t *testing.T) {
	repo := hookTestRepo(t)
	for _, args := range [][]string{
		{"-c", "user.name=t", "-c", "user.email=t@example.com", "commit", "-q", "--allow-empty", "-m", "init"},
		{"worktree", "add", "-q", "-b", "feature/x", filepath.Join(t.TempDir(), "wt")},
	} {
		cmd := exec.Command("git", append([]string{"-C", repo}, args...)...)
		cmd.Env = withoutInheritedGitScope(os.Environ())
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v: %s", args, err, out)
		}
	}
	list := exec.Command("git", "-C", repo, "worktree", "list", "--porcelain")
	list.Env = withoutInheritedGitScope(os.Environ())
	out, err := list.Output()
	if err != nil {
		t.Fatal(err)
	}
	var worktree string
	for _, line := range strings.Split(string(out), "\n") {
		if dir, ok := strings.CutPrefix(line, "worktree "); ok && filepath.Base(dir) == "wt" {
			worktree = dir
		}
	}
	if worktree == "" {
		t.Fatal("worktree not created")
	}

	shared := filepath.Join(repo, ".git", "hooks")
	legacy := NewHookManager(worktree)
	if got, _ := filepath.EvalSymlinks(legacy.hooksDir); got != mustEval(t, shared) {
		t.Fatalf("worktree hooks dir = %q, want the shared %q", legacy.hooksDir, shared)
	}
	legacy.gitBinary = fakeGitVersion(t, "2.40.0")
	if err := legacy.Install(false); err != nil {
		t.Fatalf("legacy install from a worktree: %v", err)
	}
	if data, err := os.ReadFile(filepath.Join(shared, preCommitEvent)); err != nil || !strings.Contains(string(data), hookBlockMarker()) {
		t.Fatalf("legacy hook not in the shared hooks dir: %v %q", err, data)
	}

	modern := NewHookManager(worktree)
	modern.gitBinary = fakeGitVersion(t, "2.54.0")
	if err := modern.Install(false); err != nil {
		t.Fatalf("configured install from a worktree: %v", err)
	}
	main := NewHookManager(repo)
	if event, err := main.gitConfigGet(configuredHook() + ".event"); err != nil || event != preCommitEvent {
		t.Fatalf("the registration made in a worktree must reach the repository: %q %v", event, err)
	}
}

func mustEval(t *testing.T, path string) string {
	t.Helper()
	got, err := filepath.EvalSymlinks(path)
	if err != nil {
		t.Fatal(err)
	}
	return got
}
