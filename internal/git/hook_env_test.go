package git

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// TestGitCommandsIgnoreAnInheritedHookEnvironment pins the failure that made every memory
// commit die while the memories themselves were being written to disk.
//
// A git hook exports the invocation it belongs to — GIT_INDEX_FILE, GIT_DIR, GIT_PREFIX,
// GIT_AUTHOR_DATE — and a hook that starts a long-lived process hands it that environment
// permanently. GIT_INDEX_FILE=.git/index is the sharpest of them because it is RELATIVE: it
// re-resolves against whatever repository the next `-C` names, and in a linked worktree
// `.git` is a FILE, so git fails with "Unable to create '<wt>/.git/index.lock': Not a
// directory" — a message that reads like lock contention.
func TestGitCommandsIgnoreAnInheritedHookEnvironment(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skipf("git not available: %v", err)
	}

	main := t.TempDir()
	run := func(dir string, args ...string) {
		t.Helper()
		cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %s: %v\n%s", strings.Join(args, " "), err, out)
		}
	}
	run(main, "init", "-q")
	run(main, "config", "user.email", "t@example.com")
	run(main, "config", "user.name", "T")
	if err := os.WriteFile(filepath.Join(main, "seed.txt"), []byte("seed\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run(main, "add", ".")
	run(main, "commit", "-qm", "seed")

	linked := filepath.Join(t.TempDir(), "linked")
	run(main, "worktree", "add", "-q", "-b", "side", linked)
	if info, err := os.Stat(filepath.Join(linked, ".git")); err != nil || info.IsDir() {
		t.Fatalf("expected a linked worktree whose .git is a file (err=%v)", err)
	}

	t.Setenv("GIT_INDEX_FILE", ".git/index")
	t.Setenv("GIT_PREFIX", "")
	t.Setenv("GIT_AUTHOR_DATE", "@1000000000 +0000")

	raw := exec.Command("git", "-C", linked, "add", ".")
	raw.Env = os.Environ()
	if out, err := raw.CombinedOutput(); err == nil {
		t.Fatalf("the inherited environment no longer breaks a raw git call, so this test "+
			"no longer pins anything: %s", out)
	}

	if err := os.WriteFile(filepath.Join(linked, "novo.txt"), []byte("conteudo\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	g := Default()
	if err := g.Run(linked, "add", "."); err != nil {
		t.Fatalf("staging in a linked worktree under a hook environment: %v", err)
	}
	if err := g.Run(linked, "commit", "-m", "sob ambiente de hook"); err != nil {
		t.Fatalf("committing in a linked worktree under a hook environment: %v", err)
	}

	date, err := g.RunOutput(linked, "log", "-1", "--format=%at")
	if err != nil {
		t.Fatalf("reading the commit date: %v", err)
	}
	if strings.TrimSpace(date) == "1000000000" {
		t.Error("the commit inherited GIT_AUTHOR_DATE from the hook environment")
	}
}

func TestWithoutInheritedGitScopeKeepsEverythingElse(t *testing.T) {
	in := []string{
		"PATH=/usr/bin",
		"GIT_INDEX_FILE=.git/index",
		"GIT_DIR=/somewhere/.git",
		"GIT_AUTHOR_DATE=@1000000000 +0000",
		"GIT_AUTHOR_NAME=Alguém",
		"GIT_SSH_COMMAND=ssh -v",
		"HOME=/home/x",
	}
	got := strings.Join(withoutInheritedGitScope(in), "\n")
	for _, dropped := range []string{"GIT_INDEX_FILE", "GIT_DIR=", "GIT_AUTHOR_DATE"} {
		if strings.Contains(got, dropped) {
			t.Errorf("%s survived the filter", dropped)
		}
	}
	for _, kept := range []string{"PATH=/usr/bin", "GIT_AUTHOR_NAME=Alguém", "GIT_SSH_COMMAND=ssh -v", "HOME=/home/x"} {
		if !strings.Contains(got, kept) {
			t.Errorf("%s was dropped and should not have been", kept)
		}
	}
}
