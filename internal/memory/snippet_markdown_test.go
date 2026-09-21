package memory

import (
	"strings"
	"testing"
	"unicode/utf8"
)

func TestSnippetPreservesMarkdownAndUnicode(t *testing.T) {
	body := "**Evidence**\n\n- first\n- second"
	if got := memorySnippet(body); got != body {
		t.Fatalf("markdown changed: %q", got)
	}
	body = strings.Repeat("界", 241)
	got := memorySnippet(body)
	if !utf8.ValidString(got) || utf8.RuneCountInString(got) != 241 || !strings.HasSuffix(got, "…") {
		t.Fatalf("invalid truncation: %q", got)
	}
}
