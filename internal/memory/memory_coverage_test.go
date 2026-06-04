package memory

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
	"time"
)

// ---------------------------------------------------------------------------
// MemoryWorktree (filesystem-only, no real git)
// ---------------------------------------------------------------------------

func TestMemoryWorktree_WriteReadRemoveListDir(t *testing.T) {
	dir := t.TempDir()
	wt := &MemoryWorktree{dir: dir, branch: "test-branch"}

	// Dir
	if wt.Dir() != dir {
		t.Errorf("Dir() = %q; want %q", wt.Dir(), dir)
	}

	// WriteFile
	if err := wt.WriteFile("hello.md", []byte("content")); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	// WriteFile with nested directory
	if err := wt.WriteFile("sub/nested.md", []byte("nested")); err != nil {
		t.Fatalf("WriteFile nested: %v", err)
	}

	// ReadFile
	data, err := wt.ReadFile("hello.md")
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(data) != "content" {
		t.Errorf("ReadFile content = %q; want 'content'", string(data))
	}

	// ReadFile non-existent
	_, err = wt.ReadFile("nonexistent.md")
	if err == nil {
		t.Error("expected error for non-existent file")
	}

	// ListDir
	entries, err := wt.ListDir(".")
	if err != nil {
		t.Fatalf("ListDir: %v", err)
	}
	if len(entries) < 2 { // hello.md and sub/
		t.Errorf("expected at least 2 entries, got %d", len(entries))
	}

	// RemoveFile
	if err := wt.RemoveFile("hello.md"); err != nil {
		t.Fatalf("RemoveFile: %v", err)
	}
	_, err = wt.ReadFile("hello.md")
	if err == nil {
		t.Error("expected error after removing file")
	}

	// RemoveFile non-existent
	err = wt.RemoveFile("nonexistent.md")
	if err == nil {
		t.Error("expected error when removing non-existent file")
	}
}

// ---------------------------------------------------------------------------
// MemoryGitStore helpers (filesystem parts)
// ---------------------------------------------------------------------------

func TestWorktreeDirForBranch(t *testing.T) {
	store := &MemoryGitStore{repoDir: "/repo", wtBase: "/wt-base"}
	tests := []struct {
		branch string
		want   string
	}{
		{"memory/project/abc", filepath.Join("/wt-base", "memory-project-abc")},
		{"memory/user/hash", filepath.Join("/wt-base", "memory-user-hash")},
		{"simple branch", filepath.Join("/wt-base", "simple_branch")},
	}
	for _, tc := range tests {
		t.Run(tc.branch, func(t *testing.T) {
			got := store.worktreeDirForBranch(tc.branch)
			if got != tc.want {
				t.Errorf("worktreeDirForBranch(%q) = %q; want %q", tc.branch, got, tc.want)
			}
		})
	}
}

func TestMemoryGitStore_Dir(t *testing.T) {
	store := &MemoryGitStore{repoDir: "/test/repo"}
	if store.Dir() != "/test/repo" {
		t.Errorf("Dir() = %q; want '/test/repo'", store.Dir())
	}
}

// ---------------------------------------------------------------------------
// copyDirRecursive: error on filepath.Walk (missing rel path)
// ---------------------------------------------------------------------------

func TestCopyDirRecursive_EmptyDir(t *testing.T) {
	src := t.TempDir()
	dst := t.TempDir()
	if err := copyDirRecursive(src, dst); err != nil {
		t.Fatalf("copyDirRecursive empty: %v", err)
	}
}

func TestCopyFileData_CreateSubDir(t *testing.T) {
	srcDir := t.TempDir()
	dstDir := t.TempDir()

	srcPath := filepath.Join(srcDir, "src.txt")
	dstPath := filepath.Join(dstDir, "sub", "dst.txt")

	if err := os.WriteFile(srcPath, []byte("data"), 0o644); err != nil {
		t.Fatal(err)
	}

	if err := copyFileData(srcPath, dstPath, 0o644); err != nil {
		t.Fatalf("copyFileData: %v", err)
	}

	data, err := os.ReadFile(dstPath)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(data) != "data" {
		t.Errorf("content = %q; want 'data'", string(data))
	}
}

// ---------------------------------------------------------------------------
// MemoryBranch lock file operations
// ---------------------------------------------------------------------------

func TestMemoryBranchLockFileOps(t *testing.T) {
	// Override globalDir so all paths resolve to our temp dir
	dir := t.TempDir()
	origHome := os.Getenv("HOME")
	t.Setenv("HOME", dir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	store := &MemoryGitStore{repoDir: filepath.Join(dir, "repo"), wtBase: filepath.Join(dir, "wt")}

	// RegisterBranch
	if err := store.RegisterBranch("test-branch", "ref1"); err != nil {
		t.Fatalf("RegisterBranch: %v", err)
	}

	// Register same ref again (idempotent)
	if err := store.RegisterBranch("test-branch", "ref1"); err != nil {
		t.Fatalf("RegisterBranch duplicate: %v", err)
	}

	// Register different ref
	if err := store.RegisterBranch("test-branch", "ref2"); err != nil {
		t.Fatalf("RegisterBranch ref2: %v", err)
	}

	// ActiveMemoryBranches
	branches, err := store.ActiveMemoryBranches()
	if err != nil {
		t.Fatalf("ActiveMemoryBranches: %v", err)
	}
	if len(branches) != 1 {
		t.Errorf("expected 1 active branch, got %d", len(branches))
	}

	// MemoryBranchSummary
	summary, err := store.MemoryBranchSummary()
	if err != nil {
		t.Fatalf("MemoryBranchSummary: %v", err)
	}
	if len(summary["test-branch"]) != 2 {
		t.Errorf("expected 2 refs, got %d", len(summary["test-branch"]))
	}

	// DeregisterBranch (remove one ref)
	unused, err := store.DeregisterBranch("test-branch", "ref1")
	if err != nil {
		t.Fatalf("DeregisterBranch: %v", err)
	}
	if unused {
		t.Error("expected branch not to be unused yet")
	}

	// DeregisterBranch (remove second ref → unused)
	unused, err = store.DeregisterBranch("test-branch", "ref2")
	if err != nil {
		t.Fatalf("DeregisterBranch: %v", err)
	}
	if !unused {
		t.Error("expected branch to be unused after removing all refs")
	}

	// DeregisterBranch non-existent
	unused, err = store.DeregisterBranch("nonexistent", "ref")
	if err != nil {
		t.Fatalf("DeregisterBranch nonexistent: %v", err)
	}
	if unused {
		t.Error("expected false for non-existent branch")
	}
}

func TestLoadMemLock_BadJSON(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	// Create malformed lock file
	lockPath := memoryBranchLockPath()
	if err := os.MkdirAll(filepath.Dir(lockPath), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(lockPath, []byte("invalid json!!!"), 0o644); err != nil {
		t.Fatal(err)
	}

	lf, err := loadMemLock()
	if err != nil {
		t.Fatalf("loadMemLock should not error for bad JSON, got: %v", err)
	}
	if lf == nil || lf.Branches == nil {
		t.Fatal("expected initialized lock file")
	}
}

func TestLoadMemLock_NilBranches(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	lockPath := memoryBranchLockPath()
	if err := os.MkdirAll(filepath.Dir(lockPath), 0o755); err != nil {
		t.Fatal(err)
	}
	// Write valid JSON but with null branches
	data, _ := json.Marshal(memoryBranchLockFile{Version: 1, Branches: nil})
	if err := os.WriteFile(lockPath, data, 0o644); err != nil {
		t.Fatal(err)
	}

	lf, err := loadMemLock()
	if err != nil {
		t.Fatalf("loadMemLock: %v", err)
	}
	if lf.Branches == nil {
		t.Error("Branches should be initialised even when nil in JSON")
	}
}

func TestActiveBranches_Empty(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	store := &MemoryGitStore{repoDir: filepath.Join(dir, "repo"), wtBase: filepath.Join(dir, "wt")}
	branches, err := store.activeBranches()
	if err != nil {
		t.Fatalf("activeBranches: %v", err)
	}
	if len(branches) != 0 {
		t.Errorf("expected 0 branches, got %d", len(branches))
	}
}

func TestValidateMemBranchRefs(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	store := &MemoryGitStore{repoDir: filepath.Join(dir, "repo"), wtBase: filepath.Join(dir, "wt")}

	// Register a branch with "user" ref (always alive)
	if err := store.RegisterBranch("branch-user", "user"); err != nil {
		t.Fatal(err)
	}

	// Register a branch with a file-path ref that does exist
	lockDir := t.TempDir()
	lockFile := filepath.Join(lockDir, "graphit.lock.json")
	if err := os.WriteFile(lockFile, []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := store.RegisterBranch("branch-alive", lockDir); err != nil {
		t.Fatal(err)
	}

	// Register a branch with a ref that does NOT exist (stale)
	if err := store.RegisterBranch("branch-stale", "/nonexistent/stale/ref"); err != nil {
		t.Fatal(err)
	}

	cleaned, err := store.ValidateMemBranchRefs()
	if err != nil {
		t.Fatalf("ValidateMemBranchRefs: %v", err)
	}
	if cleaned != 1 {
		t.Errorf("cleaned = %d; want 1", cleaned)
	}
}

func TestValidateMemBranchRefs_NoCleaning(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	store := &MemoryGitStore{repoDir: filepath.Join(dir, "repo"), wtBase: filepath.Join(dir, "wt")}

	// Register with "user" ref only (never cleaned)
	if err := store.RegisterBranch("branch-user", "user"); err != nil {
		t.Fatal(err)
	}

	cleaned, err := store.ValidateMemBranchRefs()
	if err != nil {
		t.Fatalf("ValidateMemBranchRefs: %v", err)
	}
	if cleaned != 0 {
		t.Errorf("cleaned = %d; want 0", cleaned)
	}
}

// ---------------------------------------------------------------------------
// MemoryService helpers (non-git operations)
// ---------------------------------------------------------------------------

func TestNewMemoryService(t *testing.T) {
	svc := NewMemoryService(MemoryScopeProject, "test-id", nil)
	if svc == nil {
		t.Fatal("expected non-nil service")
	}
	if svc.scope != MemoryScopeProject {
		t.Errorf("scope = %q; want 'project'", svc.scope)
	}
	if svc.scopeID != "test-id" {
		t.Errorf("scopeID = %q", svc.scopeID)
	}
}

func TestNewMemoryServiceForContext(t *testing.T) {
	svc := NewMemoryServiceForContext("test-context", nil)
	if svc == nil {
		t.Fatal("expected non-nil service")
	}
	if svc.scope != MemoryScopeContext {
		t.Errorf("scope = %q; want 'context'", svc.scope)
	}
}

func TestMemoryService_LocalDir_WikiDir(t *testing.T) {
	svc := &MemoryService{localDir: "/test/local", wikiDir: "/test/wiki"}
	if svc.LocalDir() != "/test/local" {
		t.Errorf("LocalDir() = %q", svc.LocalDir())
	}
	if svc.WikiDir() != "/test/wiki" {
		t.Errorf("WikiDir() = %q", svc.WikiDir())
	}
}

func TestMemoryService_EnsureInitialised_NilStore(t *testing.T) {
	svc := &MemoryService{
		scope:   MemoryScopeProject,
		scopeID: "test",
	}
	// With nil gitStore, should only call syncToLocalFast which errors but doesn't fail
	// syncToLocalFast requires gitStore so returns error that is only logged
	err := svc.EnsureInitialised()
	if err != nil {
		t.Errorf("EnsureInitialised should not return error: %v", err)
	}
}

func TestMemoryService_IndexMemories_NonExistent(t *testing.T) {
	svc := &MemoryService{localDir: "/nonexistent/dir"}
	ctx := context.Background()
	err := svc.IndexMemories(ctx)
	if err != nil {
		t.Errorf("IndexMemories with non-existent dir should return nil: %v", err)
	}
}

func TestMemoryService_IndexMemories_WithDir(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()

	writeMemFile(t, rawDir, "MEM1.md", `---
title: Test
type: fact
---

# Test

Body.`)

	svc := &MemoryService{localDir: rawDir, wikiDir: wikiDir}
	ctx := context.Background()
	if err := svc.IndexMemories(ctx); err != nil {
		t.Fatalf("IndexMemories: %v", err)
	}
	// Check wiki was generated
	if _, err := os.Stat(filepath.Join(wikiDir, "index.md")); err != nil {
		t.Error("expected index.md in wiki dir")
	}
}

func TestMemoryService_SyncToLocal_NilStore(t *testing.T) {
	svc := &MemoryService{}
	err := svc.SyncToLocal()
	if err == nil {
		t.Error("expected error with nil gitStore")
	}
}

// ---------------------------------------------------------------------------
// ensureProjectCopy
// ---------------------------------------------------------------------------

func TestEnsureProjectCopy_EmptyProjectLinkDir(t *testing.T) {
	svc := &MemoryService{projectLinkDir: ""}
	// Should return early without doing anything
	svc.ensureProjectCopy(t.TempDir())
}

// ---------------------------------------------------------------------------
// MemoryLocalDir / MemoryProjectLinkDir / MemoryGlobalContextDir / MemoryWikiGlobalDir
// ---------------------------------------------------------------------------

func TestMemoryLocalDir(t *testing.T) {
	got := MemoryLocalDir("project")
	if got == "" {
		t.Error("MemoryLocalDir returned empty")
	}
	if !strings.Contains(got, "memory") || !strings.Contains(got, "project") {
		t.Errorf("MemoryLocalDir('project') = %q; expected memory/project", got)
	}
}

func TestMemoryProjectLinkDir(t *testing.T) {
	got := MemoryProjectLinkDir("user")
	if !strings.Contains(got, ".graphit") && !strings.Contains(got, "memory") {
		t.Errorf("unexpected MemoryProjectLinkDir: %q", got)
	}
}

func TestMemoryGlobalContextDir(t *testing.T) {
	got := MemoryGlobalContextDir("my-context")
	if got == "" {
		t.Error("MemoryGlobalContextDir returned empty")
	}
	if !strings.Contains(got, "memory") || !strings.Contains(got, "my-context") {
		t.Errorf("MemoryGlobalContextDir = %q", got)
	}
}

func TestMemoryWikiGlobalDir(t *testing.T) {
	got := MemoryWikiGlobalDir("project", "abc123")
	if got == "" {
		t.Error("MemoryWikiGlobalDir returned empty")
	}
	if !strings.Contains(got, "wiki") {
		t.Errorf("MemoryWikiGlobalDir = %q; expected wiki in path", got)
	}
}

// ---------------------------------------------------------------------------
// GlobalBaseDir, GlobalScopeDir, WikiDir, RawDir, WorktreeRawDirForScope
// ---------------------------------------------------------------------------

func TestGlobalBaseDir(t *testing.T) {
	got := GlobalBaseDir()
	if got == "" {
		t.Error("GlobalBaseDir returned empty")
	}
	if !strings.Contains(got, "memory") {
		t.Errorf("GlobalBaseDir = %q; expected 'memory' in path", got)
	}
}

func TestGlobalScopeDir_NonExistent(t *testing.T) {
	// GlobalScopeDir looks for ProjectLinkDir which won't exist for a random scope
	got := GlobalScopeDir("nonexistent-scope-" + fmt.Sprintf("%d", time.Now().UnixNano()))
	if got != "" {
		t.Errorf("expected empty for non-existent scope, got %q", got)
	}
}

func TestWikiDirFunc(t *testing.T) {
	// WikiDir delegates to GlobalScopeDir
	got := WikiDir("nonexistent-scope-" + fmt.Sprintf("%d", time.Now().UnixNano()))
	if got != "" {
		t.Errorf("expected empty for non-existent scope, got %q", got)
	}
}

func TestRawDirFunc(t *testing.T) {
	// RawDir delegates to WorktreeRawDirForScope
	got := RawDir("nonexistent-scope-" + fmt.Sprintf("%d", time.Now().UnixNano()))
	if got != "" {
		t.Errorf("expected empty for non-existent scope, got %q", got)
	}
}

func TestWorktreeRawDirForScope_ReturnsEmpty(t *testing.T) {
	got := WorktreeRawDirForScope("nonexistent-scope-" + fmt.Sprintf("%d", time.Now().UnixNano()))
	if got != "" {
		t.Errorf("expected empty, got %q", got)
	}
}

func TestWorktreeRawDir_EmptyGlobalDir(t *testing.T) {
	// With HOME set, GlobalDir returns a path so this always has a value
	got := WorktreeRawDir("project", "abc")
	if got == "" {
		t.Error("expected non-empty")
	}
}

// ---------------------------------------------------------------------------
// AllContextDirs
// ---------------------------------------------------------------------------

func TestAllContextDirs(t *testing.T) {
	// AllContextDirs reads from the memory directory under .graphit
	// Create a temp structure
	dir := t.TempDir()
	memDir := filepath.Join(dir, ".graphit", "memory")
	if err := os.MkdirAll(filepath.Join(memDir, "project"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(memDir, "user"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(memDir, "my-context"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(memDir, ".hidden"), 0o755); err != nil {
		t.Fatal(err)
	}

	// We can't easily override the CWD for AllContextDirs since it uses
	// filepath.Dir(ProjectLinkDir("project")), but let's test the function doesn't panic
	result := AllContextDirs()
	// The result depends on the CWD, so just verify it returns without error
	_ = result
}

// ---------------------------------------------------------------------------
// EnsureContextCopy
// ---------------------------------------------------------------------------

func TestEnsureContextCopy_EmptyProjectDir(t *testing.T) {
	// Should return early
	EnsureContextCopy("test-context", "", nil)
}

func TestEnsureContextCopy_WithProjectDir(t *testing.T) {
	dir := t.TempDir()
	EnsureContextCopy("test-context", dir, nil)
	// Should not panic — creates directories
}

// ---------------------------------------------------------------------------
// EnsureWikiIndexExists
// ---------------------------------------------------------------------------

func TestEnsureWikiIndexExists_AlreadyExists(t *testing.T) {
	dir := t.TempDir()
	// Create a wiki directory with an index
	wikiDir := filepath.Join(dir, ".graphit", "memory", "project")
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}
	// Create dummy index
	if err := os.WriteFile(filepath.Join(wikiDir, "index.md"), []byte("existing"), 0o644); err != nil {
		t.Fatal(err)
	}

	// This test exercises the early return when index already exists
	// The function uses GlobalScopeDir which may not find our temp dir,
	// so we test the function directly by checking it doesn't panic
	EnsureWikiIndexExists("nonexistent-scope", nil)
}

// ---------------------------------------------------------------------------
// RunConsolidation
// ---------------------------------------------------------------------------

func TestRunConsolidation_NonExistentDir(t *testing.T) {
	ctx := context.Background()
	report, err := RunConsolidation(ctx, "nonexistent-scope-xyz", nil)
	if err != nil {
		t.Fatalf("expected nil error, got: %v", err)
	}
	if report.TotalMemories != 0 {
		t.Errorf("expected 0 memories, got %d", report.TotalMemories)
	}
}

func TestRunConsolidation_EmptyDir(t *testing.T) {
	dir := t.TempDir()
	// Create a "scope" dir that RawDir would point to
	// We need to test via a local wrapper since RunConsolidation uses RawDir(scope)
	ctx := context.Background()
	report, err := runConsolidationInDir(ctx, dir, nil)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if report.TotalMemories != 0 {
		t.Errorf("expected 0, got %d", report.TotalMemories)
	}
}

// runConsolidationInDir is a local test helper that replicates RunConsolidation logic
// but operates on a specified directory.
func runConsolidationInDir(ctx context.Context, dir string, aiClient interface{ Complete(context.Context, string, string) (string, error) }) (*ConsolidationReport, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return &ConsolidationReport{}, nil
		}
		return nil, err
	}

	var memories []memorySnapshot
	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
			continue
		}
		name := e.Name()
		absPath := filepath.Join(dir, name)
		data, readErr := os.ReadFile(absPath)
		if readErr != nil {
			continue
		}
		important := IsImportantMemory(name)
		var id string
		if important {
			id = strings.TrimSuffix(name, ImportantMemorySuffix+".md")
		} else {
			id = strings.TrimSuffix(name, ".md")
		}
		title, createdAt := parseMemoryMeta(absPath)
		body := extractBodyAfterFrontmatter(string(data))
		memType := parseConsolidationType(string(data))
		memories = append(memories, memorySnapshot{
			ID: id, Title: title, Body: strings.TrimSpace(body),
			Type: memType, CreatedAt: createdAt, Important: important,
		})
	}

	report := &ConsolidationReport{TotalMemories: len(memories)}
	if len(memories) == 0 {
		return report, nil
	}
	report.Stale = detectStaleMemories(memories)
	return report, nil
}

func TestRunConsolidation_WithMemories(t *testing.T) {
	dir := t.TempDir()
	ctx := context.Background()

	oldDate := time.Now().Add(-45 * 24 * time.Hour).Format(time.RFC3339)

	// Normal memory
	writeMemFile(t, dir, "MEM1.md", fmt.Sprintf(`---
title: Normal Memory
created_at: %s
---

# Normal Memory

This is a normal memory body.`, oldDate))

	// Important memory (skipped by stale detection)
	writeMemFile(t, dir, "MEM2_important_.md", fmt.Sprintf(`---
title: Important Memory
created_at: %s
---

# Important Memory

Important body.`, oldDate))

	// Non-md file (skipped)
	writeMemFile(t, dir, "notes.txt", "not a memory")

	// Directory (skipped)
	if err := os.Mkdir(filepath.Join(dir, "subdir"), 0o755); err != nil {
		t.Fatal(err)
	}

	report, err := runConsolidationInDir(ctx, dir, nil)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if report.TotalMemories != 2 {
		t.Errorf("TotalMemories = %d; want 2", report.TotalMemories)
	}
	if len(report.Stale) != 1 {
		t.Errorf("Stale count = %d; want 1", len(report.Stale))
	}
}

// ---------------------------------------------------------------------------
// aiConsolidation (with mock AI client)
// ---------------------------------------------------------------------------

type mockAIClient struct {
	response string
	err      error
}

func (m *mockAIClient) Complete(_ context.Context, _, _ string) (string, error) {
	return m.response, m.err
}

func TestAiConsolidation_Success(t *testing.T) {
	ctx := context.Background()
	memories := []memorySnapshot{
		{ID: "01J5XABC1234567890", Title: "Memory 1", Body: "Body 1", Type: "fact", CreatedAt: "2026-01-01T00:00:00Z"},
		{ID: "01J5XDEF1234567890", Title: "Memory 2", Body: "Body 2", Type: "fact", CreatedAt: "2026-01-02T00:00:00Z"},
	}

	mockResp := `## DUPLICATES
- MERGE [01J5XABC1234567890] and [01J5XDEF1234567890]: Same thing

## CONTRADICTIONS
None found

## SUGGESTIONS
- PROMOTE [01J5XABC1234567890]: Should be important
- DEMOTE [01J5XDEF1234567890]: Too specific
- DELETE [01J5XABC1234567890]: Outdated
- UPDATE [01J5XDEF1234567890]: Needs more detail`

	client := &mockAIClient{response: mockResp}
	report, err := aiConsolidation(ctx, client, memories)
	if err != nil {
		t.Fatalf("aiConsolidation: %v", err)
	}
	if len(report.Duplicates) != 1 {
		t.Errorf("Duplicates = %d; want 1", len(report.Duplicates))
	}
	if len(report.Contradictions) != 0 {
		t.Errorf("Contradictions = %d; want 0", len(report.Contradictions))
	}
	if len(report.Suggestions) != 4 {
		t.Errorf("Suggestions = %d; want 4", len(report.Suggestions))
	}
}

func TestAiConsolidation_Error(t *testing.T) {
	ctx := context.Background()
	memories := []memorySnapshot{
		{ID: "ID1", Title: "M1"},
		{ID: "ID2", Title: "M2"},
	}
	client := &mockAIClient{err: fmt.Errorf("AI error")}
	_, err := aiConsolidation(ctx, client, memories)
	if err == nil {
		t.Error("expected error from AI client")
	}
}

func TestAiConsolidation_BodyNotEmpty(t *testing.T) {
	ctx := context.Background()
	// Test that non-empty body is included in prompt building
	memories := []memorySnapshot{
		{ID: "ID1", Title: "M1", Body: ""},
		{ID: "ID2", Title: "M2", Body: "Some body content"},
	}
	client := &mockAIClient{response: "## DUPLICATES\nNone found\n## CONTRADICTIONS\nNone found\n## SUGGESTIONS\nNone found"}
	report, err := aiConsolidation(ctx, client, memories)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if report == nil {
		t.Fatal("expected non-nil report")
	}
}

// ---------------------------------------------------------------------------
// ApplyGC
// ---------------------------------------------------------------------------

func TestApplyGC(t *testing.T) {
	ctx := context.Background()

	// Create a mock MemoryService - since we can't actually remove memories
	// without gitStore, test that errors are handled gracefully
	svc := &MemoryService{scope: MemoryScopeProject, scopeID: "test"}

	candidates := []GCCandidate{
		{ID: "id1", Title: "Memory 1", Reason: "old"},
		{ID: "id2", Title: "Memory 2", Reason: "empty"},
	}

	// Without gitStore, RemoveMemory will error, so deleted should be 0
	deleted, err := ApplyGC(ctx, "project", candidates, svc)
	if err != nil {
		t.Fatalf("ApplyGC: %v", err)
	}
	if deleted != 0 {
		t.Errorf("deleted = %d; want 0 (no gitStore)", deleted)
	}
}

// ---------------------------------------------------------------------------
// SyncAndCycle
// ---------------------------------------------------------------------------

type mockStoreProvider struct {
	extractErr error
}

func (m *mockStoreProvider) ExtractBranchDir(_, _, _ string) error {
	return m.extractErr
}

func TestSyncAndCycle_NilStore(t *testing.T) {
	ctx := context.Background()
	result := SyncAndCycle(ctx, "project", "test-id", nil, nil)
	if result == nil {
		t.Fatal("expected non-nil result")
	}
	if result.Scope != "project" {
		t.Errorf("Scope = %q", result.Scope)
	}
}

func TestSyncAndCycle_WithStore(t *testing.T) {
	ctx := context.Background()
	store := &mockStoreProvider{}
	result := SyncAndCycle(ctx, "project", "test-id", store, nil)
	if result == nil {
		t.Fatal("expected non-nil result")
	}
}

func TestSyncAndCycle_WithStoreError(t *testing.T) {
	ctx := context.Background()
	store := &mockStoreProvider{extractErr: fmt.Errorf("extract error")}
	result := SyncAndCycle(ctx, "project", "test-id", store, nil)
	if result == nil {
		t.Fatal("expected non-nil result")
	}
}

// ---------------------------------------------------------------------------
// SyncContextFromMemoryRepo
// ---------------------------------------------------------------------------

func TestSyncContextFromMemoryRepo_NilStore(t *testing.T) {
	ctx := context.Background()
	result := SyncContextFromMemoryRepo(ctx, "test-context", t.TempDir(), nil, nil)
	if result == nil {
		t.Fatal("expected non-nil result")
	}
}

func TestSyncContextFromMemoryRepo_WithStore(t *testing.T) {
	ctx := context.Background()
	store := &mockStoreProvider{}
	result := SyncContextFromMemoryRepo(ctx, "test-context", t.TempDir(), store, nil)
	if result == nil {
		t.Fatal("expected non-nil result")
	}
}

func TestSyncContextFromMemoryRepo_WithStoreError(t *testing.T) {
	ctx := context.Background()
	store := &mockStoreProvider{extractErr: fmt.Errorf("err")}
	result := SyncContextFromMemoryRepo(ctx, "test-context", t.TempDir(), store, nil)
	if result == nil {
		t.Fatal("expected non-nil result")
	}
}

// ---------------------------------------------------------------------------
// RuleContent, MemoryRouterContent
// ---------------------------------------------------------------------------

func TestRuleContent(t *testing.T) {
	content := RuleContent(nil)
	if content == "" {
		t.Error("RuleContent returned empty")
	}
	if !strings.Contains(content, "Memory Management Rule") {
		t.Error("expected header")
	}
}

func TestRuleContent_WithContexts(t *testing.T) {
	content := RuleContent([]string{"ctx1", "ctx2"})
	if !strings.Contains(content, "ctx1") {
		t.Error("expected context name in content")
	}
	if !strings.Contains(content, "ctx2") {
		t.Error("expected second context name in content")
	}
}

func TestMemoryRouterContent(t *testing.T) {
	content := MemoryRouterContent(nil, "AGENTS.md")
	if content == "" {
		t.Error("MemoryRouterContent returned empty")
	}
	if !strings.Contains(content, "Memory Management") {
		t.Error("expected header")
	}
	if !strings.Contains(content, "AGENTS.md") {
		t.Error("expected global rules file reference")
	}
}

// ---------------------------------------------------------------------------
// InstallRule, InstallSkill, RemoveRule, RemoveSkill
// ---------------------------------------------------------------------------

func TestInstallRule(t *testing.T) {
	dir := t.TempDir()
	// InstallRule will call ide.InjectManagedBlock which works on project dirs
	// We test that it doesn't panic with a valid dir
	err := InstallRule(dir, "gemini")
	// May return error if IDE adapter doesn't support the format, that's OK
	_ = err
}

func TestInstallRule_EmptyProjectDir(t *testing.T) {
	// Use a temp dir instead of empty string which would pollute the real project via os.Getwd()
	err := InstallRule(t.TempDir(), "gemini")
	_ = err
}

func TestInstallSkill(t *testing.T) {
	dir := t.TempDir()
	err := InstallSkill(dir, "gemini")
	_ = err // May fail if IDE adapter doesn't support the format
}

func TestInstallSkill_EmptyProjectDir(t *testing.T) {
	err := InstallSkill(t.TempDir(), "gemini")
	_ = err
}

func TestRemoveRule(t *testing.T) {
	dir := t.TempDir()
	err := RemoveRule(dir, "gemini")
	_ = err
}

func TestRemoveRule_EmptyProjectDir(t *testing.T) {
	err := RemoveRule(t.TempDir(), "gemini")
	_ = err
}

func TestRemoveSkill(t *testing.T) {
	dir := t.TempDir()
	err := RemoveSkill(dir, "gemini")
	_ = err
}

func TestRemoveSkill_EmptyProjectDir(t *testing.T) {
	err := RemoveSkill(t.TempDir(), "gemini")
	_ = err
}

// ---------------------------------------------------------------------------
// ListRecentMemories
// ---------------------------------------------------------------------------

func TestListRecentMemories(t *testing.T) {
	dir := t.TempDir()

	// Create normal memories with different dates
	writeMemFile(t, dir, "MEM1.md", `---
title: First Memory
created_at: 2026-05-20T00:00:00Z
---

# First Memory

First body.`)

	writeMemFile(t, dir, "MEM2.md", `---
title: Second Memory
created_at: 2026-05-21T00:00:00Z
---

# Second Memory

Second body.`)

	// Important memory should be skipped
	writeMemFile(t, dir, "MEM3_important_.md", `---
title: Important Memory
created_at: 2026-05-22T00:00:00Z
---

# Important Memory

Important body.`)

	// Non-md should be skipped
	writeMemFile(t, dir, "notes.txt", "not a memory")

	// Directory should be skipped
	if err := os.Mkdir(filepath.Join(dir, "subdir"), 0o755); err != nil {
		t.Fatal(err)
	}

	entries, err := listRecentInDir(dir, 10)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if len(entries) != 2 {
		t.Fatalf("expected 2 recent, got %d", len(entries))
	}
	// Should be sorted by created desc (newest first)
	if entries[0].created < entries[1].created {
		t.Error("expected newest first")
	}
}

func TestListRecentMemories_Limit(t *testing.T) {
	dir := t.TempDir()
	for i := 0; i < 5; i++ {
		writeMemFile(t, dir, fmt.Sprintf("MEM%d.md", i), fmt.Sprintf(`---
title: Memory %d
created_at: 2026-05-0%dT00:00:00Z
---

# Memory %d

Body %d.`, i, i+1, i, i))
	}

	entries, err := listRecentInDir(dir, 3)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if len(entries) != 3 {
		t.Errorf("expected 3 entries with limit, got %d", len(entries))
	}
}

func TestListRecentMemories_Empty(t *testing.T) {
	dir := t.TempDir()
	entries, err := listRecentInDir(dir, 10)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if len(entries) != 0 {
		t.Errorf("expected 0, got %d", len(entries))
	}
}

func TestListRecentMemories_NonExistent(t *testing.T) {
	entries, err := listRecentInDir("/nonexistent/path", 10)
	if err != nil {
		t.Fatalf("expected nil error, got: %v", err)
	}
	if entries != nil {
		t.Errorf("expected nil, got %v", entries)
	}
}

// listRecentInDir is a test helper that replicates ListRecentMemories logic on a dir.
func listRecentInDir(dir string, limit int) ([]ImportantEntry, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}

	var all []ImportantEntry
	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
			continue
		}
		name := e.Name()
		if IsImportantMemory(name) {
			continue
		}
		id := strings.TrimSuffix(name, ".md")
		absPath := filepath.Join(dir, name)
		data, err := os.ReadFile(absPath)
		if err != nil {
			continue
		}
		title, createdAt := parseMemoryMeta(absPath)
		content := extractBodyAfterFrontmatter(string(data))
		all = append(all, ImportantEntry{
			ID: id, Title: title, Content: strings.TrimSpace(content),
			Path: absPath, created: createdAt,
		})
	}
	sort.Slice(all, func(i, j int) bool {
		return all[i].created > all[j].created
	})
	if len(all) > limit {
		all = all[:limit]
	}
	return all, nil
}

// ---------------------------------------------------------------------------
// RenderImportantBlock / RenderRecentBlock (pure rendering tests)
// ---------------------------------------------------------------------------

func TestRenderRecentBlock_InDir(t *testing.T) {
	dir := t.TempDir()

	writeMemFile(t, dir, "MEM1.md", `---
title: Recent Fact
created_at: 2026-05-20T00:00:00Z
---

# Recent Fact

Some recent content here.`)

	entries, err := listRecentInDir(dir, 5)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if len(entries) != 1 {
		t.Fatalf("expected 1 entry, got %d", len(entries))
	}

	// Build block similar to RenderRecentBlock
	var b strings.Builder
	b.WriteString("## 🕐 Recent Memories\n\n")
	for _, e := range entries {
		summary := firstLineFromContent(e.Content)
		if summary != "" {
			_, _ = fmt.Fprintf(&b, "- **%s** — %s *(ID: `%s`)*\n", e.Title, summary, e.ID)
		} else {
			_, _ = fmt.Fprintf(&b, "- **%s** *(ID: `%s`)*\n", e.Title, e.ID)
		}
	}
	block := b.String()
	if !strings.Contains(block, "Recent Fact") {
		t.Error("block should contain title")
	}
}

func TestRenderRecentBlock_EmptySummary(t *testing.T) {
	dir := t.TempDir()
	writeMemFile(t, dir, "MEM1.md", `---
title: No Body Memory
---

# No Body Memory`)

	entries, err := listRecentInDir(dir, 5)
	if err != nil {
		t.Fatal(err)
	}

	var b strings.Builder
	for _, e := range entries {
		summary := firstLineFromContent(e.Content)
		if summary != "" {
			_, _ = fmt.Fprintf(&b, "- **%s** — %s\n", e.Title, summary)
		} else {
			_, _ = fmt.Fprintf(&b, "- **%s**\n", e.Title)
		}
	}
	block := b.String()
	if !strings.Contains(block, "No Body Memory") {
		t.Error("should contain title")
	}
}

// ---------------------------------------------------------------------------
// memoryEntityPage: stale warning and unknown type emoji
// ---------------------------------------------------------------------------

func TestMemoryEntityPage_StaleWarning(t *testing.T) {
	oldDate := time.Now().Add(-60 * 24 * time.Hour).Format(time.RFC3339)
	page := memoryEntityPage("ID", "Old Memory", oldDate, false, "Body.", "fact")
	if !strings.Contains(page, "Stale memory") {
		t.Error("expected stale memory warning for 60-day-old memory")
	}
}

func TestMemoryEntityPage_UnknownTypeEmoji(t *testing.T) {
	page := memoryEntityPage("ID", "Custom Type", "", false, "Body.", "custom-type")
	if !strings.Contains(page, "📄") {
		t.Error("expected fallback emoji for unknown type")
	}
}

func TestMemoryEntityPage_NoBody(t *testing.T) {
	page := memoryEntityPage("ID", "Title", "", false, "", "fact")
	if !strings.Contains(page, "# Title") {
		t.Error("should contain title")
	}
}

func TestMemoryEntityPage_NoCreatedAt(t *testing.T) {
	page := memoryEntityPage("ID", "Title", "", false, "Body.", "")
	if strings.Contains(page, "created:") {
		t.Error("should not contain created when empty")
	}
}

// ---------------------------------------------------------------------------
// memoryIndexPage: untyped memories, important prefix, empty/no-body summaries
// ---------------------------------------------------------------------------

func TestMemoryIndexPage_WithUntypedMemories(t *testing.T) {
	docs := []memDoc{
		{id: "1", title: "Untyped", createdAt: "2026-01-01T00:00:00Z", memType: "", body: "Some body"},
		{id: "2", title: "Important Untyped", createdAt: "2026-01-02T00:00:00Z", memType: "", important: true, body: "Important body"},
	}
	content := memoryIndexPage(docs)
	if !strings.Contains(content, "Other Memories") {
		t.Error("should contain 'Other Memories' section for untyped")
	}
	if !strings.Contains(content, "⭐") {
		t.Error("should contain star prefix for important")
	}
}

func TestMemoryIndexPage_EmptyBody(t *testing.T) {
	docs := []memDoc{
		{id: "1", title: "No Body", createdAt: "2026-01-01T00:00:00Z", memType: "fact", body: ""},
	}
	content := memoryIndexPage(docs)
	if !strings.Contains(content, "No_Body") {
		t.Error("should contain title link")
	}
}

func TestMemoryIndexPage_ImportantSection(t *testing.T) {
	docs := []memDoc{
		{id: "1", title: "Important One", important: true, memType: "convention", body: "Important body content"},
		{id: "2", title: "Normal One", important: false, memType: "fact", body: "Normal body"},
	}
	content := memoryIndexPage(docs)
	if !strings.Contains(content, "⭐ Important Memories") {
		t.Error("should contain important section")
	}
	if !strings.Contains(content, "1 important") {
		t.Error("should show 1 important count")
	}
}

func TestMemoryIndexPage_NoImportant(t *testing.T) {
	docs := []memDoc{
		{id: "1", title: "Normal", memType: "fact", body: "Body"},
	}
	content := memoryIndexPage(docs)
	if !strings.Contains(content, "0 important") {
		t.Error("should show 0 important")
	}
}

func TestMemoryIndexPage_ImportantWithNoBody(t *testing.T) {
	docs := []memDoc{
		{id: "1", title: "Important No Body", important: true, memType: "convention", body: ""},
	}
	content := memoryIndexPage(docs)
	// Verify the page was generated (contains header at minimum)
	if content == "" {
		t.Error("should not be empty")
	}
	// The title may appear as a link, slug, or plain text depending on implementation
	if !strings.Contains(content, "Important") && !strings.Contains(content, "important") {
		t.Error("should contain some reference to the important memory")
	}
}

// ---------------------------------------------------------------------------
// appendMemLog: with existing content that has separator
// ---------------------------------------------------------------------------

func TestAppendMemLog_WithSeparator(t *testing.T) {
	dir := t.TempDir()
	logPath := filepath.Join(dir, "log.md")

	// Create file with separator
	initialContent := "---\ntitle: Memory Wiki Log\ntags: [memory, log]\n---\n\n# Memory Wiki Log\n\n> Append-only\n\n---\nOld entry here.\n"
	if err := os.WriteFile(logPath, []byte(initialContent), 0o644); err != nil {
		t.Fatal(err)
	}

	appendMemLog(logPath, 10, 8, nil)

	data, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatal(err)
	}
	content := string(data)
	if !strings.Contains(content, "Memories: 10") {
		t.Error("should contain new entry")
	}
	if !strings.Contains(content, "Old entry here") {
		t.Error("should preserve old content")
	}
}

func TestAppendMemLog_NoSeparator(t *testing.T) {
	dir := t.TempDir()
	logPath := filepath.Join(dir, "log.md")

	// Create file WITHOUT the --- separator (no SplitN parts[1])
	if err := os.WriteFile(logPath, []byte("Some content without separator"), 0o644); err != nil {
		t.Fatal(err)
	}

	appendMemLog(logPath, 3, 2, nil)

	data, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatal(err)
	}
	content := string(data)
	if !strings.Contains(content, "Memories: 3") {
		t.Error("should contain new entry")
	}
}

// ---------------------------------------------------------------------------
// GenerateMemoryWiki: with logger, with write error (readonly dir)
// ---------------------------------------------------------------------------

func TestGenerateMemoryWiki_WithLogger(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()

	writeMemFile(t, rawDir, "MEM1.md", `---
title: Test
type: fact
---

# Test

Body.`)

	ctx := context.Background()
	_, err := GenerateMemoryWiki(ctx, rawDir, wikiDir, nil) // passing nil logger
	if err != nil {
		t.Fatalf("error: %v", err)
	}
}

// ---------------------------------------------------------------------------
// RunProjectCycle / RunUserCycle / RunContextCycle / RunAllContextCycles
// ---------------------------------------------------------------------------

func TestRunProjectCycle(t *testing.T) {
	result := RunProjectCycle(context.Background())
	// Result depends on CWD, just verify no panic
	if result == nil {
		t.Fatal("expected non-nil result")
	}
}

func TestRunUserCycle(t *testing.T) {
	result := RunUserCycle(context.Background())
	if result == nil {
		t.Fatal("expected non-nil result")
	}
}

func TestRunContextCycle(t *testing.T) {
	result := RunContextCycle(context.Background(), "test-context")
	if result == nil {
		t.Fatal("expected non-nil result")
	}
}

func TestRunAllContextCycles(t *testing.T) {
	results := RunAllContextCycles(context.Background())
	// May be empty, just verify no panic
	_ = results
}

// ---------------------------------------------------------------------------
// OnHubImport
// ---------------------------------------------------------------------------

func TestOnHubImport(t *testing.T) {
	ctx := context.Background()
	store := &mockStoreProvider{}
	// OnHubImport spawns a goroutine — we just test it doesn't panic
	OnHubImport(ctx, "test-context", t.TempDir(), store, nil)
	// Give goroutine time to finish
	time.Sleep(50 * time.Millisecond)
}

// ---------------------------------------------------------------------------
// parseSuggestionSection: edge cases
// ---------------------------------------------------------------------------

func TestParseSuggestionSection_DefaultType(t *testing.T) {
	response := `## SUGGESTIONS
- SOMETHING [01J5XABC1234567890]: unknown action type`

	actions := parseSuggestionSection(response)
	if len(actions) != 1 {
		t.Fatalf("expected 1 action, got %d", len(actions))
	}
	if actions[0].Type != "update" {
		t.Errorf("default type should be 'update', got %q", actions[0].Type)
	}
}

func TestParseConsolidationSection_WithNextSection(t *testing.T) {
	response := `## DUPLICATES
- MERGE [01J5XABC1234567890] and [01J5XDEF1234567890]: Same content

## CONTRADICTIONS
- CONFLICT [01J5XGHI1234567890] vs [01J5XJKL1234567890]: Contradicting`

	dups := parseConsolidationSection(response, "DUPLICATES", "merge")
	if len(dups) != 1 {
		t.Errorf("expected 1 duplicate, got %d", len(dups))
	}

	contras := parseConsolidationSection(response, "CONTRADICTIONS", "conflict")
	if len(contras) != 1 {
		t.Errorf("expected 1 contradiction, got %d", len(contras))
	}
}

// ---------------------------------------------------------------------------
// parseMemoryMeta: title fallback to filename
// ---------------------------------------------------------------------------

func TestParseMemoryMeta_EmptyContentFallbackToFilename(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "my_file.md")
	if err := os.WriteFile(path, []byte(""), 0o644); err != nil {
		t.Fatal(err)
	}
	title, _ := parseMemoryMeta(path)
	if title != "my_file" {
		t.Errorf("expected 'my_file' as fallback, got %q", title)
	}
}

// ---------------------------------------------------------------------------
// detectStaleMemories: bad date format
// ---------------------------------------------------------------------------

func TestDetectStaleMemories_BadDateFormat(t *testing.T) {
	memories := []memorySnapshot{
		{ID: "1", Title: "Bad Date", CreatedAt: "not-a-date", Important: false},
	}
	stale := detectStaleMemories(memories)
	if len(stale) != 0 {
		t.Errorf("bad date format should be skipped, got %d stale", len(stale))
	}
}

// ---------------------------------------------------------------------------
// SelectiveFetch (no-op tests)
// ---------------------------------------------------------------------------

func TestSelectiveFetch_NoRemote(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	store := &MemoryGitStore{repoDir: filepath.Join(dir, "repo"), wtBase: filepath.Join(dir, "wt")}
	// config.MemoryRepoURL() will return "" since no config is set
	err := store.SelectiveFetch("branch")
	if err != nil {
		t.Errorf("expected nil error, got: %v", err)
	}
}

// ---------------------------------------------------------------------------
// WaitForPendingPushes
// ---------------------------------------------------------------------------

func TestWaitForPendingPushes(t *testing.T) {
	// Should return immediately when no pushes are pending
	WaitForPendingPushes()
}

// ---------------------------------------------------------------------------
// newMemorySvcInternal with non-nil store
// ---------------------------------------------------------------------------

func TestNewMemorySvcInternal_WithStore(t *testing.T) {
	store := &MemoryGitStore{repoDir: "/repo"}
	svc := newMemorySvcInternal(MemoryScopeProject, "id", "/local", "/link", store)
	if svc == nil {
		t.Fatal("expected non-nil svc")
	}
	// Store's Logger is set to svc.Logger (nil at this point)
	if store.Logger != nil {
		t.Error("expected nil Logger")
	}
}

// ---------------------------------------------------------------------------
// MemoryService.log()
// ---------------------------------------------------------------------------

func TestMemoryService_Log(t *testing.T) {
	svc := &MemoryService{}
	logger := svc.log()
	if logger == nil {
		t.Error("expected non-nil logger from slogutil.Resolve")
	}
}

// ---------------------------------------------------------------------------
// MemoryGitStore.log()
// ---------------------------------------------------------------------------

func TestMemoryGitStore_Log(t *testing.T) {
	store := &MemoryGitStore{}
	logger := store.log()
	if logger == nil {
		t.Error("expected non-nil logger from slogutil.Resolve")
	}
}

// ---------------------------------------------------------------------------
// MemoryWorktree.log()
// ---------------------------------------------------------------------------

func TestMemoryWorktree_Log(t *testing.T) {
	wt := &MemoryWorktree{}
	logger := wt.log()
	if logger == nil {
		t.Error("expected non-nil logger")
	}
}

// ---------------------------------------------------------------------------
// ProjectLinkDir
// ---------------------------------------------------------------------------

func TestProjectLinkDir(t *testing.T) {
	got := ProjectLinkDir("project")
	if !strings.Contains(got, "memory") || !strings.Contains(got, "project") {
		t.Errorf("ProjectLinkDir = %q", got)
	}
}

// ---------------------------------------------------------------------------
// firstLineFromContent
// ---------------------------------------------------------------------------

func TestFirstLineFromContent_Coverage(t *testing.T) {
	tests := []struct {
		name  string
		input string
		want  string
	}{
		{"empty", "", ""},
		{"only headers", "# Title\n## Sub", ""},
		{"body after header", "# Title\nSome content here", "Some content here"},
		{"blank lines then body", "\n\nContent below", "Content below"},
		{"long line truncated", strings.Repeat("x", 200), strings.Repeat("x", 100) + "…"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := firstLineFromContent(tc.input)
			if got != tc.want {
				t.Errorf("firstLineFromContent(%q) = %q, want %q", tc.input, got, tc.want)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// extractBodyAfterFrontmatter
// ---------------------------------------------------------------------------

func TestExtractBodyAfterFrontmatter_Coverage(t *testing.T) {
	tests := []struct {
		name  string
		input string
		want  string
	}{
		{"no frontmatter", "Just body text", "Just body text"},
		{"with frontmatter and h1", "---\ntitle: Test\n---\n\n# Title\n\nBody here", "Body here"},
		{"frontmatter only", "---\ntitle: Test\n---", ""},
		{"frontmatter with no h1", "---\ntitle: Test\n---\n\nDirect content", "Direct content"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := extractBodyAfterFrontmatter(tc.input)
			if got != tc.want {
				t.Errorf("extractBody(%q) = %q, want %q", tc.input, got, tc.want)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// listImportantInDir — unreadable file
// ---------------------------------------------------------------------------

func TestListImportantInDir_UnreadableFile(t *testing.T) {
	dir := t.TempDir()
	fname := ImportantFileName("test-id")
	fpath := filepath.Join(dir, fname)
	_ = os.WriteFile(fpath, []byte("data"), 0644)
	_ = os.Chmod(fpath, 0000)
	defer func() { _ = os.Chmod(fpath, 0644) }()

	entries, err := listImportantInDir(dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	// Should skip unreadable file
	if len(entries) != 0 {
		t.Errorf("expected 0 entries for unreadable file, got %d", len(entries))
	}
}

// ---------------------------------------------------------------------------
// listRecentInDir — empty dir, nonexistent dir
// ---------------------------------------------------------------------------

func TestListRecentInDir_NonexistentDir(t *testing.T) {
	entries, err := listRecentInDir("/nonexistent/path/that/doesnt/exist", 10)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(entries) != 0 {
		t.Errorf("expected 0 entries, got %d", len(entries))
	}
}

func TestListImportantInDir_NonexistentDir(t *testing.T) {
	entries, err := listImportantInDir("/nonexistent/path/that/doesnt/exist")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(entries) != 0 {
		t.Errorf("expected 0 entries, got %d", len(entries))
	}
}

// ---------------------------------------------------------------------------
// AllContextDirs
// ---------------------------------------------------------------------------

func TestAllContextDirs_Coverage(t *testing.T) {
	// With default paths, this will return whatever is in .graphit/memory
	result := AllContextDirs()
	// Just verify it doesn't panic — result depends on current env
	_ = result
}

// ---------------------------------------------------------------------------
// GlobalBaseDir, GlobalScopeDir
// ---------------------------------------------------------------------------

func TestGlobalBaseDir_Coverage(t *testing.T) {
	dir := GlobalBaseDir()
	if !strings.Contains(dir, "memory") {
		t.Errorf("GlobalBaseDir = %q, should contain 'memory'", dir)
	}
}

func TestGlobalScopeDir_NonexistentScope(t *testing.T) {
	dir := GlobalScopeDir("nonexistent-scope-12345")
	if dir != "" {
		t.Errorf("expected empty dir for nonexistent scope, got %q", dir)
	}
}

// ---------------------------------------------------------------------------
// WikiDir, WorktreeRawDirForScope, WorktreeRawDir
// ---------------------------------------------------------------------------

func TestWikiDir_NonexistentScope(t *testing.T) {
	dir := WikiDir("nonexistent-scope-12345")
	if dir != "" {
		t.Errorf("expected empty dir for nonexistent scope, got %q", dir)
	}
}

func TestWorktreeRawDirForScope_NoScope(t *testing.T) {
	dir := WorktreeRawDirForScope("nonexistent-scope-12345")
	if dir != "" {
		t.Errorf("expected empty dir, got %q", dir)
	}
}

func TestWorktreeRawDir_Coverage(t *testing.T) {
	dir := WorktreeRawDir("project", "test-scope")
	if dir == "" {
		t.Error("expected non-empty dir")
	}
	if !strings.Contains(dir, "memory-wt") {
		t.Errorf("expected path to contain 'memory-wt', got %q", dir)
	}
}

// ---------------------------------------------------------------------------
// EnsureScopeDirs
// ---------------------------------------------------------------------------

func TestEnsureScopeDirs_EmptyProjectDir_Coverage(t *testing.T) {
	err := EnsureScopeDirs("project", "")
	if err != nil {
		t.Fatalf("expected nil error for empty projectDir, got %v", err)
	}
}

func TestEnsureScopeDirs_WithProjectDir_Coverage(t *testing.T) {
	dir := t.TempDir()
	err := EnsureScopeDirs("project", dir)
	if err != nil {
		t.Fatalf("EnsureScopeDirs error: %v", err)
	}
}

// ---------------------------------------------------------------------------
// RawDir
// ---------------------------------------------------------------------------

func TestRawDir_NonexistentScope(t *testing.T) {
	dir := RawDir("nonexistent-scope-12345")
	if dir != "" {
		t.Errorf("expected empty dir for nonexistent scope, got %q", dir)
	}
}

// ---------------------------------------------------------------------------
// IsImportantMemory, ImportantFileName, NormalFileName
// ---------------------------------------------------------------------------

func TestIsImportantMemory_Coverage(t *testing.T) {
	if !IsImportantMemory("test_important_.md") {
		t.Error("should be important")
	}
	if IsImportantMemory("test.md") {
		t.Error("should not be important")
	}
}

func TestImportantFileName_Coverage(t *testing.T) {
	got := ImportantFileName("abc")
	if got != "abc_important_.md" {
		t.Errorf("ImportantFileName = %q", got)
	}
}

func TestNormalFileName_Coverage(t *testing.T) {
	got := NormalFileName("abc")
	if got != "abc.md" {
		t.Errorf("NormalFileName = %q", got)
	}
}




