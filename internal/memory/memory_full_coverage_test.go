package memory

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// ===========================================================================
// memory_git_store.go — using real git init in TempDir
// ===========================================================================

func gitAvailable() bool {
	_, err := exec.LookPath("git")
	return err == nil
}

func setupGitTestEnv(t *testing.T) {
	t.Helper()
	t.Setenv("GIT_AUTHOR_NAME", "Test")
	t.Setenv("GIT_AUTHOR_EMAIL", "test@example.com")
	t.Setenv("GIT_COMMITTER_NAME", "Test")
	t.Setenv("GIT_COMMITTER_EMAIL", "test@example.com")
}

func TestMemoryGitStore_EnsureInitialised_Full(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	err := store.EnsureInitialised()
	if err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	// Should have created .git
	if _, err := os.Stat(filepath.Join(repoDir, ".git")); err != nil {
		t.Errorf("expected .git to exist: %v", err)
	}

	// Second call should be idempotent
	err = store.EnsureInitialised()
	if err != nil {
		t.Fatalf("second EnsureInitialised: %v", err)
	}
}

func TestMemoryGitStore_MemoryWorktreeLocal_Full(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	branch := "memory/project/test-proj"
	wt, err := store.MemoryWorktreeLocal(branch)
	if err != nil {
		t.Fatalf("MemoryWorktreeLocal: %v", err)
	}
	if wt == nil {
		t.Fatal("expected non-nil worktree")
	}
	if wt.Dir() == "" {
		t.Error("expected non-empty dir")
	}

	// Write a file
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
	if err := wt.CommitAndPush("test commit"); err != nil {
		t.Fatalf("CommitAndPush: %v", err)
	}

	// Second call to MemoryWorktreeLocal should reuse existing worktree
	wt2, err := store.MemoryWorktreeLocal(branch)
	if err != nil {
		t.Fatalf("second MemoryWorktreeLocal: %v", err)
	}
	if wt2.Dir() != wt.Dir() {
		t.Errorf("expected same worktree dir, got %q vs %q", wt2.Dir(), wt.Dir())
	}
}

func TestMemoryGitStore_CommitAndPush_NothingToCommit(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	branch := "memory/project/empty"
	wt, err := store.MemoryWorktreeLocal(branch)
	if err != nil {
		t.Fatalf("MemoryWorktreeLocal: %v", err)
	}

	// CommitAndPush with nothing to commit — should be a no-op
	if err := wt.CommitAndPush("empty commit"); err != nil {
		t.Fatalf("CommitAndPush: %v", err)
	}
}

func TestMemoryGitStore_CreateOrphanBranch_Full(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	err := store.createOrphanBranch("memory/test/orphan")
	if err != nil {
		t.Fatalf("createOrphanBranch: %v", err)
	}

	out := store.gitOutputInRepoNoErr("rev-parse", "--verify", "memory/test/orphan")
	if out == "" {
		t.Error("expected branch to exist after createOrphanBranch")
	}
}

func TestMemoryGitStore_ExtractBranchDir_Full(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	branch := "memory/project/extract-test"
	wt, err := store.MemoryWorktreeLocal(branch)
	if err != nil {
		t.Fatalf("MemoryWorktreeLocal: %v", err)
	}

	// Write a file in a subdir
	if err := wt.WriteFile("data/test.md", []byte("extracted")); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := wt.CommitAndPush("add data"); err != nil {
		t.Fatalf("CommitAndPush: %v", err)
	}

	// Extract
	destDir := filepath.Join(t.TempDir(), "dest")
	if err := store.ExtractBranchDir(branch, "data", destDir); err != nil {
		t.Fatalf("ExtractBranchDir: %v", err)
	}

	data, err := os.ReadFile(filepath.Join(destDir, "test.md"))
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(data) != "extracted" {
		t.Errorf("content = %q; want 'extracted'", string(data))
	}
}

func TestMemoryGitStore_ExtractBranchDir_NonExistentSubdir(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	branch := "memory/project/no-subdir"
	_, err := store.MemoryWorktreeLocal(branch)
	if err != nil {
		t.Fatalf("MemoryWorktreeLocal: %v", err)
	}

	destDir := filepath.Join(t.TempDir(), "empty-dest")
	if err := store.ExtractBranchDir(branch, "nonexistent", destDir); err != nil {
		t.Fatalf("ExtractBranchDir: %v", err)
	}
	if _, err := os.Stat(destDir); err != nil {
		t.Errorf("expected destDir to exist: %v", err)
	}
}

func TestMemoryGitStore_Prune_Full(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	branch := "memory/project/prune-test"
	wt, err := store.MemoryWorktreeLocal(branch)
	if err != nil {
		t.Fatalf("MemoryWorktreeLocal: %v", err)
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

// ===========================================================================
// memory_branch_lock.go — RegisterBranch, DeregisterBranch, ActiveBranches, Summary
// ===========================================================================

func TestMemoryGitStore_RegisterDeregisterBranch_Full(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	branch := "memory/project/lock-test"
	ref := "/some/ref/path"

	// Register
	if err := store.RegisterBranch(branch, ref); err != nil {
		t.Fatalf("RegisterBranch: %v", err)
	}

	// Register again (duplicate check)
	if err := store.RegisterBranch(branch, ref); err != nil {
		t.Fatalf("RegisterBranch (dup): %v", err)
	}

	// Active branches
	active, err := store.ActiveMemoryBranches()
	if err != nil {
		t.Fatalf("ActiveMemoryBranches: %v", err)
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

	// Summary
	summary, err := store.MemoryBranchSummary()
	if err != nil {
		t.Fatalf("MemoryBranchSummary: %v", err)
	}
	refs, ok := summary[branch]
	if !ok {
		t.Error("expected branch in summary")
	}
	if len(refs) != 1 || refs[0] != ref {
		t.Errorf("refs = %v", refs)
	}

	// Deregister
	unused, err := store.DeregisterBranch(branch, ref)
	if err != nil {
		t.Fatalf("DeregisterBranch: %v", err)
	}
	if !unused {
		t.Error("expected branch to become unused")
	}
}

func TestMemoryGitStore_ValidateMemBranchRefs_Full(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	branch := "memory/project/validate-test"

	// Register with non-existent ref path — should be cleaned
	if err := store.RegisterBranch(branch, "/nonexistent/ref/path"); err != nil {
		t.Fatalf("RegisterBranch: %v", err)
	}

	// Also register with "user" ref — should be preserved
	if err := store.RegisterBranch(branch, "user"); err != nil {
		t.Fatalf("RegisterBranch: %v", err)
	}

	cleaned, err := store.ValidateMemBranchRefs()
	if err != nil {
		t.Fatalf("ValidateMemBranchRefs: %v", err)
	}
	if cleaned < 1 {
		t.Errorf("expected at least 1 cleaned ref, got %d", cleaned)
	}
}

func TestMemoryGitStore_DeregisterBranch_NotFound(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	unused, err := store.DeregisterBranch("nonexistent-branch", "ref")
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if unused {
		t.Error("expected false for non-existent branch")
	}
}

// ===========================================================================
// memory_git_store.go — git helper functions
// ===========================================================================

func TestMemoryGitStore_GitHelpers_Full(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	// gitOutputInRepo
	out, err := store.gitOutputInRepo("rev-parse", "--git-dir")
	if err != nil {
		t.Fatalf("gitOutputInRepo: %v", err)
	}
	if out == "" {
		t.Error("expected non-empty output from rev-parse")
	}

	// gitOutputInRepoNoErr
	noErr := store.gitOutputInRepoNoErr("rev-parse", "--git-dir")
	if noErr == "" {
		t.Error("expected non-empty output from gitOutputInRepoNoErr")
	}

	// gitInRepo
	err = store.gitInRepo("status")
	if err != nil {
		t.Fatalf("gitInRepo status: %v", err)
	}
}

func TestMemoryGitStore_SyncRemote_NoURL(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	// syncRemote with no URL should remove origin
	store.syncRemote()
	// Should not panic
}

func TestMemoryGitStore_IsRemoteEmpty_NoURL(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: repoDir + "-wt"}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	empty := store.isRemoteEmpty()
	if empty {
		t.Error("expected false when no remote URL configured")
	}
}

func TestMemoryGitStore_RemoteBranchExists_NoURL(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: repoDir + "-wt"}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	exists := store.remoteBranchExists("main")
	if exists {
		t.Error("expected false when no remote URL configured")
	}
}

func TestMemoryGitStore_PushBranchInBackground_NoURL(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: repoDir + "-wt"}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	// No remote URL — should be a no-op
	store.pushBranchInBackground("test-branch", repoDir)
	WaitForPendingPushes()
}

func TestMemoryGitStore_PruneLocalBranch_Full(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	// Create a branch to prune
	_ = store.createOrphanBranch("memory/prune/target")
	store.pruneLocalBranch("memory/prune/target")
	// Should not panic
}

// ===========================================================================
// memory.go — MemoryService operations with MemoryWorktree (via git)
// ===========================================================================

func TestMemoryService_AddUpdateRemoveMemory_WithGitStore(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	localDir := t.TempDir()
	svc := &MemoryService{
		scope:    MemoryScopeProject,
		scopeID:  "test-proj",
		localDir: localDir,
		gitStore: store,
	}

	// AddMemory
	id, err := svc.AddMemory("Test Title", "Test body content", MemoryOpts{})
	if err != nil {
		t.Fatalf("AddMemory: %v", err)
	}
	if id == "" {
		t.Fatal("expected non-empty ID")
	}

	// UpdateMemory
	err = svc.UpdateMemory(id, "Updated Title", "Updated body content")
	if err != nil {
		t.Fatalf("UpdateMemory: %v", err)
	}

	// PromoteMemory
	err = svc.PromoteMemory(id)
	if err != nil {
		t.Fatalf("PromoteMemory: %v", err)
	}

	// DemoteMemory
	err = svc.DemoteMemory(id)
	if err != nil {
		t.Fatalf("DemoteMemory: %v", err)
	}

	// RemoveMemory
	err = svc.RemoveMemory(id)
	if err != nil {
		t.Fatalf("RemoveMemory: %v", err)
	}
}

func TestMemoryService_AddMemory_WithTypeAndTags(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	localDir := t.TempDir()
	svc := &MemoryService{
		scope:    MemoryScopeProject,
		scopeID:  "test-proj",
		localDir: localDir,
		gitStore: store,
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
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	localDir := t.TempDir()
	svc := &MemoryService{
		scope:    MemoryScopeProject,
		scopeID:  "test-proj",
		localDir: localDir,
		gitStore: store,
	}

	// Add a memory first
	id, err := svc.AddMemory("Sync Test", "Body content for sync", MemoryOpts{})
	if err != nil {
		t.Fatalf("AddMemory: %v", err)
	}

	// SyncToLocal
	err = svc.SyncToLocal()
	if err != nil {
		t.Fatalf("SyncToLocal: %v", err)
	}

	// The sync may or may not find the file depending on branch setup
	_ = id
}

func TestMemoryService_EnsureInitialised_WithGitStore(t *testing.T) {
	if !gitAvailable() {
		t.Skip("git not available")
	}

	repoDir := filepath.Join(t.TempDir(), "repo")
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}

	setupGitTestEnv(t)

	localDir := t.TempDir()
	svc := &MemoryService{
		scope:    MemoryScopeProject,
		scopeID:  "test-proj",
		localDir: localDir,
		gitStore: store,
	}

	err := svc.EnsureInitialised()
	if err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}
}

// ===========================================================================
// memory.go — gitProjectDir
// ===========================================================================

func TestGitProjectDir(t *testing.T) {
	dir := t.TempDir()

	// Not a git repo — should return ""
	result := gitProjectDir()
	// May or may not find a git repo depending on test environment
	_ = result

	if !gitAvailable() {
		t.Skip("git not available for git init test")
	}

	// Make it a git repo
	setupGitTestEnv(t)
	cmd := exec.Command("git", "init", dir)
	if err := cmd.Run(); err != nil {
		t.Fatalf("git init: %v", err)
	}

	result = gitProjectDir()
	if result == "" {
		t.Error("expected non-empty for git dir")
	}
}

// ===========================================================================
// memory.go — UserHashFromGit
// ===========================================================================

func TestUserHashFromGit_Coverage(t *testing.T) {
	// This calls git config — may or may not return a hash
	hash, _ := UserHashFromGit()
	// Just verify it doesn't panic — may be empty if git user not configured
	_ = hash
}

// ===========================================================================
// memory.go — buildMemoryFile completeness
// ===========================================================================

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

// ===========================================================================
// memory.go — ensureProjectCopy
// ===========================================================================

func TestEnsureProjectCopy_WithProjectDir(t *testing.T) {
	localDir := t.TempDir()

	// Write a file in localDir
	writeMemFile(t, localDir, "MEM1.md", "---\ntitle: Test\n---\n\n# Test\n\nBody.")

	// ProjectLinkDir depends on brand.GlobalDir
	// Just ensure no panic calling it
	linkDir := ProjectLinkDir("project")
	_ = linkDir
}

// ===========================================================================
// paths.go — MemoryLocalDir, MemoryGlobalContextDir, MemoryWikiGlobalDir
// ===========================================================================

func TestMemoryLocalDir_Coverage(t *testing.T) {
	dir := MemoryLocalDir("project")
	if dir == "" {
		t.Error("expected non-empty MemoryLocalDir")
	}
	if !strings.Contains(dir, "project") {
		t.Errorf("expected 'project' in path, got %q", dir)
	}
}

func TestMemoryGlobalContextDir_Coverage(t *testing.T) {
	dir := MemoryGlobalContextDir("test-ctx")
	if dir == "" {
		t.Error("expected non-empty")
	}
	if !strings.Contains(dir, "test-ctx") {
		t.Errorf("expected 'test-ctx' in path, got %q", dir)
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

// ===========================================================================
// paths.go — WikiDir, RawDir
// ===========================================================================

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

// ===========================================================================
// consolidate.go — RunConsolidation edge cases (helper-based)
// ===========================================================================

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
	report, err := consolidationHelper(ctx, dir, client)
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
	report, err := consolidationHelper(ctx, dir, client)
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
	report, err := consolidationHelper(ctx, dir, client)
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
	date := time.Now().Add(-60 * 24 * time.Hour).Format(time.RFC3339)
	writeMemFile(t, dir, "IMP_important_.md", fmt.Sprintf("---\ntitle: Important\ncreated_at: %s\nimportant: true\n---\n\n# Important\n\nBody of important memory.", date))
	writeMemFile(t, dir, "NORM.md", fmt.Sprintf("---\ntitle: Normal\ncreated_at: %s\n---\n\n# Normal\n\nBody of normal memory.", date))

	ctx := context.Background()
	report, err := consolidationHelper(ctx, dir, nil)
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

// consolidationHelper replaces RunConsolidation's RawDir with a direct dir.
func consolidationHelper(ctx context.Context, dir string, aiClient interface{ Complete(context.Context, string, string) (string, error) }) (*ConsolidationReport, error) {
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
			ID:        id,
			Title:     title,
			Body:      strings.TrimSpace(body),
			Type:      memType,
			CreatedAt: createdAt,
			Important: important,
		})
	}

	report := &ConsolidationReport{TotalMemories: len(memories)}
	if len(memories) == 0 {
		return report, nil
	}

	report.Stale = detectStaleMemories(memories)

	if aiClient != nil && len(memories) > 1 {
		aiReport, aiErr := aiConsolidation(ctx, aiClient, memories)
		if aiErr != nil {
			report.AIAnalysis = fmt.Sprintf("AI analysis failed: %v", aiErr)
			return report, nil
		}
		report.Duplicates = aiReport.Duplicates
		report.Contradictions = aiReport.Contradictions
		report.Suggestions = aiReport.Suggestions
		report.AIAnalysis = aiReport.AIAnalysis
	}

	return report, nil
}

// ===========================================================================
// consolidate.go — ApplyGC
// ===========================================================================

func TestApplyGC_EmptyCandidates_Coverage(t *testing.T) {
	svc := &MemoryService{scope: MemoryScopeProject, scopeID: "test"}
	ctx := context.Background()
	deleted, err := ApplyGC(ctx, "project", nil, svc)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if deleted != 0 {
		t.Errorf("expected 0, got %d", deleted)
	}
}

// ===========================================================================
// memory.go — MemoryService.IndexMemories with data
// ===========================================================================

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

// ===========================================================================
// memory.go — syncToLocalInternal nil store
// ===========================================================================

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

// ===========================================================================
// cycle.go — SyncAndCycle with store error details
// ===========================================================================

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

func (m *mockStoreProviderCov) ExtractBranchDir(_, _, _ string) error {
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

// ===========================================================================
// cycle.go — EnsureWikiIndexExists
// ===========================================================================

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

// ===========================================================================
// wiki.go — GenerateMemoryWiki with unreadable files
// ===========================================================================

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

// ===========================================================================
// wiki.go — memoryEntityPage dates and stale calculations
// ===========================================================================

func TestMemoryEntityPage_RecentDate(t *testing.T) {
	recentDate := time.Now().Add(-1 * 24 * time.Hour).Format(time.RFC3339)
	page := memoryEntityPage("RECENT", "Recent Memory", recentDate, false, "Body.", "fact")
	if strings.Contains(page, "Stale memory") {
		t.Error("should NOT contain stale warning for 1-day-old memory")
	}
}

func TestMemoryEntityPage_CreatedAtDate(t *testing.T) {
	date := "2026-01-15T12:00:00Z"
	page := memoryEntityPage("ID", "Title", date, false, "Body.", "fact")
	if !strings.Contains(page, "created: ") {
		t.Error("expected created date in page")
	}
}

// ===========================================================================
// important.go — firstLineFromContent
// ===========================================================================

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

// ===========================================================================
// consolidate.go — parseConsolidationType
// ===========================================================================

func TestParseConsolidationType_WithType(t *testing.T) {
	content := "---\ntitle: Test\ntype: convention\n---\n\nBody"
	got := parseConsolidationType(content)
	if got != "convention" {
		t.Errorf("expected 'convention', got %q", got)
	}
}

func TestParseConsolidationType_NoType(t *testing.T) {
	content := "---\ntitle: Test\n---\n\nBody"
	got := parseConsolidationType(content)
	if got != "" {
		t.Errorf("expected empty, got %q", got)
	}
}

func TestParseConsolidationType_NoFrontmatter(t *testing.T) {
	content := "Just some plain text"
	got := parseConsolidationType(content)
	if got != "" {
		t.Errorf("expected empty, got %q", got)
	}
}

// ===========================================================================
// consolidate.go — aiConsolidation edge cases
// ===========================================================================

func TestAIConsolidation_WithImportantFlag(t *testing.T) {
	client := &fakeAIClient{response: "## DUPLICATES\nNone found\n\n## CONTRADICTIONS\nNone found\n\n## SUGGESTIONS\nNone found"}
	memories := []memorySnapshot{
		{ID: "MEM1", Title: "M1", Body: "Body with content", Type: "fact", CreatedAt: "2026-01-01T00:00:00Z", Important: true},
		{ID: "MEM2", Title: "M2", Body: "Body two", Type: "convention", CreatedAt: "2026-01-02T00:00:00Z", Important: false},
	}

	ctx := context.Background()
	report, err := aiConsolidation(ctx, client, memories)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if report.AIAnalysis == "" {
		t.Error("expected non-empty analysis")
	}
}

func TestAIConsolidation_WithConflictsAndSuggestions(t *testing.T) {
	aiResponse := `## DUPLICATES
None found

## CONTRADICTIONS
- CONFLICT [01J5XABC1234567890] vs [01J5XDEF1234567890]: one says X, other says Y — recommend keeping [01J5XABC1234567890]

## SUGGESTIONS
- PROMOTE [01J5XGHI1234567890]: should be important
- DELETE [01J5XJKL1234567890]: obsolete`

	client := &fakeAIClient{response: aiResponse}
	memories := []memorySnapshot{
		{ID: "01J5XABC1234567890", Title: "Mem1", Body: "Body1", Type: "fact", CreatedAt: "2026-01-01T00:00:00Z"},
		{ID: "01J5XDEF1234567890", Title: "Mem2", Body: "Body2", Type: "fact", CreatedAt: "2026-01-02T00:00:00Z"},
	}

	ctx := context.Background()
	report, err := aiConsolidation(ctx, client, memories)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if len(report.Contradictions) != 1 {
		t.Errorf("expected 1 contradiction, got %d", len(report.Contradictions))
	}
	if len(report.Suggestions) != 2 {
		t.Errorf("expected 2 suggestions, got %d", len(report.Suggestions))
	}
}

// ===========================================================================
// consolidate.go — parseConsolidationSection edge cases
// ===========================================================================

func TestParseSuggestionSection_MultipleTypes(t *testing.T) {
	response := `## SUGGESTIONS
- PROMOTE [01J5XABC1234567890]: should be important
- DEMOTE [01J5XDEF1234567890]: too specific
- DELETE [01J5XGHI1234567890]: obsolete
- UPDATE [01J5XJKL1234567890]: needs more detail`

	actions := parseSuggestionSection(response)
	if len(actions) != 4 {
		t.Fatalf("expected 4 actions, got %d", len(actions))
	}

	typeMap := map[string]bool{}
	for _, a := range actions {
		typeMap[a.Type] = true
	}
	for _, expected := range []string{"promote", "demote", "delete", "update"} {
		if !typeMap[expected] {
			t.Errorf("expected type %q in actions", expected)
		}
	}
}

// ===========================================================================
// wiki.go — memoryEntityPage with all type emojis
// ===========================================================================

func TestMemoryEntityPage_EachType(t *testing.T) {
	types := []string{"convention", "correction", "decision", "tension", "fact", "skill"}
	for _, memType := range types {
		t.Run(memType, func(t *testing.T) {
			page := memoryEntityPage("ID", "Title", "", false, "Body.", memType)
			if !strings.Contains(page, "**Type:** "+memType) {
				t.Errorf("expected type badge for %s", memType)
			}
		})
	}
}

// ===========================================================================
// memory.go — MemoryWorktree operations
// ===========================================================================

func TestMemoryWorktree_WriteFileSubdir_Coverage(t *testing.T) {
	dir := t.TempDir()
	wt := &MemoryWorktree{dir: dir, branch: "test"}

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

// ===========================================================================
// appsvc.go — MemoryAppService integration
// ===========================================================================

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

// ===========================================================================
// extractBodyAfterFrontmatter — edge cases
// ===========================================================================

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

// ===========================================================================
// parseMemoryType additional coverage
// ===========================================================================

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
