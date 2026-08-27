package wiki

import (
	"sort"
	"strings"
	"unicode/utf8"
)

// Engine-independent snippet handling for wiki search.

// wikiSnippetWidth is how much of a body a search result shows. A chunk is a whole
// document now, so the head of the body is almost never where the answer is.
const wikiSnippetWidth = 320

func truncateSnippet(body string, maxLen int) string {
	body = strings.TrimSpace(body)
	if len(body) <= maxLen {
		return body
	}
	// Try to break at a word boundary.
	cut := maxLen
	if idx := strings.LastIndex(body[:maxLen], " "); idx > maxLen/2 {
		cut = idx
	}
	return body[:cut] + "…"
}

// snippetAround returns a window of body centred on the first query term found in
// it, falling back to the head of the body when no term occurs.
//
// This is the only snippet builder: the compiled index used to return the first N
// characters of the body while the markdown fallback centred on the match, so the
// same query produced a useful preview through one engine and a useless one
// through the other. With a document per chunk, the head of the body is the
// document's own preamble — never the reason it matched.
func snippetAround(body, query string, width int) string {
	body = strings.TrimSpace(body)
	if body == "" {
		return ""
	}
	if len(body) <= width {
		return body
	}

	lowerBody := strings.ToLower(body)
	// Longest term first: matching "authentication" beats matching "the" inside it.
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

	// Centre the window on the hit.
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

// Browse returns wiki entries matching the given filter.
// Each slug is returned once, using the chunk with the shortest breadcrumb
