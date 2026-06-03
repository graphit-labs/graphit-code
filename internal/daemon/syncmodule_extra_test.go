package daemon

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ignorer"
)

// ---------------------------------------------------------------------------
// SyncModule — constructor fields
// ---------------------------------------------------------------------------

func TestNewSyncModule_Fields(t *testing.T) {
	t.Parallel()
	m := NewSyncModule("/my/project", "/my/cache")
	if m.projectDir != "/my/project" {
		t.Errorf("projectDir: expected '/my/project', got %q", m.projectDir)
	}
	if m.cacheDir != "/my/cache" {
		t.Errorf("cacheDir: expected '/my/cache', got %q", m.cacheDir)
	}
}

// ---------------------------------------------------------------------------
// dirtyFileMtimes — multiple files
// ---------------------------------------------------------------------------

func TestDirtyFileMtimes_MultipleFiles(t *testing.T) {
	tmpDir := t.TempDir()
	_ = os.WriteFile(filepath.Join(tmpDir, "a.go"), []byte("a"), 0o644)
	_ = os.WriteFile(filepath.Join(tmpDir, "b.go"), []byte("b"), 0o644)

	porcelain := " M  a.go\n M  b.go"
	result := dirtyFileMtimes(porcelain, tmpDir)
	if !strings.Contains(result, "a.go:") {
		t.Errorf("expected 'a.go:' in result, got %q", result)
	}
	if !strings.Contains(result, "b.go:") {
		t.Errorf("expected 'b.go:' in result, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// dirtyFileMtimes — whitespace-only filename
// ---------------------------------------------------------------------------

func TestDirtyFileMtimes_WhitespaceOnlyFilename(t *testing.T) {
	t.Parallel()
	porcelain := " M     \n"
	result := dirtyFileMtimes(porcelain, "/tmp")
	if result != "" {
		t.Errorf("expected empty for whitespace-only filename, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// filterIgnored — all files ignored
// ---------------------------------------------------------------------------

func TestFilterIgnored_AllIgnored(t *testing.T) {
	tmpDir := t.TempDir()
	_ = os.WriteFile(filepath.Join(tmpDir, ".gitignore"), []byte("*.log\n*.tmp\n"), 0o644)

	ic := ignorer.New(tmpDir, tmpDir, "", nil)

	porcelain := " M  debug.log\n M  cache.tmp"
	result := filterIgnored(porcelain, ic)
	if result != "" {
		t.Errorf("expected empty when all files are ignored, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// gitStateHash — deterministic
// ---------------------------------------------------------------------------

func TestGitStateHash_Deterministic(t *testing.T) {
	g := &mockGit{
		runOutputFn: func(repoDir string, args ...string) (string, error) {
			if len(args) > 0 && args[0] == "status" {
				return " M  file.go\n", nil
			}
			if len(args) > 0 && args[0] == "rev-parse" {
				return "fixed-hash", nil
			}
			return "", nil
		},
	}

	h1 := gitStateHash(g, "/tmp", nil)
	h2 := gitStateHash(g, "/tmp", nil)
	if h1 != h2 {
		t.Errorf("same input should produce same hash: %q != %q", h1, h2)
	}
}

// ---------------------------------------------------------------------------
// gitStateHash — nil ignoreChecker
// ---------------------------------------------------------------------------

func TestGitStateHash_NilIgnoreChecker(t *testing.T) {
	g := &mockGit{
		runOutputFn: func(repoDir string, args ...string) (string, error) {
			if len(args) > 0 && args[0] == "status" {
				return " M  file.go\n", nil
			}
			return "abc", nil
		},
	}

	hash := gitStateHash(g, "/tmp", nil)
	if hash == "" {
		t.Error("expected non-empty hash")
	}
}

// ---------------------------------------------------------------------------
// worktreeDirForBranch — empty components
// ---------------------------------------------------------------------------

func TestWorktreeDirForBranch_EmptyBranch(t *testing.T) {
	t.Parallel()
	got := worktreeDirForBranch("/tmp/wt", "")
	if got != "/tmp/wt/" {
		// filepath.Join("/tmp/wt", "") = "/tmp/wt"
		if got != "/tmp/wt" {
			t.Errorf("expected '/tmp/wt' or '/tmp/wt/', got %q", got)
		}
	}
}

// ---------------------------------------------------------------------------
// parseBranch — edge cases
// ---------------------------------------------------------------------------

func TestParseBranch_EmptyString(t *testing.T) {
	t.Parallel()
	scope, id := parseBranch("")
	if scope != "project" {
		t.Errorf("scope: expected 'project', got %q", scope)
	}
	if id != "" {
		t.Errorf("id: expected empty, got %q", id)
	}
}

func TestParseBranch_SingleSegment(t *testing.T) {
	t.Parallel()
	scope, id := parseBranch("main")
	if scope != "project" {
		t.Errorf("scope: expected 'project', got %q", scope)
	}
	if id != "" {
		t.Errorf("id: expected empty, got %q", id)
	}
}

func TestParseBranch_TwoSegments(t *testing.T) {
	t.Parallel()
	scope, id := parseBranch("memory/project")
	if scope != "project" {
		t.Errorf("scope: expected 'project', got %q", scope)
	}
	if id != "" {
		t.Errorf("id: expected empty, got %q", id)
	}
}

func TestParseBranch_FourSegments(t *testing.T) {
	t.Parallel()
	scope, id := parseBranch("memory/user/john/extra")
	if scope != "user" {
		t.Errorf("scope: expected 'user', got %q", scope)
	}
	if id != "john/extra" {
		t.Errorf("id: expected 'john/extra', got %q", id)
	}
}

// ---------------------------------------------------------------------------
// MemorySyncModule — constructor
// ---------------------------------------------------------------------------

func TestNewMemorySyncModule_NotNil(t *testing.T) {
	t.Parallel()
	m := NewMemorySyncModule()
	if m == nil {
		t.Fatal("expected non-nil module")
	}
}

// ---------------------------------------------------------------------------
// memoryWorktreeHash — deterministic
// ---------------------------------------------------------------------------

func TestMemoryWorktreeHash_Deterministic(t *testing.T) {
	g := &mockGit{
		runOutputFn: func(repoDir string, args ...string) (string, error) {
			if len(args) > 0 && args[0] == "status" {
				return " M  file.md\n", nil
			}
			if len(args) > 0 && args[0] == "rev-parse" {
				return "fixed", nil
			}
			return "", nil
		},
	}

	h1 := memoryWorktreeHash(g, "/tmp")
	h2 := memoryWorktreeHash(g, "/tmp")
	if h1 != h2 {
		t.Errorf("same input should produce same hash: %q != %q", h1, h2)
	}
}

// ---------------------------------------------------------------------------
// memoryWorktreeHash — empty output
// ---------------------------------------------------------------------------

func TestMemoryWorktreeHash_EmptyGitOutput(t *testing.T) {
	g := &mockGit{}
	hash := memoryWorktreeHash(g, "/tmp")
	if hash == "" {
		t.Error("expected non-empty hash even with empty git output")
	}
}
