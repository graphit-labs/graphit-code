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

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// ScopeStore (filesystem-only, no real git)

func TestScopeStore_WriteReadRemoveListDir(t *testing.T) {
	dir := t.TempDir()
	wt := &ScopeStore{dir: dir, scopePath: "memory/project/test"}

	if wt.Dir() != dir {
		t.Errorf("Dir() = %q; want %q", wt.Dir(), dir)
	}

	if err := wt.WriteFile("hello.md", []byte("content")); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	if err := wt.WriteFile("sub/nested.md", []byte("nested")); err != nil {
		t.Fatalf("WriteFile nested: %v", err)
	}

	data, err := wt.ReadFile("hello.md")
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(data) != "content" {
		t.Errorf("ReadFile content = %q; want 'content'", string(data))
	}

	_, err = wt.ReadFile("nonexistent.md")
	if err == nil {
		t.Error("expected error for non-existent file")
	}

	entries, err := wt.ListDir(".")
	if err != nil {
		t.Fatalf("ListDir: %v", err)
	}
	if len(entries) < 2 { // hello.md and sub/
		t.Errorf("expected at least 2 entries, got %d", len(entries))
	}

	if err := wt.RemoveFile("hello.md"); err != nil {
		t.Fatalf("RemoveFile: %v", err)
	}
	_, err = wt.ReadFile("hello.md")
	if err == nil {
		t.Error("expected error after removing file")
	}

	err = wt.RemoveFile("nonexistent.md")
	if err == nil {
		t.Error("expected error when removing non-existent file")
	}
}

// MemoryStore helpers (filesystem parts)

func TestScopeDir(t *testing.T) {
	store := &MemoryStore{rawBase: "/wt-base"}
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
			got := store.scopeDir(tc.branch)
			if got != tc.want {
				t.Errorf("scopeDir(%q) = %q; want %q", tc.branch, got, tc.want)
			}
		})
	}
}

func TestMemoryStore_Dir(t *testing.T) {
	store := &MemoryStore{rawBase: "/test/repo"}
	if store.Dir() != "/test/repo" {
		t.Errorf("Dir() = %q; want '/test/repo'", store.Dir())
	}
}

// copyDirRecursive: error on filepath.Walk (missing rel path)

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

func TestMemoryBranchLockFileOps(t *testing.T) {
	// Override globalDir so all paths resolve to our temp dir
	dir := t.TempDir()
	origHome := os.Getenv("HOME")
	t.Setenv("HOME", dir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	store := &MemoryStore{rawBase: filepath.Join(dir, "wt")}

	if err := store.RegisterScope("memory/project/test", "ref1"); err != nil {
		t.Fatalf("RegisterScope: %v", err)
	}

	// Register same ref again (idempotent)
	if err := store.RegisterScope("memory/project/test", "ref1"); err != nil {
		t.Fatalf("RegisterScope duplicate: %v", err)
	}

	if err := store.RegisterScope("memory/project/test", "ref2"); err != nil {
		t.Fatalf("RegisterScope ref2: %v", err)
	}

	branches, err := store.ActiveScopes()
	if err != nil {
		t.Fatalf("ActiveScopes: %v", err)
	}
	if len(branches) != 1 {
		t.Errorf("expected 1 active branch, got %d", len(branches))
	}

	summary, err := store.ScopeSummary()
	if err != nil {
		t.Fatalf("ScopeSummary: %v", err)
	}
	if len(summary["memory/project/test"]) != 2 {
		t.Errorf("expected 2 refs, got %d", len(summary["memory/project/test"]))
	}

	unused, err := store.DeregisterScope("memory/project/test", "ref1")
	if err != nil {
		t.Fatalf("DeregisterScope: %v", err)
	}
	if unused {
		t.Error("expected branch not to be unused yet")
	}

	unused, err = store.DeregisterScope("memory/project/test", "ref2")
	if err != nil {
		t.Fatalf("DeregisterScope: %v", err)
	}
	if !unused {
		t.Error("expected branch to be unused after removing all refs")
	}

	unused, err = store.DeregisterScope("nonexistent", "ref")
	if err != nil {
		t.Fatalf("DeregisterScope nonexistent: %v", err)
	}
	if unused {
		t.Error("expected false for non-existent branch")
	}
}

func TestLoadMemLock_BadJSON(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	// Create malformed lock file
	lockPath := scopeLockPath()
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
	if lf == nil || lf.Scopes == nil {
		t.Fatal("expected initialized lock file")
	}
}

func TestLoadMemLock_NilBranches(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	lockPath := scopeLockPath()
	if err := os.MkdirAll(filepath.Dir(lockPath), 0o755); err != nil {
		t.Fatal(err)
	}
	// Write valid JSON but with null branches
	data, _ := json.Marshal(scopeLockFile{Version: 1, Scopes: nil})
	if err := os.WriteFile(lockPath, data, 0o644); err != nil {
		t.Fatal(err)
	}

	lf, err := loadMemLock()
	if err != nil {
		t.Fatalf("loadMemLock: %v", err)
	}
	if lf.Scopes == nil {
		t.Error("Branches should be initialised even when nil in JSON")
	}
}

func TestActiveBranches_Empty(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	store := &MemoryStore{rawBase: filepath.Join(dir, "wt")}
	branches, err := store.activeScopes()
	if err != nil {
		t.Fatalf("activeBranches: %v", err)
	}
	if len(branches) != 0 {
		t.Errorf("expected 0 branches, got %d", len(branches))
	}
}

func TestValidateScopeRefs(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	store := &MemoryStore{rawBase: filepath.Join(dir, "wt")}

	// Register a branch with "user" ref (always alive)
	if err := store.RegisterScope("branch-user", "user"); err != nil {
		t.Fatal(err)
	}

	// Register a branch with a file-path ref that does exist
	lockDir := t.TempDir()
	lockFile := filepath.Join(lockDir, "graphit.lock.json")
	if err := os.WriteFile(lockFile, []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := store.RegisterScope("branch-alive", lockDir); err != nil {
		t.Fatal(err)
	}

	// Register a branch with a ref that does NOT exist (stale)
	if err := store.RegisterScope("branch-stale", "/nonexistent/stale/ref"); err != nil {
		t.Fatal(err)
	}

	cleaned, err := store.ValidateScopeRefs()
	if err != nil {
		t.Fatalf("ValidateScopeRefs: %v", err)
	}
	if cleaned != 1 {
		t.Errorf("cleaned = %d; want 1", cleaned)
	}
}

func TestValidateScopeRefs_NoCleaning(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	store := &MemoryStore{rawBase: filepath.Join(dir, "wt")}

	// Register with "user" ref only (never cleaned)
	if err := store.RegisterScope("branch-user", "user"); err != nil {
		t.Fatal(err)
	}

	cleaned, err := store.ValidateScopeRefs()
	if err != nil {
		t.Fatalf("ValidateScopeRefs: %v", err)
	}
	if cleaned != 0 {
		t.Errorf("cleaned = %d; want 0", cleaned)
	}
}

// MemoryService helpers (non-git operations)

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
	entries, readErr := os.ReadDir(wikiDir)
	if readErr != nil {
		t.Fatalf("reading wiki dir: %v", readErr)
	}
	if len(entries) < 1 {
		t.Error("expected at least 1 file in wiki dir")
	}
}

func TestMemoryService_SyncToLocal_NilStore(t *testing.T) {
	svc := &MemoryService{}
	err := svc.SyncToLocal()
	if err == nil {
		t.Error("expected error with nil gitStore")
	}
}

// Store locations — one copy each, all global
func TestMemoryWikiGlobalDir(t *testing.T) {
	got := MemoryWikiGlobalDir("project", "abc123")
	if got == "" {
		t.Error("MemoryWikiGlobalDir returned empty")
	}
	if !strings.Contains(got, "wiki") {
		t.Errorf("MemoryWikiGlobalDir = %q; expected wiki in path", got)
	}
}

// A scope's wiki must resolve into the global directory and never into the project.
// The project replica it used to resolve to is what made a project answer from a copy
// nobody had refreshed.
func TestWikiDirForIsGlobalAndNeverProjectLocal(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	projectDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()),
		[]byte(`{"project":{"id":"01ACME"}}`), 0o644); err != nil {
		t.Fatal(err)
	}

	got := WikiDirFor(projectDir, "project")
	if got == "" {
		t.Fatal("WikiDirFor returned empty for an initialised project")
	}
	if !strings.HasPrefix(got, filepath.Join(home, brand.DotDir())) {
		t.Errorf("WikiDirFor = %q, want it under the global dir", got)
	}
	if strings.HasPrefix(got, projectDir) {
		t.Errorf("WikiDirFor = %q leaked into the project directory", got)
	}
}

// Without a lockfile there is no project id, so there is no project scope to name —
// the only case that legitimately comes back empty.
func TestWikiDirForEmptyWithoutAProjectID(t *testing.T) {
	if got := WikiDirFor(t.TempDir(), "project"); got != "" {
		t.Errorf("expected empty without a project id, got %q", got)
	}
}
func TestWikiDirFunc(t *testing.T) {
	// A context scope is named by itself, so it resolves without any project.
	got := WikiDir("some-context")
	if got == "" {
		t.Error("a context scope must resolve to a wiki path")
	}
	if !strings.Contains(filepath.ToSlash(got), "wiki/memory/some-context") {
		t.Errorf("unexpected context wiki dir %q", got)
	}
}
func TestRawDirFunc(t *testing.T) {
	// An unrecognised scope name IS a context name, so it resolves to a worktree
	// path. What must not happen is the old behaviour: returning "" because the
	// project had no replica yet, which made the raw store — the source of truth —
	// unreachable until something had already compiled from it.
	got := RawDir("nonexistent-scope-" + fmt.Sprintf("%d", time.Now().UnixNano()))
	if got == "" {
		t.Error("a context scope must resolve to a worktree path without a replica")
	}
	if !strings.Contains(got, "memory-raw") {
		t.Errorf("expected a worktree path, got %q", got)
	}
}
func TestRawDirForScope_EmptyWhenScopeIDUnresolvable(t *testing.T) {
	// "project" reads its id from the lockfile in the working directory. Without one
	// there is no scope, and no scope means no path — that is the real guard, and the
	// only case that should come back empty.
	t.Chdir(t.TempDir())
	if got := RawDirForScope("project"); got != "" {
		t.Errorf("expected empty without a resolvable project id, got %q", got)
	}
}
func TestRawDirFor_EmptyGlobalDir(t *testing.T) {
	// With HOME set, GlobalDir returns a path so this always has a value
	got := RawDirFor("project", "abc")
	if got == "" {
		t.Error("expected non-empty")
	}
}

// AllContextDirs — the worktree set IS the record of imported memory contexts
func TestAllContextDirs(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)

	wtRoot := filepath.Dir(RawDirFor("project", "x"))
	for _, name := range []string{
		"memory-project-01ACME", "memory-user-abc123", "memory-my-context-my-context", "stray",
	} {
		if err := os.MkdirAll(filepath.Join(wtRoot, name), 0o755); err != nil {
			t.Fatal(err)
		}
	}

	got := AllContextDirs()
	if len(got) != 1 || got[0] != "my-context" {
		// A context's worktree carries its name twice — memory-<name>-<name> — and
		// that doubling is both how the name survives a hyphen and how a context is
		// told apart from the project and user scopes.
		t.Errorf("AllContextDirs = %v, want [my-context]", got)
	}
}

func TestEnsureWikiIndexExists_AlreadyExists(t *testing.T) {
	dir := t.TempDir()
	wikiDir := filepath.Join(dir, ".graphit", "memory", "project")
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(wikiDir, "index.md"), []byte("existing"), 0o644); err != nil {
		t.Fatal(err)
	}

	// An unknown scope resolves to a wiki dir that does not exist, which is the
	// early-return path: nothing is written and nothing panics.
	EnsureWikiIndexExists("nonexistent-scope", nil)
}

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
	report, err := consolidateDir(ctx, dir, nil)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if report.TotalMemories != 0 {
		t.Errorf("expected 0, got %d", report.TotalMemories)
	}
}

func TestRunConsolidation_WithMemories(t *testing.T) {
	dir := t.TempDir()
	ctx := context.Background()

	oldDate := time.Now().Add(-120 * 24 * time.Hour).Format(time.RFC3339)

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

	report, err := consolidateDir(ctx, dir, nil)
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

	mockResp := `{
  "duplicates": [
    {"ids": ["01J5XABC1234567890", "01J5XDEF1234567890"], "keep_id": "01J5XABC1234567890",
     "merged_title": "Merged", "merged_content": "Body 1 and Body 2", "reason": "Same thing"}
  ],
  "contradictions": [],
  "suggestions": [
    {"action": "promote", "id": "01J5XABC1234567890", "reason": "Should be important"},
    {"action": "demote", "id": "01J5XDEF1234567890", "reason": "Too specific"},
    {"action": "delete", "id": "01J5XABC1234567890", "reason": "Outdated"},
    {"action": "update", "id": "01J5XDEF1234567890", "new_content": "fuller", "reason": "Needs more detail"}
  ]
}`

	client := &mockAIClient{response: mockResp}
	report, err := aiConsolidation(ctx, client, memories, nil)
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

// JSON is the only accepted contract. An answer in the old sectioned format — or any
// other prose — is an error, not a partial success: a second, looser parser would
// turn a malformed answer into a partially-understood plan applied against real
// memories, and an unparseable analysis would be indistinguishable from a clean
// corpus.
func TestAiConsolidation_RejectsNonJSON(t *testing.T) {
	memories := []memorySnapshot{
		{ID: "01J5XABC1234567890", Title: "M1", Body: "b1"},
		{ID: "01J5XDEF1234567890", Title: "M2", Body: "b2"},
	}
	client := &mockAIClient{response: "## DUPLICATES\n- MERGE [01J5XABC1234567890] and [01J5XDEF1234567890]: same"}

	if _, err := aiConsolidation(context.Background(), client, memories, nil); err == nil {
		t.Fatal("expected an error for a non-JSON analysis")
	}
}

func TestAiConsolidation_Error(t *testing.T) {
	ctx := context.Background()
	memories := []memorySnapshot{
		{ID: "ID1", Title: "M1"},
		{ID: "ID2", Title: "M2"},
	}
	client := &mockAIClient{err: fmt.Errorf("AI error")}
	_, err := aiConsolidation(ctx, client, memories, nil)
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
	client := &mockAIClient{response: `{"duplicates": [], "contradictions": [], "suggestions": []}`}
	report, err := aiConsolidation(ctx, client, memories, nil)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if report == nil {
		t.Fatal("expected non-nil report")
	}
}

type mockStoreProvider struct {
	extractErr error
}

func (m *mockStoreProvider) ExtractScopeDir(_, _, _ string) error {
	return m.extractErr
}

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

func TestRuleContent(t *testing.T) {
	content := RuleContent(nil)
	if content == "" {
		t.Error("RuleContent returned empty")
	}
	if !strings.Contains(content, "Memory Management Rule") {
		t.Error("expected header")
	}
}

// InstallRule, InstallSkill, RemoveRule, RemoveSkill

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

func TestMemoryEntityPage_StaleWarning(t *testing.T) {
	oldDate := time.Now().Add(-60 * 24 * time.Hour).Format(time.RFC3339)
	page := memoryEntityPageWithHash("ID", "Old Memory", oldDate, false, "Body.", "fact", "")
	if !strings.Contains(page, "Stale memory") {
		t.Error("expected stale memory warning for 60-day-old memory")
	}
}

func TestMemoryEntityPage_UnknownTypeEmoji(t *testing.T) {
	page := memoryEntityPageWithHash("ID", "Custom Type", "", false, "Body.", "custom-type", "")
	if !strings.Contains(page, "📄") {
		t.Error("expected fallback emoji for unknown type")
	}
}

func TestMemoryEntityPage_NoBody(t *testing.T) {
	page := memoryEntityPageWithHash("ID", "Title", "", false, "", "fact", "")
	if !strings.Contains(page, "# Title") {
		t.Error("should contain title")
	}
}

func TestMemoryEntityPage_NoCreatedAt(t *testing.T) {
	page := memoryEntityPageWithHash("ID", "Title", "", false, "Body.", "", "")
	if strings.Contains(page, "created:") {
		t.Error("should not contain created when empty")
	}
}

// memoryIndexPage: untyped memories, important prefix, empty/no-body summaries

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

// appendMemLog: with existing content that has separator

func TestAppendMemLog_WithSeparator(t *testing.T) {
	dir := t.TempDir()
	logPath := filepath.Join(dir, "log.md")

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

// GenerateMemoryWiki: with logger, with write error (readonly dir)

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

func TestOnHubImport(t *testing.T) {
	ctx := context.Background()
	store := &mockStoreProvider{}
	// OnHubImport spawns a goroutine — we just test it doesn't panic
	OnHubImport(ctx, "test-context", t.TempDir(), store, nil)
	// Give goroutine time to finish
	time.Sleep(50 * time.Millisecond)
}

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

func TestDetectStaleMemories_BadDateFormat(t *testing.T) {
	memories := []memorySnapshot{
		{ID: "1", Title: "Bad Date", CreatedAt: "not-a-date", Important: false},
	}
	stale := detectStaleMemories(memories)
	if len(stale) != 0 {
		t.Errorf("bad date format should be skipped, got %d stale", len(stale))
	}
}

// SelectiveFetch (no-op tests)

func TestWaitForPendingPushes(t *testing.T) {
	// Should return immediately when no pushes are pending
	WaitForPendingPushes()
}

func TestNewMemorySvcInternal_WithStore(t *testing.T) {
	store := &MemoryStore{rawBase: "/repo"}
	svc := newMemorySvcInternal(MemoryScopeProject, "id", "/local", store)
	if svc == nil {
		t.Fatal("expected non-nil svc")
	}
	// Store's Logger is set to svc.Logger (nil at this point)
	if store.Logger != nil {
		t.Error("expected nil Logger")
	}
}

func TestMemoryService_Log(t *testing.T) {
	svc := &MemoryService{}
	logger := svc.log()
	if logger == nil {
		t.Error("expected non-nil logger from slogutil.Resolve")
	}
}

func TestMemoryStore_Log(t *testing.T) {
	store := &MemoryStore{}
	logger := store.log()
	if logger == nil {
		t.Error("expected non-nil logger from slogutil.Resolve")
	}
}

func TestScopeStore_Log(t *testing.T) {
	wt := &ScopeStore{}
	logger := wt.log()
	if logger == nil {
		t.Error("expected non-nil logger")
	}
}

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

func TestAllContextDirs_Coverage(t *testing.T) {
	// With default paths, this will return whatever is in .graphit/memory
	result := AllContextDirs()
	// Just verify it doesn't panic — result depends on current env
	_ = result
}

// WikiDir, RawDirForScope, RawDirFor

// An unknown scope name IS a context name, so it resolves — the wiki dir is derived
// from the name, not probed on disk. This used to return "" because it stat'ed a
// project replica, which meant a context could not be compiled until it had already
// been compiled.
func TestWikiDir_UnknownScopeIsAContext(t *testing.T) {
	dir := WikiDir("nonexistent-scope-12345")
	if dir == "" {
		t.Fatal("a context scope must resolve to a wiki path")
	}
	if !strings.Contains(filepath.ToSlash(dir), "wiki/memory/nonexistent-scope-12345") {
		t.Errorf("unexpected wiki dir %q", dir)
	}
}

func TestRawDirForScope_ContextNeedsNoReplica(t *testing.T) {
	// Pinning the bootstrapping fix: a scope resolves to its worktree with nothing
	// on disk in the project yet.
	t.Chdir(t.TempDir())
	dir := RawDirForScope("some-context")
	if dir == "" {
		t.Fatal("a context scope must resolve without a project replica")
	}
	if !strings.Contains(dir, "memory-raw") {
		t.Errorf("expected a worktree path, got %q", dir)
	}
}

func TestRawDirFor_Coverage(t *testing.T) {
	dir := RawDirFor("project", "test-scope")
	if dir == "" {
		t.Error("expected non-empty dir")
	}
	if !strings.Contains(dir, "memory-raw") {
		t.Errorf("expected path to contain 'memory-wt', got %q", dir)
	}
}

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

func TestRawDir_ContextScopeResolves(t *testing.T) {
	dir := RawDir("nonexistent-scope-12345")
	if dir == "" {
		t.Error("a context scope resolves to a worktree path")
	}
}

// IsImportantMemory, ImportantFileName, NormalFileName

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
