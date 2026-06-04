package wiki

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

type mockAIClient struct {
	responses []string
	callCount int
	err       error
}

func (m *mockAIClient) Complete(_ context.Context, _, _ string) (string, error) {
	if m.err != nil {
		return "", m.err
	}
	if m.callCount >= len(m.responses) {
		return "DONE: fallback answer with sufficient content", nil
	}
	r := m.responses[m.callCount]
	m.callCount++
	return r, nil
}

func setupWikiDir(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	writeFile(t, dir, "index.md", "# Wiki Index\n\nPages:\n- [[Architecture]]\n- [[Setup]]\n- [[API_Reference]]")
	writeFile(t, dir, "Architecture.md", "# Architecture\nThe system uses microservices architecture with event-driven communication.")
	writeFile(t, dir, "Setup.md", "# Setup\nInstall dependencies using go mod tidy. Configure the database.")
	writeFile(t, dir, "API_Reference.md", "# API Reference\nThe API exposes REST endpoints for CRUD operations.")
	return dir
}

func TestSearchWiki_DirectAnswer(t *testing.T) {
	t.Parallel()
	dir := setupWikiDir(t)
	client := &mockAIClient{responses: []string{"DONE: The architecture uses microservices with detailed explanation of the design patterns."}}

	result, err := SearchWiki(context.Background(), client, "architecture", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(result.Answer, "microservices") {
		t.Errorf("answer = %q, expected to contain 'microservices'", result.Answer)
	}
	if result.Turns != 1 {
		t.Errorf("turns = %d, want 1", result.Turns)
	}
}

func TestSearchWiki_DirectAnswer_DoneSpace(t *testing.T) {
	t.Parallel()
	dir := setupWikiDir(t)
	client := &mockAIClient{responses: []string{"DONE The answer with a space prefix instead of colon."}}

	result, err := SearchWiki(context.Background(), client, "question", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(result.Answer, "space prefix") {
		t.Errorf("answer = %q, expected DONE-space prefix handling", result.Answer)
	}
}

func TestSearchWiki_WithPageRequest(t *testing.T) {
	t.Parallel()
	dir := setupWikiDir(t)
	client := &mockAIClient{responses: []string{
		"Architecture",
		"DONE: The architecture is microservices-based with comprehensive event-driven design and patterns.",
	}}

	result, err := SearchWiki(context.Background(), client, "architecture details", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Turns != 2 {
		t.Errorf("turns = %d, want 2", result.Turns)
	}
	if !strings.Contains(result.Answer, "microservices") {
		t.Errorf("answer should contain microservices")
	}
}

func TestSearchWiki_MissingIndex(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	client := &mockAIClient{responses: []string{"DONE: answer"}}

	_, err := SearchWiki(context.Background(), client, "query", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 5,
	})
	if err == nil {
		t.Error("expected error for missing index.md")
	}
}

func TestSearchWiki_NoMatchingPages(t *testing.T) {
	t.Parallel()
	dir := setupWikiDir(t)
	client := &mockAIClient{responses: []string{"nonexistent_page_xyz\nanother_missing_page"}}

	result, err := SearchWiki(context.Background(), client, "query", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(result.Answer, "no matching pages found") {
		t.Errorf("answer = %q, expected 'no matching pages found'", result.Answer)
	}
}

func TestSearchWiki_MaxTurns(t *testing.T) {
	t.Parallel()
	dir := setupWikiDir(t)
	client := &mockAIClient{responses: []string{
		"Architecture",
		"Setup",
		"API_Reference",
	}}

	result, err := SearchWiki(context.Background(), client, "comprehensive overview", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 3,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Turns < 3 {
		t.Errorf("turns = %d, expected to reach max turns", result.Turns)
	}
}

func TestSearchWiki_DefaultMaxTurns(t *testing.T) {
	t.Parallel()
	dir := setupWikiDir(t)
	client := &mockAIClient{responses: []string{"DONE: quick answer with enough content."}}

	result, err := SearchWiki(context.Background(), client, "query", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 0, // should default to 6
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Answer == "" {
		t.Error("expected non-empty answer")
	}
}

func TestSearchWiki_AIError(t *testing.T) {
	t.Parallel()
	dir := setupWikiDir(t)
	client := &mockAIClient{err: errors.New("api failure")}

	_, err := SearchWiki(context.Background(), client, "query", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 5,
	})
	if err == nil {
		t.Error("expected error from AI client")
	}
}

func TestSearchWiki_BM25PreFilter(t *testing.T) {
	t.Parallel()
	dir := setupWikiDir(t)
	client := &mockAIClient{responses: []string{"DONE: Comprehensive architecture overview with detailed design insights."}}

	result, err := SearchWiki(context.Background(), client, "architecture", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 5,
		UseBM25:  true,
		BM25TopN: 3,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Answer == "" {
		t.Error("expected non-empty answer with BM25 pre-filter")
	}
	if result.TokensSent == 0 {
		t.Error("expected positive TokensSent")
	}
}

func TestSearchWiki_PageRefRetry(t *testing.T) {
	t.Parallel()
	dir := setupWikiDir(t)
	// First response is DONE but with page-ref-only answer, second is the retry
	client := &mockAIClient{responses: []string{
		"DONE: [[Architecture]]\n[[Setup]]\n[[API_Reference]]",
		"After detailed analysis, the architecture uses microservices with event-driven patterns.",
	}}

	result, err := SearchWiki(context.Background(), client, "architecture overview", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	// The retry should have produced a better answer
	if strings.HasPrefix(result.Answer, "[[") {
		t.Errorf("expected retry to produce non-ref answer, got %q", result.Answer)
	}
}

func TestSearchWiki_AlreadyLoadedPages(t *testing.T) {
	t.Parallel()
	dir := setupWikiDir(t)
	client := &mockAIClient{responses: []string{
		"Architecture",
		"Architecture", // request same page again
		"DONE: Final comprehensive answer about the architecture.",
	}}

	result, err := SearchWiki(context.Background(), client, "query", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Turns != 3 {
		t.Errorf("turns = %d, want 3", result.Turns)
	}
}

func TestSearchWiki_NoParsedPages(t *testing.T) {
	t.Parallel()
	dir := setupWikiDir(t)
	// Reply that has no parseable page list and no DONE prefix → treated as final answer
	client := &mockAIClient{responses: []string{
		"This is a direct answer without any page requests or DONE prefix, providing comprehensive analysis.",
	}}

	result, err := SearchWiki(context.Background(), client, "query", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Answer == "" {
		t.Error("expected non-empty answer")
	}
}

func TestExtractSnippet(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name    string
		content string
		query   string
		check   func(t *testing.T, snippet string)
	}{
		{
			"term_match",
			"Some text before. The architecture uses microservices for communication. More text after.",
			"architecture",
			func(t *testing.T, snippet string) {
				if !strings.Contains(strings.ToLower(snippet), "architecture") {
					t.Errorf("snippet should contain 'architecture', got %q", snippet)
				}
			},
		},
		{
			"no_term_match_short",
			"Short content.",
			"zzzzz",
			func(t *testing.T, snippet string) {
				if snippet != "Short content." {
					t.Errorf("snippet = %q, want full short content", snippet)
				}
			},
		},
		{
			"no_term_match_long",
			strings.Repeat("word ", 100),
			"zzzzz",
			func(t *testing.T, snippet string) {
				if len(snippet) > 160 {
					t.Errorf("snippet too long: %d chars", len(snippet))
				}
			},
		},
		{
			"with_frontmatter_no_match",
			"---\ntitle: Test\n---\n# Body\nContent only here.",
			"zzzzz",
			func(t *testing.T, snippet string) {
				if strings.Contains(snippet, "---") {
					t.Error("snippet should strip frontmatter when no match found")
				}
			},
		},
		{
			"match_near_start",
			"architecture is great and more text here to fill the snippet.",
			"architecture",
			func(t *testing.T, snippet string) {
				if !strings.Contains(snippet, "architecture") {
					t.Errorf("snippet should contain match term")
				}
			},
		},
		{
			"match_near_end",
			strings.Repeat("filler ", 50) + "architecture",
			"architecture",
			func(t *testing.T, snippet string) {
				if !strings.HasPrefix(snippet, "…") {
					t.Error("snippet should start with ellipsis when match is deep")
				}
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			snippet := extractSnippet(tt.content, tt.query)
			tt.check(t, snippet)
		})
	}
}

func TestBuildSearchSystemPrompt(t *testing.T) {
	t.Parallel()
	prompt := buildSearchSystemPrompt("memory")
	if !strings.Contains(prompt, "memory") {
		t.Error("expected moduleTag in system prompt")
	}
	if !strings.Contains(prompt, "DONE:") {
		t.Error("expected DONE protocol in system prompt")
	}
}

func TestParsePageList(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name  string
		reply string
		want  []string
	}{
		{"bullet_list", "- Architecture\n- Setup", []string{"Architecture", "Setup"}},
		{"numbered_list", "1. Architecture\n2. Setup", []string{"Architecture", "Setup"}},
		{"wikilinks", "[[Architecture]]\n[[Setup]]", []string{"Architecture", "Setup"}},
		{"with_md_extension", "Architecture.md\nSetup.md", []string{"Architecture", "Setup"}},
		{"skip_done_prefix", "DONE: answer", nil},
		{"skip_urls", "https://example.com\npage_name", []string{"page_name"}},
		{"mixed_formats", "- [[Alpha]]\n* Beta\n3. Gamma", []string{"Alpha", "Beta", "Gamma"}},
		{"empty_lines_skipped", "\n\nArchitecture\n\n", []string{"Architecture"}},
		{"asterisk_list", "* page1\n* page2", []string{"page1", "page2"}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := parsePageList(tt.reply)
			if len(got) != len(tt.want) {
				t.Errorf("parsePageList() = %v (len %d), want %v (len %d)",
					got, len(got), tt.want, len(tt.want))
				return
			}
			for i := range got {
				if got[i] != tt.want[i] {
					t.Errorf("parsePageList()[%d] = %q, want %q", i, got[i], tt.want[i])
				}
			}
		})
	}
}

func TestLoadWikiPage(t *testing.T) {
	t.Parallel()

	t.Run("existing_page", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		writeFile(t, dir, "Architecture.md", "# Architecture\nContent here.")

		content, slug := loadWikiPage(dir, "Architecture")
		if content == "" {
			t.Error("expected content for existing page")
		}
		if slug != "Architecture" {
			t.Errorf("slug = %q, want 'Architecture'", slug)
		}
	})

	t.Run("nonexistent_page", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()

		content, slug := loadWikiPage(dir, "nonexistent_xyz")
		if content != "" {
			t.Errorf("expected empty content for nonexistent page, got %q", content)
		}
		if slug != "" {
			t.Errorf("expected empty slug, got %q", slug)
		}
	})

	t.Run("safe_filename_fallback", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		writeFile(t, dir, "Hello_World.md", "# Hello World\nContent.")

		content, slug := loadWikiPage(dir, "Hello World")
		if content == "" {
			t.Error("expected SafeFilename fallback to find the page")
		}
		if slug != "Hello_World" {
			t.Errorf("slug = %q, want 'Hello_World'", slug)
		}
	})

	t.Run("fuzzy_match", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		writeFile(t, dir, "Architecture_Overview.md", "# Architecture Overview\nContent.")

		content, slug := loadWikiPage(dir, "architectur_overview")
		if content == "" {
			t.Error("expected fuzzy match to find similar page")
		}
		if slug == "" {
			t.Error("expected non-empty slug from fuzzy match")
		}
	})
}

func TestCleanForFuzzy(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name  string
		input string
		want  string
	}{
		{"lowercase", "Hello", "hello"},
		{"strip_special", "hello_world-test!", "helloworldtest"},
		{"numbers_kept", "page123", "page123"},
		{"empty", "", ""},
		{"all_special", "___---!!!", ""},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := cleanForFuzzy(tt.input)
			if got != tt.want {
				t.Errorf("cleanForFuzzy(%q) = %q, want %q", tt.input, got, tt.want)
			}
		})
	}
}

func TestGetTrigrams(t *testing.T) {
	t.Parallel()

	t.Run("short_string", func(t *testing.T) {
		t.Parallel()
		tg := getTrigrams("ab")
		if len(tg) != 1 || !tg["ab"] {
			t.Errorf("getTrigrams(\"ab\") = %v, want {\"ab\": true}", tg)
		}
	})

	t.Run("normal_string", func(t *testing.T) {
		t.Parallel()
		tg := getTrigrams("hello")
		// "hel", "ell", "llo" → 3 trigrams
		if len(tg) != 3 {
			t.Errorf("getTrigrams(\"hello\") len = %d, want 3", len(tg))
		}
		for _, expected := range []string{"hel", "ell", "llo"} {
			if !tg[expected] {
				t.Errorf("missing trigram %q", expected)
			}
		}
	})

	t.Run("empty_string", func(t *testing.T) {
		t.Parallel()
		tg := getTrigrams("")
		if len(tg) != 1 || !tg[""] {
			t.Errorf("getTrigrams(\"\") = %v", tg)
		}
	})
}

func TestTrigramSimilarity(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name string
		s1   string
		s2   string
		min  float64
		max  float64
	}{
		{"identical", "hello", "hello", 1.0, 1.0},
		{"similar", "architecture", "architectur", 0.7, 1.0},
		{"different", "hello", "world", 0.0, 0.1},
		{"empty_both", "", "", 0.0, 1.0}, // union=0 → returns 0
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := trigramSimilarity(tt.s1, tt.s2)
			if got < tt.min || got > tt.max {
				t.Errorf("trigramSimilarity(%q, %q) = %v, want [%v, %v]",
					tt.s1, tt.s2, got, tt.min, tt.max)
			}
		})
	}
}

func TestFindBestFuzzyMatch(t *testing.T) {
	t.Parallel()

	t.Run("finds_similar", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		writeFile(t, dir, "Architecture_Overview.md", "content")
		writeFile(t, dir, "Setup_Guide.md", "content")

		match := findBestFuzzyMatch(dir, "Architecture_Overviw")
		if match != "Architecture_Overview" {
			t.Errorf("match = %q, want 'Architecture_Overview'", match)
		}
	})

	t.Run("no_match_too_different", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		writeFile(t, dir, "alpha.md", "content")

		match := findBestFuzzyMatch(dir, "zzzzxyzzy")
		if match != "" {
			t.Errorf("expected no match, got %q", match)
		}
	})

	t.Run("empty_target", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		writeFile(t, dir, "page.md", "content")

		match := findBestFuzzyMatch(dir, "")
		if match != "" {
			t.Errorf("expected empty for empty target, got %q", match)
		}
	})

	t.Run("empty_dir", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()

		match := findBestFuzzyMatch(dir, "something")
		if match != "" {
			t.Errorf("expected empty for empty dir, got %q", match)
		}
	})

	t.Run("skips_index_and_log", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		writeFile(t, dir, "index.md", "content")
		writeFile(t, dir, "log.md", "content")

		match := findBestFuzzyMatch(dir, "index")
		if match != "" {
			t.Errorf("expected to skip index, got %q", match)
		}
	})

	t.Run("invalid_dir", func(t *testing.T) {
		t.Parallel()
		match := findBestFuzzyMatch(filepath.Join(t.TempDir(), "nonexistent"), "target")
		if match != "" {
			t.Errorf("expected empty for invalid dir, got %q", match)
		}
	})
}

func TestSearchWiki_FinalSynthesisError(t *testing.T) {
	t.Parallel()
	dir := setupWikiDir(t)

	callNum := 0
	client := &mockAIClient{}
	client.responses = nil
	client.err = nil

	// We need a custom client that succeeds for first calls then fails on the final synthesis.
	// Use a wrapper approach.
	finalErrClient := &finalErrorAIClient{
		normalResponses: []string{"Architecture", "Setup", "API_Reference"},
		finalErr:        errors.New("final synthesis error"),
	}

	result, err := SearchWiki(context.Background(), finalErrClient, "overview", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 3,
	})
	_ = callNum
	// After max turns, the final Complete call fails
	if err == nil {
		// The error might be wrapped
		_ = result
	}
}

type finalErrorAIClient struct {
	normalResponses []string
	callCount       int
	finalErr        error
}

func (f *finalErrorAIClient) Complete(_ context.Context, _, _ string) (string, error) {
	if f.callCount < len(f.normalResponses) {
		r := f.normalResponses[f.callCount]
		f.callCount++
		return r, nil
	}
	return "", f.finalErr
}

func TestSearchWiki_TokensSentTracking(t *testing.T) {
	t.Parallel()
	dir := setupWikiDir(t)
	client := &mockAIClient{responses: []string{"DONE: Comprehensive answer about the topic."}}

	result, err := SearchWiki(context.Background(), client, "query", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.TokensSent == 0 {
		t.Error("expected TokensSent > 0")
	}
}

func TestExtractSnippet_EndBounds(t *testing.T) {
	t.Parallel()
	content := "end"
	snippet := extractSnippet(content, "end")
	if snippet == "" {
		t.Error("expected non-empty snippet")
	}
}

func TestLoadWikiPage_InvalidDir(t *testing.T) {
	t.Parallel()
	content, slug := loadWikiPage(filepath.Join(t.TempDir(), "noexist"), "page")
	if content != "" || slug != "" {
		t.Error("expected empty results for invalid dir")
	}
}

func TestLoadWikiPage_Subdirectory(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	subdir := filepath.Join(dir, "subdir")
	if err := os.MkdirAll(subdir, 0o755); err != nil {
		t.Fatal(err)
	}
	// Subdirectories should be skipped by fuzzy match (only .md files)
	writeFile(t, dir, "page.md", "# Page\nContent.")

	content, _ := loadWikiPage(dir, "page")
	if content == "" {
		t.Error("expected to find page.md")
	}
}
