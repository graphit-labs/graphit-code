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

// important.go — ListImportantMemories, RenderImportantBlock,
// ListRecentMemories, RenderRecentBlock, firstLineFromContent,
// extractBodyAfterFrontmatter

func TestListImportantInDir_WithEntries(t *testing.T) {
	dir := t.TempDir()

	writeMemFile(t, dir, "MEM1.md", `---
important: true
title: Important Convention
created_at: 2026-01-01T00:00:00Z
---

# Important Convention

Always use gofmt.`)

	// Normal memory (should be skipped)
	writeMemFile(t, dir, "MEM2.md", `---
title: Normal Memory
---

# Normal Memory

Normal body.`)

	// Directory (should be skipped)
	if err := os.Mkdir(filepath.Join(dir, "subdir"), 0o755); err != nil {
		t.Fatal(err)
	}

	entries, err := listImportantInDir(dir)
	if err != nil {
		t.Fatalf("listImportantInDir: %v", err)
	}
	if len(entries) != 1 {
		t.Fatalf("expected 1 important entry, got %d", len(entries))
	}
	if entries[0].ID != "MEM1" {
		t.Errorf("ID = %q; want 'MEM1'", entries[0].ID)
	}
	if entries[0].Title != "Important Convention" {
		t.Errorf("Title = %q; want 'Important Convention'", entries[0].Title)
	}
	if !strings.Contains(entries[0].Content, "gofmt") {
		t.Errorf("Content should contain 'gofmt', got %q", entries[0].Content)
	}
	if entries[0].Path == "" {
		t.Error("Path should not be empty")
	}
}

func TestListImportantInDir_Empty_Boost(t *testing.T) {
	dir := t.TempDir()
	entries, err := listImportantInDir(dir)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if len(entries) != 0 {
		t.Errorf("expected 0, got %d", len(entries))
	}
}

func TestListImportantInDir_NonExistentReturnsNil(t *testing.T) {
	entries, err := listImportantInDir("/nonexistent/path/xyz")
	if err != nil {
		t.Fatalf("expected nil error for non-existent, got: %v", err)
	}
	if entries != nil {
		t.Errorf("expected nil, got %v", entries)
	}
}

func TestRenderImportantBlock_ViaDir(t *testing.T) {
	dir := t.TempDir()

	writeMemFile(t, dir, "MEM1.md", `---
important: true
title: Critical Convention
---

# Critical Convention

Never use global state.`)

	writeMemFile(t, dir, "MEM2.md", `---
important: true
title: Another Important
---

# Another Important
`)

	// Test the rendering logic directly (since RenderImportantBlock uses RawDir
	// which depends on global state, we replicate the rendering)
	entries, err := listImportantInDir(dir)
	if err != nil || len(entries) == 0 {
		t.Fatalf("expected entries, got err=%v len=%d", err, len(entries))
	}

	var b strings.Builder
	b.WriteString("## 📌 Key Project Memories\n\n")
	b.WriteString("> These are the most critical project decisions and conventions.\n")
	b.WriteString("> They are automatically maintained. Do not edit this section.\n\n")

	for _, e := range entries {
		_, _ = fmt.Fprintf(&b, "### %s\n", e.Title)
		_, _ = fmt.Fprintf(&b, "*ID: `%s`*\n\n", e.ID)
		if e.Content != "" {
			b.WriteString(e.Content + "\n")
		}
		b.WriteString("\n")
	}

	block := b.String()
	if !strings.Contains(block, "Critical Convention") {
		t.Error("block should contain 'Critical Convention'")
	}
	if !strings.Contains(block, "Key Project Memories") {
		t.Error("block should contain header")
	}
}

// ListRecentMemories and RenderRecentBlock — filesystem-based

func TestListRecentInDir_WithMixedContent(t *testing.T) {
	dir := t.TempDir()

	writeMemFile(t, dir, "MEM_A.md", `---
title: Oldest
created_at: 2026-01-01T00:00:00Z
---

# Oldest

Body A.`)

	writeMemFile(t, dir, "MEM_B.md", `---
title: Newest
created_at: 2026-06-01T00:00:00Z
---

# Newest

Body B.`)

	// Important files should be excluded
	writeMemFile(t, dir, "MEM_C.md", `---
important: true
title: Important Skip
created_at: 2026-03-01T00:00:00Z
---

# Important Skip

Should not appear.`)

	// Non-md files should be excluded
	writeMemFile(t, dir, "README.txt", "not a memory")

	entries, err := listRecentInDir(dir, 10)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if len(entries) != 2 {
		t.Fatalf("expected 2 recent entries, got %d", len(entries))
	}
	// Should be sorted newest-first
	if entries[0].Title != "Newest" {
		t.Errorf("first entry should be 'Newest', got %q", entries[0].Title)
	}
	if entries[1].Title != "Oldest" {
		t.Errorf("second entry should be 'Oldest', got %q", entries[1].Title)
	}
}

func TestRenderRecentBlock_WithContent(t *testing.T) {
	dir := t.TempDir()

	writeMemFile(t, dir, "MEM1.md", `---
title: Recent Fact
created_at: 2026-05-20T00:00:00Z
---

# Recent Fact

Some recent content here.`)

	writeMemFile(t, dir, "MEM2.md", `---
title: No Content Memory
---

# No Content Memory`)

	entries, err := listRecentInDir(dir, 5)
	if err != nil {
		t.Fatal(err)
	}

	// Replicate RenderRecentBlock logic
	var b strings.Builder
	b.WriteString("## 🕐 Recent Memories\n\n")
	b.WriteString("> Latest agent-learned facts. Check these for immediate context.\n\n")

	for _, e := range entries {
		summary := firstLineFromContent(e.Content)
		if summary != "" {
			_, _ = fmt.Fprintf(&b, "- **%s** — %s *(ID: `%s`)*\n", e.Title, summary, e.ID)
		} else {
			_, _ = fmt.Fprintf(&b, "- **%s** *(ID: `%s`)*\n", e.Title, e.ID)
		}
	}
	b.WriteString("\n")

	block := b.String()
	if !strings.Contains(block, "Recent Memories") {
		t.Error("should contain header")
	}
	if !strings.Contains(block, "Recent Fact") {
		t.Error("should contain 'Recent Fact'")
	}
}

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

func TestParseMemoryMeta_WithTitle(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.md")
	writeMemFile(t, dir, "test.md", `---
title: My Title
created_at: 2026-01-01T00:00:00Z
---

# My Title

Body.`)

	title, createdAt := parseMemoryMeta(path)
	if title != "My Title" {
		t.Errorf("title = %q; want 'My Title'", title)
	}
	if createdAt != "2026-01-01T00:00:00Z" {
		t.Errorf("createdAt = %q", createdAt)
	}
}

func TestParseMemoryMeta_TitleFromH1(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.md")
	writeMemFile(t, dir, "test.md", "# My H1 Title\n\nBody text.")

	title, createdAt := parseMemoryMeta(path)
	if title != "My H1 Title" {
		t.Errorf("title = %q; want 'My H1 Title'", title)
	}
	if createdAt != "" {
		t.Errorf("createdAt should be empty, got %q", createdAt)
	}
}

func TestParseMemoryMeta_FallbackToFilename(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "my_memory.md")
	writeMemFile(t, dir, "my_memory.md", "just some text without title or h1")

	title, _ := parseMemoryMeta(path)
	if title != "my_memory" {
		t.Errorf("title = %q; want 'my_memory'", title)
	}
}

func TestParseMemoryMeta_NonExistentFile_Boost(t *testing.T) {
	title, createdAt := parseMemoryMeta("/nonexistent/path/file.md")
	if title != "file.md" {
		t.Errorf("title = %q; want 'file.md'", title)
	}
	if createdAt != "" {
		t.Errorf("createdAt should be empty, got %q", createdAt)
	}
}

func TestParseMemoryMetaPublic(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.md")
	writeMemFile(t, dir, "test.md", `---
title: Public Title
created_at: 2026-05-01T00:00:00Z
---

# Public Title
`)

	title, createdAt := ParseMemoryMetaPublic(path)
	if title != "Public Title" {
		t.Errorf("title = %q; want 'Public Title'", title)
	}
	if createdAt != "2026-05-01T00:00:00Z" {
		t.Errorf("createdAt = %q", createdAt)
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

func TestCopyDirRecursive_WithFiles(t *testing.T) {
	src := t.TempDir()
	dst := t.TempDir()

	writeMemFile(t, src, "file1.txt", "content1")
	if err := os.Mkdir(filepath.Join(src, "subdir"), 0o755); err != nil {
		t.Fatal(err)
	}
	writeMemFile(t, src, filepath.Join("subdir", "file2.txt"), "content2")

	if err := copyDirRecursive(src, dst); err != nil {
		t.Fatalf("copyDirRecursive: %v", err)
	}

	// Verify files were copied
	data1, err := os.ReadFile(filepath.Join(dst, "file1.txt"))
	if err != nil {
		t.Fatalf("file1.txt not copied: %v", err)
	}
	if string(data1) != "content1" {
		t.Errorf("file1 content = %q", string(data1))
	}

	data2, err := os.ReadFile(filepath.Join(dst, "subdir", "file2.txt"))
	if err != nil {
		t.Fatalf("subdir/file2.txt not copied: %v", err)
	}
	if string(data2) != "content2" {
		t.Errorf("file2 content = %q", string(data2))
	}
}

func TestCopyFileData_NonExistentSource_Boost(t *testing.T) {
	err := copyFileData("/nonexistent/src.txt", t.TempDir()+"/dst.txt", 0o644)
	if err == nil {
		t.Error("expected error for non-existent source")
	}
}

func TestCopyFileData_NonExistentDestDir(t *testing.T) {
	src := t.TempDir()
	srcPath := filepath.Join(src, "src.txt")
	writeMemFile(t, src, "src.txt", "data")

	// Should create the directory
	dstPath := filepath.Join(t.TempDir(), "deep", "nested", "dst.txt")
	if err := copyFileData(srcPath, dstPath, 0o644); err != nil {
		t.Fatalf("copyFileData: %v", err)
	}

	data, err := os.ReadFile(dstPath)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "data" {
		t.Errorf("content = %q", string(data))
	}
}

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
	svc := newMemorySvcInternal(MemoryScopeProject, "id", "/local", nil)
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
