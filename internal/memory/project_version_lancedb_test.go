//go:build lancedb

package memory

import (
	"context"
	"errors"
	"github.com/graphit-labs/graphit-code/internal/gitenv"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/projectversion"
	"github.com/graphit-labs/graphit-code/internal/store"
)

const versionTestProject = "01ARZ3NDEKTSV4RRFFQ69G5FAV"

func gitIn(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	cmd.Env = append(gitenv.WithoutInvocationScope(os.Environ()), "GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@example.com",
		"GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@example.com",
		"GIT_CONFIG_GLOBAL="+os.DevNull, "GIT_CONFIG_SYSTEM="+os.DevNull)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
}

func memoryIDs(t *testing.T, uri, projectDir string) map[string]bool {
	t.Helper()
	tbl, err := OpenMemoryTableIn(context.Background(), uri, projectDir)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = tbl.Close() }()
	records, err := tbl.Live(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	out := map[string]bool{}
	for _, r := range records {
		out[r.ID] = true
	}
	return out
}

func putMemory(t *testing.T, uri, projectDir, id string) {
	t.Helper()
	tbl, err := OpenMemoryTableIn(context.Background(), uri, projectDir)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = tbl.Close() }()
	if err := tbl.Put(context.Background(), MemoryRecord{ID: id, Title: id, Body: id, Type: "fact", Revision: 1}); err != nil {
		t.Fatal(err)
	}
}

// Project memory belongs to the version checked out: a new version starts from the
// default version's memory and diverges. User memory has no version.
func TestProjectMemoryIsSeparatedByVersionAndUserMemoryIsNot(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv(projectversion.EnvVersion(), "")
	mainDir := t.TempDir()
	gitIn(t, mainDir, "init", "-q", "-b", "main")
	if err := os.WriteFile(filepath.Join(mainDir, brand.LockFileName()),
		[]byte(`{"project":{"id":"`+versionTestProject+`"}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	gitIn(t, mainDir, "add", ".")
	gitIn(t, mainDir, "commit", "-q", "-m", "init")
	featureDir := filepath.Join(t.TempDir(), "feature")
	gitIn(t, mainDir, "worktree", "add", "-q", "-b", "feature/m", featureDir)

	projectURI := store.MemoryTableDir("project", versionTestProject)
	putMemory(t, projectURI, mainDir, "shared-fact")
	if got := memoryIDs(t, projectURI, featureDir); !got["shared-fact"] {
		t.Fatalf("the feature version did not inherit the default version's memory: %v", got)
	}
	putMemory(t, projectURI, featureDir, "feature-fact")
	putMemory(t, projectURI, mainDir, "main-fact")
	if got := memoryIDs(t, projectURI, mainDir); got["feature-fact"] || !got["main-fact"] {
		t.Fatalf("main memory = %v", got)
	}
	if got := memoryIDs(t, projectURI, featureDir); !got["feature-fact"] || got["main-fact"] {
		t.Fatalf("feature memory = %v", got)
	}

	// User memory is the same from every checkout.
	userURI := store.MemoryTableDir("user", "someone")
	putMemory(t, userURI, featureDir, "user-pref")
	if got := memoryIDs(t, userURI, mainDir); !got["user-pref"] {
		t.Fatalf("user memory is versioned: %v", got)
	}
	if b := projectBranchForURI(userURI, featureDir); b != "" {
		t.Fatalf("user memory resolved branch %q", b)
	}
	if b := projectBranchForURI("s3://bucket/prefix/v2/projects/"+versionTestProject+"/memory", featureDir); b != "feature/m" {
		t.Fatalf("S3 project memory branch = %q", b)
	}
	if b := projectBranchForURI("s3://bucket/prefix/v2/users/u1/memory", featureDir); b != "" {
		t.Fatalf("S3 user memory branch = %q", b)
	}
}

// Memory reached by project ID alone — no checkout of the project — takes the version
// the running call selected, uses its branch only where it exists, and never forks one.
func TestProjectMemoryByIDUsesTheSelectedVersion(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}
	ctx := context.Background()
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv(projectversion.EnvVersion(), "")
	mainDir := t.TempDir()
	gitIn(t, mainDir, "init", "-q", "-b", "main")
	if err := os.WriteFile(filepath.Join(mainDir, brand.LockFileName()),
		[]byte(`{"project":{"id":"`+versionTestProject+`"}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	gitIn(t, mainDir, "add", ".")
	gitIn(t, mainDir, "commit", "-q", "-m", "init")
	featureDir := filepath.Join(t.TempDir(), "feature")
	gitIn(t, mainDir, "worktree", "add", "-q", "-b", "feature/m", featureDir)

	projectURI := store.MemoryTableDir("project", versionTestProject)
	putMemory(t, projectURI, mainDir, "main-fact")
	putMemory(t, projectURI, featureDir, "feature-fact")

	branches, err := ProjectVersionBranches(ctx, versionTestProject)
	if err != nil {
		t.Fatal(err)
	}
	if len(branches) != 1 || branches[0] != "feature/m" {
		t.Fatalf("memory version branches = %v, want [feature/m]", branches)
	}

	elsewhere := t.TempDir() // not a checkout of the project
	release, err := projectversion.AcquireRemote(ctx, versionTestProject, "feature/m", false)
	if err != nil {
		t.Fatal(err)
	}
	if got := memoryIDs(t, projectURI, elsewhere); !got["feature-fact"] || !got["main-fact"] {
		t.Fatalf("feature/m by project ID = %v", got)
	}
	putMemory(t, projectURI, elsewhere, "remote-fact")
	release()
	if got := memoryIDs(t, projectURI, featureDir); !got["remote-fact"] {
		t.Fatalf("a remote write to feature/m missed its branch: %v", got)
	}
	if got := memoryIDs(t, projectURI, mainDir); got["remote-fact"] {
		t.Fatalf("a remote write to feature/m reached main: %v", got)
	}

	release, err = projectversion.AcquireRemote(ctx, versionTestProject, "feature/none", false)
	if err != nil {
		t.Fatal(err)
	}
	defer release()
	if got := memoryIDs(t, projectURI, elsewhere); !got["main-fact"] || got["feature-fact"] {
		t.Fatalf("a version without a branch read %v, want the main lineage", got)
	}
	tbl, err := OpenMemoryTableIn(ctx, projectURI, elsewhere)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = tbl.Close() }()
	err = tbl.Put(ctx, MemoryRecord{ID: "refused", Title: "refused", Body: "refused", Type: "fact", Revision: 1})
	if !errors.Is(err, lancestore.ErrVersionNotShared) {
		t.Fatalf("a write to a version without a branch = %v, want ErrVersionNotShared", err)
	}
	if branches, _ := ProjectVersionBranches(ctx, versionTestProject); len(branches) != 1 {
		t.Fatalf("the refused write created a branch: %v", branches)
	}
}
