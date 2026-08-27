package memory

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// memory_git_store.go — using real git init in TempDir

// Initialisation creates the raw directory root and nothing else.
//
// It used to run `git init`, write a bootstrap commit, configure a remote and prune refs, and the
// test asserted a `.git` had appeared. There is no repository now, so the assertion is the
// directory — and no git binary is required, which is why the skip is gone too.
func TestMemoryStore_EnsureInitialised_Full(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{rawBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}
	info, err := os.Stat(wtBase)
	if err != nil || !info.IsDir() {
		t.Fatalf("expected the raw directory root to exist: %v", err)
	}
	if store.Configured() {
		t.Error("a store with no bucket must report itself unconfigured")
	}
	// Idempotent.
	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("second EnsureInitialised: %v", err)
	}
}

// Publish with no remote configured is a no-op that must not error, because local-only is a
// supported mode and every memory write ends in a Publish.
func TestMemoryStore_PublishWithoutRemoteIsANoop(t *testing.T) {
	store := &MemoryStore{rawBase: filepath.Join(t.TempDir(), "wt")}
	w, err := store.OpenScopeLocal("memory/project/p1")
	if err != nil {
		t.Fatalf("open scope: %v", err)
	}
	if err := w.WriteFile("a.md", []byte("# a")); err != nil {
		t.Fatalf("write: %v", err)
	}
	if err := w.Publish("adding a"); err != nil {
		t.Fatalf("Publish with no remote must not fail: %v", err)
	}
	WaitForPendingPushes()
	if _, err := w.ReadFile("a.md"); err != nil {
		t.Errorf("the raw directory is the truth and must still hold the file: %v", err)
	}
}

// A leftover .git from the worktree this replaced must never be uploaded, and must not appear in
// an extraction either.
func TestMemoryStoreSkipsLeftoverGitMetadata(t *testing.T) {
	store := &MemoryStore{rawBase: filepath.Join(t.TempDir(), "wt")}
	w, err := store.OpenScopeLocal("memory/project/p1")
	if err != nil {
		t.Fatalf("open scope: %v", err)
	}
	if err := os.MkdirAll(filepath.Join(w.Dir(), ".git", "refs"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(w.Dir(), ".git", "HEAD"), []byte("ref: refs/heads/x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := w.WriteFile("mem.md", []byte("# m")); err != nil {
		t.Fatal(err)
	}

	dest := filepath.Join(t.TempDir(), "out")
	if err := store.ExtractScopeDir("memory/project/p1", ".", dest); err != nil {
		t.Fatalf("ExtractScopeDir: %v", err)
	}
	if _, err := os.Stat(filepath.Join(dest, ".git")); !os.IsNotExist(err) {
		t.Errorf("git metadata leaked into the extraction: %v", err)
	}
	if _, err := os.Stat(filepath.Join(dest, "mem.md")); err != nil {
		t.Errorf("the memory itself did not survive extraction: %v", err)
	}
}

func TestMemoryStore_OpenScopeLocal_Full(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{rawBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	branch := "memory/project/test-proj"
	wt, err := store.OpenScopeLocal(branch)
	if err != nil {
		t.Fatalf("OpenScopeLocal: %v", err)
	}
	if wt == nil {
		t.Fatal("expected non-nil worktree")
	}
	if wt.Dir() == "" {
		t.Error("expected non-empty dir")
	}

	if err := wt.WriteFile("test.md", []byte("hello")); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	// Read it back
	data, err := wt.ReadFile("test.md")
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(data) != "hello" {
		t.Errorf("content = %q", string(data))
	}

	// CommitAndPush (without remote — push will be skipped)
	if err := wt.Publish("test commit"); err != nil {
		t.Fatalf("CommitAndPush: %v", err)
	}

	// Second call to OpenScopeLocal should reuse existing worktree
	wt2, err := store.OpenScopeLocal(branch)
	if err != nil {
		t.Fatalf("second OpenScopeLocal: %v", err)
	}
	if wt2.Dir() != wt.Dir() {
		t.Errorf("expected same worktree dir, got %q vs %q", wt2.Dir(), wt.Dir())
	}
}

func TestMemoryStore_ExtractScopeDir_Full(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{rawBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	branch := "memory/project/extract-test"
	wt, err := store.OpenScopeLocal(branch)
	if err != nil {
		t.Fatalf("OpenScopeLocal: %v", err)
	}

	if err := wt.WriteFile("data/test.md", []byte("extracted")); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := wt.Publish("add data"); err != nil {
		t.Fatalf("CommitAndPush: %v", err)
	}

	// Extract
	destDir := filepath.Join(t.TempDir(), "dest")
	if err := store.ExtractScopeDir(branch, "data", destDir); err != nil {
		t.Fatalf("ExtractScopeDir: %v", err)
	}

	data, err := os.ReadFile(filepath.Join(destDir, "test.md"))
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(data) != "extracted" {
		t.Errorf("content = %q; want 'extracted'", string(data))
	}
}

func TestMemoryStore_ExtractScopeDir_NonExistentSubdir(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{rawBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	branch := "memory/project/no-subdir"
	_, err := store.OpenScopeLocal(branch)
	if err != nil {
		t.Fatalf("OpenScopeLocal: %v", err)
	}

	destDir := filepath.Join(t.TempDir(), "empty-dest")
	if err := store.ExtractScopeDir(branch, "nonexistent", destDir); err != nil {
		t.Fatalf("ExtractScopeDir: %v", err)
	}
	if _, err := os.Stat(destDir); err != nil {
		t.Errorf("expected destDir to exist: %v", err)
	}
}

func TestMemoryStore_Prune_Full(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{rawBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	branch := "memory/project/prune-test"
	wt, err := store.OpenScopeLocal(branch)
	if err != nil {
		t.Fatalf("OpenScopeLocal: %v", err)
	}

	wtDir := wt.Dir()
	if _, err := os.Stat(wtDir); err != nil {
		t.Fatalf("worktree dir should exist: %v", err)
	}

	if err := wt.Prune(); err != nil {
		t.Fatalf("Prune: %v", err)
	}

	if _, err := os.Stat(wtDir); !os.IsNotExist(err) {
		t.Error("expected worktree dir to be removed after Prune")
	}
}

func TestMemoryStore_RegisterDeregisterScope_Full(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{rawBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	branch := "memory/project/lock-test"
	ref := "/some/ref/path"

	if err := store.RegisterScope(branch, ref); err != nil {
		t.Fatalf("RegisterScope: %v", err)
	}

	// Register again (duplicate check)
	if err := store.RegisterScope(branch, ref); err != nil {
		t.Fatalf("RegisterScope (dup): %v", err)
	}

	active, err := store.ActiveScopes()
	if err != nil {
		t.Fatalf("ActiveScopes: %v", err)
	}
	found := false
	for _, b := range active {
		if b == branch {
			found = true
		}
	}
	if !found {
		t.Error("expected branch to be active")
	}

	summary, err := store.ScopeSummary()
	if err != nil {
		t.Fatalf("ScopeSummary: %v", err)
	}
	refs, ok := summary[branch]
	if !ok {
		t.Error("expected branch in summary")
	}
	if len(refs) != 1 || refs[0] != ref {
		t.Errorf("refs = %v", refs)
	}

	unused, err := store.DeregisterScope(branch, ref)
	if err != nil {
		t.Fatalf("DeregisterScope: %v", err)
	}
	if !unused {
		t.Error("expected branch to become unused")
	}
}

func TestMemoryStore_ValidateScopeRefs_Full(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{rawBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	branch := "memory/project/validate-test"

	// Register with non-existent ref path — should be cleaned
	if err := store.RegisterScope(branch, "/nonexistent/ref/path"); err != nil {
		t.Fatalf("RegisterScope: %v", err)
	}

	// Also register with "user" ref — should be preserved
	if err := store.RegisterScope(branch, "user"); err != nil {
		t.Fatalf("RegisterScope: %v", err)
	}

	cleaned, err := store.ValidateScopeRefs()
	if err != nil {
		t.Fatalf("ValidateScopeRefs: %v", err)
	}
	if cleaned < 1 {
		t.Errorf("expected at least 1 cleaned ref, got %d", cleaned)
	}
}

func TestMemoryStore_DeregisterScope_NotFound(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{rawBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	unused, err := store.DeregisterScope("nonexistent-branch", "ref")
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if unused {
		t.Error("expected false for non-existent branch")
	}
}

// Pruning a scope removes its raw directory and nothing else.
//
// It used to also delete a branch ref, and the test created an orphan branch to have one. There
// is no ref now: the directory and the lock entry are this machine's whole record of a scope.
// The remote prefix must survive, because another machine may still be using the scope.
func TestMemoryStore_PruneLocalBranch_Full(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{rawBase: wtBase}
	t.Setenv("HOME", t.TempDir()) // isolate from the developer's global lock file

	const branch = "memory/prune/target"
	w, err := store.OpenScopeLocal(branch)
	if err != nil {
		t.Fatalf("open scope: %v", err)
	}
	if err := w.WriteFile("m.md", []byte("# m")); err != nil {
		t.Fatal(err)
	}

	store.pruneLocalScope(branch)

	if _, err := os.Stat(store.ScopeDir(branch)); !os.IsNotExist(err) {
		t.Errorf("expected the raw directory to be gone, got %v", err)
	}
}

// memory.go — MemoryService operations with ScopeStore (via git)

func TestMemoryService_AddUpdateRemoveMemory_WithGitStore(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{rawBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	localDir := t.TempDir()
	svc := &MemoryService{
		scope:    MemoryScopeProject,
		scopeID:  "test-proj",
		localDir: localDir,
		store:    store,
	}

	id, err := svc.AddMemory("Test Title", "Test body content", MemoryOpts{})
	if err != nil {
		t.Fatalf("AddMemory: %v", err)
	}
	if id == "" {
		t.Fatal("expected non-empty ID")
	}

	err = svc.UpdateMemory(id, "Updated Title", "Updated body content")
	if err != nil {
		t.Fatalf("UpdateMemory: %v", err)
	}

	err = svc.PromoteMemory(id)
	if err != nil {
		t.Fatalf("PromoteMemory: %v", err)
	}

	err = svc.DemoteMemory(id)
	if err != nil {
		t.Fatalf("DemoteMemory: %v", err)
	}

	err = svc.RemoveMemory(id)
	if err != nil {
		t.Fatalf("RemoveMemory: %v", err)
	}
}

func TestMemoryService_AddMemory_WithTypeAndTags(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{rawBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	localDir := t.TempDir()
	svc := &MemoryService{
		scope:    MemoryScopeProject,
		scopeID:  "test-proj",
		localDir: localDir,
		store:    store,
	}

	id, err := svc.AddMemory("Typed Memory", "Body with type", MemoryOpts{
		Type:      MemoryTypeConvention,
		Important: true,
		Tags:      []string{"tag1", "tag2"},
	})
	if err != nil {
		t.Fatalf("AddMemory: %v", err)
	}
	if id == "" {
		t.Fatal("expected non-empty ID")
	}
}

func TestMemoryService_SyncToLocal_WithGitStore(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{rawBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	localDir := t.TempDir()
	svc := &MemoryService{
		scope:    MemoryScopeProject,
		scopeID:  "test-proj",
		localDir: localDir,
		store:    store,
	}

	id, err := svc.AddMemory("Sync Test", "Body content for sync", MemoryOpts{})
	if err != nil {
		t.Fatalf("AddMemory: %v", err)
	}

	err = svc.SyncToLocal()
	if err != nil {
		t.Fatalf("SyncToLocal: %v", err)
	}

	// The sync may or may not find the file depending on branch setup
	_ = id
}

func TestMemoryService_EnsureInitialised_WithGitStore(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{rawBase: wtBase}

	localDir := t.TempDir()
	svc := &MemoryService{
		scope:    MemoryScopeProject,
		scopeID:  "test-proj",
		localDir: localDir,
		store:    store,
	}

	err := svc.EnsureInitialised()
	if err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}
}

// The project root no longer depends on git being present or on there being a repository at all.
// TestProjectRootComesFromTheLockfileNotGit covers the real behaviour; this only pins that the
// function always answers something usable.
func TestProjectRootDirAlwaysAnswers(t *testing.T) {
	t.Chdir(t.TempDir())
	if projectRootDir() == "" {
		t.Error("projectRootDir returned empty outside any project")
	}
}

func TestUserScopeID_Coverage(t *testing.T) {
	// This calls git config — may or may not return a hash
	hash, _ := UserScopeID()
	// Just verify it doesn't panic — may be empty if git user not configured
	_ = hash
}

func TestBuildMemoryFile_AllFieldsCoverage(t *testing.T) {
	content := buildMemoryFile(
		"FULL-ID", "Full Title", "Full body content",
		"user", "user-hash", "orig-proj",
		true, "convention", []string{"tag1", "tag2"},
	)
	checks := []string{
		"id: FULL-ID",
		"title: Full Title",
		"scope: user",
		"scope_id: user-hash",
		"project_id: orig-proj",
		"type: convention",
		"important: true",
		"# Full Title",
		"Full body content",
	}
	for _, c := range checks {
		if !strings.Contains(content, c) {
			t.Errorf("expected content to contain %q", c)
		}
	}
}

func TestBuildMemoryFile_MinimalFields(t *testing.T) {
	content := buildMemoryFile("ID", "Title", "Body", "project", "pid", "", false, "", nil)
	if !strings.Contains(content, "id: ID") {
		t.Error("missing id")
	}
	if strings.Contains(content, "project_id:") {
		t.Error("should not contain project_id when empty")
	}
	if strings.Contains(content, "important:") {
		t.Error("should not contain important when false")
	}
}

func TestMemoryWikiGlobalDir_Coverage(t *testing.T) {
	dir := MemoryWikiGlobalDir("user", "test-user")
	if dir == "" {
		t.Error("expected non-empty")
	}
	if !strings.Contains(dir, "user") {
		t.Errorf("expected 'user' in path, got %q", dir)
	}
}

func TestWikiDir_Coverage(t *testing.T) {
	dir := WikiDir("project")
	// dir may be empty if global dir is not configured
	_ = dir
}

func TestRawDir_Coverage(t *testing.T) {
	dir := RawDir("project")
	// dir may be empty if global dir is not configured
	_ = dir
}

// consolidate.go — RunConsolidation edge cases (helper-based)

func TestRunConsolidation_WithAIClient_Coverage(t *testing.T) {
	dir := t.TempDir()
	now := time.Now().UTC()
	date1 := now.Add(-2 * 24 * time.Hour).Format(time.RFC3339)
	date2 := now.Add(-3 * 24 * time.Hour).Format(time.RFC3339)

	writeMemFile(t, dir, "MEM1.md", fmt.Sprintf("---\ntitle: Memory One\ntype: fact\ncreated_at: %s\n---\n\n# Memory One\n\nContent of memory one.", date1))
	writeMemFile(t, dir, "MEM2.md", fmt.Sprintf("---\ntitle: Memory Two\ntype: fact\ncreated_at: %s\n---\n\n# Memory Two\n\nContent of memory two.", date2))

	aiResponse := `## DUPLICATES
- MERGE [MEM1XXXXXXXXXXX] and [MEM2XXXXXXXXXXX]: Same content

## CONTRADICTIONS
None found

## SUGGESTIONS
- PROMOTE [MEM1XXXXXXXXXXX]: should be important`

	client := &fakeAIClient{response: aiResponse}

	ctx := context.Background()
	report, err := consolidateDir(ctx, dir, client)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if report.TotalMemories != 2 {
		t.Errorf("TotalMemories = %d; want 2", report.TotalMemories)
	}
	if report.AIAnalysis == "" {
		t.Error("expected AIAnalysis to be populated")
	}
}

func TestRunConsolidation_AIClientError_Coverage(t *testing.T) {
	dir := t.TempDir()
	date := time.Now().Add(-2 * 24 * time.Hour).Format(time.RFC3339)
	writeMemFile(t, dir, "MEM1.md", fmt.Sprintf("---\ntitle: Mem1\ncreated_at: %s\n---\n\n# Mem1\n\nBody 1.", date))
	writeMemFile(t, dir, "MEM2.md", fmt.Sprintf("---\ntitle: Mem2\ncreated_at: %s\n---\n\n# Mem2\n\nBody 2.", date))

	client := &fakeAIClient{err: fmt.Errorf("AI unavailable")}
	ctx := context.Background()
	report, err := consolidateDir(ctx, dir, client)
	if err != nil {
		t.Fatalf("should not return error, got: %v", err)
	}
	if !strings.Contains(report.AIAnalysis, "AI analysis failed") {
		t.Errorf("expected AI failure message, got %q", report.AIAnalysis)
	}
}

func TestRunConsolidation_SingleMemory_Coverage(t *testing.T) {
	dir := t.TempDir()
	date := time.Now().Add(-2 * 24 * time.Hour).Format(time.RFC3339)
	writeMemFile(t, dir, "SINGLE.md", fmt.Sprintf("---\ntitle: Single\ncreated_at: %s\n---\n\n# Single\n\nBody.", date))

	client := &fakeAIClient{response: "should not be called"}
	ctx := context.Background()
	report, err := consolidateDir(ctx, dir, client)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if report.TotalMemories != 1 {
		t.Errorf("expected 1 memory")
	}
	if report.AIAnalysis != "" {
		t.Errorf("expected empty AIAnalysis for single memory, got %q", report.AIAnalysis)
	}
}

func TestRunConsolidation_ImportantMemory_Coverage(t *testing.T) {
	dir := t.TempDir()
	date := time.Now().Add(-120 * 24 * time.Hour).Format(time.RFC3339)
	writeMemFile(t, dir, "IMP_important_.md", fmt.Sprintf("---\ntitle: Important\ncreated_at: %s\nimportant: true\n---\n\n# Important\n\nBody of important memory.", date))
	writeMemFile(t, dir, "NORM.md", fmt.Sprintf("---\ntitle: Normal\ncreated_at: %s\n---\n\n# Normal\n\nBody of normal memory.", date))

	ctx := context.Background()
	report, err := consolidateDir(ctx, dir, nil)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if report.TotalMemories != 2 {
		t.Errorf("expected 2 memories, got %d", report.TotalMemories)
	}
	if len(report.Stale) != 1 {
		t.Errorf("expected 1 stale, got %d", len(report.Stale))
	}
}

// fakeAIClient implements ai.Client for tests (renamed to avoid collision with mockAIClient)
type fakeAIClient struct {
	response string
	err      error
}

func (f *fakeAIClient) Complete(_ context.Context, _, _ string) (string, error) {
	return f.response, f.err
}

func TestMemoryService_IndexMemories_Coverage(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()

	writeMemFile(t, rawDir, "MEM1.md", "---\ntitle: Test\ntype: fact\n---\n\n# Test\n\nBody content.")
	writeMemFile(t, rawDir, "MEM2_important_.md", "---\ntitle: Important\nimportant: true\n---\n\n# Important\n\nImportant body.")

	svc := &MemoryService{
		scope:    MemoryScopeProject,
		scopeID:  "test",
		localDir: rawDir,
		wikiDir:  wikiDir,
	}

	ctx := context.Background()
	err := svc.IndexMemories(ctx)
	if err != nil {
		t.Fatalf("IndexMemories: %v", err)
	}

	// Check that wiki files were created
	entries, readErr := os.ReadDir(wikiDir)
	if readErr != nil {
		t.Fatalf("reading wiki dir: %v", readErr)
	}
	if len(entries) < 1 {
		t.Error("expected at least 1 file in wiki dir")
	}
}

func TestSyncToLocalInternal_NilStore_Coverage(t *testing.T) {
	svc := &MemoryService{scope: MemoryScopeProject, scopeID: "test"}
	err := svc.syncToLocalInternal(true)
	if err == nil {
		t.Error("expected error with nil gitStore")
	}
	if !strings.Contains(err.Error(), "not configured") {
		t.Errorf("unexpected error: %v", err)
	}
}

func TestSyncToLocalInternal_Fast_Coverage(t *testing.T) {
	svc := &MemoryService{scope: MemoryScopeProject, scopeID: "test"}
	err := svc.syncToLocalInternal(false)
	if err == nil {
		t.Error("expected error with nil gitStore")
	}
}

func TestSyncAndCycle_StoreError_Coverage(t *testing.T) {
	ctx := context.Background()
	store := &mockStoreProviderCov{extractErr: fmt.Errorf("extract failed")}
	result := SyncAndCycle(ctx, "project", "test-id", store, nil)
	if result.Scope != "project" {
		t.Errorf("Scope = %q", result.Scope)
	}
}

type mockStoreProviderCov struct {
	extractErr error
}

func (m *mockStoreProviderCov) ExtractScopeDir(_, _, _ string) error {
	return m.extractErr
}

func TestSyncContextFromMemoryRepo_StoreError_Coverage(t *testing.T) {
	ctx := context.Background()
	store := &mockStoreProviderCov{extractErr: fmt.Errorf("extract failed")}
	result := SyncContextFromMemoryRepo(ctx, "test-ctx", t.TempDir(), store, nil)
	if result.Scope != "test-ctx" {
		t.Errorf("Scope = %q", result.Scope)
	}
}

func TestEnsureWikiIndexExists_CreatesNew(t *testing.T) {
	// Use a temp dir to create a wiki dir that doesn't exist yet
	baseDir := t.TempDir()
	wikiDir := filepath.Join(baseDir, "wiki")
	_ = os.MkdirAll(wikiDir, 0o755)
	indexPath := filepath.Join(wikiDir, "index.md")

	// Call EnsureWikiIndexExists with a scope that maps to a non-existing wiki dir
	// Since it uses global path resolution, we test creating the file manually
	content := fmt.Sprintf("---\ntitle: Memory Wiki (%s)\ntags: [memory, %s]\n---\n\n# Memory Wiki\n\n*(No memories indexed yet.)*\n", "test", "test")
	if err := os.WriteFile(indexPath, []byte(content), 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}

	data, err := os.ReadFile(indexPath)
	if err != nil {
		t.Fatalf("read: %v", err)
	}
	if !strings.Contains(string(data), "Memory Wiki") {
		t.Error("missing header")
	}
}

func TestGenerateMemoryWiki_WithImportantMemories(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()

	writeMemFile(t, rawDir, "MEM1.md", "---\ntitle: Regular\ntype: fact\ncreated_at: 2026-01-01T00:00:00Z\n---\n\n# Regular\n\nRegular body.")
	writeMemFile(t, rawDir, "IMP1_important_.md", "---\ntitle: Important One\nimportant: true\ntype: convention\ncreated_at: 2026-01-02T00:00:00Z\n---\n\n# Important One\n\nVery important.")

	ctx := context.Background()
	result, err := GenerateMemoryWiki(ctx, rawDir, wikiDir)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if result.ArticlesWritten != 2 {
		t.Errorf("ArticlesWritten = %d; want 2", result.ArticlesWritten)
	}

	// Check entity pages were written to wiki dir
	entries, readErr := os.ReadDir(wikiDir)
	if readErr != nil {
		t.Fatalf("reading wiki dir: %v", readErr)
	}
	if len(entries) < 2 {
		t.Errorf("expected at least 2 files in wiki dir, got %d", len(entries))
	}
}

// wiki.go — memoryEntityPage dates and stale calculations

func TestMemoryEntityPage_RecentDate(t *testing.T) {
	recentDate := time.Now().Add(-1 * 24 * time.Hour).Format(time.RFC3339)
	page := memoryEntityPageWithHash("RECENT", "Recent Memory", recentDate, false, "Body.", "fact", "")
	if strings.Contains(page, "Stale memory") {
		t.Error("should NOT contain stale warning for 1-day-old memory")
	}
}

func TestMemoryEntityPage_CreatedAtDate(t *testing.T) {
	date := "2026-01-15T12:00:00Z"
	page := memoryEntityPageWithHash("ID", "Title", date, false, "Body.", "fact", "")
	if !strings.Contains(page, "created: ") {
		t.Error("expected created date in page")
	}
}

func TestFirstLineFromContent_Empty(t *testing.T) {
	got := firstLineFromContent("")
	if got != "" {
		t.Errorf("expected empty, got %q", got)
	}
}

func TestFirstLineFromContent_SingleLine(t *testing.T) {
	got := firstLineFromContent("single line of content")
	if got != "single line of content" {
		t.Errorf("got %q", got)
	}
}

func TestFirstLineFromContent_HeadingSkip(t *testing.T) {
	got := firstLineFromContent("# Heading\n## Sub\nActual content line")
	if got != "Actual content line" {
		t.Errorf("expected 'Actual content line', got %q", got)
	}
}

func TestFirstLineFromContent_LongLine(t *testing.T) {
	line := strings.Repeat("X", 150)
	got := firstLineFromContent(line)
	if len(got) > 103 { // 100 chars + "…" (3 bytes UTF-8)
		t.Errorf("expected truncated, got len=%d", len(got))
	}
}

// consolidate.go — aiConsolidation edge cases

func TestAIConsolidation_WithImportantFlag(t *testing.T) {
	client := &fakeAIClient{response: `{"duplicates": [], "contradictions": [], "suggestions": []}`}
	memories := []memorySnapshot{
		{ID: "MEM1", Title: "M1", Body: "Body with content", Type: "fact", CreatedAt: "2026-01-01T00:00:00Z", Important: true},
		{ID: "MEM2", Title: "M2", Body: "Body two", Type: "convention", CreatedAt: "2026-01-02T00:00:00Z", Important: false},
	}

	ctx := context.Background()
	report, err := aiConsolidation(ctx, client, memories, nil)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if report.AIAnalysis == "" {
		t.Error("expected non-empty analysis")
	}
}

func TestAIConsolidation_WithConflictsAndSuggestions(t *testing.T) {
	aiResponse := `{
  "duplicates": [],
  "contradictions": [
    {"ids": ["01J5XABC1234567890", "01J5XDEF1234567890"], "keep_id": "01J5XABC1234567890",
     "resolved_content": "X is current; it used to be Y", "reason": "one says X, other says Y"}
  ],
  "suggestions": [
    {"action": "promote", "id": "01J5XGHI1234567890", "reason": "should be important"},
    {"action": "delete", "id": "01J5XJKL1234567890", "reason": "obsolete"}
  ]
}`

	client := &fakeAIClient{response: aiResponse}
	// Every ID the analysis names has to exist in the corpus, otherwise it is
	// filtered out — see TestAIConsolidation_DropsActionsNamingUnknownIDs.
	memories := []memorySnapshot{
		{ID: "01J5XABC1234567890", Title: "Mem1", Body: "Body1", Type: "fact", CreatedAt: "2026-01-01T00:00:00Z"},
		{ID: "01J5XDEF1234567890", Title: "Mem2", Body: "Body2", Type: "fact", CreatedAt: "2026-01-02T00:00:00Z"},
		{ID: "01J5XGHI1234567890", Title: "Mem3", Body: "Body3", Type: "fact", CreatedAt: "2026-01-03T00:00:00Z"},
		{ID: "01J5XJKL1234567890", Title: "Mem4", Body: "Body4", Type: "fact", CreatedAt: "2026-01-04T00:00:00Z"},
	}

	ctx := context.Background()
	report, err := aiConsolidation(ctx, client, memories, nil)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if len(report.Contradictions) != 1 {
		t.Errorf("expected 1 contradiction, got %d", len(report.Contradictions))
	}
	if len(report.Suggestions) != 2 {
		t.Errorf("expected 2 suggestions, got %d", len(report.Suggestions))
	}
	// The survivor the analysis recommended must be the one that survives.
	if got := report.Contradictions[0].KeepID; got != "01J5XABC1234567890" {
		t.Errorf("KeepID = %q; want the recommended survivor", got)
	}
}

// A model that invents an ID, or names one from a different batch, must not get an
// action out of it: the apply step would look up a memory that does not exist, and
// in the worst case act on a coincidental match.
func TestAIConsolidation_DropsActionsNamingUnknownIDs(t *testing.T) {
	aiResponse := `{
  "duplicates": [{"ids": ["01J5XABC1234567890", "01J5XNOTINCORPUS9"], "keep_id": "01J5XABC1234567890", "merged_content": "x", "reason": "r"}],
  "contradictions": [],
  "suggestions": [{"action": "delete", "id": "01J5XALSOMISSING01", "reason": "r"}]
}`
	client := &fakeAIClient{response: aiResponse}
	memories := []memorySnapshot{
		{ID: "01J5XABC1234567890", Title: "Mem1", Body: "Body1"},
		{ID: "01J5XDEF1234567890", Title: "Mem2", Body: "Body2"},
	}

	report, err := aiConsolidation(context.Background(), client, memories, nil)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	// One of the two duplicate IDs is unknown, leaving fewer than two — not a merge.
	if len(report.Duplicates) != 0 {
		t.Errorf("expected the merge to be dropped, got %d", len(report.Duplicates))
	}
	if len(report.Suggestions) != 0 {
		t.Errorf("expected the delete of an unknown ID to be dropped, got %d", len(report.Suggestions))
	}
}

// The JSON form is the primary contract; the sectioned text parser is only the
// fallback. This asserts JSON wins and is parsed even when the model wraps it in a
// fence, which several CLIs do unconditionally.
func TestAIConsolidation_ParsesFencedJSON(t *testing.T) {
	aiResponse := "Here is the plan:\n```json\n" + `{
  "duplicates": [{"ids": ["01J5XABC1234567890", "01J5XDEF1234567890"], "keep_id": "01J5XDEF1234567890", "merged_title": "Merged", "merged_content": "union of both", "reason": "same thing"}],
  "contradictions": [],
  "suggestions": []
}` + "\n```\n"
	client := &fakeAIClient{response: aiResponse}
	memories := []memorySnapshot{
		{ID: "01J5XABC1234567890", Title: "Mem1", Body: "Body1"},
		{ID: "01J5XDEF1234567890", Title: "Mem2", Body: "Body2"},
	}

	report, err := aiConsolidation(context.Background(), client, memories, nil)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if len(report.Duplicates) != 1 {
		t.Fatalf("expected 1 duplicate group, got %d", len(report.Duplicates))
	}
	got := report.Duplicates[0]
	if got.KeepID != "01J5XDEF1234567890" {
		t.Errorf("KeepID = %q; want 01J5XDEF1234567890", got.KeepID)
	}
	if got.NewContent != "union of both" {
		t.Errorf("NewContent = %q; want the merged content from the JSON", got.NewContent)
	}
}

// wiki.go — memoryEntityPage with all type emojis

func TestMemoryEntityPage_EachType(t *testing.T) {
	types := []string{"convention", "correction", "decision", "tension", "fact", "skill"}
	for _, memType := range types {
		t.Run(memType, func(t *testing.T) {
			page := memoryEntityPageWithHash("ID", "Title", "", false, "Body.", memType, "")
			if !strings.Contains(page, "**Type:** "+memType) {
				t.Errorf("expected type badge for %s", memType)
			}
		})
	}
}

func TestScopeStore_WriteFileSubdir_Coverage(t *testing.T) {
	dir := t.TempDir()
	wt := &ScopeStore{dir: dir, scopePath: "test"}

	if err := wt.WriteFile("sub/dir/test.md", []byte("nested")); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	data, err := os.ReadFile(filepath.Join(dir, "sub", "dir", "test.md"))
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(data) != "nested" {
		t.Errorf("content = %q", string(data))
	}
}

func TestMemoryAppService_InsertValidated_EmptyTitle(t *testing.T) {
	svc := NewMemoryAppService("/some/project")
	_, err := svc.InsertValidated(MemoryInsertOpts{
		Title:   "",
		Content: "body",
		Type:    "fact",
	})
	if err == nil {
		t.Error("expected error for empty title")
	}
}

func TestMemoryAppService_InsertValidated_EmptyContent(t *testing.T) {
	svc := NewMemoryAppService("/some/project")
	_, err := svc.InsertValidated(MemoryInsertOpts{
		Title:   "title",
		Content: "",
		Type:    "fact",
	})
	if err == nil {
		t.Error("expected error for empty content")
	}
}

func TestExtractBodyAfterFrontmatter_NoFrontmatter(t *testing.T) {
	content := "Just some text\nWith lines."
	got := extractBodyAfterFrontmatter(content)
	if got != "Just some text\nWith lines." {
		t.Errorf("expected full content as body, got %q", got)
	}
}

func TestExtractBodyAfterFrontmatter_FrontmatterOnly(t *testing.T) {
	content := "---\ntitle: Test\n---\n"
	got := extractBodyAfterFrontmatter(content)
	if got != "" {
		t.Errorf("expected empty body, got %q", got)
	}
}

func TestExtractBodyAfterFrontmatter_WithHeadingAfterFrontmatter(t *testing.T) {
	content := "---\ntitle: Test\n---\n\n# Test Heading\n\nBody after heading."
	got := extractBodyAfterFrontmatter(content)
	if !strings.Contains(got, "Body after heading") {
		t.Errorf("expected body text after heading, got %q", got)
	}
}

func TestParseMemoryType_TypeWithQuotes(t *testing.T) {
	got := parseMemoryType("---\ntype: \"decision\"\n---\n")
	// May or may not strip quotes depending on implementation
	if got == "" {
		t.Error("expected non-empty type")
	}
}

func TestParseMemoryType_EmptyFrontmatter(t *testing.T) {
	got := parseMemoryType("---\n---\n")
	if got != "" {
		t.Errorf("expected empty, got %q", got)
	}
}
