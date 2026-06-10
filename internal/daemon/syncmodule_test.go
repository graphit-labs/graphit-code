package daemon

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ignorer"
)

// ---------------------------------------------------------------------------
// mockGit for syncmodule tests
// ---------------------------------------------------------------------------

type mockGit struct {
	runOutputFn func(repoDir string, args ...string) (string, error)
}

func (m *mockGit) Run(repoDir string, args ...string) error { return nil }
func (m *mockGit) RunOutput(repoDir string, args ...string) (string, error) {
	if m.runOutputFn != nil {
		return m.runOutputFn(repoDir, args...)
	}
	return "", nil
}
func (m *mockGit) RunSilent(repoDir string, args ...string) string { return "" }
func (m *mockGit) RunWithStdin(repoDir string, data []byte, args ...string) (string, error) {
	return "", nil
}
func (m *mockGit) RunWithEnv(repoDir string, env map[string]string, args ...string) error {
	return nil
}
func (m *mockGit) RunOutputWithEnv(repoDir string, env map[string]string, args ...string) (string, error) {
	return "", nil
}
func (m *mockGit) RunGlobal(args ...string) error                     { return nil }
func (m *mockGit) RunGlobalOutput(args ...string) (string, error)     { return "", nil }

// ---------------------------------------------------------------------------
// SyncModule — Name
// ---------------------------------------------------------------------------

func TestSyncModule_Name(t *testing.T) {
	m := NewSyncModule("/tmp", "/cache")
	if m.Name() != "sync" {
		t.Errorf("expected 'sync', got %q", m.Name())
	}
}

// ---------------------------------------------------------------------------
// gitStateHash
// ---------------------------------------------------------------------------

func TestGitStateHash_Empty(t *testing.T) {
	g := &mockGit{}
	hash := gitStateHash(g, "/tmp", nil)
	if hash == "" {
		t.Error("expected non-empty hash")
	}
}

func TestGitStateHash_WithData(t *testing.T) {
	tmpDir := t.TempDir()
	// Create a file so mtime resolution works
	testFile := filepath.Join(tmpDir, "file.go")
	_ = os.WriteFile(testFile, []byte("content"), 0o644)

	g := &mockGit{
		runOutputFn: func(repoDir string, args ...string) (string, error) {
			if len(args) > 0 && args[0] == "status" {
				return " M  file.go\n", nil
			}
			if len(args) > 0 && args[0] == "rev-parse" {
				return "abc123", nil
			}
			return "", nil
		},
	}
	hash := gitStateHash(g, tmpDir, nil)
	if hash == "" {
		t.Error("expected non-empty hash")
	}
}

func TestGitStateHash_WithIgnoreChecker(t *testing.T) {
	tmpDir := t.TempDir()
	// Create a .gitignore that ignores *.log files
	_ = os.WriteFile(filepath.Join(tmpDir, ".gitignore"), []byte("*.log\n"), 0o644)

	g := &mockGit{
		runOutputFn: func(repoDir string, args ...string) (string, error) {
			if len(args) > 0 && args[0] == "status" {
				return " M  file.go\n M  ignored.log\n", nil
			}
			if len(args) > 0 && args[0] == "rev-parse" {
				return "abc", nil
			}
			return "", nil
		},
	}

	ic := ignorer.New(tmpDir, tmpDir, "", nil)

	// With ignore checker, ignored.log should be filtered out
	hashWithIC := gitStateHash(g, tmpDir, ic)
	hashWithoutIC := gitStateHash(g, tmpDir, nil)

	// Hashes should differ because the ignored file is filtered
	if hashWithIC == hashWithoutIC {
		t.Error("hash with ignore checker should differ from hash without")
	}
}

func TestGitStateHash_DifferentData(t *testing.T) {
	g1 := &mockGit{
		runOutputFn: func(repoDir string, args ...string) (string, error) {
			if len(args) > 0 && args[0] == "rev-parse" {
				return "hash1", nil
			}
			return "", nil
		},
	}
	g2 := &mockGit{
		runOutputFn: func(repoDir string, args ...string) (string, error) {
			if len(args) > 0 && args[0] == "rev-parse" {
				return "hash2", nil
			}
			return "", nil
		},
	}

	h1 := gitStateHash(g1, "/tmp", nil)
	h2 := gitStateHash(g2, "/tmp", nil)
	if h1 == h2 {
		t.Error("different HEAD commits should produce different hashes")
	}
}

// ---------------------------------------------------------------------------
// filterIgnored
// ---------------------------------------------------------------------------

func TestFilterIgnored_Empty(t *testing.T) {
	tmpDir := t.TempDir()
	ic := ignorer.New(tmpDir, tmpDir, "", nil)

	result := filterIgnored("", ic)
	if result != "" {
		t.Errorf("expected empty, got %q", result)
	}
}

func TestFilterIgnored_ShortLines(t *testing.T) {
	tmpDir := t.TempDir()
	ic := ignorer.New(tmpDir, tmpDir, "", nil)

	result := filterIgnored("AB\nC\n", ic)
	if result != "" {
		t.Errorf("expected empty for short lines, got %q", result)
	}
}

func TestFilterIgnored_NoMatch(t *testing.T) {
	tmpDir := t.TempDir()
	ic := ignorer.New(tmpDir, tmpDir, "", nil)

	porcelain := " M  main.go\n M  utils.go"
	result := filterIgnored(porcelain, ic)
	if !strings.Contains(result, "main.go") {
		t.Errorf("expected 'main.go' in result, got %q", result)
	}
	if !strings.Contains(result, "utils.go") {
		t.Errorf("expected 'utils.go' in result, got %q", result)
	}
}

func TestFilterIgnored_WithIgnoredFiles(t *testing.T) {
	tmpDir := t.TempDir()
	// Create gitignore that ignores *.log
	_ = os.WriteFile(filepath.Join(tmpDir, ".gitignore"), []byte("*.log\n"), 0o644)
	ic := ignorer.New(tmpDir, tmpDir, "", nil)

	porcelain := " M  main.go\n M  debug.log\n M  utils.go"
	result := filterIgnored(porcelain, ic)
	if strings.Contains(result, "debug.log") {
		t.Error("debug.log should be filtered out")
	}
	if !strings.Contains(result, "main.go") {
		t.Errorf("main.go should be kept, got %q", result)
	}
	if !strings.Contains(result, "utils.go") {
		t.Errorf("utils.go should be kept, got %q", result)
	}
}

func TestFilterIgnored_EmptyPath(t *testing.T) {
	tmpDir := t.TempDir()
	ic := ignorer.New(tmpDir, tmpDir, "", nil)

	porcelain := " M     \n M  real.go"
	result := filterIgnored(porcelain, ic)
	if !strings.Contains(result, "real.go") {
		t.Errorf("expected 'real.go' in result, got %q", result)
	}
}

func TestFilterIgnored_DirectoryPath(t *testing.T) {
	tmpDir := t.TempDir()
	ic := ignorer.New(tmpDir, tmpDir, "", nil)

	// Trailing "/" is stripped by TrimSuffix
	porcelain := " M  somedir/"
	result := filterIgnored(porcelain, ic)
	if !strings.Contains(result, "somedir") {
		t.Errorf("expected 'somedir' in result, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// waitDebounce
// ---------------------------------------------------------------------------

func TestWaitDebounce_ContextCancelled(t *testing.T) {
	g := &mockGit{}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	ok := waitDebounce(ctx, g, "/tmp", nil, "hash")
	if ok {
		t.Error("expected false when context is cancelled")
	}
}

func TestWaitDebounce_TimerFires(t *testing.T) {
	g := &mockGit{
		runOutputFn: func(repoDir string, args ...string) (string, error) {
			return "", nil // stable hash
		},
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	ok := waitDebounce(ctx, g, "/tmp", nil, "stable-hash")
	if !ok {
		t.Error("expected true after debounce timer fires")
	}
}

func TestWaitDebounce_HashChanges(t *testing.T) {
	callCount := 0
	g := &mockGit{
		runOutputFn: func(repoDir string, args ...string) (string, error) {
			callCount++
			if len(args) > 0 && args[0] == "rev-parse" {
				// Change the hash on second poll
				if callCount > 2 {
					return "new-head", nil
				}
				return "old-head", nil
			}
			return "", nil
		},
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	ok := waitDebounce(ctx, g, "/tmp", nil, "initial-hash")
	if !ok {
		t.Error("expected true after debounce stabilizes")
	}
}

// ---------------------------------------------------------------------------
// dirtyFileMtimes
// ---------------------------------------------------------------------------

func TestDirtyFileMtimes_EmptyInput(t *testing.T) {
	result := dirtyFileMtimes("", "/tmp")
	if result != "" {
		t.Errorf("expected empty string, got %q", result)
	}
}

func TestDirtyFileMtimes_ShortLines(t *testing.T) {
	result := dirtyFileMtimes("AB\nC\n", "/tmp")
	if result != "" {
		t.Errorf("expected empty string for short lines, got %q", result)
	}
}

func TestDirtyFileMtimes_DirectoryEntries(t *testing.T) {
	result := dirtyFileMtimes(" M  somedir/\n", "/tmp")
	if result != "" {
		t.Errorf("expected empty for directory entries, got %q", result)
	}
}

func TestDirtyFileMtimes_ValidFile(t *testing.T) {
	tmpDir := t.TempDir()
	testFile := filepath.Join(tmpDir, "test.txt")
	if err := os.WriteFile(testFile, []byte("hello"), 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}
	porcelain := " M  test.txt"
	result := dirtyFileMtimes(porcelain, tmpDir)
	if !strings.Contains(result, "test.txt:") {
		t.Errorf("expected 'test.txt:' in result, got %q", result)
	}
}

func TestDirtyFileMtimes_NonexistentFile(t *testing.T) {
	tmpDir := t.TempDir()
	porcelain := " M  nonexistent.txt"
	result := dirtyFileMtimes(porcelain, tmpDir)
	if result != "" {
		t.Errorf("expected empty for nonexistent file, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// SyncModule.reindex — calls external packages
// ---------------------------------------------------------------------------

func TestSyncModule_Reindex(t *testing.T) {
	tmpDir := t.TempDir()
	if err := os.MkdirAll(filepath.Join(tmpDir, ".graphit", "ast", "project"), 0o755); err != nil {
		t.Fatal(err)
	}

	m := NewSyncModule(tmpDir, filepath.Join(tmpDir, ".graphit", "ast", "project"))
	ctx := context.Background()
	// Should not panic
	m.reindex(ctx)
}

func TestSyncModule_ReindexKnowledge_NoDocs(t *testing.T) {
	tmpDir := t.TempDir()
	m := NewSyncModule(tmpDir, "")
	ctx := context.Background()
	// When no docs directory exists, this should return early
	m.reindexKnowledge(ctx, nil)
}

func TestSyncModule_ReindexKnowledge_WithDocs(t *testing.T) {
	tmpDir := t.TempDir()
	docsDir := filepath.Join(tmpDir, "docs")
	if err := os.MkdirAll(docsDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(docsDir, "test.md"), []byte("# Test"), 0o644); err != nil {
		t.Fatal(err)
	}

	m := NewSyncModule(tmpDir, "")
	ctx := context.Background()
	m.reindexKnowledge(ctx, nil)
}

func TestSyncModule_ReindexAST(t *testing.T) {
	tmpDir := t.TempDir()
	if err := os.MkdirAll(filepath.Join(tmpDir, ".graphit", "ast", "project"), 0o755); err != nil {
		t.Fatal(err)
	}

	m := NewSyncModule(tmpDir, filepath.Join(tmpDir, ".graphit", "ast", "project"))
	ctx := context.Background()
	m.reindexAST(ctx, nil)
}

// ---------------------------------------------------------------------------
// worktreeDirForBranch
// ---------------------------------------------------------------------------

func TestWorktreeDirForBranch(t *testing.T) {
	tests := []struct {
		name     string
		wtBase   string
		branch   string
		contains string
	}{
		{
			name:     "simple branch",
			wtBase:   "/tmp/wt",
			branch:   "main",
			contains: "main",
		},
		{
			name:     "branch with slashes",
			wtBase:   "/tmp/wt",
			branch:   "memory/project/abc",
			contains: "memory-project-abc",
		},
		{
			name:     "branch with spaces",
			wtBase:   "/tmp/wt",
			branch:   "feature branch",
			contains: "feature_branch",
		},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := worktreeDirForBranch(tc.wtBase, tc.branch)
			if !strings.Contains(got, tc.contains) {
				t.Errorf("expected path to contain %q, got %q", tc.contains, got)
			}
			if !strings.HasPrefix(got, tc.wtBase) {
				t.Errorf("expected path to start with %q, got %q", tc.wtBase, got)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// parseBranch
// ---------------------------------------------------------------------------

func TestParseBranch(t *testing.T) {
	tests := []struct {
		branch    string
		wantScope string
		wantID    string
	}{
		{"memory/project/abc123", "project", "abc123"},
		{"memory/user/johndoe", "user", "johndoe"},
		{"memory/project/some/nested/path", "project", "some/nested/path"},
		{"too-short", "project", ""},
		{"only/two", "project", ""},
	}
	for _, tc := range tests {
		t.Run(tc.branch, func(t *testing.T) {
			scope, id := parseBranch(tc.branch)
			if scope != tc.wantScope {
				t.Errorf("scope: expected %q, got %q", tc.wantScope, scope)
			}
			if id != tc.wantID {
				t.Errorf("id: expected %q, got %q", tc.wantID, id)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// memoryWorktreeHash
// ---------------------------------------------------------------------------

func TestMemoryWorktreeHash(t *testing.T) {
	g := &mockGit{
		runOutputFn: func(repoDir string, args ...string) (string, error) {
			if len(args) > 0 && args[0] == "status" {
				return " M  memory.md\n", nil
			}
			if len(args) > 0 && args[0] == "rev-parse" {
				return "abc123", nil
			}
			return "", nil
		},
	}

	hash := memoryWorktreeHash(g, t.TempDir())
	if hash == "" {
		t.Error("expected non-empty hash")
	}
}

func TestMemoryWorktreeHash_DifferentContent(t *testing.T) {
	g1 := &mockGit{
		runOutputFn: func(repoDir string, args ...string) (string, error) {
			if len(args) > 0 && args[0] == "rev-parse" {
				return "aaa", nil
			}
			return "", nil
		},
	}
	g2 := &mockGit{
		runOutputFn: func(repoDir string, args ...string) (string, error) {
			if len(args) > 0 && args[0] == "rev-parse" {
				return "bbb", nil
			}
			return "", nil
		},
	}

	h1 := memoryWorktreeHash(g1, "/tmp")
	h2 := memoryWorktreeHash(g2, "/tmp")
	if h1 == h2 {
		t.Error("different HEADs should produce different hashes")
	}
}
