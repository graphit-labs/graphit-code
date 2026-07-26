package daemon

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
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
func (m *mockGit) RunGlobal(args ...string) error                 { return nil }
func (m *mockGit) RunGlobalOutput(args ...string) (string, error) { return "", nil }

// ---------------------------------------------------------------------------
// SyncModule — Name
// ---------------------------------------------------------------------------

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
	m.reindexAST(ctx, nil, nil, nil) // nil scope = full scan
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
