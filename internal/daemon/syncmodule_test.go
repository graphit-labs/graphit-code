package daemon

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

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
	// Lines shorter than 4 chars should be skipped
	result := dirtyFileMtimes("AB\nC\n", "/tmp")
	if result != "" {
		t.Errorf("expected empty string for short lines, got %q", result)
	}
}

func TestDirtyFileMtimes_DirectoryEntries(t *testing.T) {
	// Lines ending with "/" are directories and should be skipped
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
	// Porcelain format: " M test.txt" (2 status chars + space + filename)
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
// SyncModule — Name
// ---------------------------------------------------------------------------

func TestSyncModule_Name(t *testing.T) {
	m := NewSyncModule("/tmp", "/cache")
	if m.Name() != "sync" {
		t.Errorf("expected 'sync', got %q", m.Name())
	}
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
		{
			branch:    "memory/project/abc123",
			wantScope: "project",
			wantID:    "abc123",
		},
		{
			branch:    "memory/user/johndoe",
			wantScope: "user",
			wantID:    "johndoe",
		},
		{
			branch:    "memory/project/some/nested/path",
			wantScope: "project",
			wantID:    "some/nested/path",
		},
		{
			branch:    "too-short",
			wantScope: "project",
			wantID:    "",
		},
		{
			branch:    "only/two",
			wantScope: "project",
			wantID:    "",
		},
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
// MemorySyncModule — Name
// ---------------------------------------------------------------------------

func TestMemorySyncModule_Name(t *testing.T) {
	m := NewMemorySyncModule()
	if m.Name() != "memory-sync" {
		t.Errorf("expected 'memory-sync', got %q", m.Name())
	}
}
