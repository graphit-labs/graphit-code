package memory

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// helpers: MemoryFileName, MemoryIDFromFileName, IsImportantContent

func TestMemoryFileName(t *testing.T) {
	tests := []struct {
		id   string
		want string
	}{
		{"123", "123.md"},
		{"01J5X", "01J5X.md"},
		{"", ".md"},
	}
	for _, tc := range tests {
		t.Run(tc.id, func(t *testing.T) {
			got := MemoryFileName(tc.id)
			if got != tc.want {
				t.Errorf("MemoryFileName(%q) = %q; want %q", tc.id, got, tc.want)
			}
		})
	}
}

func TestMemoryIDFromFileName(t *testing.T) {
	tests := []struct {
		filename string
		want     string
	}{
		{"abc.md", "abc"},
		{"01J123.md", "01J123"},
		{"dir/nested.md", "nested"},
		{"abc.txt", "abc.txt"},
		{"", ""},
	}
	for _, tc := range tests {
		t.Run(tc.filename, func(t *testing.T) {
			got := MemoryIDFromFileName(tc.filename)
			if got != tc.want {
				t.Errorf("MemoryIDFromFileName(%q) = %q; want %q", tc.filename, got, tc.want)
			}
		})
	}
}

func TestIsImportantContent(t *testing.T) {
	tests := []struct {
		name    string
		content string
		want    bool
	}{
		{"flag set", "---\ntitle: T\nimportant: true\n---\n\n# T\n\nBody.", true},
		{"flag absent", "---\ntitle: T\n---\n\n# T\n\nBody.", false},
		{"flag false", "---\ntitle: T\nimportant: false\n---\n\n# T\n\nBody.", false},
		{"word in body only", "---\ntitle: T\n---\n\n# T\n\nimportant: true", false},
		{"no frontmatter", "# T\n\nBody.", false},
		{"empty", "", false},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := IsImportantContent(tc.content)
			if got != tc.want {
				t.Errorf("IsImportantContent(%q) = %v; want %v", tc.content, got, tc.want)
			}
		})
	}
}

func TestValidMemoryType(t *testing.T) {
	tests := []struct {
		typ  string
		want bool
	}{
		{"convention", true},
		{"correction", true},
		{"decision", true},
		{"tension", true},
		{"fact", true},
		{"skill", true},
		{"invalid", false},
		{"", false},
		{"FACT", false}, // case-sensitive
		{"Convention", false},
	}
	for _, tc := range tests {
		t.Run(tc.typ, func(t *testing.T) {
			got := ValidMemoryType(tc.typ)
			if got != tc.want {
				t.Errorf("ValidMemoryType(%q) = %v; want %v", tc.typ, got, tc.want)
			}
		})
	}
}

func TestExtractBodyAfterFrontmatter(t *testing.T) {
	tests := []struct {
		name    string
		content string
		want    string
	}{
		{
			name: "full frontmatter and H1",
			content: `---
title: My Title
tags: [memory]
---

# My Title

Body content starts here.
Another line.`,
			want: "Body content starts here.\nAnother line.",
		},
		{
			name:    "no frontmatter",
			content: "Just some text without frontmatter",
			want:    "Just some text without frontmatter",
		},
		{
			name: "frontmatter no H1",
			content: `---
title: Test
---

Body after frontmatter only.`,
			want: "Body after frontmatter only.",
		},
		{
			name:    "empty",
			content: "",
			want:    "",
		},
		{
			name: "frontmatter only",
			content: `---
title: Title Only
---`,
			want: "",
		},
		{
			name: "frontmatter H1 no body",
			content: `---
title: Test
---

# Test`,
			want: "",
		},
		{
			name: "multiple H1s",
			content: `---
title: Test
---

# First Heading

Some content.

# Second Heading

More content.`,
			want: "Some content.\n\n# Second Heading\n\nMore content.",
		},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := extractBodyAfterFrontmatter(tc.content)
			if got != tc.want {
				t.Errorf("extractBodyAfterFrontmatter:\n  got:  %q\n  want: %q", got, tc.want)
			}
		})
	}
}

func TestParseMemoryMeta(t *testing.T) {
	tests := []struct {
		name      string
		content   string
		wantTitle string
		wantDate  string
	}{
		{
			name: "with frontmatter title and created_at",
			content: `---
title: My Important Memory
created_at: 2026-05-26T10:30:00Z
---

# My Important Memory

Details here.`,
			wantTitle: "My Important Memory",
			wantDate:  "2026-05-26T10:30:00Z",
		},
		{
			name: "frontmatter title no created_at",
			content: `---
title: No Date Memory
---

# No Date Memory

Content.`,
			wantTitle: "No Date Memory",
			wantDate:  "",
		},
		{
			name: "H1 title fallback (no frontmatter title key)",
			content: `# Standalone Title

Content here.`,
			wantTitle: "Standalone Title",
			wantDate:  "",
		},
		{
			name:      "empty file",
			content:   "",
			wantTitle: "", // will fall back to filename base
		},
		{
			name:    "no title at all",
			content: "Some random content without title or heading",
		},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			dir := t.TempDir()
			path := filepath.Join(dir, "test_memory.md")
			if err := os.WriteFile(path, []byte(tc.content), 0o644); err != nil {
				t.Fatalf("WriteFile: %v", err)
			}
			gotTitle, gotDate := ParseMemoryMetaPublic(path)
			if tc.wantTitle != "" && gotTitle != tc.wantTitle {
				t.Errorf("title = %q; want %q", gotTitle, tc.wantTitle)
			}
			if tc.wantDate != "" && gotDate != tc.wantDate {
				t.Errorf("createdAt = %q; want %q", gotDate, tc.wantDate)
			}
		})
	}
}

func TestParseMemoryMeta_NonExistentFile(t *testing.T) {
	title, createdAt := ParseMemoryMetaPublic("/nonexistent/path/does_not_exist.md")
	if title != "does_not_exist.md" {
		t.Errorf("expected filename fallback, got %q", title)
	}
	if createdAt != "" {
		t.Errorf("expected empty createdAt, got %q", createdAt)
	}
}

func TestFirstLineFromContent(t *testing.T) {
	tests := []struct {
		name string
		body string
		want string
	}{
		{"empty", "", ""},
		{"single line", "Hello world", "Hello world"},
		{"skip heading", "# Heading\n\nActual content", "Actual content"},
		{"truncate at 100", strings.Repeat("A", 120), strings.Repeat("A", 100) + "…"},
		{"blank lines", "\n\n\n", ""},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := firstLineFromContent(tc.body)
			if got != tc.want {
				t.Errorf("firstLineFromContent(%q) = %q; want %q", tc.body, got, tc.want)
			}
		})
	}
}

func TestSafeMemFilename(t *testing.T) {
	tests := []struct {
		name string
		in   string
		want string
	}{
		{"simple", "hello world", "hello_world"},
		{"slashes", "path/to/thing", "path-to-thing"},
		{"special chars", "what? *really* yes!", "what_really_yes"},
		{"colons", "key: value", "key-_value"},
		{"double underscores", "foo__bar", "foo_bar"},
		{"double dashes", "foo--bar", "foo-bar"},
		{"leading trailing", "_-hello-_", "hello"},
		{"empty", "", ""},
		{"unicode letters", "café résumé", "café_résumé"},
		{"dots preserved", "file.name.ext", "file.name.ext"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := wiki.SafeSlug(tc.in)
			if got != tc.want {
				t.Errorf("wiki.SafeSlug(%q) = %q; want %q", tc.in, got, tc.want)
			}
		})
	}
}

func TestUniqueMemSlug(t *testing.T) {
	used := make(map[string]bool)

	// First use — no collision
	slug1 := wiki.UniqueSlug("test", used)
	if slug1 != "test" {
		t.Errorf("expected 'test', got %q", slug1)
	}
	if !used["test"] {
		t.Error("expected 'test' to be marked as used")
	}

	// Second use — collision → suffix _2
	slug2 := wiki.UniqueSlug("test", used)
	if slug2 != "test_2" {
		t.Errorf("expected 'test_2', got %q", slug2)
	}
	if !used["test_2"] {
		t.Error("expected 'test_2' to be marked as used")
	}

	// Third use — collision → suffix _3
	slug3 := wiki.UniqueSlug("test", used)
	if slug3 != "test_3" {
		t.Errorf("expected 'test_3', got %q", slug3)
	}
}

func TestParseTags(t *testing.T) {
	tests := []struct {
		name string
		csv  string
		want []string
	}{
		{"empty", "", nil},
		{"single", "auth", []string{"auth"}},
		{"multi", "auth,security,login", []string{"auth", "security", "login"}},
		{"with spaces", " auth , security , login ", []string{"auth", "security", "login"}},
		{"trailing comma", "auth,security,", []string{"auth", "security"}},
		{"empty segments", "auth,,security", []string{"auth", "security"}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := ParseTags(tc.csv)
			if tc.want == nil {
				if got != nil {
					t.Errorf("ParseTags(%q) = %v; want nil", tc.csv, got)
				}
				return
			}
			if len(got) != len(tc.want) {
				t.Fatalf("ParseTags(%q) length = %d; want %d", tc.csv, len(got), len(tc.want))
			}
			for i := range tc.want {
				if got[i] != tc.want[i] {
					t.Errorf("ParseTags(%q)[%d] = %q; want %q", tc.csv, i, got[i], tc.want[i])
				}
			}
		})
	}
}

func TestBuildMemoryFile(t *testing.T) {
	content := buildMemoryFile(
		"TEST-ID", "Test Title", "Body text here",
		"project", "proj-123", "",
		true, "convention", []string{"tag1", "tag2"},
	)

	checks := []struct {
		desc    string
		substr  string
		present bool
	}{
		{"contains id", "id: TEST-ID", true},
		{"contains title", "title: Test Title", true},
		{"contains scope", "scope: project", true},
		{"contains scope_id", "scope_id: proj-123", true},
		{"contains type", "type: convention", true},
		{"contains important", "important: true", true},
		{"contains created_at", "created_at:", true},
		{"contains updated_at", "updated_at:", true},
		{"contains tags", "tags: [memory, project, convention, tag1, tag2]", true},
		{"contains H1", "# Test Title", true},
		{"contains body", "Body text here", true},
		{"no project_id for project scope", "project_id:", false},
	}
	for _, c := range checks {
		t.Run(c.desc, func(t *testing.T) {
			found := strings.Contains(content, c.substr)
			if found != c.present {
				if c.present {
					t.Errorf("expected content to contain %q", c.substr)
				} else {
					t.Errorf("expected content NOT to contain %q", c.substr)
				}
			}
		})
	}
}

func TestBuildMemoryFile_UserScope(t *testing.T) {
	content := buildMemoryFile(
		"ID2", "User Memory", "Body",
		"user", "user-hash", "orig-project-id",
		false, "fact", nil,
	)
	if !strings.Contains(content, "project_id: orig-project-id") {
		t.Error("expected project_id field for user scope with origProjectID")
	}
	if strings.Contains(content, "important: true") {
		t.Error("should not contain important: true when not important")
	}
}

func TestBuildMemoryFile_NoType(t *testing.T) {
	content := buildMemoryFile("ID3", "Title", "Body", "project", "pid", "", false, "", nil)
	if strings.Contains(content, "type:") {
		t.Error("should not contain type field when type is empty")
	}
}

func TestBuildMemoryFile_BodyTrailingNewline(t *testing.T) {
	content := buildMemoryFile("ID4", "Title", "Body\n", "project", "pid", "", false, "fact", nil)
	// Body already ends with \n, so buildMemoryFile should NOT add an extra one.
	if strings.Contains(content, "Body\n\n\n") {
		t.Error("double newline after body that already ends with newline")
	}
}

func TestConsolidationReport_HasActions(t *testing.T) {
	tests := []struct {
		name string
		r    ConsolidationReport
		want bool
	}{
		{"empty", ConsolidationReport{}, false},
		{"only duplicates", ConsolidationReport{Duplicates: []ConsolidationAction{{}}}, true},
		{"only contradictions", ConsolidationReport{Contradictions: []ConsolidationAction{{}}}, true},
		{"only stale", ConsolidationReport{Stale: []ConsolidationAction{{}}}, true},
		{"only suggestions", ConsolidationReport{Suggestions: []ConsolidationAction{{}}}, true},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := tc.r.HasActions()
			if got != tc.want {
				t.Errorf("HasActions() = %v; want %v", got, tc.want)
			}
		})
	}
}

func TestConsolidationReport_TotalActions(t *testing.T) {
	r := ConsolidationReport{
		Duplicates:     []ConsolidationAction{{}, {}},
		Contradictions: []ConsolidationAction{{}},
		Stale:          []ConsolidationAction{{}, {}, {}},
		Suggestions:    []ConsolidationAction{{}},
	}
	if got := r.TotalActions(); got != 7 {
		t.Errorf("TotalActions() = %d; want 7", got)
	}
}

func TestParseMemoryType(t *testing.T) {
	tests := []struct {
		name    string
		content string
		want    string
	}{
		{"fact", "---\ntype: fact\n---\n", "fact"},
		{"correction", "type: correction\nother: val", "correction"},
		{"no type", "title: Something", ""},
		{"empty", "", ""},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := parseMemoryType(tc.content)
			if got != tc.want {
				t.Errorf("parseMemoryType() = %q; want %q", got, tc.want)
			}
		})
	}
}

func TestDetectStaleMemories(t *testing.T) {
	now := time.Now().UTC()
	// Past staleAfter (90 days), and comfortably inside it, respectively.
	oldDate := now.Add(-120 * 24 * time.Hour).Format(time.RFC3339)
	freshDate := now.Add(-10 * 24 * time.Hour).Format(time.RFC3339)

	memories := []memorySnapshot{
		{ID: "old-1", Title: "Old Memory", CreatedAt: oldDate, Important: false},
		{ID: "fresh-1", Title: "Fresh Memory", CreatedAt: freshDate, Important: false},
		{ID: "old-important", Title: "Old Important", CreatedAt: oldDate, Important: true},
		{ID: "no-date", Title: "No Date", CreatedAt: "", Important: false},
	}

	stale := detectStaleMemories(memories)

	// Only old-1 should be stale (old-important is skipped because important, fresh is fresh, no-date is skipped)
	if len(stale) != 1 {
		t.Fatalf("expected 1 stale memory, got %d", len(stale))
	}
	if stale[0].MemoryIDs[0] != "old-1" {
		t.Errorf("expected stale ID 'old-1', got %q", stale[0].MemoryIDs[0])
	}
	if !strings.Contains(stale[0].Reason, "120 days old") {
		t.Errorf("expected reason to mention age, got %q", stale[0].Reason)
	}
	// Staleness proposes review, never deletion: age means "not revised", not "wrong".
	if stale[0].Type != ActionUpdate {
		t.Errorf("stale action type = %q; want %q", stale[0].Type, ActionUpdate)
	}
}

func TestDetectStaleMemories_AllFresh(t *testing.T) {
	now := time.Now().UTC()
	freshDate := now.Add(-5 * 24 * time.Hour).Format(time.RFC3339)

	memories := []memorySnapshot{
		{ID: "1", Title: "Fresh", CreatedAt: freshDate, Important: false},
	}

	stale := detectStaleMemories(memories)
	if len(stale) != 0 {
		t.Errorf("expected 0 stale memories, got %d", len(stale))
	}
}

func TestDetectStaleMemories_Empty(t *testing.T) {
	stale := detectStaleMemories(nil)
	if len(stale) != 0 {
		t.Errorf("expected 0 stale memories for nil input, got %d", len(stale))
	}
}

// ListMemories (filesystem-based via MemoryService)

func TestListImportantInDir(t *testing.T) {
	dir := t.TempDir()

	impContent := `---
title: Critical Rule
important: true
---

# Critical Rule

Always do X before Y.`
	writeMemFile(t, dir, "RULE1.md", impContent)

	normContent := `---
title: Normal Fact
---

# Normal Fact

Regular content.`
	writeMemFile(t, dir, "FACT1.md", normContent)

	results, err := listImportantInDir(dir)
	if err != nil {
		t.Fatalf("listImportantInDir: %v", err)
	}
	if len(results) != 1 {
		t.Fatalf("expected 1 important, got %d", len(results))
	}
	if results[0].ID != "RULE1" {
		t.Errorf("ID = %q; want 'RULE1'", results[0].ID)
	}
	if results[0].Title != "Critical Rule" {
		t.Errorf("Title = %q; want 'Critical Rule'", results[0].Title)
	}
	if !strings.Contains(results[0].Content, "Always do X before Y") {
		t.Error("expected content to include body text")
	}
}

func TestListImportantInDir_Empty(t *testing.T) {
	dir := t.TempDir()
	results, err := listImportantInDir(dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(results) != 0 {
		t.Errorf("expected 0, got %d", len(results))
	}
}

func TestListImportantInDir_NonExistent(t *testing.T) {
	results, err := listImportantInDir("/nonexistent/path")
	if err != nil {
		t.Fatalf("expected nil error for non-existent dir, got: %v", err)
	}
	if results != nil {
		t.Errorf("expected nil, got %v", results)
	}
}

func TestScopePrefix(t *testing.T) {
	tests := []struct {
		scope   MemoryScope
		scopeID string
		want    string
	}{
		{MemoryScopeProject, "proj-abc", "memory/project/proj-abc"},
		{MemoryScopeUser, "user-hash", "memory/user/user-hash"},
		{MemoryScopeContext, "ctx-name", "memory/project/ctx-name"},
	}
	for _, tc := range tests {
		t.Run(string(tc.scope), func(t *testing.T) {
			svc := &MemoryService{scope: tc.scope, scopeID: tc.scopeID}
			got := svc.ScopePrefix()
			if got != tc.want {
				t.Errorf("ScopePrefix() = %q; want %q", got, tc.want)
			}
		})
	}
}

func TestRenderImportantBlock_Inline(t *testing.T) {
	dir := t.TempDir()
	content := `---
title: Critical Rule
important: true
---

# Critical Rule

Always follow this rule.`
	writeMemFile(t, dir, "RULE1.md", content)

	// We test listImportantInDir + rendering inline since RenderImportantBlock
	// uses global scope resolution that depends on project state.
	entries, err := listImportantInDir(dir)
	if err != nil {
		t.Fatalf("listImportantInDir: %v", err)
	}
	if len(entries) != 1 {
		t.Fatalf("expected 1, got %d", len(entries))
	}

	// Build a block similar to RenderImportantBlock
	var b strings.Builder
	b.WriteString("## 📌 Key Project Memories\n\n")
	for _, e := range entries {
		_, _ = fmt.Fprintf(&b, "### %s\n", e.Title)
		_, _ = fmt.Fprintf(&b, "*ID: `%s`*\n\n", e.ID)
		if e.Content != "" {
			b.WriteString(e.Content + "\n")
		}
		b.WriteString("\n")
	}
	block := b.String()
	if !strings.Contains(block, "Critical Rule") {
		t.Error("block should contain memory title")
	}
	if !strings.Contains(block, "RULE1") {
		t.Error("block should contain memory ID")
	}
}

func TestMemoryService_Close(t *testing.T) {
	svc := &MemoryService{}
	if err := svc.Close(); err != nil {
		t.Errorf("Close() should return nil, got: %v", err)
	}
}

func TestMemoryService_NoGitStore_Errors(t *testing.T) {
	svc := &MemoryService{
		scope:   MemoryScopeProject,
		scopeID: "test",
	}

	_, err := svc.AddMemory("title", "body", MemoryOpts{})
	if err == nil {
		t.Error("AddMemory should error without gitStore")
	}

	err = svc.UpdateMemory("id", "title", "body")
	if err == nil {
		t.Error("UpdateMemory should error without gitStore")
	}

	err = svc.RemoveMemory("id")
	if err == nil {
		t.Error("RemoveMemory should error without gitStore")
	}

	err = svc.PromoteMemory("id")
	if err == nil {
		t.Error("PromoteMemory should error without gitStore")
	}

	err = svc.DemoteMemory("id")
	if err == nil {
		t.Error("DemoteMemory should error without gitStore")
	}

	// SyncToLocal is deliberately NOT in this list any more. Every call above is a WRITE, and a
	// write with no store must refuse. SyncToLocal only recompiles, and it runs after a write that
	// already succeeded — erroring there would report a stored memory as unstored.
	if err := svc.SyncToLocal(); err != nil {
		t.Errorf("recompiling must not fail: %v", err)
	}
}

func TestCycleResult_Fields(t *testing.T) {
	cr := &CycleResult{
		Scope:     "project",
		WikiFiles: 5,
		Err:       nil,
	}
	if cr.Scope != "project" {
		t.Errorf("Scope = %q; want 'project'", cr.Scope)
	}
	if cr.WikiFiles != 5 {
		t.Errorf("WikiFiles = %d; want 5", cr.WikiFiles)
	}
}

func TestMemoryEntry_Fields(t *testing.T) {
	e := MemoryEntry{
		ID:        "TEST-ID",
		Title:     "Test Title",
		CreatedAt: "2026-01-01T00:00:00Z",
		Scope:     MemoryScopeProject,
		ScopeID:   "proj-id",
		Important: true,
		Type:      MemoryTypeConvention,
		Tags:      []string{"tag1", "tag2"},
	}
	if e.ID != "TEST-ID" {
		t.Errorf("ID = %q", e.ID)
	}
	if e.Scope != MemoryScopeProject {
		t.Errorf("Scope = %q", e.Scope)
	}
}

func TestRawDirFor(t *testing.T) {
	got := RawDirFor("project", "abc123")
	// Should contain "memory-raw" and sanitized branch name
	if !strings.Contains(got, "memory-raw") {
		t.Errorf("expected path to contain 'memory-wt', got %q", got)
	}
	if !strings.Contains(got, "memory-project-abc123") {
		t.Errorf("expected path to contain 'memory-project-abc123', got %q", got)
	}
}

func TestMemoryScopeConstants(t *testing.T) {
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

func TestMemoryTypeConstants(t *testing.T) {
	expected := map[MemoryType]string{
		MemoryTypeConvention: "convention",
		MemoryTypeCorrection: "correction",
		MemoryTypeDecision:   "decision",
		MemoryTypeTension:    "tension",
		MemoryTypeFact:       "fact",
		MemoryTypeSkill:      "skill",
	}
	for k, v := range expected {
		if string(k) != v {
			t.Errorf("MemoryType %q != %q", k, v)
		}
	}
}

func TestConsolidationAction_Fields(t *testing.T) {
	a := ConsolidationAction{
		Type:       "merge",
		MemoryIDs:  []string{"ID1", "ID2"},
		Title:      "Merged Memory",
		Reason:     "They say the same thing",
		NewContent: "Merged content",
		NewTitle:   "New Title",
	}
	if a.Type != "merge" {
		t.Errorf("Type = %q", a.Type)
	}
	if len(a.MemoryIDs) != 2 {
		t.Errorf("MemoryIDs len = %d", len(a.MemoryIDs))
	}
}

func TestCopyDirRecursive(t *testing.T) {
	srcDir := t.TempDir()
	dstDir := t.TempDir()

	writeMemFile(t, srcDir, "file1.txt", "content1")
	subdir := filepath.Join(srcDir, "sub")
	if err := os.MkdirAll(subdir, 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	writeMemFile(t, subdir, "file2.txt", "content2")

	if err := copyDirRecursive(srcDir, dstDir); err != nil {
		t.Fatalf("copyDirRecursive: %v", err)
	}

	// Verify
	data1, err := os.ReadFile(filepath.Join(dstDir, "file1.txt"))
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(data1) != "content1" {
		t.Errorf("file1 content = %q", string(data1))
	}

	data2, err := os.ReadFile(filepath.Join(dstDir, "sub", "file2.txt"))
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(data2) != "content2" {
		t.Errorf("file2 content = %q", string(data2))
	}
}

func TestCopyFileData(t *testing.T) {
	srcDir := t.TempDir()
	dstDir := t.TempDir()

	srcPath := filepath.Join(srcDir, "source.txt")
	dstPath := filepath.Join(dstDir, "dest.txt")

	if err := os.WriteFile(srcPath, []byte("hello copy"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	if err := copyFileData(srcPath, dstPath, 0o644); err != nil {
		t.Fatalf("copyFileData: %v", err)
	}

	data, err := os.ReadFile(dstPath)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(data) != "hello copy" {
		t.Errorf("content = %q", string(data))
	}
}

func TestCopyFileData_NonExistentSource(t *testing.T) {
	dstDir := t.TempDir()
	err := copyFileData("/nonexistent/source.txt", filepath.Join(dstDir, "dest.txt"), 0o644)
	if err == nil {
		t.Error("expected error for non-existent source")
	}
}

func TestMemoryInsertOpts_DTO(t *testing.T) {
	opts := MemoryInsertOpts{
		Title:     "Test",
		Content:   "Body",
		Type:      "fact",
		Tags:      "a,b",
		Scope:     "project",
		Important: true,
	}
	if opts.Title != "Test" {
		t.Errorf("Title = %q", opts.Title)
	}
}

func TestNewMemoryAppService(t *testing.T) {
	svc := NewMemoryAppService("/some/project")
	if svc == nil {
		t.Fatal("expected non-nil service")
	}
}

func TestEnsureScopeDirs(t *testing.T) {
	dir := t.TempDir()
	err := EnsureScopeDirs("project", dir)
	if err != nil {
		t.Fatalf("EnsureScopeDirs: %v", err)
	}
}

func TestEnsureScopeDirs_EmptyProjectDir(t *testing.T) {
	err := EnsureScopeDirs("project", "")
	if err != nil {
		t.Fatalf("EnsureScopeDirs with empty projectDir should not error: %v", err)
	}
}

func TestImportanceIsNotEncodedInTheFileName(t *testing.T) {
	content := renderMemoryFile(MemoryFrontmatter{ID: "MEM1", Title: "T", Important: true}, "Body.")
	if !strings.Contains(content, "\nimportant: true\n") {
		t.Errorf("rendered memory carries no important flag:\n%s", content)
	}
	if name := MemoryFileName("MEM1"); name != "MEM1.md" {
		t.Errorf("MemoryFileName for an important memory = %q; want the plain id", name)
	}
}

func writeMemFile(t *testing.T, dir, name, content string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, name), []byte(content), 0o644); err != nil {
		t.Fatalf("writeMemFile(%s): %v", name, err)
	}
}
