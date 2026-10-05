//go:build lancedb

package ast

import (
	"context"
	"github.com/graphit-labs/graphit-code/internal/gitenv"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/store"
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

// indexVersion indexes projectDir the way the CLI and MCP writers do: ensure the
// version's store (forking it from the default version when new), then run the
// pipeline against the resolved paths.
func indexVersion(t *testing.T, projectDir string) *PipelineResult {
	t.Helper()
	if _, err := store.EnsureASTProjectDir(projectDir); err != nil {
		t.Fatalf("ensure store for %s: %v", projectDir, err)
	}
	db := NewLadybugDB(LadybugConfigFor(projectDir))
	defer func() { _ = db.Close() }()
	res, err := RunPipeline(context.Background(), db, projectDir,
		PipelineOptions{CacheDir: store.ASTProjectDir(projectDir), Workers: 1, SkipExternal: true})
	if err != nil {
		t.Fatalf("index %s: %v", projectDir, err)
	}
	return res
}

func versionFinds(t *testing.T, projectDir, symbol string) bool {
	t.Helper()
	idx, err := OpenSearchIndex(context.Background(), store.ASTProjectDir(projectDir))
	if err != nil {
		t.Fatalf("open search index for %s: %v", projectDir, err)
	}
	defer func() { _ = idx.Close() }()
	results, err := idx.Search(context.Background(), symbol, 10)
	if err != nil {
		t.Fatalf("search %s: %v", symbol, err)
	}
	for _, r := range results {
		if r.Name == symbol {
			return true
		}
	}
	return false
}

// Two worktrees of one repository share the lockfile ULID but hold different code.
// Each indexes into, and searches, the store of its own version only, and the feature
// version starts from the default version's store rather than from nothing.
func TestASTStoresAreSeparatedByProjectVersion(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}
	t.Setenv("LADYBUGDB_PATH", "")
	t.Setenv(brand.EnvVar("PROJECT_VERSION"), "")
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)

	mainDir := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	if err := os.WriteFile(filepath.Join(mainDir, brand.LockFileName()),
		[]byte(`{"project":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(mainDir, "shared.go"),
		[]byte("package demo\n\nfunc SharedEverywhere() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(mainDir, "stable.go"),
		[]byte("package demo\n\nfunc StableOnBothBranches() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	gitIn(t, mainDir, "init", "-q", "-b", "main")
	gitIn(t, mainDir, "add", ".")
	gitIn(t, mainDir, "commit", "-q", "-m", "init")
	featureDir := filepath.Join(t.TempDir(), "feature")
	gitIn(t, mainDir, "worktree", "add", "-q", "-b", "feature/x", featureDir)

	if err := os.WriteFile(filepath.Join(mainDir, "main_only.go"),
		[]byte("package demo\n\nfunc OnlyOnMainBranch() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	indexVersion(t, mainDir)
	if !versionFinds(t, mainDir, "SharedEverywhere") {
		t.Fatal("the default version did not index its own code")
	}

	if err := os.Remove(filepath.Join(featureDir, "shared.go")); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(featureDir, "shared.go"),
		[]byte("package demo\n\nfunc SharedEverywhere() {}\n\nfunc ChangedOnFeature() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(featureDir, "feature_only.go"),
		[]byte("package demo\n\nfunc OnlyOnFeatureBranch() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	feature := indexVersion(t, featureDir)
	// shared.go changed and feature_only.go is new; stable.go was inherited from the
	// default version's store and must not be parsed again.
	if feature.ParsedFiles != 2 {
		t.Errorf("the feature version parsed %d of %d files; a fork reparses only what differs",
			feature.ParsedFiles, feature.TotalFiles)
	}

	if store.ASTProjectDir(mainDir) == store.ASTProjectDir(featureDir) {
		t.Fatalf("both worktrees resolved to %s", store.ASTProjectDir(mainDir))
	}
	for symbol, want := range map[string]bool{
		"OnlyOnMainBranch": true, "OnlyOnFeatureBranch": false, "ChangedOnFeature": false, "SharedEverywhere": true,
	} {
		if got := versionFinds(t, mainDir, symbol); got != want {
			t.Errorf("main version finds %s = %v, want %v", symbol, got, want)
		}
	}
	for symbol, want := range map[string]bool{
		"OnlyOnMainBranch": false, "OnlyOnFeatureBranch": true, "ChangedOnFeature": true, "SharedEverywhere": true,
		"StableOnBothBranches": true,
	} {
		if got := versionFinds(t, featureDir, symbol); got != want {
			t.Errorf("feature version finds %s = %v, want %v", symbol, got, want)
		}
	}
}
