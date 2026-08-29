package wiki

import (
	"fmt"
	"strings"
)

// searchPreviewWidth is how much of a page a search hit shows WHEN a preview was asked for.
//
// It is deliberately narrower than the 200-320 characters the previews used to carry. A
// preview exists to break a tie between two plausible titles, not to answer the question:
// the answer comes from graphit_wiki_source, which slices, so paying for a long preview on
// every one of twenty hits buys context for the nineteen the agent will not open.
const searchPreviewWidth = 140

func trimPreview(s string) string {
	s = strings.ReplaceAll(s, "|", "/")
	s = strings.ReplaceAll(s, "\n", " ")
	s = strings.TrimSpace(s)
	if len(s) <= searchPreviewWidth {
		return s
	}
	cut := strings.LastIndex(s[:searchPreviewWidth], " ")
	if cut < searchPreviewWidth/2 {
		cut = searchPreviewWidth
	}
	return s[:cut] + "…"
}

// FormatSearchResultsTOON renders search hits for an agent.
//
// The default row is IDENTITY AND RANKING ONLY — slug, title, type, score — and carries no
// text from the page. What a search is for is deciding WHICH page to open; the deciding is
// done on the title, and the reading is a separate, deliberate call to graphit_wiki_source,
// which can slice a long page down to the part that matters. Shipping a preview per hit
// inverted that: the expensive half was paid up front, for every hit, whether or not the
// agent went on to read anything.
//
// withPreview restores a short excerpt for the caller that genuinely ranks on wording
// rather than on titles.
func FormatSearchResultsTOON(results []WikiSearchResult, withPreview bool) string {
	if len(results) == 0 {
		return "results[0]{}:"
	}
	var sb strings.Builder
	if withPreview {
		fmt.Fprintf(&sb, "results[%d]{slug|title|type|score|preview}:\n", len(results))
	} else {
		fmt.Fprintf(&sb, "results[%d]{slug|title|type|score}:\n", len(results))
	}
	for _, r := range results {
		if withPreview {
			fmt.Fprintf(&sb, "  %s|%s|%s|%.1f|%s\n", r.Slug, r.Title, r.DocType, r.Score, trimPreview(r.Summary))
			continue
		}
		fmt.Fprintf(&sb, "  %s|%s|%s|%.1f\n", r.Slug, r.Title, r.DocType, r.Score)
	}
	return strings.TrimRight(sb.String(), "\n")
}

// FormatBM25ResultsTOON is FormatSearchResultsTOON for the markdown-ranked path, which
// knowledge_search and memory_search go through. Same contract: titles by default, the
// page itself only when the agent decides to open it.
func FormatBM25ResultsTOON(results []BM25Result, withPreview bool) string {
	if len(results) == 0 {
		return "results[0]{}:"
	}
	var sb strings.Builder
	if withPreview {
		fmt.Fprintf(&sb, "results[%d]{slug|title|type|score|preview}:\n", len(results))
	} else {
		fmt.Fprintf(&sb, "results[%d]{slug|title|type|score}:\n", len(results))
	}
	for _, r := range results {
		slug := strings.TrimSuffix(r.Path, ".md")
		if withPreview {
			fmt.Fprintf(&sb, "  %s|%s|%s|%.1f|%s\n", slug, r.Title, r.DocType, r.Score, trimPreview(r.Snippet))
			continue
		}
		fmt.Fprintf(&sb, "  %s|%s|%s|%.1f\n", slug, r.Title, r.DocType, r.Score)
	}
	return strings.TrimRight(sb.String(), "\n")
}

// FormatBrowseResultsTOON formats BrowseEntry slices in compact TOON format.
func FormatBrowseResultsTOON(entries []BrowseEntry) string {
	if len(entries) == 0 {
		return "results[0]{}:"
	}
	var sb strings.Builder
	fmt.Fprintf(&sb, "results[%d]{slug|title|type|confidence|words|summary}:\n", len(entries))
	for _, e := range entries {
		summary := e.Summary
		if len(summary) > 150 {
			summary = summary[:150] + "…"
		}
		summary = strings.ReplaceAll(summary, "|", "/")
		summary = strings.ReplaceAll(summary, "\n", " ")
		fmt.Fprintf(&sb, "  %s|%s|%s|%.1f|%d|%s\n", e.Slug, e.Title, e.DocType, e.Confidence, e.WordCount, summary)
	}
	return strings.TrimRight(sb.String(), "\n")
}

// FormatXRefResultsTOON formats XRefResult slices in compact TOON format.
func FormatXRefResultsTOON(query string, depth int, refs []XRefResult) string {
	if len(refs) == 0 {
		return fmt.Sprintf("xrefs[0]{%s|depth:%d}:", query, depth)
	}
	var sb strings.Builder
	fmt.Fprintf(&sb, "xrefs[%d]{slug|title|type|direction}:\n", len(refs))
	for _, r := range refs {
		fmt.Fprintf(&sb, "  %s|%s|%s|%s\n", r.Slug, r.Title, r.RefType, r.Direction)
	}
	return strings.TrimRight(sb.String(), "\n")
}

// FormatSyncLogTOON formats SyncLogEntry slices in compact TOON format.
func FormatSyncLogTOON(entries []SyncLogEntry) string {
	if len(entries) == 0 {
		return "log[0]{}:"
	}
	var sb strings.Builder
	fmt.Fprintf(&sb, "log[%d]{id|timestamp|docs|written|added|updated|deleted}:\n", len(entries))
	for _, e := range entries {
		fmt.Fprintf(&sb, "  %d|%s|%d|%d|%s|%s|%s\n",
			e.ID, e.Timestamp, e.TotalDocs, e.ArticlesWritten,
			strings.Join(e.Added, ","),
			strings.Join(e.Updated, ","),
			strings.Join(e.Deleted, ","))
	}
	return strings.TrimRight(sb.String(), "\n")
}
