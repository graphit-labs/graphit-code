//go:build lancedb

package task

import (
	"context"
	"github.com/graphit-labs/graphit-code/internal/gitenv"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/projectversion"
)

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

// twoVersions is one project checked out on main and, in a linked worktree, on
// feature/x: one lockfile ULID, two project versions.
func twoVersions(t *testing.T) (mainDir, featureDir string) {
	t.Helper()
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}
	t.Setenv(projectversion.EnvVersion(), "")
	mainDir = t.TempDir()
	gitIn(t, mainDir, "init", "-q", "-b", "main")
	if err := os.WriteFile(filepath.Join(mainDir, brand.LockFileName()),
		[]byte(`{"project":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	gitIn(t, mainDir, "add", ".")
	gitIn(t, mainDir, "commit", "-q", "-m", "init")
	featureDir = filepath.Join(t.TempDir(), "feature")
	gitIn(t, mainDir, "worktree", "add", "-q", "-b", "feature/x", featureDir)
	return mainDir, featureDir
}

func taskTitles(t *testing.T, svc *Service) map[string]bool {
	t.Helper()
	tasks, err := svc.List(context.Background(), ListOptions{})
	if err != nil {
		t.Fatal(err)
	}
	out := map[string]bool{}
	for _, task := range tasks {
		out[task.Title] = true
	}
	return out
}

func sessionTitles(t *testing.T, svc *Service) map[string]bool {
	t.Helper()
	sessions, err := svc.SessionList(context.Background(), SessionListOptions{})
	if err != nil {
		t.Fatal(err)
	}
	out := map[string]bool{}
	for _, s := range sessions {
		out[s.Title] = true
	}
	return out
}

func createTask(t *testing.T, svc *Service, title string) {
	t.Helper()
	if _, err := svc.Create(context.Background(), CreateInput{
		Title: title, Description: title, AcceptanceCriteria: []string{"done"}, Tests: []string{"checked"},
		IdempotencyKey: title, Actor: "tester",
	}); err != nil {
		t.Fatal(err)
	}
}

// Tasks and sessions belong to a project version: a new version starts from the
// default version's (the existing, unversioned data) and diverges from it.
func TestTasksAndSessionsAreSeparatedByProjectVersion(t *testing.T) {
	mainDir, featureDir := twoVersions(t)
	ctx := context.Background()

	main, err := Open(mainDir)
	if err != nil {
		t.Fatal(err)
	}
	createTask(t, main, "before-the-branch")
	if _, err := main.SessionCreate(ctx, SessionCreateInput{Title: "main-session", Description: "d", Strategy: "s", Actor: "tester"}); err != nil {
		t.Fatal(err)
	}

	feature, err := Open(featureDir)
	if err != nil {
		t.Fatal(err)
	}
	if got := taskTitles(t, feature); !got["before-the-branch"] {
		t.Fatalf("the feature version did not inherit the default version's tasks: %v", got)
	}
	if got := sessionTitles(t, feature); !got["main-session"] {
		t.Fatalf("the feature version did not inherit the default version's sessions: %v", got)
	}

	createTask(t, feature, "only-on-feature")
	if _, err := feature.SessionCreate(ctx, SessionCreateInput{Title: "feature-session", Description: "d", Strategy: "s", Actor: "tester"}); err != nil {
		t.Fatal(err)
	}
	createTask(t, main, "only-on-main")

	if got := taskTitles(t, main); got["only-on-feature"] || !got["only-on-main"] {
		t.Fatalf("main tasks = %v", got)
	}
	if got := sessionTitles(t, main); got["feature-session"] {
		t.Fatalf("a feature session reached main: %v", got)
	}
	if got := taskTitles(t, feature); !got["only-on-feature"] || got["only-on-main"] {
		t.Fatalf("feature tasks = %v", got)
	}

	// A service opened by project ID alone has no checkout: it reads the default
	// version, the main lineage.
	byID, err := OpenProject(ctx, "01ARZ3NDEKTSV4RRFFQ69G5FAV")
	if err != nil {
		t.Fatal(err)
	}
	if got := taskTitles(t, byID); got["only-on-feature"] || !got["only-on-main"] {
		t.Fatalf("project-ID service tasks = %v", got)
	}
}
