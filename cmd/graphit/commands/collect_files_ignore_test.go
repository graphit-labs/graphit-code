package commands

import (
	"os"
	"path/filepath"
	"testing"
)

func TestCollectFilesForPathHonorsGitignore(t *testing.T) {
	root := t.TempDir()
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
	mk(".gitignore", "node_modules/\n")

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

func TestCollectFilesForPathDotDirectoriesAreRuledByIgnores(t *testing.T) {
	root := t.TempDir()
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

func TestCollectFilesForPathScopedStaysInsideProjectBoundary(t *testing.T) {
	root := t.TempDir()
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

func TestCollectFilesForPathHonorsSubdirectoryGitignore(t *testing.T) {
	root := t.TempDir()
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
	mk(".opencode/node_modules/@pkg/index.js", "const i = 1;")
	mk(".opencode/node_modules/zod/v4/core/standard-schema.js", "const z = 1;")
	mk(".opencode/keep.js", "const k = 1;")
	mk("keep/a.js", "const a = 1;")
	mk(".opencode/.gitignore", "node_modules\n")

	files, err := collectFilesForPath(root, root)
	if err != nil {
		t.Fatalf("collect: %v", err)
	}
	paths := allPaths(files)
	want := []string{
		filepath.Join(root, "keep", "a.js"),
		filepath.Join(root, ".opencode", "keep.js"),
	}
	if len(paths) != len(want) {
		t.Errorf("subdirectory gitignore wrong count: got %v, want %v", paths, want)
	}
	for _, w := range want {
		found := false
		for _, p := range paths {
			if p == w {
				found = true
				break
			}
		}
		if !found {
			t.Errorf("expected %v to be indexed, got %v", w, paths)
		}
	}
}

func allPaths(files []string) []string {
	out := make([]string, 0, len(files))
	for _, f := range files {
		out = append(out, filepath.ToSlash(f))
	}
	return out
}
