package ignorer

import (
	"os"
	"path/filepath"
	"testing"
)

func TestIgnoreChecker(t *testing.T) {
	// Create temp directory structure
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
		{"nested/file.log", false, true},    // matching *.log from root .gitignore
		{"nested/file.tmp", false, true},    // matching *.tmp from nested .gitignore
		{"nested/file.bak", false, true},    // matching *.bak from custom.ignore
		{"nested/file.default", false, true}, // matching default patterns
		{"nested/file.txt", false, false},   // not ignored
		{"build", true, true},               // matching /build/ from root
		{"nested/build", true, false},       // /build/ is rooted, so nested/build is not ignored
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
	// Test readPatternsFromFile with invalid file path
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
	// Create temp directory structure with negation patterns
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
		// antlr/ is ignored but has negated children → must descend
		{"internal/ast/antlr", true},
		// internal/ast is NOT ignored (so ShouldDescend is irrelevant but should still be correct)
		{"internal/ast", true},
		// internal is a prefix of a negation → should descend
		{"internal", true},
		// vendor/ is ignored with NO negation children → do NOT descend
		{"vendor", false},
		// Edge cases
		{"", false},
		{".", false},
		// A dir that's not mentioned at all
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
