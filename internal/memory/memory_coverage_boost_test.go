package memory

import (
	"os"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// important.go — ListImportantMemories, RenderImportantBlock,
// ListRecentMemories, RenderRecentBlock, firstLineFromContent,
// extractBodyAfterFrontmatter

// ListRecentMemories and RenderRecentBlock — filesystem-based

func TestExtractBodyAfterFrontmatter_AllCases(t *testing.T) {
	tests := []struct {
		name  string
		input string
		want  string
	}{
		{
			"no frontmatter, no h1",
			"Just content\nMore lines",
			"Just content\nMore lines",
		},
		{
			"frontmatter then direct content (no h1)",
			"---\ntitle: Test\n---\n\nDirect content after frontmatter",
			"Direct content after frontmatter",
		},
		{
			"frontmatter then blank lines then h1 then body",
			"---\ntitle: Test\n---\n\n\n# My Title\n\nBody text here",
			"Body text here",
		},
		{
			"frontmatter with h1 then multi-line body",
			"---\ntitle: Test\n---\n\n# Title\n\nLine 1\nLine 2\nLine 3",
			"Line 1\nLine 2\nLine 3",
		},
		{
			"frontmatter only, no body after",
			"---\ntitle: Test\n---",
			"",
		},
		{
			"empty string",
			"",
			"",
		},
		{
			"no frontmatter, has h1",
			"# Title\nBody line",
			"# Title\nBody line",
		},
		{
			"frontmatter with blank h1 gap",
			"---\ntitle: T\n---\n\n\n\n# H1\n\nContent",
			"Content",
		},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := extractBodyAfterFrontmatter(tc.input)
			if got != tc.want {
				t.Errorf("extractBody = %q; want %q", got, tc.want)
			}
		})
	}
}

// consolidate.go — ConsolidationReport methods

func TestConsolidationReport_HasActions_Boost(t *testing.T) {
	r := &ConsolidationReport{}
	if r.HasActions() {
		t.Error("empty report should not have actions")
	}
	if r.TotalActions() != 0 {
		t.Errorf("TotalActions = %d; want 0", r.TotalActions())
	}

	r.Duplicates = []ConsolidationAction{{Type: "merge"}}
	if !r.HasActions() {
		t.Error("report with duplicates should have actions")
	}
	if r.TotalActions() != 1 {
		t.Errorf("TotalActions = %d; want 1", r.TotalActions())
	}

	r.Contradictions = []ConsolidationAction{{Type: "conflict"}}
	r.Stale = []ConsolidationAction{{Type: "update"}}
	r.Suggestions = []ConsolidationAction{{Type: "promote"}}
	if r.TotalActions() != 4 {
		t.Errorf("TotalActions = %d; want 4", r.TotalActions())
	}
}

func TestDetectStaleMemories_AllBranches(t *testing.T) {
	oldDate := time.Now().Add(-120 * 24 * time.Hour).Format(time.RFC3339)
	recentDate := time.Now().Add(-5 * 24 * time.Hour).Format(time.RFC3339)

	memories := []memorySnapshot{
		{ID: "old", Title: "Old One", CreatedAt: oldDate, Important: false},            // stale
		{ID: "recent", Title: "Recent One", CreatedAt: recentDate, Important: false},   // not stale
		{ID: "important", Title: "Important Old", CreatedAt: oldDate, Important: true}, // skipped (important)
		{ID: "nodate", Title: "No Date", CreatedAt: "", Important: false},              // skipped (no date)
		{ID: "baddate", Title: "Bad Date", CreatedAt: "invalid", Important: false},     // skipped (bad date format)
	}

	stale := detectStaleMemories(memories)
	if len(stale) != 1 {
		t.Fatalf("expected 1 stale, got %d", len(stale))
	}
	if stale[0].MemoryIDs[0] != "old" {
		t.Errorf("stale ID = %q; want 'old'", stale[0].MemoryIDs[0])
	}
	if !strings.Contains(stale[0].Reason, "days old") {
		t.Errorf("Reason should mention days old, got %q", stale[0].Reason)
	}
}

func TestValidMemoryType_AllTypes(t *testing.T) {
	validTypes := []string{"convention", "correction", "decision", "tension", "fact", "skill"}
	for _, typ := range validTypes {
		if !ValidMemoryType(typ) {
			t.Errorf("ValidMemoryType(%q) = false; want true", typ)
		}
	}

	invalidTypes := []string{"", "unknown", "random", "Convention", "FACT"}
	for _, typ := range invalidTypes {
		if ValidMemoryType(typ) {
			t.Errorf("ValidMemoryType(%q) = true; want false", typ)
		}
	}
}

func TestMemoryService_ScopePrefix(t *testing.T) {
	tests := []struct {
		scope   MemoryScope
		scopeID string
		want    string
	}{
		{MemoryScopeProject, "proj-id", "memory/project/proj-id"},
		{MemoryScopeUser, "user-hash", "memory/user/user-hash"},
		{MemoryScopeContext, "my-context", "memory/project/my-context"},
	}
	for _, tc := range tests {
		t.Run(string(tc.scope)+"/"+tc.scopeID, func(t *testing.T) {
			svc := &MemoryService{scope: tc.scope, scopeID: tc.scopeID}
			got := svc.ScopePrefix()
			if got != tc.want {
				t.Errorf("ScopePrefix() = %q; want %q", got, tc.want)
			}
		})
	}
}

// memory.go — AddMemory / UpdateMemory / RemoveMemory / changeRelevance
// (nil gitStore error paths)

func TestMemoryService_AddMemory_NilStore(t *testing.T) {
	svc := &MemoryService{scope: MemoryScopeProject, scopeID: "test"}
	_, err := svc.AddMemory("title", "body", MemoryOpts{})
	if err == nil {
		t.Error("expected error with nil gitStore")
	}
	if !strings.Contains(err.Error(), "not configured") {
		t.Errorf("error should mention 'not configured', got: %v", err)
	}
}

func TestMemoryService_UpdateMemory_NilStore(t *testing.T) {
	svc := &MemoryService{scope: MemoryScopeProject, scopeID: "test"}
	err := svc.UpdateMemory("id", "title", "body")
	if err == nil {
		t.Error("expected error with nil gitStore")
	}
	if !strings.Contains(err.Error(), "not configured") {
		t.Errorf("error should mention 'not configured', got: %v", err)
	}
}

func TestMemoryService_RemoveMemory_NilStore(t *testing.T) {
	svc := &MemoryService{scope: MemoryScopeProject, scopeID: "test"}
	err := svc.RemoveMemory("id")
	if err == nil {
		t.Error("expected error with nil gitStore")
	}
	if !strings.Contains(err.Error(), "not configured") {
		t.Errorf("error should mention 'not configured', got: %v", err)
	}
}

func TestMemoryService_PromoteMemory_NilStore(t *testing.T) {
	svc := &MemoryService{scope: MemoryScopeProject, scopeID: "test"}
	err := svc.PromoteMemory("id")
	if err == nil {
		t.Error("expected error with nil gitStore")
	}
}

func TestMemoryService_DemoteMemory_NilStore(t *testing.T) {
	svc := &MemoryService{scope: MemoryScopeProject, scopeID: "test"}
	err := svc.DemoteMemory("id")
	if err == nil {
		t.Error("expected error with nil gitStore")
	}
}

func TestBuildMemoryFile_Boost(t *testing.T) {
	content := buildMemoryFile(
		"TEST-ID", "Test Title", "Test body content",
		"project", "scope-id", "",
		false, "fact", []string{"tag1", "tag2"},
	)

	if !strings.Contains(content, "id: TEST-ID") {
		t.Error("should contain id")
	}
	if !strings.Contains(content, "title: Test Title") {
		t.Error("should contain title")
	}
	if !strings.Contains(content, "scope: project") {
		t.Error("should contain scope")
	}
	if !strings.Contains(content, "scope_id: scope-id") {
		t.Error("should contain scope_id")
	}
	if !strings.Contains(content, "type: fact") {
		t.Error("should contain type")
	}
	if !strings.Contains(content, "# Test Title") {
		t.Error("should contain h1 title")
	}
	if !strings.Contains(content, "Test body content") {
		t.Error("should contain body")
	}
	if !strings.Contains(content, "tag1") || !strings.Contains(content, "tag2") {
		t.Error("should contain user tags")
	}
	if strings.Contains(content, "important:") {
		t.Error("should not contain important flag when false")
	}
	if strings.Contains(content, "project_id:") {
		t.Error("should not contain project_id when empty")
	}
}

func TestBuildMemoryFile_WithImportantAndProjectID(t *testing.T) {
	content := buildMemoryFile(
		"ID", "Title", "Body",
		"user", "user-hash", "proj-123",
		true, "convention", nil,
	)

	if !strings.Contains(content, "important: true") {
		t.Error("should contain important flag")
	}
	if !strings.Contains(content, "project_id: proj-123") {
		t.Error("should contain project_id for user scope")
	}
}

func TestBuildMemoryFile_EmptyBody(t *testing.T) {
	content := buildMemoryFile("ID", "Title", "", "project", "sid", "", false, "", nil)
	if !strings.Contains(content, "# Title") {
		t.Error("should contain h1")
	}
	// Should not have extra newline after title when body is empty
	if strings.Contains(content, "type:") {
		t.Error("should not contain type when empty")
	}
}

func TestBuildMemoryFile_BodyWithoutTrailingNewline(t *testing.T) {
	content := buildMemoryFile("ID", "Title", "body without newline", "project", "sid", "", false, "fact", nil)
	if !strings.HasSuffix(content, "\n") {
		t.Error("content should end with newline")
	}
}

func TestBuildMemoryFile_BodyWithTrailingNewline(t *testing.T) {
	content := buildMemoryFile("ID", "Title", "body with newline\n", "project", "sid", "", false, "fact", nil)
	// Should not have double trailing newline
	if strings.HasSuffix(content, "\n\n\n") {
		t.Error("should not have triple newline at end")
	}
}

func TestMemoryService_Close_Boost(t *testing.T) {
	svc := &MemoryService{}
	if err := svc.Close(); err != nil {
		t.Errorf("Close: %v", err)
	}
}

func TestSafeMemFilename_Boost(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{"Simple Title", "Simple_Title"},
		{"With/slashes\\and:colons", "With-slashes-and-colons"},
		{"Special?*chars", "Specialchars"},
		{"Multiple   Spaces", "Multiple_Spaces"},
		{"_leading_underscores_", "leading_underscores"},
		{"-leading-dashes-", "leading-dashes"},
		{"double__underscores", "double_underscores"},
		{"double--dashes", "double-dashes"},
		{"Hello, World!", "Hello_World"},
		{"Über Cool 🎉", "Über_Cool"},
		{"", ""},
	}
	for _, tc := range tests {
		t.Run(tc.input, func(t *testing.T) {
			got := wiki.SafeSlug(tc.input)
			if got != tc.want {
				t.Errorf("wiki.SafeSlug(%q) = %q; want %q", tc.input, got, tc.want)
			}
		})
	}
}

func TestUniqueMemSlug_Boost(t *testing.T) {
	used := make(map[string]bool)

	slug1 := wiki.UniqueSlug("test", used)
	if slug1 != "test" {
		t.Errorf("first slug = %q; want 'test'", slug1)
	}

	slug2 := wiki.UniqueSlug("test", used)
	if slug2 != "test_2" {
		t.Errorf("second slug = %q; want 'test_2'", slug2)
	}

	slug3 := wiki.UniqueSlug("test", used)
	if slug3 != "test_3" {
		t.Errorf("third slug = %q; want 'test_3'", slug3)
	}

	// Different base should work
	slug4 := wiki.UniqueSlug("other", used)
	if slug4 != "other" {
		t.Errorf("different base = %q; want 'other'", slug4)
	}
}

func TestParseMemoryType_Boost(t *testing.T) {
	tests := []struct {
		name    string
		content string
		want    string
	}{
		{"no type", "no type here", ""},
		{"has type", "---\ntype: convention\n---", "convention"},
		{"type with spaces", "type:   skill  ", "skill"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := parseMemoryType(tc.content)
			if got != tc.want {
				t.Errorf("parseMemoryType = %q; want %q", got, tc.want)
			}
		})
	}
}

func TestParseTags_Boost(t *testing.T) {
	tests := []struct {
		name  string
		input string
		want  []string
	}{
		{"empty", "", nil},
		{"single tag", "auth", []string{"auth"}},
		{"multiple tags", "auth,security,api", []string{"auth", "security", "api"}},
		{"with spaces", " auth , security , api ", []string{"auth", "security", "api"}},
		{"empty elements", "auth,,security,,,api", []string{"auth", "security", "api"}},
		{"only commas", ",,,", nil},
		{"single with spaces", "  tag1  ", []string{"tag1"}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := ParseTags(tc.input)
			if tc.want == nil && got != nil {
				t.Errorf("ParseTags(%q) = %v; want nil", tc.input, got)
				return
			}
			if len(got) != len(tc.want) {
				t.Errorf("ParseTags(%q) = %v (len %d); want %v (len %d)", tc.input, got, len(got), tc.want, len(tc.want))
				return
			}
			for i := range got {
				if got[i] != tc.want[i] {
					t.Errorf("ParseTags(%q)[%d] = %q; want %q", tc.input, i, got[i], tc.want[i])
				}
			}
		})
	}
}

func TestNewMemoryAppService_Boost(t *testing.T) {
	svc := NewMemoryAppService("/test/project")
	if svc == nil {
		t.Fatal("expected non-nil service")
	}
	if svc.projectDir != "/test/project" {
		t.Errorf("projectDir = %q", svc.projectDir)
	}
}

func TestMemoryAppService_InsertValidated_InvalidType(t *testing.T) {
	svc := NewMemoryAppService(t.TempDir())
	_, err := svc.InsertValidated(MemoryInsertOpts{
		Title:   "Test",
		Content: "Body",
		Type:    "invalid-type",
	})
	if err == nil {
		t.Error("expected error for invalid type")
	}
	if !strings.Contains(err.Error(), "invalid memory type") {
		t.Errorf("error should mention 'invalid memory type', got: %v", err)
	}
}

// memory_git_store.go — copyDirRecursive with files

func TestMemoryScopeConstants_Boost(t *testing.T) {
	if MemoryScopeProject != "project" {
		t.Errorf("MemoryScopeProject = %q", MemoryScopeProject)
	}
	if MemoryScopeUser != "user" {
		t.Errorf("MemoryScopeUser = %q", MemoryScopeUser)
	}
	if MemoryScopeContext != "context" {
		t.Errorf("MemoryScopeContext = %q", MemoryScopeContext)
	}
}

func TestMemoryTypeConstants_Boost(t *testing.T) {
	types := map[MemoryType]string{
		MemoryTypeConvention: "convention",
		MemoryTypeCorrection: "correction",
		MemoryTypeDecision:   "decision",
		MemoryTypeTension:    "tension",
		MemoryTypeFact:       "fact",
		MemoryTypeSkill:      "skill",
	}
	for typ, want := range types {
		if string(typ) != want {
			t.Errorf("MemoryType %v = %q; want %q", typ, string(typ), want)
		}
	}
}

func TestEnsureScopeDirs_WithValidProjectDir(t *testing.T) {
	dir := t.TempDir()
	err := EnsureScopeDirs("project", dir)
	if err != nil {
		t.Fatalf("EnsureScopeDirs: %v", err)
	}

	// A project without a lockfile has no project scope, so there is nothing to
	// create and nothing to fail — the wiki lives under the global dir keyed by the
	// project id, not under the project.
	if got := WikiDirFor(dir, "project"); got != "" {
		t.Errorf("expected no project scope without a lockfile, got %q", got)
	}
}

func TestSaveMemLock_Success(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	lf := &scopeLockFile{
		Version: 1,
		Scopes:  map[string]*scopeMeta{"test": {Refs: []string{"ref1"}, LastUsed: "now"}},
	}
	err := saveMemLock(lf)
	if err != nil {
		t.Fatalf("saveMemLock: %v", err)
	}

	// Verify file was written
	data, err := os.ReadFile(scopeLockPath())
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), "test") {
		t.Error("saved lock file should contain 'test' branch")
	}
}

func TestImportantFlagRoundTrip_Boost(t *testing.T) {
	content := renderMemoryFile(MemoryFrontmatter{ID: "MEM1", Title: "T", Important: true}, "Body.")
	if !IsImportantContent(content) {
		t.Error("a memory rendered as important does not read back as important")
	}
	if IsImportantContent(withImportantFlag(content, false)) {
		t.Error("a demoted memory still reads back as important")
	}
}

// Recompiling after a write must NOT fail the write.
//
// This asserted the opposite — that `syncToLocalFast` errors when the store is unusable — and that
// was right while it also did the syncing: a failed pull meant the local copy was wrong. It only
// recompiles now, and it runs AFTER a write that already succeeded, so turning a failed recompile
// into an error would report a stored memory as unstored. The failure is logged instead.
func TestSyncToLocalDoesNotFailAWriteThatAlreadySucceeded(t *testing.T) {
	svc := &MemoryService{}
	if err := svc.syncToLocalFast(); err != nil {
		t.Errorf("recompiling must not fail the write it follows: %v", err)
	}
}

func TestDetectStaleMemories_EmptyCreatedAt(t *testing.T) {
	memories := []memorySnapshot{
		{ID: "1", Title: "No Date", CreatedAt: "", Important: false},
	}
	stale := detectStaleMemories(memories)
	if len(stale) != 0 {
		t.Errorf("expected 0 stale for empty date, got %d", len(stale))
	}
}

func TestDetectStaleMemories_RecentDate(t *testing.T) {
	recentDate := time.Now().Add(-5 * 24 * time.Hour).Format(time.RFC3339)
	memories := []memorySnapshot{
		{ID: "1", Title: "Recent", CreatedAt: recentDate, Important: false},
	}
	stale := detectStaleMemories(memories)
	if len(stale) != 0 {
		t.Errorf("expected 0 stale for recent date, got %d", len(stale))
	}
}

// memory.go — MemoryService creation edge cases

func TestNewMemorySvcInternal_NilStore(t *testing.T) {
	svc := newMemorySvcInternal(MemoryScopeProject, "id", nil)
	if svc == nil {
		t.Fatal("expected non-nil svc")
	}
	if svc.store != nil {
		t.Error("gitStore should be nil")
	}
}

func TestMemoryInsertOpts_Fields(t *testing.T) {
	opts := MemoryInsertOpts{
		Title:     "Test",
		Content:   "Body",
		Type:      "fact",
		Tags:      "tag1,tag2",
		Scope:     "project",
		Important: true,
	}
	if opts.Title != "Test" || opts.Content != "Body" {
		t.Error("unexpected field values")
	}
	if opts.Type != "fact" || opts.Tags != "tag1,tag2" {
		t.Error("unexpected field values")
	}
	if opts.Scope != "project" || !opts.Important {
		t.Error("unexpected field values")
	}
}

func TestMemoryOpts_Fields(t *testing.T) {
	opts := MemoryOpts{
		ProjectID: "proj-1",
		Important: true,
		Type:      MemoryTypeConvention,
		Tags:      []string{"tag1", "tag2"},
	}
	if opts.ProjectID != "proj-1" {
		t.Error("unexpected ProjectID")
	}
	if !opts.Important {
		t.Error("should be important")
	}
	if opts.Type != MemoryTypeConvention {
		t.Error("unexpected type")
	}
	if len(opts.Tags) != 2 {
		t.Error("expected 2 tags")
	}
}
