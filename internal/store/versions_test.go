package store

import (
	"github.com/graphit-labs/graphit-code/internal/gitenv"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func gitRepoOnBranch(t *testing.T, branch string) string {
	t.Helper()
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}
	dir := t.TempDir()
	run := func(args ...string) {
		cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
		cmd.Env = append(gitenv.WithoutInvocationScope(os.Environ()), "GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@example.com",
			"GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@example.com",
			"GIT_CONFIG_GLOBAL="+os.DevNull, "GIT_CONFIG_SYSTEM="+os.DevNull)
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}
	run("init", "-q", "-b", "main")
	writeLockfile(t, dir, storeTestProjectID)
	run("add", ".")
	run("commit", "-q", "-m", "init")
	if branch != "main" {
		run("checkout", "-q", "-b", branch)
	}
	return dir
}

func writeFile(t *testing.T, path, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

func readFile(t *testing.T, path string) string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}

func TestProjectStoresAreKeyedByVersion(t *testing.T) {
	home := withHome(t)
	t.Setenv("GRAPHIT_PROJECT_VERSION", "")
	feature := gitRepoOnBranch(t, "feature/api")
	nonGit := t.TempDir()
	writeLockfile(t, nonGit, storeTestProjectID)

	root := filepath.Join(home, ".graphit", "ast", "project", storeTestProjectID)
	if got, want := ASTProjectDir(feature), filepath.Join(root, "versions", VersionPathSegment("feature/api")); got != want {
		t.Fatalf("feature branch: got %q want %q", got, want)
	}
	if !strings.HasPrefix(filepath.Base(ASTProjectDir(feature)), "~") {
		t.Fatalf("a branch with a slash must be one opaque segment, got %q", ASTProjectDir(feature))
	}
	if got, want := ASTProjectDir(nonGit), filepath.Join(root, "versions", "main"); got != want {
		t.Fatalf("non-Git project: got %q want %q", got, want)
	}
	if ASTProjectDir(feature) == ASTProjectDir(nonGit) {
		t.Fatal("two versions of one project must not share a store")
	}
	wikiRoot := filepath.Join(home, ".graphit", "wiki", "knowledge", "project", storeTestProjectID)
	if got, want := KnowledgeProjectDir(feature), filepath.Join(wikiRoot, "versions", VersionPathSegment("feature/api")); got != want {
		t.Fatalf("knowledge: got %q want %q", got, want)
	}
}

func TestLegacyStoreMovesIntoTheDefaultVersion(t *testing.T) {
	withHome(t)
	t.Setenv("GRAPHIT_PROJECT_VERSION", "")
	project := gitRepoOnBranch(t, "feature/x")
	root := ASTProjectRoot(project)
	legacyBundle := filepath.Join(root, "graph.icebug")
	writeFile(t, filepath.Join(legacyBundle, "schema.cypher"),
		"CREATE NODE TABLE e(id STRING) WITH (storage = '"+cypherEscaped(legacyBundle)+"', format = 'icebug-disk');\n")
	writeFile(t, filepath.Join(legacyBundle, "nodes_0.parquet"), "parquet")
	writeFile(t, filepath.Join(root, "manifest.json"), `{"dir":"`+jsonEscaped(root)+`"}`)

	// Reading from a feature branch migrates the legacy store into the default version
	// without creating the feature version.
	featureDir := ASTProjectDir(project)
	mainDir := VersionStoreDir(root, "main")
	if exists(featureDir) {
		t.Fatalf("a read must not create version %q", featureDir)
	}
	if hasLegacyStore(root) {
		t.Fatal("legacy entries remain at the store root")
	}
	if got := readFile(t, filepath.Join(mainDir, "graph.icebug", "nodes_0.parquet")); got != "parquet" {
		t.Fatalf("data lost in migration: %q", got)
	}
	schema := readFile(t, filepath.Join(mainDir, "graph.icebug", "schema.cypher"))
	if !strings.Contains(schema, cypherEscaped(filepath.Join(mainDir, "graph.icebug"))) {
		t.Fatalf("mount DDL still names the old location: %s", schema)
	}
	if manifest := readFile(t, filepath.Join(mainDir, "manifest.json")); !strings.Contains(manifest, jsonEscaped(mainDir)) {
		t.Fatalf("manifest still names the old location: %s", manifest)
	}
}

func TestNewVersionForksTheDefaultVersion(t *testing.T) {
	withHome(t)
	t.Setenv("GRAPHIT_PROJECT_VERSION", "")
	project := gitRepoOnBranch(t, "feature/x")
	root := ASTProjectRoot(project)
	mainDir := VersionStoreDir(root, "main")
	mainBundle := filepath.Join(mainDir, "graph.icebug")
	writeFile(t, filepath.Join(mainBundle, "schema.cypher"), "storage = '"+cypherEscaped(mainBundle)+"'")
	writeFile(t, filepath.Join(mainBundle, "nodes_0.parquet"), "parquet")
	writeFile(t, filepath.Join(mainDir, "search", "data", "frag.lance"), "fragment")
	writeFile(t, filepath.Join(mainDir, "graph.icebug.tmp.abc", "partial"), "in flight")

	dir, err := EnsureASTProjectDir(project)
	if err != nil {
		t.Fatal(err)
	}
	if dir != ASTProjectDir(project) || dir == mainDir {
		t.Fatalf("EnsureASTProjectDir = %q, want the feature version", dir)
	}
	if got := readFile(t, filepath.Join(dir, "search", "data", "frag.lance")); got != "fragment" {
		t.Fatalf("inherited data: %q", got)
	}
	if exists(filepath.Join(dir, "graph.icebug.tmp.abc")) {
		t.Fatal("an in-flight publication directory must not be inherited")
	}
	schema := readFile(t, filepath.Join(dir, "graph.icebug", "schema.cypher"))
	if !strings.Contains(schema, cypherEscaped(filepath.Join(dir, "graph.icebug"))) {
		t.Fatalf("forked DDL must mount the fork's own bundle: %s", schema)
	}
	if mainSchema := readFile(t, filepath.Join(mainBundle, "schema.cypher")); !strings.Contains(mainSchema, cypherEscaped(mainBundle)) {
		t.Fatalf("the default version's DDL changed: %s", mainSchema)
	}
	if runtime.GOOS != "windows" {
		a, _ := os.Stat(filepath.Join(mainBundle, "nodes_0.parquet"))
		b, _ := os.Stat(filepath.Join(dir, "graph.icebug", "nodes_0.parquet"))
		if !os.SameFile(a, b) {
			t.Fatal("data files should be hard-linked, not copied")
		}
		a, _ = os.Stat(filepath.Join(mainBundle, "schema.cypher"))
		b, _ = os.Stat(filepath.Join(dir, "graph.icebug", "schema.cypher"))
		if os.SameFile(a, b) {
			t.Fatal("location metadata must be copied, never shared")
		}
	}

	// Diverging afterwards does not reach the default version.
	writeFile(t, filepath.Join(dir, "only-feature"), "x")
	if exists(filepath.Join(mainDir, "only-feature")) {
		t.Fatal("a write to the fork reached the default version")
	}
	again, err := EnsureASTProjectDir(project)
	if err != nil || again != dir {
		t.Fatalf("a second ensure must reuse the version: %q %v", again, err)
	}
}

func TestDefaultVersionIsNeverForked(t *testing.T) {
	withHome(t)
	t.Setenv("GRAPHIT_PROJECT_VERSION", "")
	project := gitRepoOnBranch(t, "main")
	dir, err := EnsureKnowledgeProjectDir(project)
	if err != nil {
		t.Fatal(err)
	}
	if want := VersionStoreDir(KnowledgeProjectRoot(project), "main"); dir != want {
		t.Fatalf("got %q want %q", dir, want)
	}
	if exists(dir) {
		t.Fatal("ensure must leave creating the default version to its indexer")
	}
}
