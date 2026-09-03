//go:build lancedb

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

// EnsureInitialised has nothing to create, and that is the assertion.
//
// It asserted that the store's local root EXISTS afterwards, back when that root held markdown a
// compile read. Opening a Lance table creates it, so preparing a directory in advance buys nothing —
// and an empty root left behind on every run reads as though the retired store were still a thing.
func TestEnsureInitialisedCreatesNothing(t *testing.T) {
	base := filepath.Join(t.TempDir(), "tables")
	st := &MemoryStore{tableBase: base}
	if err := st.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}
	if _, err := os.Stat(base); !os.IsNotExist(err) {
		t.Errorf("the local table root was created: %v", err)
	}
}

func TestMemoryStore_RegisterDeregisterScope_Full(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{tableBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	branch := "memory/project/lock-test"
	ref := "/some/ref/path"

	if err := store.RegisterScope(branch, ref); err != nil {
		t.Fatalf("RegisterScope: %v", err)
	}

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
	store := &MemoryStore{tableBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	branch := "memory/project/validate-test"

	if err := store.RegisterScope(branch, "/nonexistent/ref/path"); err != nil {
		t.Fatalf("RegisterScope: %v", err)
	}

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
	store := &MemoryStore{tableBase: wtBase}

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

func TestMemoryStore_PruneLocalBranch_Full(t *testing.T) {
	tableBase := filepath.Join(t.TempDir(), "tables")
	store := &MemoryStore{tableBase: tableBase}
	t.Setenv("HOME", t.TempDir())

	const scopePath = "memory/prune/target"
	dir := store.scopeDir(scopePath)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "data.lance"), []byte("x"), 0o600); err != nil {
		t.Fatal(err)
	}

	store.pruneLocalScope(scopePath)

	if _, err := os.Stat(dir); !os.IsNotExist(err) {
		t.Errorf("expected the table directory to be gone, got %v", err)
	}
	if dir != filepath.Join(tableBase, "memory-prune-target") {
		t.Errorf("scopeDir = %q, want the flattened scope path under the table root", dir)
	}
}

func TestMemoryService_AddUpdateRemoveMemory_WithGitStore(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{tableBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	svc := &MemoryService{
		scope:   MemoryScopeProject,
		scopeID: "test-proj",
		store:   store,
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
	store := &MemoryStore{tableBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	svc := &MemoryService{
		scope:   MemoryScopeProject,
		scopeID: "test-proj",
		store:   store,
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

func TestMemoryServiceSyncWiki(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{tableBase: wtBase}

	if err := store.EnsureInitialised(); err != nil {
		t.Fatalf("EnsureInitialised: %v", err)
	}

	svc := &MemoryService{
		scope:   MemoryScopeProject,
		scopeID: "test-proj",
		store:   store,
	}

	id, err := svc.AddMemory("Sync Test", "Body content for sync", MemoryOpts{})
	if err != nil {
		t.Fatalf("AddMemory: %v", err)
	}

	err = svc.SyncWiki()
	if err != nil {
		t.Fatalf("SyncWiki: %v", err)
	}

	_ = id
}

func TestMemoryService_EnsureInitialised_WithGitStore(t *testing.T) {
	wtBase := filepath.Join(t.TempDir(), "wt")
	store := &MemoryStore{tableBase: wtBase}

	svc := &MemoryService{
		scope:   MemoryScopeProject,
		scopeID: "test-proj",
		store:   store,
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
	hash, _ := UserScopeID()
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
	_ = dir
}

func TestRunConsolidation_WithAIClient_Coverage(t *testing.T) {
	now := time.Now().UTC()
	date1 := now.Add(-2 * 24 * time.Hour).Format(time.RFC3339)
	date2 := now.Add(-3 * 24 * time.Hour).Format(time.RFC3339)

	corpus := []memorySnapshot{
		{ID: "MEM1", Title: "Memory One", Body: "Content of memory one.", Type: "fact", CreatedAt: date1},
		{ID: "MEM2", Title: "Memory Two", Body: "Content of memory two.", Type: "fact", CreatedAt: date2},
	}

	aiResponse := `## DUPLICATES
- MERGE [MEM1XXXXXXXXXXX] and [MEM2XXXXXXXXXXX]: Same content

## CONTRADICTIONS
None found

## SUGGESTIONS
- PROMOTE [MEM1XXXXXXXXXXX]: should be important`

	client := &fakeAIClient{response: aiResponse}

	report := consolidateSnapshots(context.Background(), corpus, client, nil)
	if report.TotalMemories != 2 {
		t.Errorf("TotalMemories = %d; want 2", report.TotalMemories)
	}
	if report.AIAnalysis == "" {
		t.Error("expected AIAnalysis to be populated")
	}
}

func TestRunConsolidation_AIClientError_Coverage(t *testing.T) {
	date := time.Now().Add(-2 * 24 * time.Hour).Format(time.RFC3339)
	corpus := []memorySnapshot{
		{ID: "MEM1", Title: "Mem1", Body: "Body 1.", CreatedAt: date},
		{ID: "MEM2", Title: "Mem2", Body: "Body 2.", CreatedAt: date},
	}

	client := &fakeAIClient{err: fmt.Errorf("AI unavailable")}
	report := consolidateSnapshots(context.Background(), corpus, client, nil)
	if !report.AIFailed {
		t.Error("a failed analysis must be flagged, not only described in prose")
	}
	if !strings.Contains(report.AIAnalysis, "AI analysis failed") {
		t.Errorf("expected AI failure message, got %q", report.AIAnalysis)
	}
}

func TestRunConsolidation_SingleMemory_Coverage(t *testing.T) {
	date := time.Now().Add(-2 * 24 * time.Hour).Format(time.RFC3339)
	corpus := []memorySnapshot{{ID: "SINGLE", Title: "Single", Body: "Body.", CreatedAt: date}}

	client := &fakeAIClient{response: "should not be called"}
	report := consolidateSnapshots(context.Background(), corpus, client, nil)
	if report.TotalMemories != 1 {
		t.Errorf("expected 1 memory")
	}
	if report.AIAnalysis != "" {
		t.Errorf("expected empty AIAnalysis for single memory, got %q", report.AIAnalysis)
	}
}

func TestRunConsolidation_ImportantMemory_Coverage(t *testing.T) {
	date := time.Now().Add(-120 * 24 * time.Hour).Format(time.RFC3339)
	corpus := []memorySnapshot{
		{ID: "IMP", Title: "Important", Body: "Body of important memory.", CreatedAt: date, Important: true},
		{ID: "NORM", Title: "Normal", Body: "Body of normal memory.", CreatedAt: date},
	}

	report := consolidateSnapshots(context.Background(), corpus, nil, nil)
	if report.TotalMemories != 2 {
		t.Errorf("expected 2 memories, got %d", report.TotalMemories)
	}
	if len(report.Stale) != 1 {
		t.Errorf("expected 1 stale, got %d", len(report.Stale))
	}
}

type fakeAIClient struct {
	response string
	err      error
}

func (f *fakeAIClient) Complete(_ context.Context, _, _ string) (string, error) {
	return f.response, f.err
}

func TestMemoryService_IndexMemories_Coverage(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "")
	wikiDir := filepath.Join(t.TempDir(), "wiki")

	svc := &MemoryService{
		scope:    MemoryScopeProject,
		scopeID:  "test",
		store:    &MemoryStore{tableBase: filepath.Join(t.TempDir(), "tables")},
		tableURI: filepath.Join(t.TempDir(), "table"),
		wikiDir:  wikiDir,
	}
	if _, err := svc.AddMemory("Test", "Body content.", MemoryOpts{Type: MemoryTypeFact}); err != nil {
		t.Fatalf("AddMemory: %v", err)
	}
	if _, err := svc.AddMemory("Important", "Important body.", MemoryOpts{Important: true}); err != nil {
		t.Fatalf("AddMemory: %v", err)
	}

	if err := svc.IndexMemories(context.Background()); err != nil {
		t.Fatalf("IndexMemories: %v", err)
	}

	live, superseded, _ := indexedMemoryPages(t, wikiDir)
	if live != 2 || superseded != 0 {
		t.Errorf("indexed %d live and %d superseded rows, want 2 and 0", live, superseded)
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
	if len(got) > 103 {
		t.Errorf("expected truncated, got len=%d", len(got))
	}
}

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
