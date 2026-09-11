package wiki

import (
	"sort"
	"strings"
	"unicode/utf8"
)

// wikiSnippetWidth is how much of a body a search result shows. A chunk is a whole
// document now, so the head of the body is almost never where the answer is.
const wikiSnippetWidth = 320

func truncateSnippet(body string, maxLen int) string {
	body = strings.TrimSpace(body)
	if len(body) <= maxLen {
		return body
	}
	cut := maxLen
	if idx := strings.LastIndex(body[:maxLen], " "); idx > maxLen/2 {
		cut = idx
	}
	return body[:cut] + "…"
}

func snippetAround(body, query string, width int) string {
	body = strings.TrimSpace(body)
	if body == "" {
		return ""
	}
	if len(body) <= width {
		return body
	}

	lowerBody := strings.ToLower(body)
	terms := strings.Fields(strings.ToLower(query))
	sort.Slice(terms, func(i, j int) bool { return len(terms[i]) > len(terms[j]) })

	hit := -1
	for _, term := range terms {
		if len(term) < 3 {
			continue
		}
		if idx := strings.Index(lowerBody, term); idx >= 0 {
			hit = idx
			break
		}
	}
	if hit < 0 {
		return truncateSnippet(body, width)
	}

	start := hit - width/3
	if start < 0 {
		start = 0
	}
	end := start + width
	if end > len(body) {
		end = len(body)
		start = end - width
		if start < 0 {
			start = 0
		}
	}

	// Pull both edges out to a word boundary, but only a little: an unbroken run
	// of bytes (a base64 blob, a minified line) must not drag the window across
	// the whole document. When no boundary turns up inside the slack, snap to a
	// rune boundary instead, so the preview is never cut mid-character.
	const slack = 48
	for i := 0; i < slack && start > 0 && !isASCIISpace(body[start-1]); i++ {
		start--
	}
	for start > 0 && !utf8.RuneStart(body[start]) {
		start--
	}
	for i := 0; i < slack && end < len(body) && !isASCIISpace(body[end]); i++ {
		end++
	}
	for end < len(body) && !utf8.RuneStart(body[end]) {
		end++
	}

	out := strings.TrimSpace(body[start:end])
	if start > 0 {
		out = "…" + out
	}
	if end < len(body) {
		out += "…"
	}
	return out
}

func isASCIISpace(c byte) bool {
	return c == ' ' || c == '\t' || c == '\n' || c == '\r'
}
