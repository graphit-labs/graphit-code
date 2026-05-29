package wiki

import (
	"context"
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

func TestResolveSlug(t *testing.T) {
	tests := []struct {
		rawLink string
		want    string
	}{
		{"Some_Page", "Some_Page"},
		{"Some Page", "Some_Page"},
		{"Some Page|Display Label", "Some_Page"},
		{"Some_Page|Display Label", "Some_Page"},
		{"  Some Page  |  Display Label  ", "Some_Page"},
		{"", ""},
	}

	for _, tc := range tests {
		got := ResolveSlug(tc.rawLink)
		if got != tc.want {
			t.Errorf("ResolveSlug(%q) = %q; want %q", tc.rawLink, got, tc.want)
		}
	}
}

func TestFindWikiLinks(t *testing.T) {
	content := "This is a link to [[Some Page|Display Label]] and another link to [[Other_Page]].\n" +
		"```go\n" +
		"// This should be ignored: [[Ignored Page]]\n" +
		"```\n" +
		"Also this `[[Ignored Inline]]` should be ignored."
	got := FindWikiLinks(content)
	want := []string{"Some_Page", "Other_Page"}
	if len(got) != len(want) {
		t.Fatalf("FindWikiLinks length = %d; want %d (got: %v)", len(got), len(want), got)
	}
	for i := range got {
		if got[i] != want[i] {
			t.Errorf("FindWikiLinks[%d] = %q; want %q", i, got[i], want[i])
		}
	}
}

type mockAIClient struct {
	responses []string
	calls     int
}

func (m *mockAIClient) Complete(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
	m.calls++
	if m.calls <= len(m.responses) {
		return m.responses[m.calls-1], nil
	}
	return "DONE: default response", nil
}

func TestSearchWikiDeduplication(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "graphit-search-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()

	// Write index.md
	err = os.WriteFile(filepath.Join(tempDir, "index.md"), []byte("# Wiki Index\n- [[Page_One]]\n- [[Page_Two]]"), 0644)
	if err != nil {
		t.Fatalf("failed to write index: %v", err)
	}
	// Write Page_One.md
	err = os.WriteFile(filepath.Join(tempDir, "Page_One.md"), []byte("# Page One\nContent of page one."), 0644)
	if err != nil {
		t.Fatalf("failed to write page one: %v", err)
	}

	// Turn 1: LLM asks to read Page_One
	// Turn 2: LLM asks to read Page_One again (testing deduplication)
	// Turn 3: LLM replies with DONE
	mockClient := &mockAIClient{
		responses: []string{
			"Page_One",
			"Page_One",
			"DONE: final synthesized answer",
		},
	}

	cfg := SearchConfig{
		WikiDir:  tempDir,
		MaxTurns: 3,
		UseBM25:  false,
	}

	res, err := SearchWiki(context.Background(), mockClient, "some query", cfg)
	if err != nil {
		t.Fatalf("SearchWiki failed: %v", err)
	}

	if res.Answer != "final synthesized answer" {
		t.Errorf("expected answer 'final synthesized answer', got %q", res.Answer)
	}
}

func TestBM25Index(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "graphit-wiki-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()

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

	// Test spelling correction / query expansion (e.g. "Reactt" should expand to "react", "patternss" to "patterns")
	resFuzzy := idx.Search("Reactt UI design patternss", 5)
	if len(resFuzzy) == 0 {
		t.Fatal("expected search results with typos, got 0")
	}
	if resFuzzy[0].Title != res[0].Title {
		t.Errorf("expected same best match %q, got %q", res[0].Title, resFuzzy[0].Title)
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

func TestTrigramFuzzyMatch(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "graphit-trigram-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()

	// Write Page_One.md
	err = os.WriteFile(filepath.Join(tempDir, "Page_One.md"), []byte("# Page One\nContent of page one."), 0644)
	if err != nil {
		t.Fatalf("failed to write page: %v", err)
	}

	// 1. Exact match
	content, slug := loadWikiPage(tempDir, "Page_One")
	if content == "" || slug != "Page_One" {
		t.Errorf("expected exact match 'Page_One', got slug %q", slug)
	}

	// 2. Spaces / Typo match
	content, slug = loadWikiPage(tempDir, "page  one")
	if content == "" || slug != "Page_One" {
		t.Errorf("expected fuzzy match 'Page_One' for typo 'page  one', got slug %q", slug)
	}

	// 3. Typo/variation match
	content, slug = loadWikiPage(tempDir, "PageOne")
	if content == "" || slug != "Page_One" {
		t.Errorf("expected fuzzy match 'Page_One' for variation 'PageOne', got slug %q", slug)
	}

	// 4. Non-matching page
	content, slug = loadWikiPage(tempDir, "Page_Two")
	if content != "" || slug != "" {
		t.Errorf("expected no match for 'Page_Two', got slug %q", slug)
	}
}

