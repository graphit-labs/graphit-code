package memory

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// helpers: IsImportantMemory, ImportantFileName, NormalFileName

func TestIsImportantMemory(t *testing.T) {
	tests := []struct {
		filename string
		want     bool
	}{
		{"abc_important_.md", true},
		{"01J123_important_.md", true},
		{"abc.md", false},
		{"abc_important.md", false}, // missing trailing underscore
		{"_important_.md", true},    // edge: no id prefix
		{"dir/nested_important_.md", true},
		{"abc.txt", false},
		{"", false},
	}
	for _, tc := range tests {
		t.Run(tc.filename, func(t *testing.T) {
			got := IsImportantMemory(tc.filename)
			if got != tc.want {
				t.Errorf("IsImportantMemory(%q) = %v; want %v", tc.filename, got, tc.want)
			}
		})
	}
}

func TestImportantFileName(t *testing.T) {
	tests := []struct {
		id   string
		want string
	}{
		{"123", "123_important_.md"},
		{"01J5X", "01J5X_important_.md"},
		{"", "_important_.md"},
	}
	for _, tc := range tests {
		t.Run(tc.id, func(t *testing.T) {
			got := ImportantFileName(tc.id)
			if got != tc.want {
				t.Errorf("ImportantFileName(%q) = %q; want %q", tc.id, got, tc.want)
			}
		})
	}
}

func TestNormalFileName(t *testing.T) {
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
			got := NormalFileName(tc.id)
			if got != tc.want {
				t.Errorf("NormalFileName(%q) = %q; want %q", tc.id, got, tc.want)
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

func TestFirstLine(t *testing.T) {
	tests := []struct {
		name string
		body string
		want string
	}{
		{"empty", "", ""},
		{"single line", "Hello world", "Hello world"},
		{"skip heading", "# Heading\n\nActual content", "Actual content"},
		{"skip blank", "\n\n\nContent after blanks", "Content after blanks"},
		{"truncate long line", strings.Repeat("X", 200), strings.Repeat("X", 120) + "…"},
		{"skip heading then body", "# H1\nBody text", "Body text"},
		{"all headings", "# H1\n## H2\n### H3", ""},
		{"whitespace only lines", "  \n\t\n   \n", ""},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := firstLine(tc.body)
			if got != tc.want {
				t.Errorf("firstLine(%q) = %q; want %q", tc.body, got, tc.want)
			}
		})
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

func TestListMemories(t *testing.T) {
	dir := t.TempDir()

	impContent := `---
title: Important Memory
created_at: 2026-05-20T00:00:00Z
---

# Important Memory

Details.`
	writeMemFile(t, dir, "ABC_important_.md", impContent)

	normContent := `---
title: Normal Memory
created_at: 2026-05-21T00:00:00Z
---

# Normal Memory

Content.`
	writeMemFile(t, dir, "DEF.md", normContent)

	// Create a non-md file (should be skipped)
	writeMemFile(t, dir, "notes.txt", "not a memory")

	// Create a subdirectory (should be skipped)
	if err := os.Mkdir(filepath.Join(dir, "subdir"), 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}

	svc := &MemoryService{
		scope:    MemoryScopeProject,
		scopeID:  "test-proj",
		localDir: dir,
	}

	memories, err := svc.ListMemories()
	if err != nil {
		t.Fatalf("ListMemories: %v", err)
	}
	if len(memories) != 2 {
		t.Fatalf("expected 2 memories, got %d", len(memories))
	}

	// Check that we correctly identified important vs normal
	var foundImportant, foundNormal bool
	for _, m := range memories {
		if m.ID == "ABC" && m.Important && m.Title == "Important Memory" {
			foundImportant = true
		}
		if m.ID == "DEF" && !m.Important && m.Title == "Normal Memory" {
			foundNormal = true
		}
	}
	if !foundImportant {
		t.Error("expected to find important memory with ID 'ABC'")
	}
	if !foundNormal {
		t.Error("expected to find normal memory with ID 'DEF'")
	}
}

func TestListMemories_EmptyDir(t *testing.T) {
	dir := t.TempDir()
	svc := &MemoryService{localDir: dir}
	memories, err := svc.ListMemories()
	if err != nil {
		t.Fatalf("ListMemories: %v", err)
	}
	if len(memories) != 0 {
		t.Errorf("expected 0 memories, got %d", len(memories))
	}
}

func TestListMemories_NonExistentDir(t *testing.T) {
	svc := &MemoryService{localDir: "/nonexistent/path/xyz"}
	memories, err := svc.ListMemories()
	if err != nil {
		t.Fatalf("ListMemories should return nil for non-existent dir, got error: %v", err)
	}
	if memories != nil {
		t.Errorf("expected nil, got %v", memories)
	}
}

func TestListImportantInDir(t *testing.T) {
	dir := t.TempDir()

	impContent := `---
title: Critical Rule
---

# Critical Rule

Always do X before Y.`
	writeMemFile(t, dir, "RULE1_important_.md", impContent)

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

func TestGenerateMemoryWiki(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()

	mem1 := `---
title: Convention Alpha
type: convention
created_at: 2026-05-20T00:00:00Z
---

# Convention Alpha

Always follow alpha pattern.`

	mem2 := `---
title: Fact Beta
type: fact
created_at: 2026-05-21T00:00:00Z
important: true
---

# Fact Beta

Beta is the second letter.`

	writeMemFile(t, rawDir, "MEM1.md", mem1)
	writeMemFile(t, rawDir, "MEM2_important_.md", mem2)

	ctx := context.Background()
	result, err := GenerateMemoryWiki(ctx, rawDir, wikiDir)
	if err != nil {
		t.Fatalf("GenerateMemoryWiki: %v", err)
	}
	if result.ArticlesWritten != 2 {
		t.Errorf("ArticlesWritten = %d; want 2", result.ArticlesWritten)
	}

	// Verify entity pages were written to wikiDir
	entries, readErr := os.ReadDir(wikiDir)
	if readErr != nil {
		t.Fatalf("reading wiki dir: %v", readErr)
	}
	if len(entries) < 2 {
		t.Errorf("expected at least 2 files in wiki dir, got %d", len(entries))
	}
}

func TestGenerateMemoryWiki_EmptyDir(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()

	ctx := context.Background()
	result, err := GenerateMemoryWiki(ctx, rawDir, wikiDir)
	if err != nil {
		t.Fatalf("GenerateMemoryWiki: %v", err)
	}
	if result.ArticlesWritten != 0 {
		t.Errorf("ArticlesWritten = %d; want 0", result.ArticlesWritten)
	}
}

func TestGenerateMemoryWiki_NonExistentRawDir(t *testing.T) {
	wikiDir := t.TempDir()
	ctx := context.Background()
	result, err := GenerateMemoryWiki(ctx, "/nonexistent/raw", wikiDir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.ArticlesWritten != 0 {
		t.Errorf("ArticlesWritten = %d; want 0", result.ArticlesWritten)
	}
}

func TestGenerateMemoryWiki_SkipsIndexAndLog(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()

	// These should be skipped during wiki generation
	writeMemFile(t, rawDir, "index.md", "---\ntitle: Index\n---\n")
	writeMemFile(t, rawDir, "log.md", "---\ntitle: Log\n---\n")
	writeMemFile(t, rawDir, "Memory_Wiki_Something.md", "---\ntitle: Wiki\n---\n")

	// This one should be processed
	writeMemFile(t, rawDir, "VALID.md", "---\ntitle: Valid Memory\n---\n# Valid Memory\n\nContent.")

	ctx := context.Background()
	result, err := GenerateMemoryWiki(ctx, rawDir, wikiDir)
	if err != nil {
		t.Fatalf("GenerateMemoryWiki: %v", err)
	}
	if result.ArticlesWritten != 1 {
		t.Errorf("ArticlesWritten = %d; want 1", result.ArticlesWritten)
	}
}

// RunCycle (filesystem-based)

func TestRunCycle_NonExistentRawDir(t *testing.T) {
	ctx := context.Background()
	result := RunCycle(ctx, "test-scope", "/nonexistent/raw/dir", t.TempDir())
	if result.Scope != "test-scope" {
		t.Errorf("Scope = %q; want 'test-scope'", result.Scope)
	}
	if result.Err != nil {
		t.Errorf("expected nil error, got: %v", result.Err)
	}
	if result.WikiFiles != 0 {
		t.Errorf("WikiFiles = %d; want 0", result.WikiFiles)
	}
}

func TestRunCycle_WithValidData(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()

	content := `---
title: Test Memory
type: fact
created_at: 2026-05-20T00:00:00Z
---

# Test Memory

Content for testing.`
	writeMemFile(t, rawDir, "TEST1.md", content)

	ctx := context.Background()
	result := RunCycle(ctx, "project", rawDir, wikiDir)
	if result.Err != nil {
		t.Fatalf("RunCycle error: %v", result.Err)
	}
	if result.WikiFiles != 1 {
		t.Errorf("WikiFiles = %d; want 1", result.WikiFiles)
	}
}

func TestMemoryBranch(t *testing.T) {
	tests := []struct {
		scope   string
		scopeID string
		want    string
	}{
		{"project", "abc123", "memory/project/abc123"},
		{"user", "userhash", "memory/user/userhash"},
		{"my-context", "ctx-id", "memory/project/my-context"},
	}
	for _, tc := range tests {
		t.Run(tc.scope+"_"+tc.scopeID, func(t *testing.T) {
			got := memoryBranch(tc.scope, tc.scopeID)
			if got != tc.want {
				t.Errorf("memoryBranch(%q, %q) = %q; want %q", tc.scope, tc.scopeID, got, tc.want)
			}
		})
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

func TestMemoryEntityPage(t *testing.T) {
	page := memoryEntityPageWithHash("ID1", "Test Memory", "2026-05-20T00:00:00Z", true, "Content here.", "convention", "")
	checks := []struct {
		desc   string
		substr string
	}{
		{"title frontmatter", "title: Test Memory"},
		{"id frontmatter", "id: ID1"},
		{"H1 heading", "# Test Memory"},
		{"body", "Content here."},
		{"important note", "⭐ **Important memory**"},
		{"type frontmatter", "type: convention"},
		{"tags memory", "- memory"},
		{"tags important", "- important"},
		{"tags convention", "- convention"},
	}
	for _, c := range checks {
		if !strings.Contains(page, c.substr) {
			t.Errorf("[%s] page should contain %q", c.desc, c.substr)
		}
	}
}

func TestMemoryEntityPage_NotImportant(t *testing.T) {
	page := memoryEntityPageWithHash("ID2", "Simple", "", false, "Body.", "", "")
	if strings.Contains(page, "Important memory") {
		t.Error("should not contain important badge")
	}
	if strings.Contains(page, "**Type:**") {
		t.Error("should not contain type badge when type is empty")
	}
}

// RenderImportantBlock (filesystem-based with listImportantInDir)

func TestRenderImportantBlock_Inline(t *testing.T) {
	dir := t.TempDir()
	content := `---
title: Critical Rule
---

# Critical Rule

Always follow this rule.`
	writeMemFile(t, dir, "RULE1_important_.md", content)

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

	err = svc.SyncToLocal()
	if err == nil {
		t.Error("SyncToLocal should error without gitStore")
	}
}

func TestAppendMemLog_NewFile(t *testing.T) {
	dir := t.TempDir()
	logPath := filepath.Join(dir, "log.md")

	appendMemLog(logPath, 5, 3, nil)

	data, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	content := string(data)
	if !strings.Contains(content, "Memory Wiki Log") {
		t.Error("should contain header")
	}
	if !strings.Contains(content, "Memories: 5") {
		t.Error("should contain memory count")
	}
	if !strings.Contains(content, "Articles written: 3") {
		t.Error("should contain articles count")
	}
}

func TestAppendMemLog_ExistingFile(t *testing.T) {
	dir := t.TempDir()
	logPath := filepath.Join(dir, "log.md")

	appendMemLog(logPath, 3, 2, nil)
	appendMemLog(logPath, 5, 4, nil)

	data, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	content := string(data)

	// Should contain both entries
	if !strings.Contains(content, "Memories: 5") {
		t.Error("should contain latest entry")
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

func TestImportantMemorySuffix(t *testing.T) {
	if ImportantMemorySuffix != "_important_" {
		t.Errorf("ImportantMemorySuffix = %q; want '_important_'", ImportantMemorySuffix)
	}
}

func writeMemFile(t *testing.T, dir, name, content string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, name), []byte(content), 0o644); err != nil {
		t.Fatalf("writeMemFile(%s): %v", name, err)
	}
}
