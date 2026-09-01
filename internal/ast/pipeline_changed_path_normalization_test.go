package ast

import (
	"path/filepath"
	"runtime"
	"testing"
)

// ChangedPaths reach the pipeline in two forms — repo-relative from the CLI, absolute from
// a watcher — and the absolute form used to be stored as-is. The stored path then differed
// from the one a full scan produces, so the graph carried TWO File nodes for one file, and
// a traversal anchored on File{path} could resolve to the wrong one and answer with
// another file's children. Neither failure raises an error, which is why it survived.
func TestRepoRelativePathsAcceptsBothFormsOfInput(t *testing.T) {
	t.Parallel()

	root := "/tmp/project"
	if runtime.GOOS == "windows" {
		t.Skip("path shapes below are POSIX")
	}

	got := repoRelativePaths(root, []string{
		filepath.Join(root, "internal", "ast", "pipeline.go"), // absolute, from a watcher
		"internal/ast/rule.go",                                // repo-relative, from the CLI
		"./internal/ast/writer.go",                            // relative, unclean
	})

	want := []string{
		"internal/ast/pipeline.go",
		"internal/ast/rule.go",
		"internal/ast/writer.go",
	}
	if len(got) != len(want) {
		t.Fatalf("got %d paths, want %d: %v", len(got), len(want), got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("path %d: got %q, want %q", i, got[i], want[i])
		}
	}
}

// The normalization must not resolve against the process working directory: the daemon
// serves several projects at once and cannot chdir into any of them, so a path is only
// ever meaningful relative to the root it was handed.
func TestRepoRelativePathsIgnoresTheWorkingDirectory(t *testing.T) {
	t.Parallel()

	if runtime.GOOS == "windows" {
		t.Skip("path shapes below are POSIX")
	}

	// A root that is nothing like the test's working directory. If the implementation
	// reached for filepath.Abs, the result would be prefixed with the CWD instead.
	got := repoRelativePaths("/srv/elsewhere", []string{"/srv/elsewhere/pkg/a.go"})
	if len(got) != 1 || got[0] != "pkg/a.go" {
		t.Errorf("normalization is working-directory dependent: %v", got)
	}
}

// A path outside the root has no relative form. Passing it through unchanged lets the
// discovery filters reject it; rewriting it would invent a file that is not there.
func TestRepoRelativePathsLeavesOutOfTreePathsAlone(t *testing.T) {
	t.Parallel()

	if runtime.GOOS == "windows" {
		t.Skip("path shapes below are POSIX")
	}

	outside := "/etc/passwd"
	got := repoRelativePaths("/srv/project", []string{outside})
	if len(got) != 1 || got[0] != outside {
		t.Errorf("an out-of-tree path was rewritten: %v", got)
	}
}

func TestRepoRelativePathsOnEmptyInput(t *testing.T) {
	t.Parallel()

	if got := repoRelativePaths("/srv/project", nil); got != nil {
		t.Errorf("nil input should stay nil, got %v", got)
	}
}
