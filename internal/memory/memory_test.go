package memory

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestHelpers(t *testing.T) {
	// IsImportantMemory
	if !IsImportantMemory("abc_important_.md") {
		t.Error("expected abc_important_.md to be important")
	}
	if IsImportantMemory("abc.md") {
		t.Error("expected abc.md NOT to be important")
	}

	// FileNames
	if ImportantFileName("123") != "123_important_.md" {
		t.Errorf("unexpected ImportantFileName: %s", ImportantFileName("123"))
	}
	if NormalFileName("123") != "123.md" {
		t.Errorf("unexpected NormalFileName: %s", NormalFileName("123"))
	}

	// firstLineFromContent
	testsFirstLine := []struct {
		body string
		want string
	}{
		{"", ""},
		{"\n# Heading\n  \nBody text line here\nOther line", "Body text line here"},
		{strings.Repeat("A", 120), strings.Repeat("A", 100) + "…"},
	}
	for _, tc := range testsFirstLine {
		got := firstLineFromContent(tc.body)
		if got != tc.want {
			t.Errorf("firstLineFromContent(%q) = %q; want %q", tc.body, got, tc.want)
		}
	}

	// extractBodyAfterFrontmatter
	content := `---
title: My Title
---
# Header Title

Body content starts here.
Another line.`
	extracted := extractBodyAfterFrontmatter(content)
	expectedExtracted := "Body content starts here.\nAnother line."
	if extracted != expectedExtracted {
		t.Errorf("expected %q, got %q", expectedExtracted, extracted)
	}
}

func TestDirsAndScope(t *testing.T) {
	tempHome, err := os.MkdirTemp("", "graphit-memdir-test-*")
	if err != nil {
		t.Fatalf("failed to create temp home: %v", err)
	}
	defer os.RemoveAll(tempHome)

	origHome := os.Getenv("HOME")
	os.Setenv("HOME", tempHome)
	defer os.Setenv("HOME", origHome)

	baseDir := GlobalBaseDir()
	expectedBase := filepath.Join(tempHome, "."+brand.Brand, "memory")
	if baseDir != expectedBase {
		t.Errorf("expected %s, got %s", expectedBase, baseDir)
	}

	projectLink := ProjectLinkDir("project")
	expectedLink := filepath.Join(brand.DotDir(), "memory", "project")
	if projectLink != expectedLink {
		t.Errorf("expected %s, got %s", expectedLink, projectLink)
	}

	err = EnsureScopeDirs("project", tempHome)
	if err != nil {
		t.Errorf("EnsureScopeDirs failed: %v", err)
	}
}

func TestListAndRenderMemoriesInDir(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "graphit-mem-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tempDir)

	// Create an important memory file
	impFile := filepath.Join(tempDir, "123_important_.md")
	impContent := `---
title: Important Title
created: 2026-05-26T00:00:00Z
---
# Important Title

This is the important memory details.`
	_ = os.WriteFile(impFile, []byte(impContent), 0644)

	// Create a normal memory file
	normFile := filepath.Join(tempDir, "456.md")
	normContent := `---
title: Recent Title
created: 2026-05-26T01:00:00Z
---
# Recent Title

This is recent memory body.`
	_ = os.WriteFile(normFile, []byte(normContent), 0644)

	// Test listImportantInDir
	impList, err := listImportantInDir(tempDir)
	if err != nil {
		t.Fatalf("listImportantInDir failed: %v", err)
	}
	if len(impList) != 1 || impList[0].ID != "123" {
		t.Errorf("expected 1 important memory with ID 123, got %v", impList)
	}

	// We cannot test ListImportantMemories directly because it resolves RawDir(scope),
	// but we can test the rendering logic by passing the list directly if we mock or test helpers.
	// We can test listImportantInDir since it is the core execution logic.
}
