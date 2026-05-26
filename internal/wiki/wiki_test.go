package wiki

import (
	"os"
	"path/filepath"
	"testing"
)

func TestValidationHelpers(t *testing.T) {
	// 1. isPageRefLine
	testsRefLine := []struct {
		line string
		want bool
	}{
		{"[[Some_Page]]", true},
		{"[source-1]/[[Some_Page]]", true},
		{"src/doc.md", true},
		{"my_doc_page_ref", true},      // >= 2 underscores, > 10 chars, no space
		{"my doc page ref", false},     // contains space
		{"word", false},
		{"simple text with spaces", false},
	}

	for _, tc := range testsRefLine {
		got := isPageRefLine(tc.line)
		if got != tc.want {
			t.Errorf("isPageRefLine(%q) = %t; want %t", tc.line, got, tc.want)
		}
	}

	// 2. isPageRefOnlyAnswer
	testsRefAnswer := []struct {
		answer string
		want   bool
	}{
		{"", true},
		{"  \n  ", true},
		{"- [[Page_One]]\n- [[Page_Two]]", true},
		{"1. [[Page_One]]\n2. [[Page_Two]]", true},
		{"This is a normal paragraph answer explaining details.", false},
		{"- [[Page_One]]\nThis is normal explanation line.", false},
	}

	for _, tc := range testsRefAnswer {
		got := isPageRefOnlyAnswer(tc.answer)
		if got != tc.want {
			t.Errorf("isPageRefOnlyAnswer(%q) = %t; want %t", tc.answer, got, tc.want)
		}
	}

	// 3. buildSynthesisRetryPrompt
	prompt := buildSynthesisRetryPrompt("my query", "my context")
	if !stringsContains(prompt, "my query") || !stringsContains(prompt, "my context") {
		t.Errorf("unexpected buildSynthesisRetryPrompt: %s", prompt)
	}
}

func TestBM25Index(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "graphit-wiki-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tempDir)

	// Create test files
	doc1 := `---
title: Doc 1
tags: [tag]
---
# Document One Title
This is the body content of document one. It describes the design patterns and Go guidelines.`
	err = os.WriteFile(filepath.Join(tempDir, "doc1.md"), []byte(doc1), 0644)
	if err != nil {
		t.Fatalf("failed to write doc1: %v", err)
	}

	doc2 := `
# Document Two Title
This is another document detailing the implementation and test coverage. It focuses on React UI.`
	err = os.WriteFile(filepath.Join(tempDir, "doc2.md"), []byte(doc2), 0644)
	if err != nil {
		t.Fatalf("failed to write doc2: %v", err)
	}

	// Initialize BM25 index
	idx, err := NewBM25Index(tempDir, DefaultBM25Config())
	if err != nil {
		t.Fatalf("NewBM25Index failed: %v", err)
	}

	if idx.totalDocs != 2 {
		t.Errorf("expected 2 documents, got %d", idx.totalDocs)
	}

	// Run Search
	res := idx.Search("React UI design patterns", 5)
	if len(res) == 0 {
		t.Fatal("expected search results, got 0")
	}

	// First result should be doc2 (React UI match) or doc1 (design patterns match)
	if res[0].Title == "" {
		t.Error("expected non-empty result title")
	}

	// Search with no terms (empty or stopwords only)
	resEmpty := idx.Search("the and", 5)
	if len(resEmpty) != 0 {
		t.Errorf("expected 0 results, got %v", resEmpty)
	}
}

func stringsContains(s, sub string) bool {
	return len(s) >= len(sub) && (s == sub || stringsContainsHelper(s, sub))
}

func stringsContainsHelper(s, sub string) bool {
	for i := 0; i <= len(s)-len(sub); i++ {
		if s[i:i+len(sub)] == sub {
			return true
		}
	}
	return false
}
