package mcpstdio

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func isolateHome(t *testing.T) string {
	t.Helper()
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	return home
}

func TestASTConfigForProjectResolvesTheNamedProjectsStore(t *testing.T) {
	t.Setenv("LADYBUGDB_PATH", "")
	isolateHome(t)

	target := t.TempDir()
	bystander := t.TempDir()
	t.Chdir(bystander)

	got := astConfigForProject(target, "").StoreDir
	if want := store.ASTProjectDir(target); got != want {
		t.Errorf("StoreDir = %q; want %q", got, want)
	}
	if !filepath.IsAbs(got) {
		t.Errorf("StoreDir = %q; want an absolute path", got)
	}
	if got == store.ASTProjectDir(bystander) {
		t.Error("the request for one project resolved to the working-directory project's store")
	}
	if strings.HasPrefix(got, target) {
		t.Errorf("StoreDir = %q; the store must live in the global directory", got)
	}
}

func TestSyncASTPipelineOptionsKeepsTheProjectIDInTheCachePath(t *testing.T) {
	t.Setenv("LADYBUGDB_PATH", "")
	isolateHome(t)

	project := t.TempDir()
	lock := []byte(`{"project":{"id":"01KSH1CRFFG8Z74B5ZS78WW808"}}`)
	if err := os.WriteFile(filepath.Join(project, brand.LockFileName()), lock, 0o644); err != nil {
		t.Fatalf("write project lock: %v", err)
	}

	got := syncASTPipelineOptions(project, nil).CacheDir
	want := store.ASTProjectDir(project)
	if got != want {
		t.Fatalf("CacheDir = %q; want project-scoped store %q", got, want)
	}
	if got == filepath.Dir(want) {
		t.Fatalf("CacheDir = %q; project identity was stripped", got)
	}
}

// TestASTConfigForProjectKeepsAbsolutePaths guards the environment override, which
// names a store outright and must not be rewritten.
func TestASTConfigForProjectKeepsAbsolutePaths(t *testing.T) {
	absolute := filepath.Join(t.TempDir(), "gnaisse", "graph.icebug")
	t.Setenv("LADYBUGDB_PATH", absolute)

	project := t.TempDir()
	cfg := astConfigForProject(project, "")
	if cfg.IcebugDir != absolute || cfg.StoreDir != filepath.Dir(absolute) {
		t.Errorf("override not honored: IcebugDir=%q StoreDir=%q; want IcebugDir=%q StoreDir=%q",
			cfg.IcebugDir, cfg.StoreDir, absolute, filepath.Dir(absolute))
	}
}

// TestOpenASTDBReadWriteWritesIntoTheRequestedProjectsStore proves the path through
// an actual write, because the backend connects lazily: a wrong path is invisible
// until something forces the open, which is exactly why the old chdir-and-restore
// approach looked correct and was not.
func TestOpenASTDBReadWriteWritesIntoTheRequestedProjectsStore(t *testing.T) {
	t.Setenv("LADYBUGDB_PATH", "")
	isolateHome(t)

	target := t.TempDir()
	bystander := t.TempDir()

	t.Chdir(bystander)

	db, err := openASTDBReadWrite(target, "")
	if err != nil {
		t.Fatalf("openASTDBReadWrite() error: %v", err)
	}
	defer func() { _ = db.Close() }()

	wantDB := store.ASTProjectDir(target)
	pathed, ok := db.(interface{ StoreDir() string })
	if !ok {
		t.Fatalf("backend %T does not expose StoreDir", db)
	}
	if got := pathed.StoreDir(); got != wantDB {
		t.Fatalf("StoreDir = %q; want %q", got, wantDB)
	}

	if err := os.MkdirAll(wantDB, 0o755); err != nil {
		t.Fatalf("store dir: %v", err)
	}
	if _, err := db.Execute(context.Background(),
		"CREATE NODE TABLE Olivina(uid STRING, PRIMARY KEY(uid))", nil); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}

	if _, err := os.Stat(wantDB); err != nil {
		t.Errorf("database was not created in the requested project's store: %v", err)
	}
	if _, err := os.Stat(store.ASTProjectDir(bystander)); !os.IsNotExist(err) {
		t.Errorf("indexing %s created a store for the working-directory project %s", target, bystander)
	}
	if _, err := os.Stat(filepath.Join(target, brand.DotDir())); !os.IsNotExist(err) {
		t.Errorf("indexing created %s inside the project; the store is global", brand.DotDir())
	}
}

// TestOpenASTDBReportsMissingDatabaseInRequestedProject covers the read path: a
// database belonging to the server's own project must not satisfy a request for
// another one, and the error has to name the store that was actually looked for.
func TestOpenASTDBReportsMissingDatabaseInRequestedProject(t *testing.T) {
	t.Setenv("LADYBUGDB_PATH", "")
	isolateHome(t)

	target := t.TempDir()
	bystander := t.TempDir()

	if err := os.MkdirAll(store.ASTProjectIcebugDir(bystander), 0o755); err != nil {
		t.Fatalf("seed bystander bundle: %v", err)
	}
	t.Chdir(bystander)

	_, err := openASTDB(target, "")
	if err == nil {
		t.Fatal("openASTDB() succeeded; want a missing-database error for the target project")
	}
	if want := store.ASTProjectIcebugDir(target); !strings.Contains(err.Error(), want) {
		t.Errorf("error = %q; want it to name the target bundle %q", err, want)
	}
}

func TestResolveWikiDirResolvesTheNamedProjectsWiki(t *testing.T) {
	isolateHome(t)

	target := t.TempDir()
	bystander := t.TempDir()
	t.Chdir(bystander)

	got := resolveWikiDir("knowledge", target, "")
	if want := store.KnowledgeProjectDir(target); got != want {
		t.Errorf("resolveWikiDir() = %q; want %q", got, want)
	}
	if got == store.KnowledgeProjectDir(bystander) {
		t.Error("the request for one project resolved to the working-directory project's wiki")
	}
	if strings.HasPrefix(got, target) {
		t.Errorf("resolveWikiDir() = %q; the wiki must live in the global directory", got)
	}
}

// A memory scope resolves the same way, and an imported context resolves by name
// rather than by project — a distinction the single `context` parameter carries for
// both wikis.
func TestResolveWikiDirCoversMemoryScopesAndContexts(t *testing.T) {
	isolateHome(t)
	target := t.TempDir()

	if got := resolveWikiDir("memory", target, "project"); got != "" {
		t.Errorf("a project with no lockfile has no project scope, got %q", got)
	}
	if got := resolveWikiDir("memory", target, "some-context"); got == "" {
		t.Error("a memory context is named by itself and must resolve without a lockfile")
	}
	if got := resolveWikiDir("knowledge", target, "some-docs"); got != store.KnowledgeContextDir("some-docs") {
		t.Errorf("knowledge context resolved to %q", got)
	}
	if got := resolveWikiDir("nonsense", target, ""); got != "" {
		t.Errorf("an unknown module must resolve to nothing, got %q", got)
	}
}
