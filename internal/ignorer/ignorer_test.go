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

	// Create root directory structure:
	// tempDir/
	//   .git/
	//   .gitignore (contains *.log, /build/)
	//   nested/
	//     .gitignore (contains *.tmp)
	//     custom.ignore (contains *.bak)
	//     file.log
	//     file.tmp
	//     file.bak
	//     file.txt

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

	// 1. Initialize Checker
	ic := New(tempDir, nestedDir, "custom.ignore", []string{"*.default", "   ", "# default comment"})

	// 2. Validate IsIgnored
	tests := []struct {
		path  string
		isDir bool
		want  bool
	}{
		{"", false, false},
		{".", false, false},
		{"nested/file.log", false, true},     // matching *.log from root .gitignore
		{"nested/file.tmp", false, true},     // matching *.tmp from nested .gitignore
		{"nested/file.bak", false, true},     // matching *.bak from custom.ignore
		{"nested/file.default", false, true}, // matching default patterns
		{"nested/file.txt", false, false},    // not ignored
		{"build", true, true},                // matching /build/ from root
		{"nested/build", true, false},        // /build/ is rooted, so nested/build is not ignored
	}

	for _, tc := range tests {
		got := ic.IsIgnored(tc.path, tc.isDir)
		if got != tc.want {
			t.Errorf("IsIgnored(%q, isDir=%t) = %t; want %t", tc.path, tc.isDir, got, tc.want)
		}
	}

	// 3. Test with non-existent startDir and empty customFileName
	ic2 := New(tempDir, "", "", nil)
	if ic2.IsIgnored("nested/file.log", false) != true {
		t.Error("expected nested/file.log to be ignored in fallback checker")
	}

	// 4. Test when no .git is found (parent directory resolves to itself at root "/" or similar)
	// We pass a root-like directory as rootPath
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

	// Test domainForFile with invalid root path / relational errors
	dom := domainForFile("/a/b/c/.gitignore", "d/e/f")
	// Since /a/b/c is absolute and d/e/f is relative, filepath.Rel will return error
	if dom != nil {
		t.Errorf("expected nil domain on error, got %v", dom)
	}

	// Test findGitRoot and collectIgnoreFiles with root path and disconnected dirs
	icNoGit := New("/nonexistent_root_xyz", "/nonexistent_root_xyz/sub", "custom.ignore", []string{"#comment", ""})
	if icNoGit == nil {
		t.Error("expected non-nil IgnoreChecker")
	}

	// Test collectIgnoreFiles traversing to root of filesystem
	// by setting startDir to a temp path and rootDir to a completely different path
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

	// .astignore with negation patterns (mimics the real .astignore):
	//   internal/ast/antlr/         ← ignore entire dir
	//   !internal/ast/antlr/common/ ← but re-include common/
	//   !internal/ast/antlr/*/driver.go ← and re-include driver.go in each sub-dir
	//   vendor/                     ← ignore vendor with no negations
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

	// No ignore files on disk — all patterns come from defaults
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
		// Directory negation
		{"internal/ast/antlr/common/", nil, "internal/ast/antlr/common"},
		// Glob pattern — stops at first wildcard
		{"internal/ast/antlr/*/driver.go", nil, "internal/ast/antlr"},
		// Simple file negation
		{"keep.txt", nil, "keep.txt"},
		// With domain
		{"sub/file.go", []string{"nested"}, "nested/sub/file.go"},
		// Glob at start
		{"*.go", nil, ""},
		// Trailing spaces
		{"path/to/dir  ", nil, "path/to/dir"},
	}

	for _, tc := range tests {
		got := negationToPrefix(tc.body, tc.domain)
		if got != tc.want {
			t.Errorf("negationToPrefix(%q, %v) = %q; want %q", tc.body, tc.domain, got, tc.want)
		}
	}
}

// THE NO-GIT PATH, which is what every other test in this file was hiding.
//
// Each of them creates a `.git` directory — not because the code under test needs a repository, but
// because findBoundary had nothing else to stop at. That made the whole custom-ignore mechanism
// untested without git, right as the framework stopped requiring one. These cover it.

// writeFile is a helper: the failure of a fixture write is never the thing under test.
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

// A KNOWN LIMITATION, asserted so it is a decision and not a surprise: an ignore file ABOVE the
// project root does not apply to the project.
//
// It never did. Collection used to walk up to the repository root and read it, but domainForFile
// computes a pattern's domain with filepath.Rel(project, dir), so a file above the project got a
// domain of ".." segments that can never match a real path — collected, and silently inert. The
// boundary is the project now, which makes the same outcome honest and removes the hazard of
// reaching into an unrelated ancestor repository.
//
// The consequence to know about: in a monorepo, node_modules/ in the repository-root .gitignore
// does not exclude it from a sub-project's index. Fixing that means computing domains against the
// collection root, which is a separate change.
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
