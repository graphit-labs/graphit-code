//go:build lancedb

package memory

import (
	"context"
	"github.com/graphit-labs/graphit-code/internal/gitenv"
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
