//go:build lancedb

package ast

import (
	"context"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

func TestPipelineKeepsShallowBaseAndIndexesOnlyCheckoutDelta(t *testing.T) {
	ctx := context.Background()
	source := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	baseSource := "package sample\nfunc Base() {}\n"
	if err := os.WriteFile(filepath.Join(source, "sample.go"), []byte(baseSource), 0o644); err != nil {
		t.Fatal(err)
	}
	gitShallowTest(t, source, "init", "-b", "main")
	gitShallowTest(t, source, "add", ".")
	gitShallowTest(t, source, "-c", "user.name=Graphit Test", "-c", "user.email=test@example.invalid", "commit", "-m", "base")
	commit := strings.TrimSpace(gitShallowTest(t, source, "rev-parse", "HEAD"))
	sourceStore := filepath.Join(t.TempDir(), "source-store")
	indexShallowTest(t, ctx, source, sourceStore)

	clone := filepath.Join(t.TempDir(), "clone")
	cmd := exec.Command("git", "clone", "--quiet", source, clone)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("clone: %v: %s", err, out)
	}
	cloneStore := filepath.Join(t.TempDir(), "clone-store")
	base, err := lancestore.Open(ctx, lancestore.Config{URI: LanceIndexPath(sourceStore)})
	if err != nil {
		t.Fatal(err)
	}
	local, err := lancestore.Open(ctx, lancestore.Config{URI: LanceIndexPath(cloneStore), Writable: true})
	if err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"files", "entities"} {
		table, err := base.OpenTable(ctx, name)
		if err != nil {
			t.Fatal(err)
		}
		version, err := table.CurrentVersion(ctx)
		if err != nil {
			t.Fatal(err)
		}
		if err := table.PutTag(ctx, "git-"+commit, version); err != nil {
			t.Fatal(err)
		}
		if _, err := local.CloneTable(ctx, name, filepath.Join(LanceIndexPath(sourceStore), name+".lance"), lancestore.CloneOptions{SourceTag: "git-" + commit}); err != nil {
			t.Fatal(err)
		}
	}
	if err := local.Close(); err != nil {
		t.Fatal(err)
	}
	if err := base.Close(); err != nil {
		t.Fatal(err)
	}
	marker, err := json.Marshal(map[string]any{"branch": "branch/main", "base": map[string]string{"commit": commit}})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(LanceIndexPath(cloneStore), ".graphit-base.json"), marker, 0o600); err != nil {
		t.Fatal(err)
	}

	indexShallowTest(t, ctx, clone, cloneStore)
	if got, ok := FileSourceAt(ctx, cloneStore, "sample.go"); !ok || got != baseSource {
		t.Fatalf("initial shallow source = %q, %v", got, ok)
	}
	localSource := "package sample\nfunc Base() {}\nfunc Local() {}\n"
	if err := os.WriteFile(filepath.Join(clone, "sample.go"), []byte(localSource), 0o644); err != nil {
		t.Fatal(err)
	}
	indexShallowTest(t, ctx, clone, cloneStore)
	if got, ok := FileSourceAt(ctx, cloneStore, "sample.go"); !ok || got != localSource {
		t.Fatalf("reconciled source = %q, %v", got, ok)
	}
	if got, ok := FileSourceAt(ctx, sourceStore, "sample.go"); !ok || got != baseSource {
		t.Fatalf("published source changed = %q, %v", got, ok)
	}
	if count := countIndexedEntities(t, filepath.Join(cloneStore, "ladybugdb"), "sample.go"); count < 2 {
		t.Fatalf("local function missing from shallow overlay: %d entities", count)
	}
}

func TestPipelineReconcilesBranchHeadForNonGitProject(t *testing.T) {
	ctx := context.Background()
	source := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	for name, body := range map[string]string{
		"sample.go":      "package sample\nfunc Published() {}\n",
		"remote_only.go": "package sample\nfunc RemoteOnly() {}\n",
	} {
		if err := os.WriteFile(filepath.Join(source, name), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	gitShallowTest(t, source, "init", "-b", "main")
	gitShallowTest(t, source, "add", ".")
	gitShallowTest(t, source, "-c", "user.name=Graphit Test", "-c", "user.email=test@example.invalid", "commit", "-m", "published")
	baseCommit := strings.TrimSpace(gitShallowTest(t, source, "rev-parse", "HEAD"))
	sourceStore := filepath.Join(t.TempDir(), "source-store")
	indexShallowTest(t, ctx, source, sourceStore)

	localRepo := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	for name, body := range map[string]string{
		"sample.go":     "package sample\nfunc Local() {}\n",
		"local_only.go": "package sample\nfunc LocalOnly() {}\n",
	} {
		if err := os.WriteFile(filepath.Join(localRepo, name), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	cloneStore := filepath.Join(t.TempDir(), "clone-store")
	base, err := lancestore.Open(ctx, lancestore.Config{URI: LanceIndexPath(sourceStore)})
	if err != nil {
		t.Fatal(err)
	}
	local, err := lancestore.Open(ctx, lancestore.Config{URI: LanceIndexPath(cloneStore), Writable: true})
	if err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"files", "entities"} {
		table, err := base.OpenTable(ctx, name)
		if err != nil {
			t.Fatal(err)
		}
		version, err := table.CurrentVersion(ctx)
		if err != nil {
			t.Fatal(err)
		}
		tag := "git-" + baseCommit
		if err := table.PutTag(ctx, tag, version); err != nil {
			t.Fatal(err)
		}
		if _, err := local.CloneTable(ctx, name, filepath.Join(LanceIndexPath(sourceStore), name+".lance"), lancestore.CloneOptions{SourceTag: tag}); err != nil {
			t.Fatal(err)
		}
	}
	if err := local.Close(); err != nil {
		t.Fatal(err)
	}
	if err := base.Close(); err != nil {
		t.Fatal(err)
	}
	marker, err := json.Marshal(map[string]any{"branch": "branch/main", "base": map[string]string{"commit": baseCommit}})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(LanceIndexPath(cloneStore), ".graphit-base.json"), marker, 0o600); err != nil {
		t.Fatal(err)
	}
	indexShallowTest(t, ctx, localRepo, cloneStore)
	if got, ok := FileSourceAt(ctx, cloneStore, "sample.go"); !ok || !strings.Contains(got, "func Local()") {
		t.Fatalf("local replacement = %q, %v", got, ok)
	}
	if _, ok := FileSourceAt(ctx, cloneStore, "remote_only.go"); ok {
		t.Fatal("remote-only file survived branch-head reconciliation")
	}
	if got, ok := FileSourceAt(ctx, cloneStore, "local_only.go"); !ok || !strings.Contains(got, "func LocalOnly()") {
		t.Fatalf("local-only file = %q, %v", got, ok)
	}
	for _, name := range []string{"sample.go", "local_only.go"} {
		if err := os.Remove(filepath.Join(localRepo, name)); err != nil {
			t.Fatal(err)
		}
	}
	indexShallowTest(t, ctx, localRepo, cloneStore)
	for _, name := range []string{"sample.go", "local_only.go", "remote_only.go"} {
		if _, ok := FileSourceAt(ctx, cloneStore, name); ok {
			t.Fatalf("%s survived reconciliation with an empty checkout", name)
		}
	}
}

func indexShallowTest(t *testing.T, ctx context.Context, project, cacheDir string) {
	t.Helper()
	db := NewLadybugDB(LadybugConfig{StoreDir: cacheDir, IcebugDir: filepath.Join(cacheDir, "graph.icebug")})
	if _, err := RunPipeline(ctx, db, project, PipelineOptions{CacheDir: cacheDir}); err != nil {
		_ = db.Close()
		t.Fatal(err)
	}
	if err := db.Close(); err != nil {
		t.Fatal(err)
	}
}

func gitShallowTest(t *testing.T, dir string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v: %s", args, err, out)
	}
	return string(out)
}
