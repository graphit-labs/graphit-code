//go:build lancedb

package memory

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// ScopeStore (filesystem-only, no real git)

// MemoryStore helpers (filesystem parts)

func TestScopeDir(t *testing.T) {
	store := &MemoryStore{tableBase: "/wt-base"}
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
	store := &MemoryStore{tableBase: "/test/repo"}
	if store.Dir() != "/test/repo" {
		t.Errorf("Dir() = %q; want '/test/repo'", store.Dir())
	}
}

// copyDirRecursive: error on filepath.Walk (missing rel path)

func TestMemoryBranchLockFileOps(t *testing.T) {
	// Override globalDir so all paths resolve to our temp dir
	dir := t.TempDir()
	origHome := os.Getenv("HOME")
	t.Setenv("HOME", dir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	store := &MemoryStore{tableBase: filepath.Join(dir, "wt")}

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

	store := &MemoryStore{tableBase: filepath.Join(dir, "wt")}
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

	store := &MemoryStore{tableBase: filepath.Join(dir, "wt")}

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

	store := &MemoryStore{tableBase: filepath.Join(dir, "wt")}

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

func TestMemoryService_EnsureInitialised_NilStore(t *testing.T) {
	svc := &MemoryService{
		scope:   MemoryScopeProject,
		scopeID: "test",
	}
	// With a nil store, the post-write wiki sync may fail but the completed write is not rolled back.
	err := svc.EnsureInitialised()
	if err != nil {
		t.Errorf("EnsureInitialised should not return error: %v", err)
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

// AllContextDirs — the set of compiled memory WIKIS is the record of imported memory contexts.
//
// It used to enumerate the raw markdown store, and when that root stopped existing this answered
// empty rather than failing: `os.ReadDir` on a missing directory and on a machine with no imported
// contexts are the same answer. So the guard asserts through the store helpers rather than against
// literals, and it asserts the ABSENCE of the two roots that must NOT decide this — the retired raw
// store, and the local table root, which holds nothing at all when a bucket is configured.
func TestAllContextDirs(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv("GRAPHIT_HUB_BUCKET", "acme-hub")

	mkdirs := func(dirs ...string) {
		for _, d := range dirs {
			if err := os.MkdirAll(d, 0o755); err != nil {
				t.Fatal(err)
			}
		}
	}

	// Neither of the roots that must not decide this may produce a context, even fully populated.
	// With a bucket configured the table root is where a context has NO local directory at all.
	mkdirs(
		filepath.Join(home, brand.DotDir(), "memory-raw", "memory-raw-context-raw-context"),
		filepath.Join(home, brand.DotDir(), "memory-table", "memory-table-context-table-context"),
	)
	if got := AllContextDirs(); len(got) != 0 {
		t.Fatalf("AllContextDirs = %v, want none — only the compiled wiki is the record", got)
	}

	// The wiki layout: `wiki/memory/<scope>/<id>`, where a context is the scope whose halves match.
	mkdirs(
		store.MemoryWikiDir("project", "01ACME"),
		store.MemoryWikiDir("user", "abc123"),
		store.MemoryWikiDir("my-context", "my-context"),
		store.MemoryWikiDir("half-installed", "something-else"),
	)
	got := AllContextDirs()
	if len(got) != 1 || got[0] != "my-context" {
		t.Errorf("AllContextDirs = %v, want [my-context]", got)
	}
}

// A context service must compile into the SAME wiki directory that every reader of a context opens.
// It did not: the wiki was named from the scope WORD, which for a context is the literal "context",
// so the compile wrote `wiki/memory/context/<name>` and the readers looked in
// `wiki/memory/<name>/<name>`. Nothing failed — the wiki simply went somewhere nobody reads.
func TestAContextServiceCompilesIntoTheDirectoryItsReadersOpen(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)

	svc := NewMemoryServiceForContext("shared-notes", nil)
	want := MemoryWikiGlobalDir("shared-notes", "shared-notes")
	if got := svc.WikiDir(); got != want {
		t.Errorf("context wiki dir = %q, want %q", got, want)
	}
	if strings.Contains(filepath.ToSlash(svc.WikiDir()), "/memory/context/") {
		t.Error("the wiki was named from the scope word, so no reader will find it")
	}

	// And the same directory is what the listing recognises as an installed context.
	if err := os.MkdirAll(svc.WikiDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	if got := AllContextDirs(); len(got) != 1 || got[0] != "shared-notes" {
		t.Errorf("AllContextDirs = %v, want [shared-notes] — the compile and the listing disagree", got)
	}
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

func TestConsolidationOfAnEmptyCorpusProposesNothing(t *testing.T) {
	report := consolidateSnapshots(context.Background(), nil, nil, nil)
	if report.TotalMemories != 0 {
		t.Errorf("expected 0, got %d", report.TotalMemories)
	}
	if report.HasActions() {
		t.Error("an empty corpus must propose nothing")
	}
}

// The analysis is handed its corpus, which is why it does not care where the memories were stored.
// It used to be reached through a directory loader — the same shape RunConsolidation had before it
// read the table — so these cases went through markdown files in a temp dir.
func TestStaleDetectionSkipsImportantMemories(t *testing.T) {
	oldDate := time.Now().Add(-120 * 24 * time.Hour).Format(time.RFC3339)

	report := consolidateSnapshots(context.Background(), []memorySnapshot{
		{ID: "MEM1", Title: "Normal Memory", Body: "This is a normal memory body.", CreatedAt: oldDate},
		{ID: "MEM2", Title: "Important Memory", Body: "Important body.", CreatedAt: oldDate, Important: true},
	}, nil, nil)

	if report.TotalMemories != 2 {
		t.Errorf("TotalMemories = %d; want 2", report.TotalMemories)
	}
	if len(report.Stale) != 1 {
		t.Errorf("Stale count = %d; want 1 — an important memory is not flagged for age", len(report.Stale))
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

// Compiling an imported context needs only its NAME.
//
// This replaces three tests that differed only in the store provider they passed — nil, a working
// one, and one that failed to extract. That parameter is gone with the download it drove: a context's
// memories are a table at a prefix derived from its name, so there is nothing to provide.
func TestSyncContextFromMemoryRepoNeedsOnlyTheName(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "")
	t.Setenv("HOME", t.TempDir())
	result := SyncContextFromMemoryRepo(context.Background(), "test-context")
	if result == nil {
		t.Fatal("expected non-nil result")
	}
	if result.Err != nil {
		t.Errorf("compiling an empty context must not error: %v", result.Err)
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
	t.Setenv("GRAPHIT_HUB_BUCKET", "")
	t.Setenv("HOME", t.TempDir())
	// The completion handle makes temp-directory teardown deterministic.
	<-OnHubImport(ctx, "test-context", nil)
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

func TestNewMemorySvcInternal_WithStore(t *testing.T) {
	store := &MemoryStore{tableBase: "/repo"}
	svc := newMemorySvcInternal(MemoryScopeProject, "id", store)
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

// MemoryFileName, MemoryIDFromFileName, IsImportantContent

func TestIsImportantContent_Coverage(t *testing.T) {
	if !IsImportantContent("---\nimportant: true\n---\n\nbody") {
		t.Error("should be important")
	}
	if IsImportantContent("---\ntitle: t\n---\n\nbody") {
		t.Error("should not be important")
	}
}

func TestMemoryFileName_Coverage(t *testing.T) {
	got := MemoryFileName("abc")
	if got != "abc.md" {
		t.Errorf("MemoryFileName = %q", got)
	}
}

func TestMemoryIDFromFileName_Coverage(t *testing.T) {
	got := MemoryIDFromFileName("abc.md")
	if got != "abc" {
		t.Errorf("MemoryIDFromFileName = %q", got)
	}
}
