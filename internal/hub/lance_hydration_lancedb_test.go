//go:build lancedb

package hub

import (
	"context"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	gitstate "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/testsupport"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

func TestGitCommitHydratesASTAndKnowledgeWithLocalOnlyOverlay(t *testing.T) {
	ctx := context.Background()
	_, endpoint := testsupport.StartFakeS3(t, "graphit-hydration")
	globalDir := filepath.Join(t.TempDir(), "publisher-global")
	seedHydrationAuth(t, globalDir, endpoint)
	remote, err := NewS3Store(ctx, nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	allowProjects(t, ctx, remote, hubaccess.Selector{All: true})
	registry := &RegistryManager{store: remote, entries: make(map[ArtifactType]map[string]*Entry), projects: make(map[string]*Project)}
	if _, err := registry.UpsertProject(ctx, testProjectOne, "graphit-code", ""); err != nil {
		t.Fatal(err)
	}

	source := filepath.Join(t.TempDir(), "source")
	if err := os.MkdirAll(source, 0o755); err != nil {
		t.Fatal(err)
	}
	writeHydrationFixture(t, source)
	runHydrationGit(t, source, "init", "-b", "main")
	runHydrationGit(t, source, "add", ".")
	runHydrationGit(t, source, "-c", "user.name=Graphit Test", "-c", "user.email=test@example.invalid", "commit", "-m", "base")
	snapshot, err := gitstate.InspectSnapshot(source)
	if err != nil || snapshot.Dirty {
		t.Fatalf("source Git snapshot = %#v, %v", snapshot, err)
	}

	targets := []struct {
		artType ArtifactType
		id      string
		part    string
	}{
		{TypeAST, "graphit-code-hydration-ast", ast.SearchBundleDir},
		{TypeKnowledge, "graphit-code-hydration-knowledge", wiki.WikiIndexDirName},
	}
	for _, target := range targets {
		stage := t.TempDir()
		storePath := filepath.Join(stage, target.part)
		local, err := lancestore.Open(ctx, remote.lanceConfig(storePath, true))
		if err != nil {
			t.Fatal(err)
		}
		table, err := local.CreateTable(ctx, "meta", lancestore.Schema{Fields: []lancestore.Field{
			{Name: "key", Type: lancestore.FieldString},
			{Name: "value", Type: lancestore.FieldString},
		}})
		if err != nil {
			t.Fatal(err)
		}
		if err := table.Append(ctx, []lancestore.Row{{"key": "commit", "value": snapshot.Commit}}); err != nil {
			t.Fatal(err)
		}
		if err := local.Close(); err != nil {
			t.Fatal(err)
		}
		meta := &Entry{ID: target.id, Type: target.artType, ProjectID: testProjectOne}
		history, err := registry.publishBranchLance(ctx, target.id, "branch/main", meta, stage, snapshot)
		if err != nil {
			t.Fatal(err)
		}
		if target.artType == TypeAST {
			// Hydration must use the recorded table version even when the
			// corresponding Git tag is unavailable in LanceDB.
			ref := history.Commits[0].Tables["meta"]
			ref.Tag = "git-tag-not-present"
			history.Commits[0].Tables["meta"] = ref
		}
		if err := remote.writeBranchHistory(ctx, target.artType, target.id, "branch/main", testProjectOne, history); err != nil {
			t.Fatal(err)
		}
		entry := entryFile{Version: hubManifestVersion, Entry: Entry{ID: target.id, Type: target.artType,
			ProjectID: testProjectOne, Versions: []string{"branch/main"}}}
		data, err := json.Marshal(entry)
		if err != nil {
			t.Fatal(err)
		}
		if err := remote.WriteFile(ctx, hubaccess.ProjectRegistryKey(testProjectOne, string(target.artType), target.id), data); err != nil {
			t.Fatal(err)
		}
	}

	clone := filepath.Join(t.TempDir(), "downloaded")
	cloneCmd := exec.Command("git", "clone", "--quiet", source, clone)
	if output, err := cloneCmd.CombinedOutput(); err != nil {
		t.Fatalf("clone checkout: %v: %s", err, output)
	}
	seedHydrationAuth(t, filepath.Join(t.TempDir(), "clone-global"), endpoint)
	artifactPrefix := hubaccess.ProjectRoot(testProjectOne) + "/artifacts/"
	before, err := remote.ListPage(ctx, artifactPrefix, 1000, "")
	if err != nil {
		t.Fatal(err)
	}
	if err := HydrateProjectLance(ctx, clone, nil); err != nil {
		t.Fatal(err)
	}
	for _, target := range targets {
		var targetPath string
		if target.artType == TypeAST {
			targetPath = filepath.Join(store.ASTProjectDir(clone), ast.SearchBundleDir)
		} else {
			targetPath = wiki.WikiIndexPath(store.KnowledgeProjectDir(clone))
		}
		local, err := lancestore.Open(ctx, remote.lanceConfig(targetPath, true))
		if err != nil {
			t.Fatal(err)
		}
		table, err := local.OpenTable(ctx, "meta")
		if err != nil {
			t.Fatal(err)
		}
		rows, err := table.Rows(ctx)
		if err != nil || len(rows) != 1 || rows[0]["value"] != snapshot.Commit {
			t.Fatalf("%s inherited rows = %#v, %v", target.artType, rows, err)
		}
		if err := table.Upsert(ctx, "key", []lancestore.Row{{"key": "local", "value": "overlay"}}); err != nil {
			t.Fatal(err)
		}
		if err := local.Close(); err != nil {
			t.Fatal(err)
		}
		local, err = lancestore.Open(ctx, remote.lanceConfig(targetPath, true))
		if err != nil {
			t.Fatal(err)
		}
		table, err = local.OpenTable(ctx, "meta")
		if err != nil {
			t.Fatal(err)
		}
		if count, err := table.Count(ctx); err != nil || count != 2 {
			t.Fatalf("%s local overlay count = %d, %v", target.artType, count, err)
		}
		if err := local.Close(); err != nil {
			t.Fatal(err)
		}
	}
	after, err := remote.ListPage(ctx, artifactPrefix, 1000, "")
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(before.Objects, after.Objects) {
		t.Fatal("local overlay changed S3 artifact objects")
	}
	// Looking up the same remote branch must retain the local layer.
	if err := HydrateProjectLance(ctx, clone, nil); err != nil {
		t.Fatal(err)
	}
	for _, target := range targets {
		path := hydrationTargetPath(clone, target.artType)
		assertHydrationRows(t, ctx, remote, path, map[string]string{"commit": snapshot.Commit, "local": "overlay"})
	}

	// A later commit advances both remote branch heads. The next sync should
	// replace the old base, after which checkout changes form a fresh local layer.
	if err := os.WriteFile(filepath.Join(source, "example.go"), []byte("package example\nfunc Base() {}\nfunc Next() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	runHydrationGit(t, source, "add", "example.go")
	runHydrationGit(t, source, "-c", "user.name=Graphit Test", "-c", "user.email=test@example.invalid", "commit", "-m", "next")
	next, err := gitstate.InspectSnapshot(source)
	if err != nil {
		t.Fatal(err)
	}
	for _, target := range targets {
		stage := t.TempDir()
		local, err := lancestore.Open(ctx, remote.lanceConfig(filepath.Join(stage, target.part), true))
		if err != nil {
			t.Fatal(err)
		}
		table, err := local.CreateTable(ctx, "meta", lancestore.Schema{Fields: []lancestore.Field{
			{Name: "key", Type: lancestore.FieldString}, {Name: "value", Type: lancestore.FieldString},
		}})
		if err != nil {
			t.Fatal(err)
		}
		if err := table.Append(ctx, []lancestore.Row{{"key": "commit", "value": next.Commit}}); err != nil {
			t.Fatal(err)
		}
		if err := local.Close(); err != nil {
			t.Fatal(err)
		}
		meta := &Entry{ID: target.id, Type: target.artType, ProjectID: testProjectOne}
		history, err := registry.publishBranchLance(ctx, target.id, "branch/main", meta, stage, next)
		if err != nil {
			t.Fatal(err)
		}
		if err := remote.writeBranchHistory(ctx, target.artType, target.id, "branch/main", testProjectOne, history); err != nil {
			t.Fatal(err)
		}
	}
	runHydrationGit(t, clone, "pull", "--quiet", "--ff-only")
	if err := os.WriteFile(filepath.Join(clone, "example.go"), []byte("package example\nfunc Base() {}\nfunc Next() {}\nfunc Local() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	before, err = remote.ListPage(ctx, artifactPrefix, 1000, "")
	if err != nil {
		t.Fatal(err)
	}
	if err := HydrateProjectLance(ctx, clone, nil); err != nil {
		t.Fatal(err)
	}
	for _, target := range targets {
		path := hydrationTargetPath(clone, target.artType)
		assertHydrationRows(t, ctx, remote, path, map[string]string{"commit": next.Commit})
		local, err := lancestore.Open(ctx, remote.lanceConfig(path, true))
		if err != nil {
			t.Fatal(err)
		}
		table, err := local.OpenTable(ctx, "meta")
		if err != nil {
			t.Fatal(err)
		}
		if err := table.Upsert(ctx, "key", []lancestore.Row{{"key": "local", "value": "reconciled"}}); err != nil {
			t.Fatal(err)
		}
		if err := local.Close(); err != nil {
			t.Fatal(err)
		}
		assertHydrationRows(t, ctx, remote, path, map[string]string{"commit": next.Commit, "local": "reconciled"})
	}
	after, err = remote.ListPage(ctx, artifactPrefix, 1000, "")
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(before.Objects, after.Objects) {
		t.Fatal("reconciled local overlay changed S3 artifact objects")
	}

	// This checkout has the same branch name but no published commit in its
	// history. It must clone the latest published branch base, not skip hydration.
	unrelated := filepath.Join(t.TempDir(), "unrelated")
	if err := os.MkdirAll(unrelated, 0o755); err != nil {
		t.Fatal(err)
	}
	writeHydrationFixture(t, unrelated)
	runHydrationGit(t, unrelated, "init", "-b", "main")
	runHydrationGit(t, unrelated, "add", ".")
	runHydrationGit(t, unrelated, "-c", "user.name=Graphit Test", "-c", "user.email=test@example.invalid", "commit", "-m", "independent")
	seedHydrationAuth(t, filepath.Join(t.TempDir(), "unrelated-global"), endpoint)
	if result, err := HydrateProjectLanceWithResult(ctx, unrelated, nil); err != nil ||
		result.ASTBaseCommit != next.Commit || result.KnowledgeBaseCommit != next.Commit {
		t.Fatalf("branch-head fallback = %#v, %v", result, err)
	}
	for _, target := range targets {
		assertHydrationRows(t, ctx, remote, hydrationTargetPath(unrelated, target.artType), map[string]string{"commit": next.Commit})
	}
}

func hydrationTargetPath(projectDir string, artType ArtifactType) string {
	if artType == TypeAST {
		return filepath.Join(store.ASTProjectDir(projectDir), ast.SearchBundleDir)
	}
	return wiki.WikiIndexPath(store.KnowledgeProjectDir(projectDir))
}

func assertHydrationRows(t *testing.T, ctx context.Context, remote *S3Store, path string, want map[string]string) {
	t.Helper()
	localConfig := remote.lanceConfigFor(ctx, path, false)
	if sourceURI := lancestore.ShallowSourceURI(path); sourceURI != "" {
		localConfig = remote.lanceConfigFor(ctx, sourceURI, false)
		localConfig.URI = path
	}
	local, err := lancestore.Open(ctx, localConfig)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = local.Close() }()
	table, err := local.OpenTable(ctx, "meta")
	if err != nil {
		t.Fatal(err)
	}
	rows, err := table.Rows(ctx)
	if err != nil {
		t.Fatal(err)
	}
	got := make(map[string]string, len(rows))
	for _, row := range rows {
		got[row["key"].(string)] = row["value"].(string)
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("hydrated rows = %#v, want %#v", got, want)
	}
}

func seedHydrationAuth(t *testing.T, globalDir, endpoint string) {
	t.Helper()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	account, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "test", Type: auth.ProviderLocal, Local: &auth.LocalConfig{},
		S3: auth.S3Config{Bucket: "graphit-hydration", Region: "us-east-1", Endpoint: endpoint}}
	if err := account.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := account.Login(auth.Profile{Name: "alice", Provider: "test", Username: "alice",
		S3: auth.S3Credentials{AccessKeyID: "test", SecretAccessKey: "test"}}); err != nil {
		t.Fatal(err)
	}
}

func writeHydrationFixture(t *testing.T, dir string) {
	t.Helper()
	lock := Lockfile{Project: ProjectIdentity{ID: testProjectOne, Name: "graphit-code"}, Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{}}
	data, err := json.Marshal(lock)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, brand.LockFileName()), data, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, ".gitignore"), []byte(".graphit/\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "example.go"), []byte("package example\nfunc Base() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
}

func runHydrationGit(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	if output, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git %v: %v: %s", args, err, output)
	}
}
