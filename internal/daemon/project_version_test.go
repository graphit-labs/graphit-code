package daemon

import (
	"context"
	"github.com/graphit-labs/graphit-code/internal/gitenv"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/projectversion"
)

func supervisorFor(d *Daemon, id string) (*ProjectSupervisor, bool) {
	for _, s := range d.supervisors {
		if s.projectID == id {
			return s, true
		}
	}
	return nil, false
}

func parkedFor(d *Daemon, id string) bool {
	for _, p := range d.parked {
		if p.ID == id {
			return true
		}
	}
	return false
}

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

// Worktrees of one repository share the project ULID but hold different branches:
// each is supervised on its own, and switching a checkout's branch replaces its
// supervisor so the new one resolves the new version's stores.
func TestWorktreesAndBranchSwitchesGetTheirOwnSupervisors(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}
	t.Setenv(projectversion.EnvVersion(), "")
	main := t.TempDir()
	gitIn(t, main, "init", "-q", "-b", "main")
	if err := os.WriteFile(filepath.Join(main, "a.txt"), []byte("a"), 0o644); err != nil {
		t.Fatal(err)
	}
	gitIn(t, main, "add", ".")
	gitIn(t, main, "commit", "-q", "-m", "init")
	feature := filepath.Join(t.TempDir(), "feature")
	gitIn(t, main, "worktree", "add", "-q", "-b", "feature/x", feature)

	lf, err := os.Create(filepath.Join(t.TempDir(), "daemon.log"))
	if err != nil {
		t.Fatal(err)
	}
	defer lf.Close()
	var built []string
	d := &Daemon{
		logFile:     lf,
		supervisors: make(map[string]*ProjectSupervisor),
		builder: func(dir string) ([]WatchModule, []func() error, error) {
			built = append(built, dir)
			return []WatchModule{&fakeModule{name: "sync", startFn: func(ctx context.Context) error { <-ctx.Done(); return ctx.Err() }}}, nil, nil
		},
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	discover := func() ([]ProjectInfo, error) {
		return []ProjectInfo{{ID: "p1", Dir: main}, {ID: "p1", Dir: feature}}, nil
	}

	d.reconcileProjects(ctx, discover)
	if len(d.supervisors) != 2 || len(built) != 2 {
		t.Fatalf("two worktrees of one project: %d supervisors, %d builds", len(d.supervisors), len(built))
	}

	d.reconcileProjects(ctx, discover)
	if len(built) != 2 {
		t.Fatalf("an unchanged checkout must keep its supervisor, rebuilt %d times", len(built)-2)
	}

	gitIn(t, main, "checkout", "-q", "-b", "topic")
	d.reconcileProjects(ctx, discover)
	if len(d.supervisors) != 2 || len(built) != 3 || built[2] != main {
		t.Fatalf("a branch switch must replace that checkout's supervisor: %d supervisors, builds %v", len(d.supervisors), built)
	}
	if _, ok := d.supervisors[supervisionKey(ProjectInfo{ID: "p1", Dir: main})]; !ok {
		t.Fatal("no supervisor for the checkout's new version")
	}
	d.shutdown()
}

// A project's child runs pinned to the version checked out when it starts, which
// also overrides a version pinned in the daemon's own environment.
func TestProjectChildrenArePinnedToTheirCheckoutsVersion(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}
	t.Setenv(projectversion.EnvVersion(), "leaked-from-daemon")
	project := t.TempDir()
	gitIn(t, project, "init", "-q", "-b", "feature/y")

	runner := &recordingRunner{}
	useRunner(t, runner)
	if err := runProjectChild(context.Background(), project, nil, "sync"); err != nil {
		t.Fatal(err)
	}
	specs, _ := runner.recorded()
	if len(specs) != 1 {
		t.Fatalf("recorded %d children", len(specs))
	}
	want := projectversion.EnvVersion() + "=feature/y"
	found := false
	for _, kv := range specs[0].Env {
		found = found || kv == want
	}
	if !found {
		t.Fatalf("child env %v lacks %s", specs[0].Env, want)
	}
}
