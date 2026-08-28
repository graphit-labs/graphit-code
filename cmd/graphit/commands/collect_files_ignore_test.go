package commands

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast"
)

// collectFilesForPath is the CLI's discovery pass, and it runs where the pipeline's
// own discovery does not: `ast index` feeds the pipeline a scoped ChangedPaths list,
// so collectFiles — which applies the ignore checker — never executes. Whatever
// ignore rules are honoured therefore depends on THIS function applying them. These
// tests pin that: .gitignore and .astignore have to be obeyed, a scoped path
// (`ast index internal/ui`) still obeys the PROJECT-root rules, and nothing — not
// even a dot-directory — is excluded structurally: exclusion comes from the rules.
func TestCollectFilesForPathHonorsGitignore(t *testing.T) {
	root := t.TempDir()
	if !ast.HasParserForExtensionIn(root, ".js") {
		t.Skip("grammar unavailable on this machine")
	}
	mk := func(rel, body string) {
		t.Helper()
		p := filepath.Join(root, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	mk("keep/a.js", "const a = 1;")
	mk("node_modules/pkg/b.js", "const b = 1;")
	mk("internal/ui/node_modules/pkg/c.js", "const c = 1;")
	mk(".gitignore", "internal/ui/node_modules/\n")

	files, err := collectFilesForPath(root, root)
	if err != nil {
		t.Fatalf("collect: %v", err)
	}
	paths := allPaths(files)
	if len(paths) != 1 || paths[0] != filepath.Join(root, "keep", "a.js") {
		t.Errorf("node_modules must be ignored, got %v", paths)
	}
}

func TestCollectFilesForPathHonorsAstignore(t *testing.T) {
	root := t.TempDir()
	if !ast.HasParserForExtensionIn(root, ".go") && !ast.HasParserForExtensionIn(root, ".js") {
		t.Skip("grammar unavailable on this machine")
	}
	mk := func(rel, body string) {
		t.Helper()
		p := filepath.Join(root, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	mk("keep/a.go", "package keep\n")
	mk("secret/x.js", "const x = 1;")
	mk(".astignore", "secret/\n")

	files, err := collectFilesForPath(root, root)
	if err != nil {
		t.Fatalf("collect: %v", err)
	}
	paths := allPaths(files)
	if len(paths) != 1 || paths[0] != filepath.Join(root, "keep", "a.go") {
		t.Errorf(".astignore must be obeyed, got %v", paths)
	}
}

// Dot-directories are not excluded structurally: the rules decide. A dot-directory
// with no rule against it is indexed; the same directory is skipped once a rule
// names it.
func TestCollectFilesForPathDotDirectoriesAreRuledByIgnores(t *testing.T) {
	root := t.TempDir()
	if !ast.HasParserForExtensionIn(root, ".js") {
		t.Skip("grammar unavailable on this machine")
	}
	mk := func(rel, body string) {
		t.Helper()
		p := filepath.Join(root, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	mk(".opencode/kept.js", "const k = 1;")

	files, err := collectFilesForPath(root, root)
	if err != nil {
		t.Fatalf("collect: %v", err)
	}
	found := false
	for _, f := range files {
		if f == filepath.Join(root, ".opencode", "kept.js") {
			found = true
		}
	}
	if !found {
		t.Errorf("an unruled dot-directory was excluded structurally: %v", allPaths(files))
	}

	// Now rule it out and re-collect: the same path must disappear.
	if err := os.WriteFile(filepath.Join(root, ".astignore"), []byte(".opencode/\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	files, err = collectFilesForPath(root, root)
	if err != nil {
		t.Fatalf("collect: %v", err)
	}
	for _, f := range files {
		if f == filepath.Join(root, ".opencode", "kept.js") {
			t.Errorf("a ruled-out dot-directory was collected anyway: %v", allPaths(files))
		}
	}
}

// A scoped index (`ast index internal/ui`) walks only the subdirectory but must
// honour the rules anchored at the PROJECT root — that is what the boundary
// argument exists for.
func TestCollectFilesForPathScopedStaysInsideProjectBoundary(t *testing.T) {
	root := t.TempDir()
	if !ast.HasParserForExtensionIn(root, ".js") {
		t.Skip("grammar unavailable on this machine")
	}
	mk := func(rel, body string) {
		t.Helper()
		p := filepath.Join(root, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	mk("internal/ui/keep.ts", "export const x = 1;")
	mk("internal/ui/node_modules/pkg/z.js", "const z = 1;")
	mk(".gitignore", "internal/ui/node_modules/\n")

	scoped := filepath.Join(root, "internal", "ui")
	files, err := collectFilesForPath(scoped, root)
	if err != nil {
		t.Fatalf("collect: %v", err)
	}
	paths := allPaths(files)
	if len(paths) != 1 || paths[0] != filepath.Join(scoped, "keep.ts") {
		t.Errorf("project-root rules must apply to a scoped path, got %v", paths)
	}
}

func allPaths(files []string) []string {
	out := make([]string, 0, len(files))
	for _, f := range files {
		out = append(out, filepath.ToSlash(f))
	}
	return out
}
