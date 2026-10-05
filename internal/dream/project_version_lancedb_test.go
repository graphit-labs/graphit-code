//go:build lancedb

package dream

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/projectversion"
)

func writeFile(t *testing.T, path, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

// fakeCheckouts lays out Git's own files for a repository on main and a linked
// worktree on feature/d. This package's tests run without git on PATH, and
// projectversion reads these files rather than running git.
func fakeCheckouts(t *testing.T) (mainDir, featureDir string) {
	t.Helper()
	mainDir, featureDir = t.TempDir(), t.TempDir()
	gitDir := filepath.Join(mainDir, ".git")
	writeFile(t, filepath.Join(gitDir, "HEAD"), "ref: refs/heads/main\n")
	writeFile(t, filepath.Join(gitDir, "refs", "heads", "main"), "0000000000000000000000000000000000000001\n")
	worktreeGitDir := filepath.Join(gitDir, "worktrees", "feature")
	writeFile(t, filepath.Join(worktreeGitDir, "HEAD"), "ref: refs/heads/feature/d\n")
	writeFile(t, filepath.Join(worktreeGitDir, "commondir"), "../..\n")
	writeFile(t, filepath.Join(featureDir, ".git"), "gitdir: "+worktreeGitDir+"\n")
	for _, dir := range []string{mainDir, featureDir} {
		writeFile(t, filepath.Join(dir, brand.LockFileName()), `{"project":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}}`)
	}
	return mainDir, featureDir
}

func recordRun(t *testing.T, projectDir, runID string, at time.Time) {
	t.Helper()
	ledger, err := openRunLedger(context.Background(), projectDir)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = ledger.Close() }()
	if err := ledger.Put(context.Background(), newRunRecord(projectDir, runID, "agent", "cli", at)); err != nil {
		t.Fatal(err)
	}
}

// Dream runs are recorded per project version, like the memory they curate.
func TestDreamRunsAreSeparatedByProjectVersion(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv(projectversion.EnvVersion(), "")
	mainDir, featureDir := fakeCheckouts(t)
	if v := projectversion.Current(featureDir); v.Version != "feature/d" || v.Default != "main" {
		t.Fatalf("fake worktree resolved %+v", v)
	}

	base := time.Now()
	recordRun(t, mainDir, "run-main-1", base)
	if got, err := LatestRun(context.Background(), featureDir); err != nil || got == nil || got.RunID != "run-main-1" {
		t.Fatalf("feature did not inherit the default version's runs: %#v %v", got, err)
	}
	recordRun(t, featureDir, "run-feature", base.Add(time.Minute))
	recordRun(t, mainDir, "run-main-2", base.Add(2*time.Minute))
	if got, err := LatestRun(context.Background(), featureDir); err != nil || got == nil || got.RunID != "run-feature" {
		t.Fatalf("feature latest run = %#v %v", got, err)
	}
	if got, err := LatestRun(context.Background(), mainDir); err != nil || got == nil || got.RunID != "run-main-2" {
		t.Fatalf("main latest run = %#v %v", got, err)
	}
}
