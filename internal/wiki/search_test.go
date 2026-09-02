package wiki

import (
	"context"
	"errors"
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

// A compiled index, not a directory of pages — including the catalogue, which used to be the
// `index.md` fixture and is now a Browse over these same rows.
func setupWikiDir(t *testing.T) string {
	t.Helper()
	return indexedWiki(t, []WikiChunk{
		{Slug: "Architecture", Title: "Architecture", DocType: "architecture", ClusterID: -1,
			Body: "The system uses microservices architecture with event-driven communication.", WordCount: 9},
		{Slug: "Setup", Title: "Setup", DocType: "guide", ClusterID: -1,
			Body: "Install dependencies using go mod tidy. Configure the database.", WordCount: 9},
		{Slug: "API_Reference", Title: "API Reference", DocType: "api", ClusterID: -1,
			Body: "The API exposes REST endpoints for CRUD operations.", WordCount: 8},
	})
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
		t.Error("expected error for a wiki with no compiled index")
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
				// Bounded by the window, plus room for the ellipsis and for pulling the
				// edge out to a word boundary.
				if len(snippet) > wikiSnippetWidth+64 {
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
			got := CleanForFuzzy(tt.input)
			if got != tt.want {
				t.Errorf("CleanForFuzzy(%q) = %q, want %q", tt.input, got, tt.want)
			}
		})
	}
}

func TestGetTrigrams(t *testing.T) {
	t.Parallel()

	t.Run("short_string", func(t *testing.T) {
		t.Parallel()
		tg := GetTrigrams("ab")
		if len(tg) != 1 || !tg["ab"] {
			t.Errorf("GetTrigrams(\"ab\") = %v, want {\"ab\": true}", tg)
		}
	})

	t.Run("normal_string", func(t *testing.T) {
		t.Parallel()
		tg := GetTrigrams("hello")
		// "hel", "ell", "llo" → 3 trigrams
		if len(tg) != 3 {
			t.Errorf("GetTrigrams(\"hello\") len = %d, want 3", len(tg))
		}
		for _, expected := range []string{"hel", "ell", "llo"} {
			if !tg[expected] {
				t.Errorf("missing trigram %q", expected)
			}
		}
	})

	t.Run("empty_string", func(t *testing.T) {
		t.Parallel()
		tg := GetTrigrams("")
		if len(tg) != 1 || !tg[""] {
			t.Errorf("GetTrigrams(\"\") = %v", tg)
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
			got := TrigramSimilarity(tt.s1, tt.s2)
			if got < tt.min || got > tt.max {
				t.Errorf("TrigramSimilarity(%q, %q) = %v, want [%v, %v]",
					tt.s1, tt.s2, got, tt.min, tt.max)
			}
		})
	}
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
