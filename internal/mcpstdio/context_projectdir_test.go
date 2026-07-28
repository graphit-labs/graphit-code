package mcpstdio

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// projectDBPath is where a project's own AST database belongs.
func projectDBPath(projectDir string) string {
	return filepath.Join(projectDir, brand.DotDir(), "ast", "project", "ladybugdb")
}

// TestASTConfigForProjectAnchorsRelativePaths pins the resolution rule itself,
// without needing a database: the config constructors hand back a path relative
// to a project root, and it must come out anchored to the project that was asked
// for rather than to wherever the server happens to be running.
func TestASTConfigForProjectAnchorsRelativePaths(t *testing.T) {
	t.Setenv("LADYBUGDB_PATH", "")

	target := t.TempDir()
	bystander := t.TempDir()
	t.Chdir(bystander)

	got := astConfigForProject(target, "").DBPath
	if want := projectDBPath(target); got != want {
		t.Errorf("DBPath = %q; want %q", got, want)
	}
	if !filepath.IsAbs(got) {
		t.Errorf("DBPath = %q; want an absolute path", got)
	}
}

// TestASTConfigForProjectKeepsAbsolutePaths guards the pass-through: imported
// contexts live in the home directory, and anchoring must not drag them into the
// project tree.
func TestASTConfigForProjectKeepsAbsolutePaths(t *testing.T) {
	absolute := filepath.Join(t.TempDir(), "gnaisse", "ladybugdb")
	t.Setenv("LADYBUGDB_PATH", absolute)

	project := t.TempDir()
	if got := astConfigForProject(project, "").DBPath; got != absolute {
		t.Errorf("DBPath = %q; want the absolute override %q", got, absolute)
	}
}

// TestOpenASTDBReadWriteWritesInsideRequestedProject is the regression test for
// the contamination bug: indexing a project from a server sitting in a different
// one wrote the nodes into the server's own graph and still reported success.
//
// The backend connects lazily, so the path is only proven once a write forces the
// open — which is exactly why the old chdir-and-restore approach looked correct
// and was not.
func TestOpenASTDBReadWriteWritesInsideRequestedProject(t *testing.T) {
	t.Setenv("LADYBUGDB_PATH", "")

	target := t.TempDir()
	bystander := t.TempDir()

	// The MCP server runs from wherever it was started, not from the project it
	// is asked to index. That mismatch is the whole bug.
	t.Chdir(bystander)

	db, err := openASTDBReadWrite(target, "")
	if err != nil {
		t.Fatalf("openASTDBReadWrite() error: %v", err)
	}
	defer func() { _ = db.Close() }()

	wantDB := projectDBPath(target)
	pathed, ok := db.(interface{ DBPath() string })
	if !ok {
		t.Fatalf("backend %T does not expose DBPath", db)
	}
	if got := pathed.DBPath(); got != wantDB {
		t.Fatalf("DBPath = %q; want %q", got, wantDB)
	}

	if _, err := db.Execute(context.Background(),
		"CREATE NODE TABLE Olivina(uid STRING, PRIMARY KEY(uid))", nil); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}

	if _, err := os.Stat(wantDB); err != nil {
		t.Errorf("database was not created in the requested project: %v", err)
	}
	if _, err := os.Stat(filepath.Join(bystander, brand.DotDir())); !os.IsNotExist(err) {
		t.Errorf("indexing %s created %s in the working-directory project %s",
			target, brand.DotDir(), bystander)
	}
}

// TestOpenASTDBReportsMissingDatabaseInRequestedProject covers the read path,
// where an os.Stat run inside the old chdir block masked the same bug: the
// database was looked for in the right place and then opened from the wrong one.
func TestOpenASTDBReportsMissingDatabaseInRequestedProject(t *testing.T) {
	t.Setenv("LADYBUGDB_PATH", "")

	target := t.TempDir()
	bystander := t.TempDir()

	// A database in the server's own project must not satisfy a request for the
	// target project.
	if err := os.MkdirAll(projectDBPath(bystander), 0o755); err != nil {
		t.Fatalf("seed bystander db: %v", err)
	}
	t.Chdir(bystander)

	_, err := openASTDB(target, "")
	if err == nil {
		t.Fatal("openASTDB() succeeded; want a missing-database error for the target project")
	}
	if want := projectDBPath(target); !strings.Contains(err.Error(), want) {
		t.Errorf("error = %q; want it to name the target database %q", err, want)
	}
}

// TestResolveWikiDirAnchorsToProject covers the same escape in the knowledge
// wiki path, which is built relative to a project root and was returned out of
// the chdir block unresolved.
func TestResolveWikiDirAnchorsToProject(t *testing.T) {
	target := t.TempDir()
	bystander := t.TempDir()
	t.Chdir(bystander)

	got := resolveWikiDir("knowledge", target, "")
	if want := filepath.Join(target, brand.DotDir(), "knowledge", "project"); got != want {
		t.Errorf("resolveWikiDir() = %q; want %q", got, want)
	}
}
