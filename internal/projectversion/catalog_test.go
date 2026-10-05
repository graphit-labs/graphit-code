package projectversion

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

func TestWorktreesListEveryCheckoutWithItsVersion(t *testing.T) {
	clearEnv(t)
	repo := newRepo(t, "main")
	wt := filepath.Join(t.TempDir(), "wt")
	git(t, repo, "worktree", "add", "-q", "-b", "feature/w", wt)
	gone := filepath.Join(t.TempDir(), "gone")
	git(t, repo, "worktree", "add", "-q", "-b", "feature/gone", gone)
	if err := os.RemoveAll(gone); err != nil {
		t.Fatal(err)
	}

	for _, from := range []string{repo, wt} {
		got := map[string]string{}
		for _, w := range Worktrees(from) {
			got[mustEval(t, w.Dir)] = w.Version
		}
		want := map[string]string{mustEval(t, repo): "main", mustEval(t, wt): "feature/w"}
		if len(got) != len(want) {
			t.Fatalf("from %s: worktrees = %v, want %v (a removed worktree is not a checkout)", from, got, want)
		}
		for dir, version := range want {
			if got[dir] != version {
				t.Errorf("from %s: %s has %q, want %q", from, dir, got[dir], version)
			}
		}
	}
	if Worktrees(t.TempDir()) != nil {
		t.Error("a directory outside Git has worktrees")
	}
}

func mustEval(t *testing.T, path string) string {
	t.Helper()
	resolved, err := filepath.EvalSymlinks(path)
	if err != nil {
		t.Fatal(err)
	}
	return resolved
}

func TestBranchExistsSeesLooseAndPackedBranches(t *testing.T) {
	clearEnv(t)
	repo := newRepo(t, "main")
	git(t, repo, "branch", "feature/packed")
	git(t, repo, "pack-refs", "--all")
	git(t, repo, "branch", "feature/loose")
	for name, want := range map[string]bool{"main": true, "feature/packed": true, "feature/loose": true, "feature/deleted": false, "": false} {
		if got := BranchExists(repo, name); got != want {
			t.Errorf("BranchExists(%q) = %v, want %v", name, got, want)
		}
	}
}

func TestVersionFromLanceBranchReversesTheEncoding(t *testing.T) {
	for _, version := range []string{"feature/x", "release-1.2", "main", "feature/ação", "h-literal", "with space"} {
		branch := LanceBranchName(version, "trunk")
		if got := VersionFromLanceBranch(branch); got != version {
			t.Errorf("%q -> %q -> %q", version, branch, got)
		}
	}
}

func TestAcquireRemoteScopesOneProjectID(t *testing.T) {
	ctx := context.Background()
	if RemoteLanceBranch("01ARZ3NDEKTSV4RRFFQ69G5FAV") != "" {
		t.Fatal("a project with no scope has a branch")
	}
	release, err := AcquireRemote(ctx, "01ARZ3NDEKTSV4RRFFQ69G5FAV", "feature/r", false)
	if err != nil {
		t.Fatal(err)
	}
	if got := RemoteLanceBranch("01ARZ3NDEKTSV4RRFFQ69G5FAV"); got != "feature/r" {
		t.Errorf("scoped branch = %q", got)
	}
	if RemoteForkable("01ARZ3NDEKTSV4RRFFQ69G5FAV") {
		t.Error("an unconfirmed version may fork")
	}
	if got := RemoteLanceBranch("01BX5ZZKBKACTAV9WEVGEMMVRZ"); got != "" {
		t.Errorf("another project inherited the scope: %q", got)
	}
	release()
	if got := RemoteLanceBranch("01ARZ3NDEKTSV4RRFFQ69G5FAV"); got != "" {
		t.Errorf("the scope outlived its release: %q", got)
	}
	if _, err := AcquireRemote(ctx, "01ARZ3NDEKTSV4RRFFQ69G5FAV", "bad\x00name", false); err == nil {
		t.Error("an invalid version was accepted")
	}
}

func TestAConfirmedRemoteVersionMayFork(t *testing.T) {
	ctx := context.Background()
	release, err := AcquireRemote(ctx, "01ARZ3NDEKTSV4RRFFQ69G5FAV", "feature/c", true)
	if err != nil {
		t.Fatal(err)
	}
	if !RemoteForkable("01ARZ3NDEKTSV4RRFFQ69G5FAV") {
		t.Error("a confirmed version may not fork")
	}
	release()
	if !RemoteForkable("01ARZ3NDEKTSV4RRFFQ69G5FAV") {
		t.Error("the default version (no scope) must be forkable: it needs no branch")
	}
	release, err = AcquireRemote(ctx, "01ARZ3NDEKTSV4RRFFQ69G5FAV", "feature/u", false)
	if err != nil {
		t.Fatal(err)
	}
	defer release()
	if RemoteForkable("01ARZ3NDEKTSV4RRFFQ69G5FAV") {
		t.Error("confirmation leaked from an earlier scope")
	}
}
