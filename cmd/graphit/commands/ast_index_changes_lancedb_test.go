//go:build lancedb

package commands

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast"
)

func indexedFiles(t *testing.T) []string {
	t.Helper()
	db := ast.NewLadybugDBReadOnly(ast.DefaultLadybugConfig())
	defer func() { _ = db.Close() }()
	got, err := db.Query(context.Background(), "MATCH (f:File) RETURN f.relative_path AS p", nil)
	if err != nil {
		t.Fatalf("list files: %v", err)
	}
	var files []string
	for _, rec := range got.Records {
		files = append(files, fmt.Sprint(rec["p"]))
	}
	sort.Strings(files)
	return files
}

func writeGo(t *testing.T, root, name string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(root, name), []byte("package fixture\n\nfunc "+name[:1]+"() {}\n"), 0o600); err != nil {
		t.Fatal(err)
	}
}

func TestASTIndexChangeSetsAndFullReconcileRemoveDeletedFiles(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "false")
	profile, err := os.ReadFile(filepath.Join(testRepoRoot(t), "internal/ast/queries/go.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	root := t.TempDir()
	t.Chdir(root)
	queryDir := filepath.Join(root, ".graphit/ast/queries")
	if err := os.MkdirAll(queryDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(queryDir, "go.yaml"), profile, 0o600); err != nil {
		t.Fatal(err)
	}
	writeGo(t, root, "a.go")
	writeGo(t, root, "b.go")

	if err := runASTIndex([]string{"."}, 1, false, false, "", nil, false, "", nil); err != nil {
		t.Fatalf("full index: %v", err)
	}
	if got := indexedFiles(t); fmt.Sprint(got) != "[a.go b.go]" {
		t.Fatalf("after full index: %v", got)
	}

	if err := os.Remove(filepath.Join(root, "b.go")); err != nil {
		t.Fatal(err)
	}
	if err := runASTIndex([]string{"."}, 1, false, false, "", nil, false, "", &astChangeSet{Removed: []string{"b.go"}}); err != nil {
		t.Fatalf("change-set index: %v", err)
	}
	if got := indexedFiles(t); fmt.Sprint(got) != "[a.go]" {
		t.Fatalf("after removing b.go through a change set: %v", got)
	}

	if err := os.Remove(filepath.Join(root, "a.go")); err != nil {
		t.Fatal(err)
	}
	writeGo(t, root, "c.go")
	if err := runASTIndex([]string{"."}, 1, false, false, "", nil, false, "", nil); err != nil {
		t.Fatalf("second full index: %v", err)
	}
	if got := indexedFiles(t); fmt.Sprint(got) != "[c.go]" {
		t.Fatalf("full index kept a deleted file or missed a new one: %v", got)
	}
}

func testRepoRoot(t *testing.T) string {
	t.Helper()
	wd, err := filepath.Abs(filepath.Join("..", "..", ".."))
	if err != nil {
		t.Fatal(err)
	}
	return wd
}
