package projectversion

import (
	"github.com/graphit-labs/graphit-code/internal/gitenv"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"testing"
	"time"
)

func requireGit(t *testing.T) {
	t.Helper()
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}
}

func git(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	cmd.Env = append(gitenv.WithoutInvocationScope(os.Environ()),
		"GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@example.com",
		"GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@example.com",
		"GIT_CONFIG_GLOBAL="+os.DevNull, "GIT_CONFIG_SYSTEM="+os.DevNull)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
}

// newRepo creates a repository whose first branch is initial, with one commit.
func newRepo(t *testing.T, initial string) string {
	t.Helper()
	requireGit(t)
	dir := t.TempDir()
	git(t, dir, "init", "-q", "-b", initial)
	if err := os.WriteFile(filepath.Join(dir, "a.txt"), []byte("a"), 0o644); err != nil {
		t.Fatal(err)
	}
	git(t, dir, "add", "a.txt")
	git(t, dir, "commit", "-q", "-m", "init")
	return dir
}

func clearEnv(t *testing.T) {
	t.Setenv(EnvVersion(), "")
	t.Setenv(EnvBaseBranch(), "")
}

func mustResolve(t *testing.T, dir, explicit string) Info {
	t.Helper()
	info, err := Resolve(dir, explicit)
	if err != nil {
		t.Fatal(err)
	}
	return info
}

func TestResolveNonGitIsMain(t *testing.T) {
	clearEnv(t)
	info := mustResolve(t, t.TempDir(), "")
	if info.Version != "main" || info.Default != "main" || info.Git || !info.IsDefault() {
		t.Fatalf("got %+v", info)
	}
	if info.LanceBranch() != "" {
		t.Fatalf("default version must use the main lineage, got %q", info.LanceBranch())
	}
}

func TestResolveCheckedOutBranchAndSubdirectory(t *testing.T) {
	clearEnv(t)
	repo := newRepo(t, "main")
	git(t, repo, "checkout", "-q", "-b", "feature/x")
	sub := filepath.Join(repo, "pkg", "deep")
	if err := os.MkdirAll(sub, 0o755); err != nil {
		t.Fatal(err)
	}
	for _, dir := range []string{repo, sub} {
		info := mustResolve(t, dir, "")
		if info.Version != "feature/x" || info.Default != "main" || !info.Git || info.IsDefault() {
			t.Fatalf("%s: got %+v", dir, info)
		}
	}
}

func TestResolveLinkedWorktree(t *testing.T) {
	clearEnv(t)
	repo := newRepo(t, "main")
	wt := filepath.Join(t.TempDir(), "wt")
	git(t, repo, "worktree", "add", "-q", "-b", "feature/y", wt)
	if info := mustResolve(t, wt, ""); info.Version != "feature/y" || info.Default != "main" {
		t.Fatalf("worktree: got %+v", info)
	}
	if info := mustResolve(t, repo, ""); info.Version != "main" || !info.IsDefault() {
		t.Fatalf("main checkout: got %+v", info)
	}
}

func TestResolveDetachedHead(t *testing.T) {
	clearEnv(t)
	repo := newRepo(t, "main")
	git(t, repo, "checkout", "-q", "--detach")
	info := mustResolve(t, repo, "")
	if !info.Detached || info.Version != "main" {
		t.Fatalf("detached without base branch: got %+v", info)
	}
	t.Setenv(EnvBaseBranch(), "refs/heads/release/1.2")
	info = mustResolve(t, repo, "")
	if !info.Detached || info.Version != "release/1.2" {
		t.Fatalf("detached with base branch: got %+v", info)
	}
}

func TestDefaultFromOriginHead(t *testing.T) {
	clearEnv(t)
	repo := newRepo(t, "main")
	git(t, repo, "branch", "develop")
	git(t, repo, "update-ref", "refs/remotes/origin/develop", "HEAD")
	git(t, repo, "symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/develop")
	if info := mustResolve(t, repo, ""); info.Default != "develop" || info.Version != "main" {
		t.Fatalf("got %+v", info)
	}
}

func TestDefaultMasterAndPackedRefs(t *testing.T) {
	clearEnv(t)
	repo := newRepo(t, "master")
	git(t, repo, "checkout", "-q", "-b", "topic")
	git(t, repo, "pack-refs", "--all")
	if info := mustResolve(t, repo, ""); info.Default != "master" || info.Version != "topic" {
		t.Fatalf("got %+v", info)
	}
}

func TestDefaultOfFreshRepositoryIsItsOnlyBranch(t *testing.T) {
	clearEnv(t)
	requireGit(t)
	dir := t.TempDir()
	git(t, dir, "init", "-q", "-b", "trunk")
	if info := mustResolve(t, dir, ""); info.Version != "trunk" || info.Default != "trunk" {
		t.Fatalf("got %+v", info)
	}
}

func TestExplicitBeatsEnvironmentBeatsCheckout(t *testing.T) {
	clearEnv(t)
	repo := newRepo(t, "main")
	t.Chdir(repo)
	t.Setenv(EnvVersion(), "from-env")
	envRootOnce = sync.Once{}
	if info := mustResolve(t, repo, ""); info.Version != "from-env" || !info.Explicit {
		t.Fatalf("env: got %+v", info)
	}
	if info := mustResolve(t, repo, " explicit "); info.Version != "explicit" || !info.Explicit {
		t.Fatalf("explicit: got %+v", info)
	}
	other := newRepo(t, "main")
	if info := mustResolve(t, other, ""); info.Version != "main" || info.Explicit {
		t.Fatalf("env pin must not leak to another project: got %+v", info)
	}
	if _, err := Resolve(repo, "bad\x00"); err == nil {
		t.Fatal("control characters must be rejected")
	}
}

func TestAcquireScopesExplicitVersion(t *testing.T) {
	clearEnv(t)
	repo := newRepo(t, "main")
	release, err := Acquire(repo, "feature/z")
	if err != nil {
		t.Fatal(err)
	}
	if got := Current(repo).Version; got != "feature/z" {
		t.Fatalf("inside scope: %q", got)
	}
	shared := make(chan string)
	go func() {
		r, _ := Acquire(repo, "")
		shared <- Current(repo).Version
		r()
	}()
	select {
	case v := <-shared:
		t.Fatalf("a shared scope must wait for the explicit one, saw %q", v)
	case <-time.After(50 * time.Millisecond):
	}
	release()
	if v := <-shared; v != "main" {
		t.Fatalf("after release: %q", v)
	}
	if got := Current(repo).Version; got != "main" {
		t.Fatalf("outside scope: %q", got)
	}
}

func TestLanceBranchName(t *testing.T) {
	cases := []struct{ version, def, want string }{
		{"main", "main", ""},
		{"develop", "develop", ""},
		{"feature/x", "main", "feature/x"},
		{"v1.2_rc-3", "main", "v1.2_rc-3"},
		{"fix+1", "main", "h-6669782b31"},
		{"a..b", "main", "h-612e2e62"},
		{"x.lock", "main", "h-782e6c6f636b"},
		{"main", "develop", "h-6d61696e"},
		{"h-literal", "main", "h-682d6c69746572616c"},
		{"feat//y", "main", "h-666561742f2f79"},
		{"ação", "main", "h-61c3a7c3a36f"},
	}
	for _, c := range cases {
		if got := LanceBranchName(c.version, c.def); got != c.want {
			t.Errorf("LanceBranchName(%q,%q)=%q want %q", c.version, c.def, got, c.want)
		}
	}
}

// A Git hook exports its own invocation scope, and Graphit resolves versions from hooks
// (post-checkout, pre-commit gates) and from the processes they start. A reftable
// repository is resolved through the git CLI, which must not follow the hook's GIT_DIR or
// its relative GIT_INDEX_FILE into another repository.
func TestResolveInsideAGitHookEnvironment(t *testing.T) {
	clearEnv(t)
	requireGit(t)
	repo := t.TempDir()
	init := exec.Command("git", "-C", repo, "init", "-q", "--ref-format=reftable", "-b", "main")
	init.Env = gitenv.WithoutInvocationScope(os.Environ())
	if out, err := init.CombinedOutput(); err != nil {
		t.Skipf("git cannot create a reftable repository: %v: %s", err, out)
	}
	if err := os.WriteFile(filepath.Join(repo, "a.txt"), []byte("a"), 0o644); err != nil {
		t.Fatal(err)
	}
	git(t, repo, "add", "a.txt")
	git(t, repo, "commit", "-q", "-m", "init")
	git(t, repo, "checkout", "-q", "-b", "feature/hook")
	wt := filepath.Join(t.TempDir(), "wt")
	git(t, repo, "worktree", "add", "-q", "-b", "feature/linked", wt)

	other := newRepo(t, "elsewhere")
	t.Setenv("GIT_DIR", filepath.Join(other, ".git"))
	t.Setenv("GIT_WORK_TREE", other)
	t.Setenv("GIT_INDEX_FILE", ".git/index")
	t.Setenv("GIT_PREFIX", "")

	for dir, want := range map[string]string{repo: "feature/hook", wt: "feature/linked"} {
		if info := mustResolve(t, dir, ""); info.Version != want || info.Default != "main" {
			t.Errorf("%s under a hook environment resolved %+v, want %s from main", dir, info, want)
		}
	}
}
