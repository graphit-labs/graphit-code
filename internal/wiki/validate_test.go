package wiki

import (
	"strings"
	"testing"
)

func TestIsPageRefOnlyAnswer(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name   string
		answer string
		want   bool
	}{
		{"empty", "", true},
		{"whitespace_only", "   \n\n  ", true},
		{"pure_text_answer", "This is a detailed answer about the topic.", false},
		{"single_wikilink", "[[Some_Page]]", true},
		{"single_path_ref", "knowledge/page", true},
		{"slug_ref_with_underscores", "Some_Long_Page_Name", true},
		{"mostly_refs_80_percent", "[[Page1]]\n[[Page2]]\n[[Page3]]\n[[Page4]]\nSome text", true},
		{"below_threshold", "[[Page1]]\nSome text\nMore text\nEven more text\nAnother line", false},
		{"bullet_list_refs", "- [[Page1]]\n- [[Page2]]\n* [[Page3]]", true},
		{"numbered_list_refs", "1. [[Page1]]\n2. [[Page2]]\n3. [[Page3]]", true},
		{"mixed_content", "Here is a detailed explanation.\nIt covers many aspects.\n[[Page1]] for reference.", false},
		{"bracket_slash_format", "[knowledge]/page\n[memory]/other", true},
		{"short_slug_under_10_chars", "Sh_rt", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := isPageRefOnlyAnswer(tt.answer)
			if got != tt.want {
				t.Errorf("isPageRefOnlyAnswer(%q) = %v, want %v", tt.answer, got, tt.want)
			}
		})
	}
}

func TestIsPageRefLine(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name string
		line string
		want bool
	}{
		{"wikilink", "[[Some_Page]]", true},
		{"path_ref", "knowledge/page", true},
		{"bracket_slash", "[knowledge]/page", true},
		{"plain_text", "this is plain text", false},
		{"slug_with_underscores_long", "Some_Really_Long_Page_Name", true},
		{"slug_with_underscores_short", "Sh_rt", false},
		{"empty", "", false},
		{"path_with_spaces", "knowledge/page name", false},
		{"single_word", "hello", false},
		{"url_like", "https://example.com", true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := isPageRefLine(tt.line)
			if got != tt.want {
				t.Errorf("isPageRefLine(%q) = %v, want %v", tt.line, got, tt.want)
			}
		})
	}
}

func TestBuildSynthesisRetryPrompt(t *testing.T) {
	t.Parallel()
	result := buildSynthesisRetryPrompt("what is X?", "some context here")
	if !strings.Contains(result, "what is X?") {
		t.Error("expected query to be included in retry prompt")
	}
	if !strings.Contains(result, "some context here") {
		t.Error("expected context to be included in retry prompt")
	}
	if !strings.Contains(result, "WIKI CONTENT") {
		t.Error("expected WIKI CONTENT delimiter in retry prompt")
	}
	if !strings.Contains(result, "QUESTION:") {
		t.Error("expected QUESTION: prefix in retry prompt")
	}
}

func TestSynthesisSystemPrompt(t *testing.T) {
	t.Parallel()
	if synthesisSystemPrompt == "" {
		t.Error("synthesisSystemPrompt should not be empty")
	}
	if !strings.Contains(synthesisSystemPrompt, "wiki") {
		t.Error("expected 'wiki' in synthesis system prompt")
	}
}
