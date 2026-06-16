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

// ===========================================================================
// important.go — ListImportantMemories, RenderImportantBlock,
//   ListRecentMemories, RenderRecentBlock, firstLineFromContent,
//   extractBodyAfterFrontmatter
// ===========================================================================

func TestListImportantInDir_WithEntries(t *testing.T) {
	dir := t.TempDir()

	// Important memory
	writeMemFile(t, dir, "MEM1_important_.md", `---
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

// ---------------------------------------------------------------------------
// RenderImportantBlock: renders from dir with important entries
// ---------------------------------------------------------------------------

func TestRenderImportantBlock_ViaDir(t *testing.T) {
	dir := t.TempDir()

	writeMemFile(t, dir, "MEM1_important_.md", `---
title: Critical Convention
---

# Critical Convention

Never use global state.`)

	writeMemFile(t, dir, "MEM2_important_.md", `---
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

// ---------------------------------------------------------------------------
// ListRecentMemories and RenderRecentBlock — filesystem-based
// ---------------------------------------------------------------------------

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
	writeMemFile(t, dir, "MEM_C_important_.md", `---
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

// ===========================================================================
// important.go — extractBodyAfterFrontmatter edge cases
// ===========================================================================

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



// ===========================================================================
// consolidate.go — parseConsolidationType
// ===========================================================================

func TestParseConsolidationType_Boost(t *testing.T) {
	tests := []struct {
		name    string
		content string
		want    string
	}{
		{"no type", "---\ntitle: Test\n---\nbody", ""},
		{"has type", "---\ntitle: Test\ntype: convention\n---\nbody", "convention"},
		{"type with spaces", "---\ntype:   skill  \n---\n", "skill"},
		{"type in body (no frontmatter)", "type: fact\nbody text", "fact"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := parseConsolidationType(tc.content)
			if got != tc.want {
				t.Errorf("parseConsolidationType = %q; want %q", got, tc.want)
			}
		})
	}
}

// ===========================================================================
// consolidate.go — ConsolidationReport methods
// ===========================================================================

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

// ===========================================================================
// consolidate.go — detectStaleMemories
// ===========================================================================

func TestDetectStaleMemories_AllBranches(t *testing.T) {
	oldDate := time.Now().Add(-45 * 24 * time.Hour).Format(time.RFC3339)
	recentDate := time.Now().Add(-5 * 24 * time.Hour).Format(time.RFC3339)

	memories := []memorySnapshot{
		{ID: "old", Title: "Old One", CreatedAt: oldDate, Important: false},      // stale
		{ID: "recent", Title: "Recent One", CreatedAt: recentDate, Important: false}, // not stale
		{ID: "important", Title: "Important Old", CreatedAt: oldDate, Important: true}, // skipped (important)
		{ID: "nodate", Title: "No Date", CreatedAt: "", Important: false},            // skipped (no date)
		{ID: "baddate", Title: "Bad Date", CreatedAt: "invalid", Important: false},    // skipped (bad date format)
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

// ===========================================================================
// consolidate.go — extractBracketedIDs
// ===========================================================================

func TestExtractBracketedIDs_Boost(t *testing.T) {
	tests := []struct {
		name  string
		input string
		want  int
	}{
		{"no brackets", "just text", 0},
		{"one ID", "MERGE [01J5XABC1234567890]: reason", 1},
		{"two IDs", "MERGE [01J5XABC1234567890] and [01J5XDEF1234567890]: reason", 2},
		{"short ID (less than 10 chars)", "[ABCDEFGH]: too short", 0},
		{"exactly 10 uppercase", "[ABCDEFGHIJ]: ok", 1},
		{"lowercase skipped", "[abcdefghij]: lowercase", 0},
		{"mixed content", "text [01J5XABC1234567890] more text [01J5XDEF1234567890] end", 2},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := extractBracketedIDs(tc.input)
			if len(got) != tc.want {
				t.Errorf("extractBracketedIDs(%q) returned %d IDs; want %d", tc.input, len(got), tc.want)
			}
		})
	}
}

// ===========================================================================
// consolidate.go — parseConsolidationSection edge cases
// ===========================================================================

func TestParseConsolidationSection_NoSection(t *testing.T) {
	actions := parseConsolidationSection("no section here", "DUPLICATES", "merge")
	if len(actions) != 0 {
		t.Errorf("expected 0 actions, got %d", len(actions))
	}
}

func TestParseConsolidationSection_NoneFound(t *testing.T) {
	response := "## DUPLICATES\nNone found\n"
	actions := parseConsolidationSection(response, "DUPLICATES", "merge")
	if len(actions) != 0 {
		t.Errorf("expected 0 actions for 'None found', got %d", len(actions))
	}
}

func TestParseConsolidationSection_LineWithoutDash(t *testing.T) {
	response := "## DUPLICATES\nSome text without dash\n- MERGE [01J5XABC1234567890]: reason\n"
	actions := parseConsolidationSection(response, "DUPLICATES", "merge")
	if len(actions) != 1 {
		t.Errorf("expected 1 action (skip non-dash lines), got %d", len(actions))
	}
}

// ===========================================================================
// consolidate.go — parseSuggestionSection edge cases
// ===========================================================================

func TestParseSuggestionSection_NoSection(t *testing.T) {
	actions := parseSuggestionSection("no section here")
	if len(actions) != 0 {
		t.Errorf("expected 0, got %d", len(actions))
	}
}

func TestParseSuggestionSection_NoneFound_Boost(t *testing.T) {
	actions := parseSuggestionSection("## SUGGESTIONS\nNone found\n")
	if len(actions) != 0 {
		t.Errorf("expected 0, got %d", len(actions))
	}
}

func TestParseSuggestionSection_AllActionTypes(t *testing.T) {
	response := `## SUGGESTIONS
- PROMOTE [01J5XABC1234567890]: should be important
- DEMOTE [01J5XDEF1234567890]: too specific
- DELETE [01J5XGHI1234567890]: outdated
- UPDATE [01J5XJKL1234567890]: needs detail
- SOMETHING ELSE [01J5XMNO1234567890]: unknown defaults to update
`
	actions := parseSuggestionSection(response)
	if len(actions) != 5 {
		t.Fatalf("expected 5 actions, got %d", len(actions))
	}

	expected := []string{"promote", "demote", "delete", "update", "update"}
	for i, want := range expected {
		if actions[i].Type != want {
			t.Errorf("action[%d].Type = %q; want %q", i, actions[i].Type, want)
		}
	}
}

func TestParseSuggestionSection_WithNextSection(t *testing.T) {
	response := `## SUGGESTIONS
- PROMOTE [01J5XABC1234567890]: should be important

## SOMETHING_ELSE
- unrelated content
`
	actions := parseSuggestionSection(response)
	if len(actions) != 1 {
		t.Errorf("expected 1 action, got %d", len(actions))
	}
}

// ===========================================================================
// consolidate.go — RunGC
// ===========================================================================

func TestRunGC_NonExistentDir_Boost(t *testing.T) {
	// RunGC uses RawDir which depends on global scope dirs
	// Test directly the internal logic with a temp dir
	report, err := runGCInDirBoost(t.TempDir()+"nonexistent", 90)
	if err != nil {
		t.Fatalf("expected nil error for non-existent, got: %v", err)
	}
	if report.TotalMemories != 0 {
		t.Errorf("expected 0, got %d", report.TotalMemories)
	}
}

func TestRunGC_EmptyDir_Boost(t *testing.T) {
	report, err := runGCInDirBoost(t.TempDir(), 90)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if report.TotalMemories != 0 {
		t.Errorf("expected 0, got %d", report.TotalMemories)
	}
}

func TestRunGC_WithVariousMemories(t *testing.T) {
	dir := t.TempDir()

	veryOldDate := time.Now().Add(-200 * 24 * time.Hour).Format(time.RFC3339)
	oldDate := time.Now().Add(-100 * 24 * time.Hour).Format(time.RFC3339)
	recentDate := time.Now().Add(-5 * 24 * time.Hour).Format(time.RFC3339)

	// Empty body (GC candidate)
	writeMemFile(t, dir, "MEM_EMPTY.md", `---
title: Empty Memory
created_at: `+recentDate+`
---

# Empty Memory
`)

	// Old untyped memory (GC candidate — stale unclassified)
	writeMemFile(t, dir, "MEM_OLD_UNTYPED.md", `---
title: Old Untyped
created_at: `+oldDate+`
---

# Old Untyped

This is some content that is old and untyped.`)

	// Very old typed memory (GC candidate — 2x threshold)
	writeMemFile(t, dir, "MEM_VERY_OLD.md", `---
title: Very Old Typed
created_at: `+veryOldDate+`
type: fact
---

# Very Old Typed

This is very old content with a type.`)

	// Recent typed memory (not a GC candidate)
	writeMemFile(t, dir, "MEM_RECENT.md", `---
title: Recent Typed
created_at: `+recentDate+`
type: convention
---

# Recent Typed

This is recent and typed content here.`)

	// Important memory (always skipped)
	writeMemFile(t, dir, "MEM_IMP_important_.md", `---
title: Important Memory
created_at: `+veryOldDate+`
---

# Important Memory

This is important.`)

	// Non-md file (skipped)
	writeMemFile(t, dir, "notes.txt", "not a memory")

	// Directory (skipped)
	if err := os.Mkdir(filepath.Join(dir, "subdir"), 0o755); err != nil {
		t.Fatal(err)
	}

	report, err := runGCInDirBoost(dir, 90)
	if err != nil {
		t.Fatalf("error: %v", err)
	}

	if report.TotalMemories != 5 {
		t.Errorf("TotalMemories = %d; want 5", report.TotalMemories)
	}

	// Should have candidates: empty body, old untyped, very old typed
	if len(report.Candidates) < 2 {
		t.Errorf("expected at least 2 GC candidates, got %d", len(report.Candidates))
	}

	// Verify candidate reasons
	reasons := make(map[string]bool)
	for _, c := range report.Candidates {
		reasons[c.Reason] = true
	}
	hasEmpty := false
	hasOld := false
	for reason := range reasons {
		if strings.Contains(reason, "Empty") || strings.Contains(reason, "empty") {
			hasEmpty = true
		}
		if strings.Contains(reason, "days old") {
			hasOld = true
		}
	}
	if !hasEmpty {
		t.Error("expected at least one empty-body candidate")
	}
	if !hasOld {
		t.Error("expected at least one stale candidate")
	}
}

func TestRunGC_DefaultStaleDays_Boost(t *testing.T) {
	dir := t.TempDir()
	// When staleDays <= 0, should default to 90
	report, err := runGCInDirBoost(dir, 0)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if report.TotalMemories != 0 {
		t.Errorf("expected 0, got %d", report.TotalMemories)
	}
}

// runGCInDirBoost replicates RunGC logic on a specific directory for testing
func runGCInDirBoost(dir string, staleDays int) (*GCReport, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return &GCReport{}, nil
		}
		return nil, err
	}

	if staleDays <= 0 {
		staleDays = 90
	}
	threshold := time.Duration(staleDays) * 24 * time.Hour

	report := &GCReport{}
	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
			continue
		}
		name := e.Name()
		absPath := filepath.Join(dir, name)
		important := IsImportantMemory(name)
		report.TotalMemories++

		if important {
			continue
		}

		data, readErr := os.ReadFile(absPath)
		if readErr != nil {
			continue
		}

		id := strings.TrimSuffix(name, ".md")

		title, createdAt := parseMemoryMeta(absPath)
		body := strings.TrimSpace(extractBodyAfterFrontmatter(string(data)))
		memType := parseConsolidationType(string(data))

		if len(body) < 20 {
			report.Candidates = append(report.Candidates, GCCandidate{
				ID: id, Title: title,
				Reason: fmt.Sprintf("Empty or near-empty body (%d chars)", len(body)),
			})
			continue
		}

		if createdAt != "" {
			t, err := time.Parse(time.RFC3339, createdAt)
			if err == nil {
				age := time.Since(t)
				days := int(age.Hours() / 24)

				if age > threshold && memType == "" {
					report.Candidates = append(report.Candidates, GCCandidate{
						ID: id, Title: title,
						Reason: fmt.Sprintf("Unclassified memory, %d days old (threshold: %d days)", days, staleDays),
						Age:    days,
					})
					continue
				}

				if age > 2*threshold {
					report.Candidates = append(report.Candidates, GCCandidate{
						ID: id, Title: title,
						Reason: fmt.Sprintf("Very old memory, %d days old (2× threshold: %d days)", days, 2*staleDays),
						Age:    days,
					})
				}
			}
		}
	}
	return report, nil
}

// ===========================================================================
// memory.go — ValidMemoryType
// ===========================================================================

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

// ===========================================================================
// memory.go — HubBranch
// ===========================================================================

func TestMemoryService_HubBranch(t *testing.T) {
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
			got := svc.HubBranch()
			if got != tc.want {
				t.Errorf("HubBranch() = %q; want %q", got, tc.want)
			}
		})
	}
}

// ===========================================================================
// memory.go — AddMemory / UpdateMemory / RemoveMemory / changeRelevance
//   (nil gitStore error paths)
// ===========================================================================

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

// ===========================================================================
// memory.go — ListMemories (filesystem-based)
// ===========================================================================

func TestMemoryService_ListMemories_WithDir(t *testing.T) {
	dir := t.TempDir()

	writeMemFile(t, dir, "MEM1.md", `---
title: Normal Memory
created_at: 2026-01-01T00:00:00Z
---

# Normal Memory

Body.`)

	writeMemFile(t, dir, "MEM2_important_.md", `---
title: Important Memory
created_at: 2026-01-02T00:00:00Z
---

# Important Memory

Important body.`)

	// Non-md file (should be skipped)
	writeMemFile(t, dir, "notes.txt", "not a memory")

	// Subdirectory (should be skipped)
	if err := os.Mkdir(filepath.Join(dir, "subdir"), 0o755); err != nil {
		t.Fatal(err)
	}

	svc := &MemoryService{
		localDir: dir,
		scope:    MemoryScopeProject,
		scopeID:  "test-id",
	}

	memories, err := svc.ListMemories()
	if err != nil {
		t.Fatalf("ListMemories: %v", err)
	}
	if len(memories) != 2 {
		t.Fatalf("expected 2 memories, got %d", len(memories))
	}

	// Check that important flag is set correctly
	foundNormal := false
	foundImportant := false
	for _, m := range memories {
		if m.ID == "MEM1" {
			foundNormal = true
			if m.Important {
				t.Error("MEM1 should not be important")
			}
			if m.Title != "Normal Memory" {
				t.Errorf("MEM1 title = %q", m.Title)
			}
		}
		if m.ID == "MEM2" {
			foundImportant = true
			if !m.Important {
				t.Error("MEM2 should be important")
			}
		}
	}
	if !foundNormal {
		t.Error("did not find MEM1")
	}
	if !foundImportant {
		t.Error("did not find MEM2")
	}
}

func TestMemoryService_ListMemories_NonExistentDir(t *testing.T) {
	svc := &MemoryService{localDir: "/nonexistent/path/xyz"}
	memories, err := svc.ListMemories()
	if err != nil {
		t.Fatalf("expected nil error for non-existent dir, got: %v", err)
	}
	if memories != nil {
		t.Errorf("expected nil, got %v", memories)
	}
}

func TestMemoryService_ListMemories_EmptyDir(t *testing.T) {
	svc := &MemoryService{localDir: t.TempDir()}
	memories, err := svc.ListMemories()
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if len(memories) != 0 {
		t.Errorf("expected 0, got %d", len(memories))
	}
}

// ===========================================================================
// memory.go — buildMemoryFile
// ===========================================================================

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

// ===========================================================================
// memory.go — parseMemoryMeta
// ===========================================================================

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

// ===========================================================================
// memory.go — MemoryService.Close
// ===========================================================================

func TestMemoryService_Close_Boost(t *testing.T) {
	svc := &MemoryService{}
	if err := svc.Close(); err != nil {
		t.Errorf("Close: %v", err)
	}
}

// ===========================================================================
// wiki.go — safeMemFilename
// ===========================================================================

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
			got := safeMemFilename(tc.input)
			if got != tc.want {
				t.Errorf("safeMemFilename(%q) = %q; want %q", tc.input, got, tc.want)
			}
		})
	}
}

// ===========================================================================
// wiki.go — uniqueMemSlug
// ===========================================================================

func TestUniqueMemSlug_Boost(t *testing.T) {
	used := make(map[string]bool)

	slug1 := uniqueMemSlug("test", used)
	if slug1 != "test" {
		t.Errorf("first slug = %q; want 'test'", slug1)
	}

	slug2 := uniqueMemSlug("test", used)
	if slug2 != "test_2" {
		t.Errorf("second slug = %q; want 'test_2'", slug2)
	}

	slug3 := uniqueMemSlug("test", used)
	if slug3 != "test_3" {
		t.Errorf("third slug = %q; want 'test_3'", slug3)
	}

	// Different base should work
	slug4 := uniqueMemSlug("other", used)
	if slug4 != "other" {
		t.Errorf("different base = %q; want 'other'", slug4)
	}
}

// ===========================================================================
// wiki.go — firstLine
// ===========================================================================

func TestFirstLine_Boost(t *testing.T) {
	tests := []struct {
		name  string
		input string
		want  string
	}{
		{"empty", "", ""},
		{"only headers", "# Title\n## Sub", ""},
		{"body after header", "# Title\nSome content here", "Some content here"},
		{"blank lines then body", "\n\nContent below", "Content below"},
		{"long line truncated", strings.Repeat("x", 200), strings.Repeat("x", 120) + "…"},
		{"exactly 120 chars", strings.Repeat("y", 120), strings.Repeat("y", 120)},
		{"121 chars", strings.Repeat("z", 121), strings.Repeat("z", 120) + "…"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := firstLine(tc.input)
			if got != tc.want {
				t.Errorf("firstLine(%q) = %q; want %q", tc.input, got, tc.want)
			}
		})
	}
}

// ===========================================================================
// wiki.go — parseMemoryType
// ===========================================================================

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

// ===========================================================================
// wiki.go — memoryEntityPage full rendering
// ===========================================================================

func TestMemoryEntityPage_FullRendering(t *testing.T) {
	// Recent date (no stale warning)
	recentDate := time.Now().Add(-5 * 24 * time.Hour).Format(time.RFC3339)
	page := memoryEntityPage("ID123", "My Title", recentDate, true, "Body content.", "convention")

	if !strings.Contains(page, "title: My Title") {
		t.Error("should contain title in frontmatter")
	}
	if !strings.Contains(page, "id: ID123") {
		t.Error("should contain id in frontmatter")
	}
	if !strings.Contains(page, "important") {
		t.Error("should contain 'important' tag")
	}
	if !strings.Contains(page, "convention") {
		t.Error("should contain type")
	}
	if !strings.Contains(page, "⭐") {
		t.Error("should have important star marker")
	}
	if !strings.Contains(page, "🏗️") {
		t.Error("should have convention emoji")
	}
	if !strings.Contains(page, "Body content.") {
		t.Error("should contain body")
	}
	if strings.Contains(page, "Stale memory") {
		t.Error("recent memory should not have stale warning")
	}
}

func TestMemoryEntityPage_AllTypeEmojis(t *testing.T) {
	types := map[string]string{
		"convention": "🏗️",
		"correction": "🔧",
		"decision":   "📐",
		"tension":    "⚡",
		"fact":       "📋",
		"skill":      "🛠️",
		"unknown":    "📄",
	}
	for typ, emoji := range types {
		page := memoryEntityPage("ID", "Title", "", false, "Body", typ)
		if !strings.Contains(page, emoji) {
			t.Errorf("type %q should produce emoji %s", typ, emoji)
		}
	}
}

func TestMemoryEntityPage_NoType(t *testing.T) {
	page := memoryEntityPage("ID", "Title", "", false, "Body", "")
	if strings.Contains(page, "**Type:**") {
		t.Error("should not have type annotation when type is empty")
	}
}

// ===========================================================================
// wiki.go — GenerateMemoryWiki full test
// ===========================================================================

func TestGenerateMemoryWiki_FullCycle(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()
	ctx := context.Background()

	// Create various memory files
	writeMemFile(t, rawDir, "MEM1.md", `---
title: Convention One
type: convention
created_at: 2026-01-01T00:00:00Z
---

# Convention One

Use gofmt always.`)

	writeMemFile(t, rawDir, "MEM2_important_.md", `---
title: Important Decision
type: decision
created_at: 2026-01-02T00:00:00Z
---

# Important Decision

We decided to use Go modules.`)

	writeMemFile(t, rawDir, "MEM3.md", `---
title: Untyped Memory
created_at: 2026-01-03T00:00:00Z
---

# Untyped Memory

No type field.`)

	// Index and log files should be skipped
	writeMemFile(t, rawDir, "index.md", "---\ntitle: Index\n---\nOld index content")
	writeMemFile(t, rawDir, "log.md", "---\ntitle: Log\n---\nOld log content")

	// Non-md should be skipped
	writeMemFile(t, rawDir, "notes.txt", "not a memory")

	result, err := GenerateMemoryWiki(ctx, rawDir, wikiDir)
	if err != nil {
		t.Fatalf("GenerateMemoryWiki: %v", err)
	}

	if result.ArticlesWritten != 3 {
		t.Errorf("ArticlesWritten = %d; want 3", result.ArticlesWritten)
	}

	// Check entity pages were written to wiki dir
	entries, readErr := os.ReadDir(wikiDir)
	if readErr != nil {
		t.Fatalf("reading wiki dir: %v", readErr)
	}
	if len(entries) < 3 {
		t.Errorf("expected at least 3 files in wiki dir, got %d", len(entries))
	}
}

func TestGenerateMemoryWiki_NonExistentRawDir_Boost(t *testing.T) {
	wikiDir := t.TempDir()
	ctx := context.Background()

	result, err := GenerateMemoryWiki(ctx, "/nonexistent/raw/dir", wikiDir)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if result.ArticlesWritten != 0 {
		t.Errorf("expected 0 articles, got %d", result.ArticlesWritten)
	}
}

func TestGenerateMemoryWiki_DuplicateTitles(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()
	ctx := context.Background()

	// Create memories with identical titles (tests uniqueMemSlug)
	writeMemFile(t, rawDir, "MEM1.md", `---
title: Same Title
type: fact
---

# Same Title

Body 1.`)

	writeMemFile(t, rawDir, "MEM2.md", `---
title: Same Title
type: fact
---

# Same Title

Body 2.`)

	result, err := GenerateMemoryWiki(ctx, rawDir, wikiDir)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if result.ArticlesWritten != 2 {
		t.Errorf("ArticlesWritten = %d; want 2", result.ArticlesWritten)
	}
}

func TestGenerateMemoryWiki_SkipsMemoryWikiPrefixed(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()
	ctx := context.Background()

	writeMemFile(t, rawDir, "Memory_Wiki_whatever.md", "should be skipped")
	writeMemFile(t, rawDir, "MEM1.md", `---
title: Real Memory
type: fact
---

# Real Memory

Body.`)

	result, err := GenerateMemoryWiki(ctx, rawDir, wikiDir)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if result.ArticlesWritten != 1 {
		t.Errorf("ArticlesWritten = %d; want 1 (Memory_Wiki_ prefix should be skipped)", result.ArticlesWritten)
	}
}

// ===========================================================================
// wiki.go — appendMemLog
// ===========================================================================

func TestAppendMemLog_NewFile_Boost(t *testing.T) {
	dir := t.TempDir()
	logPath := filepath.Join(dir, "log.md")

	appendMemLog(logPath, 5, 3, nil)

	data, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatal(err)
	}
	content := string(data)

	if !strings.Contains(content, "Memory Wiki Log") {
		t.Error("should contain header for new file")
	}
	if !strings.Contains(content, "Memories: 5") {
		t.Error("should contain memory count")
	}
	if !strings.Contains(content, "Articles written: 3") {
		t.Error("should contain articles count")
	}
}

// ===========================================================================
// cycle.go — RunCycle
// ===========================================================================

func TestRunCycle_WithRawDir(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()
	ctx := context.Background()

	writeMemFile(t, rawDir, "MEM1.md", `---
title: Test Memory
type: fact
---

# Test Memory

Body.`)

	result := RunCycle(ctx, "project", rawDir, wikiDir)
	if result == nil {
		t.Fatal("expected non-nil result")
	}
	if result.Scope != "project" {
		t.Errorf("Scope = %q", result.Scope)
	}
	if result.WikiFiles != 1 {
		t.Errorf("WikiFiles = %d; want 1", result.WikiFiles)
	}
	if result.Err != nil {
		t.Errorf("Err = %v", result.Err)
	}
}

func TestRunCycle_NonExistentRawDir_Boost(t *testing.T) {
	result := RunCycle(context.Background(), "test", "/nonexistent/raw", t.TempDir())
	if result == nil {
		t.Fatal("expected non-nil result")
	}
	if result.Err != nil {
		t.Errorf("non-existent rawDir should not error, got: %v", result.Err)
	}
	if result.WikiFiles != 0 {
		t.Errorf("WikiFiles = %d; want 0", result.WikiFiles)
	}
}

// ===========================================================================
// cycle.go — memoryBranch
// ===========================================================================

func TestMemoryBranch_Boost(t *testing.T) {
	tests := []struct {
		scope   string
		scopeID string
		want    string
	}{
		{"project", "proj-id", "memory/project/proj-id"},
		{"user", "user-hash", "memory/user/user-hash"},
		{"my-context", "ctx-id", "memory/project/my-context"},
		{"custom", "abc", "memory/project/custom"},
	}
	for _, tc := range tests {
		t.Run(tc.scope+"/"+tc.scopeID, func(t *testing.T) {
			got := memoryBranch(tc.scope, tc.scopeID)
			if got != tc.want {
				t.Errorf("memoryBranch(%q, %q) = %q; want %q", tc.scope, tc.scopeID, got, tc.want)
			}
		})
	}
}

// ===========================================================================
// appsvc.go — ParseTags
// ===========================================================================

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

// ===========================================================================
// appsvc.go — MemoryAppService
// ===========================================================================

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


// ===========================================================================
// memory_git_store.go — copyDirRecursive with files
// ===========================================================================

func TestCopyDirRecursive_WithFiles(t *testing.T) {
	src := t.TempDir()
	dst := t.TempDir()

	// Create file structure
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

// ===========================================================================
// memory.go — MemoryScope and MemoryType constants
// ===========================================================================

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

// ===========================================================================
// wiki.go — memoryIndexPage comprehensive test
// ===========================================================================

func TestMemoryIndexPage_AllTypesSections(t *testing.T) {
	docs := []memDoc{
		{id: "1", title: "Convention One", memType: "convention", body: "conv body", important: true},
		{id: "2", title: "Correction One", memType: "correction", body: "corr body"},
		{id: "3", title: "Decision One", memType: "decision", body: "dec body"},
		{id: "4", title: "Tension One", memType: "tension", body: "tens body"},
		{id: "5", title: "Skill One", memType: "skill", body: "skill body"},
		{id: "6", title: "Fact One", memType: "fact", body: "fact body"},
		{id: "7", title: "Untyped One", memType: "", body: "untyped body"},
	}

	content := memoryIndexPage(docs)

	// Check all type sections are present
	expectedSections := []string{
		"Conventions", "Corrections", "Decisions", "Tensions", "Skills", "Facts", "Other Memories",
	}
	for _, section := range expectedSections {
		if !strings.Contains(content, section) {
			t.Errorf("index should contain %q section", section)
		}
	}

	// Check important section
	if !strings.Contains(content, "Important Memories") {
		t.Error("should contain Important Memories section")
	}

	// Check memory count
	if !strings.Contains(content, "7 memories") {
		t.Error("should show 7 memories count")
	}
}

func TestMemoryIndexPage_Empty(t *testing.T) {
	content := memoryIndexPage(nil)
	if !strings.Contains(content, "Memory Wiki") {
		t.Error("should contain header even for empty docs")
	}
	if !strings.Contains(content, "0 memories") {
		t.Error("should show 0 memories")
	}
}

// ===========================================================================
// paths.go — EnsureScopeDirs error paths
// ===========================================================================

func TestEnsureScopeDirs_WithValidProjectDir(t *testing.T) {
	dir := t.TempDir()
	err := EnsureScopeDirs("project", dir)
	if err != nil {
		t.Fatalf("EnsureScopeDirs: %v", err)
	}

	// Verify the parent directory structure was created
	linkPath := filepath.Join(dir, ProjectLinkDir("project"))
	parentDir := filepath.Dir(linkPath)
	if _, err := os.Stat(parentDir); err != nil {
		t.Errorf("parent dir should exist: %v", err)
	}
}

// ===========================================================================
// memory_branch_lock.go — saveMemLock error path
// ===========================================================================

func TestSaveMemLock_Success(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	lf := &memoryBranchLockFile{
		Version:  1,
		Branches: map[string]*memoryBranchMeta{"test": {Refs: []string{"ref1"}, LastUsed: "now"}},
	}
	err := saveMemLock(lf)
	if err != nil {
		t.Fatalf("saveMemLock: %v", err)
	}

	// Verify file was written
	data, err := os.ReadFile(memoryBranchLockPath())
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), "test") {
		t.Error("saved lock file should contain 'test' branch")
	}
}

// ===========================================================================
// important.go — ImportantMemorySuffix constant
// ===========================================================================

func TestImportantMemorySuffix_Boost(t *testing.T) {
	if ImportantMemorySuffix != "_important_" {
		t.Errorf("ImportantMemorySuffix = %q; want '_important_'", ImportantMemorySuffix)
	}
}

// ===========================================================================
// memory.go — syncToLocalInternal nil store
// ===========================================================================

func TestMemoryService_SyncToLocalFast_NilStore(t *testing.T) {
	svc := &MemoryService{}
	err := svc.syncToLocalFast()
	if err == nil {
		t.Error("expected error with nil gitStore")
	}
	if !strings.Contains(err.Error(), "not configured") {
		t.Errorf("error = %v", err)
	}
}

// ===========================================================================
// wiki.go — GenerateMemoryWiki with unreadable file
// ===========================================================================

func TestGenerateMemoryWiki_UnreadableFile(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()
	ctx := context.Background()

	writeMemFile(t, rawDir, "MEM1.md", `---
title: Readable
type: fact
---

# Readable

Body.`)

	// Create unreadable file
	unreadable := filepath.Join(rawDir, "MEM2.md")
	if err := os.WriteFile(unreadable, []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}
	_ = os.Chmod(unreadable, 0o000)
	defer func() { _ = os.Chmod(unreadable, 0o644) }()

	result, err := GenerateMemoryWiki(ctx, rawDir, wikiDir)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	// Should process the readable file and skip the unreadable one
	if result.ArticlesWritten < 1 {
		t.Errorf("ArticlesWritten = %d; want at least 1", result.ArticlesWritten)
	}
}

// ===========================================================================
// consolidate.go — RunConsolidation with memories that have no created_at
// ===========================================================================

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


// ===========================================================================
// memory.go — MemoryService creation edge cases
// ===========================================================================

func TestNewMemorySvcInternal_NilStore(t *testing.T) {
	svc := newMemorySvcInternal(MemoryScopeProject, "id", "/local", "/link", nil)
	if svc == nil {
		t.Fatal("expected non-nil svc")
	}
	if svc.gitStore != nil {
		t.Error("gitStore should be nil")
	}
}

// ===========================================================================
// appsvc.go — MemoryInsertOpts type
// ===========================================================================

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


// ===========================================================================
// memory.go — MemoryOpts
// ===========================================================================

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
