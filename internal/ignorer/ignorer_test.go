package ignorer

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestIgnoreChecker(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "ignorer-test")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()

	err = os.MkdirAll(filepath.Join(tempDir, ".git"), 0755)
	if err != nil {
		t.Fatalf("failed to create .git dir: %v", err)
	}

	err = os.WriteFile(filepath.Join(tempDir, ".gitignore"), []byte("*.log\n/build/\n# comment line\n   \n"), 0644)
	if err != nil {
		t.Fatalf("failed to write root .gitignore: %v", err)
	}

	nestedDir := filepath.Join(tempDir, "nested")
	err = os.MkdirAll(nestedDir, 0755)
	if err != nil {
		t.Fatalf("failed to create nested dir: %v", err)
	}

	err = os.WriteFile(filepath.Join(nestedDir, ".gitignore"), []byte("*.tmp\n"), 0644)
	if err != nil {
		t.Fatalf("failed to write nested .gitignore: %v", err)
	}

	err = os.WriteFile(filepath.Join(nestedDir, "custom.ignore"), []byte("*.bak\n"), 0644)
	if err != nil {
		t.Fatalf("failed to write custom ignore file: %v", err)
	}

	ic := New(tempDir, nestedDir, "custom.ignore", []string{"*.default", "   ", "# default comment"})

	tests := []struct {
		path  string
		isDir bool
		want  bool
	}{
		{"", false, false},
		{".", false, false},
		{"nested/file.log", false, true},
		{"nested/file.tmp", false, true},
		{"nested/file.bak", false, true},
		{"nested/file.default", false, true},
		{"nested/file.txt", false, false},
		{"build", true, true},
		{"nested/build", true, false},
	}

	for _, tc := range tests {
		got := ic.IsIgnored(tc.path, tc.isDir)
		if got != tc.want {
			t.Errorf("IsIgnored(%q, isDir=%t) = %t; want %t", tc.path, tc.isDir, got, tc.want)
		}
	}

	ic2 := New(tempDir, "", "", nil)
	if ic2.IsIgnored("nested/file.log", false) != true {
		t.Error("expected nested/file.log to be ignored in fallback checker")
	}

	ic3 := New(tempDir, tempDir, "", nil)
	if ic3 == nil {
		t.Error("expected non-nil IgnoreChecker")
	}
}

func TestUncoveredHelperFunctions(t *testing.T) {
	pats := readPatternsFromFile("/nonexistent/file", nil)
	if pats != nil {
		t.Errorf("expected nil patterns for nonexistent file, got %v", pats)
	}

	dom := domainForFile("/a/b/c/.gitignore", "d/e/f")
	if dom != nil {
		t.Errorf("expected nil domain on error, got %v", dom)
	}

	icNoGit := New("/nonexistent_root_xyz", "/nonexistent_root_xyz/sub", "custom.ignore", []string{"#comment", ""})
	if icNoGit == nil {
		t.Error("expected non-nil IgnoreChecker")
	}

	files := collectIgnoreFiles("/a/b/c", "/d/e/f", ".gitignore")
	if len(files) != 0 {
		t.Errorf("expected 0 files, got %d", len(files))
	}
}

func TestShouldDescend(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "ignorer-descend-test")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()

	err = os.MkdirAll(filepath.Join(tempDir, ".git"), 0755)
	if err != nil {
		t.Fatalf("failed to create .git dir: %v", err)
	}

	astignoreContent := "internal/ast/antlr/\n!internal/ast/antlr/common/\n!internal/ast/antlr/*/driver.go\nvendor/\n"
	err = os.WriteFile(filepath.Join(tempDir, ".astignore"), []byte(astignoreContent), 0644)
	if err != nil {
		t.Fatalf("failed to write .astignore: %v", err)
	}

	ic := New(tempDir, tempDir, ".astignore", nil)

	tests := []struct {
		dir  string
		want bool
	}{
		{"internal/ast/antlr", true},
		{"internal/ast", true},
		{"internal", true},
		{"internal/ast/antlr/plsql", true},
		{"internal/ast/antlr/java", true},
		{"internal/ast/antlr/common", true},
		{"vendor", false},
		{"", false},
		{".", false},
		{"some/other/dir", false},
	}

	for _, tc := range tests {
		got := ic.ShouldDescend(tc.dir)
		if got != tc.want {
			t.Errorf("ShouldDescend(%q) = %t; want %t", tc.dir, got, tc.want)
		}
	}
}

func TestShouldDescendWithDefaultPatterns(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "ignorer-default-neg-test")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()

	err = os.MkdirAll(filepath.Join(tempDir, ".git"), 0755)
	if err != nil {
		t.Fatalf("failed to create .git dir: %v", err)
	}

	ic := New(tempDir, tempDir, "", []string{
		"generated/",
		"!generated/keep/",
	})

	if !ic.ShouldDescend("generated") {
		t.Error("ShouldDescend(\"generated\") should be true with negated default pattern")
	}
	if ic.ShouldDescend("other") {
		t.Error("ShouldDescend(\"other\") should be false")
	}
}

func TestNegationToPrefix(t *testing.T) {
	tests := []struct {
		body   string
		domain []string
		want   string
	}{
		{"internal/ast/antlr/common/", nil, "internal/ast/antlr/common"},
		{"internal/ast/antlr/*/driver.go", nil, "internal/ast/antlr"},
		{"keep.txt", nil, "keep.txt"},
		{"sub/file.go", []string{"nested"}, "nested/sub/file.go"},
		{"*.go", nil, ""},
		{"path/to/dir  ", nil, "path/to/dir"},
	}

	for _, tc := range tests {
		got := negationToPrefix(tc.body, tc.domain)
		if got != tc.want {
			t.Errorf("negationToPrefix(%q, %v) = %q; want %q", tc.body, tc.domain, got, tc.want)
		}
	}
}

func writeIgnoreFixture(t *testing.T, path, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

// A project marked only by its lockfile honours its custom ignore file, at the root and nested.
func TestCustomIgnoreWorksWithNoGitAnywhere(t *testing.T) {
	root := t.TempDir()
	writeIgnoreFixture(t, filepath.Join(root, brand.LockFileName()), "{}")
	writeIgnoreFixture(t, filepath.Join(root, ".astignore"), "*.generated.go\nsecret/\n")

	ic := New(root, root, ".astignore", nil)

	for _, c := range []struct {
		path string
		want bool
	}{
		{"api.generated.go", true},
		{"pkg/api.generated.go", true},
		{"secret/keys.txt", true},
		{"main.go", false},
	} {
		if got := ic.IsIgnored(c.path, strings.HasSuffix(c.path, "/")); got != c.want {
			t.Errorf("ShouldIgnore(%q) = %v, want %v — custom ignore is not working without git",
				c.path, got, c.want)
		}
	}
}

// A .gitignore is still honoured with no repository present: the syntax is the contract, not git.
func TestGitignoreIsHonouredWithNoRepository(t *testing.T) {
	root := t.TempDir()
	writeIgnoreFixture(t, filepath.Join(root, brand.LockFileName()), "{}")
	writeIgnoreFixture(t, filepath.Join(root, ".gitignore"), "*.log\nbuild/\n!keep.log\n")

	ic := New(root, root, ".astignore", nil)

	if !ic.IsIgnored("app.log", false) {
		t.Error("*.log from .gitignore was not applied without a repository")
	}
	if !ic.IsIgnored("build/out.bin", false) {
		t.Error("build/ from .gitignore was not applied without a repository")
	}
	if ic.IsIgnored("keep.log", false) {
		t.Error("the negation !keep.log was not applied")
	}
}

// Collection starts at startDir and walks up to the project root, so a docs-level ignore file and a
// project-level one both apply — the scoped-build shape knowledge uses, without git.
func TestIgnoreFilesAreCollectedUpToTheProjectRootWithoutGit(t *testing.T) {
	root := t.TempDir()
	docs := filepath.Join(root, "docs")
	writeIgnoreFixture(t, filepath.Join(root, brand.LockFileName()), "{}")
	writeIgnoreFixture(t, filepath.Join(root, ".wikiignore"), "*.draft.md\n")
	writeIgnoreFixture(t, filepath.Join(docs, ".wikiignore"), "internal/\n")

	ic := New(root, docs, ".wikiignore", nil)

	if !ic.IsIgnored("docs/spec.draft.md", false) {
		t.Error("the project-root .wikiignore was not collected from the docs tree")
	}
	if !ic.IsIgnored("docs/internal/notes.md", false) {
		t.Error("the docs-level .wikiignore was not applied")
	}
	if ic.IsIgnored("docs/spec.md", false) {
		t.Error("an ordinary page was ignored")
	}
}

func TestAnIgnoreFileAboveTheProjectDoesNotApply(t *testing.T) {
	repo := t.TempDir()
	project := filepath.Join(repo, "packages", "app")
	writeIgnoreFixture(t, filepath.Join(repo, ".git", "HEAD"), "ref: refs/heads/main\n")
	writeIgnoreFixture(t, filepath.Join(repo, ".gitignore"), "node_modules/\n")
	writeIgnoreFixture(t, filepath.Join(project, brand.LockFileName()), "{}")
	writeIgnoreFixture(t, filepath.Join(project, ".gitignore"), "dist/\n")

	ic := New(project, project, ".astignore", nil)

	if ic.IsIgnored("node_modules/left-pad/index.js", false) {
		t.Error("a pattern from above the project applied — the boundary is no longer the project")
	}
	if !ic.IsIgnored("dist/bundle.js", false) {
		t.Error("the project's OWN .gitignore was not applied")
	}
}

// A nested ignore file applies when collection starts inside it, which is the scoped-build shape.
// Collection walks UP from startDir, so a nested file is not reached from the project root — that
// is the contract, not a bug.
func TestANestedIgnoreFileAppliesFromInside(t *testing.T) {
	root := t.TempDir()
	pkg := filepath.Join(root, "pkg")
	writeIgnoreFixture(t, filepath.Join(root, brand.LockFileName()), "{}")
	writeIgnoreFixture(t, filepath.Join(pkg, ".astignore"), "local.go\n")

	fromRoot := New(root, root, ".astignore", nil)
	if fromRoot.IsIgnored("pkg/local.go", false) {
		t.Error("a nested ignore file was read from the project root — collection walks up, not down")
	}

	fromPkg := New(root, pkg, ".astignore", nil)
	if !fromPkg.IsIgnored("pkg/local.go", false) {
		t.Error("the nested ignore file was not applied when collection started inside it")
	}
}

// THE CONTRACT, stated plainly: the project's .gitignore AND its custom ignore file, together, in
// the same checker. This already worked and must keep working — it is the reason the boundary
// change had to be careful rather than clever.
func TestProjectGitignorePlusCustomIgnoreBothApply(t *testing.T) {
	root := t.TempDir()
	writeIgnoreFixture(t, filepath.Join(root, brand.LockFileName()), "{}")
	writeIgnoreFixture(t, filepath.Join(root, ".gitignore"), "*.log\nbuild/\n")
	writeIgnoreFixture(t, filepath.Join(root, ".astignore"), "*.generated.go\ntestdata/\n")

	ic := New(root, root, ".astignore", []string{".graphit/"})

	for _, c := range []struct {
		path   string
		isDir  bool
		want   bool
		source string
	}{
		{"app.log", false, true, ".gitignore"},
		{"build/out.bin", false, true, ".gitignore"},
		{"api.generated.go", false, true, ".astignore"},
		{"testdata/fixture.json", false, true, ".astignore"},
		{".graphit/db", false, true, "default patterns"},
		{"main.go", false, false, "nothing"},
		{"docs/readme.md", false, false, "nothing"},
	} {
		if got := ic.IsIgnored(c.path, c.isDir); got != c.want {
			t.Errorf("IsIgnored(%q) = %v, want %v (from %s)", c.path, got, c.want, c.source)
		}
	}
}

// At is the walker hook: crossing into a directory picks up ITS ignore files,
// with the same git scope. `.opencode/.gitignore` with `node_modules` must
// exclude `.opencode/node_modules/` and nothing else.
func TestAtAppliesSubdirectoryIgnoreFiles(t *testing.T) {
	root := t.TempDir()
	writeIgnoreFixture(t, filepath.Join(root, brand.LockFileName()), "{}")
	writeIgnoreFixture(t, filepath.Join(root, ".opencode", ".gitignore"), "node_modules\n")
	writeIgnoreFixture(t, filepath.Join(root, ".opencode", ".astignore"), "cache/\n")

	atRoot := New(root, root, ".astignore", nil)

	if atRoot.IsIgnored(".opencode/node_modules/x.js", false) {
		t.Error("subdirectory ignore files were read before At crossed into it")
	}

	inside := atRoot.At(".opencode")
	if !inside.IsIgnored(".opencode/node_modules/x.js", false) {
		t.Error("node_modules from .opencode/.gitignore was not applied after At(.opencode)")
	}
	if !inside.IsIgnored(".opencode/node_modules/@pkg/index.js", false) {
		t.Error("a deep node_modules child was not applied after At(.opencode)")
	}
	if !inside.IsIgnored(".opencode/cache/dirs.zip", false) {
		t.Error("cache from .opencode/.astignore was not applied after At(.opencode)")
	}
	if inside.IsIgnored(".opencode/src.js", false) {
		t.Error("a kept file inside .opencode was ignored")
	}
	if inside.IsIgnored("keep/node_modules/x.js", false) {
		t.Error("node_modules scope leaked outside .opencode")
	}

	deeper := inside.At(".opencode/node_modules")
	if !deeper.IsIgnored(".opencode/node_modules/x.js", false) {
		t.Error("crossing deeper lost the parent's rules")
	}
	if !deeper.IsIgnored(".opencode/node_modules/.gitignore", false) {
		t.Error("crossing a nested dir must keep its own parent rules intact")
	}
}
